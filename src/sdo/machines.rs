use heapless::vec::*;
use crate::interfaces::*;
use crate::machine::*;
use crate::sdo::*;

/// Possible errors during SDO communications
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdoError {
    StateResponseMismatch,
    CanIndexMismatch(CanIndex, CanIndex),
    TransferAborted(AbortCode),
    ToggleMismatch,
    BufferOverflow,
}

/// Client states
enum ClientState {
    Idle,
    InitUploading,
//    SingleSegmentUploaded,
    UploadingMultiples(ToggleBit),
//    MultiplesUploaded,

    InitiateSingleDownload(usize),
    InitiateMultipleDownload(usize),
    DownloadingSegments(ToggleBit, usize),
//    DownloadCompleted,
}

/// Client context
pub struct ClientMachine<const N: usize, RR, RW> {
    current_index: CanIndex,
    current_mode: Vec<u8, 254>,
    state: ClientState,
    data_index: usize,
    read_responder: Option<RR>,
    write_responder: Option<RW>,
    data: [u8; N],
}

/// Possible final result that machine produces
pub enum ClientResult<const N: usize, RR, RW> {
    UploadCompleted(CanBaseIndex, [u8; N], usize, Option<RR>),
    DownloadCompleted(Option<RW>),
    TransferAborted(AbortCode),
}
        
/// All possible observations of client machine
pub enum ClientOutput<const N: usize, RR, RW> {
    Output(ClientRequest),
    Done(ClientResult<N, RR, RW>),
    Error(SdoError),
    Ready,
}

impl<const N: usize, RR, RW> ClientOutput<N, RR, RW> {
    pub fn is_ready(self: &Self) -> bool {
        match self {
            ClientOutput::Output(_) => false,
            _ => true,
        }
    }
}

impl<const N: usize, RR, RW> Default for ClientMachine<N, RR, RW> {
    fn default() -> Self {
        ClientMachine {
            read_responder: None,
            write_responder: None,
            current_index: CanIndex::new(0, 0),
            current_mode: Vec::new(),
            state: ClientState::Idle,
            data_index: 0,
            data: [0; N],
        }
    }
}

impl<const N: usize, RR, RW> ClientMachine<N, RR, RW> {
    
    /// Initiates SDO read
    pub fn read(self: &mut Self, indices: CanIndices, r: RR) -> ClientOutput<N, RR, RW> {
        self.current_index = indices.base_index.into();
        self.current_mode = indices.can_type.is_compound();
        self.read_responder = Some(r);
        self.init_upload()
    }

    /// Initiates SDO write
    pub fn write<T>(self: &mut Self, indices: CanIndices, t: T, r: RW) -> ClientOutput<N, RR, RW>
    where
        T: IntoBuf,
    {
        self.current_index = indices.base_index.into();
        self.current_mode = indices.can_type.is_compound();
        let n = t.into_buf(&mut self.data);
        self.write_responder = Some(r);
        
        self.init_download(n)
    }

    fn init_upload(self: &mut Self) -> ClientOutput<N, RR, RW> {
        self.state = ClientState::InitUploading;
        ClientOutput::Output(ClientRequest::InitUpload(self.current_index))
    }
 
    fn init_download(self: &mut Self, n: usize) -> ClientOutput<N, RR, RW> {
        let req = if n <= 4 {
            self.state = ClientState::InitiateSingleDownload(n);
            let mut data = [0; 4];
            data.copy_from_slice(&self.data[0..n]);
            
            ClientRequest::InitSingleSegmentDownload(self.current_index, n as u8, data)
        } else {
            self.state = ClientState::InitiateMultipleDownload(n);
            ClientRequest::InitMultipleDownload(self.current_index, n as u32)        
        };

        ClientOutput::Output(req)
    }
        
    fn output_data(self: &mut Self) -> ClientOutput<N, RR, RW> {
        use crate::sdo::machines::ClientResult::*;
        use crate::sdo::machines::ClientOutput::*;
            
        let resp = core::mem::replace(&mut self.read_responder, None);
        let res =
            UploadCompleted(self.current_index.into(),
                            self.data.clone(), 
                            self.data_index,
                            resp);
        Done(res)
    }

    fn continue_uploading(self: &mut Self) -> ClientOutput<N, RR, RW> {
        use crate::sdo::machines::ClientState::*;
        
        let _ = self.current_mode.pop();
        if self.current_mode.len() == 0 {   
            self.state = Idle;
            self.output_data()
        } else {
            self.current_index.inc_sub();
            self.init_upload()
        }
    }

    fn continue_downloading(self: &mut Self) -> ClientOutput<N, RR, RW> {
        use crate::sdo::machines::ClientState::*;
        
        let _ = self.current_mode.pop();
        if self.current_mode.len() == 0 {   
            self.state = Idle;
            self.complete_downloading()
        } else {
            self.current_index.inc_sub();
            self.init_download(self.current_mode[0] as usize)
        }
    }

    fn complete_downloading(self: &mut Self) -> ClientOutput<N, RR, RW> {
        use crate::sdo::machines::ClientOutput::*;
        use crate::sdo::machines::ClientResult::*;
        
        let resp = core::mem::replace(&mut self.write_responder, None);
        Done(DownloadCompleted(resp))
    }

    fn download_segment(self: &mut Self, t: ToggleBit, len: usize) -> ClientOutput<N, RR, RW> {
        use crate::sdo::machines::ClientOutput::*;
        use crate::sdo::ClientRequest::*;

        self.state = ClientState::DownloadingSegments(t, len);
        
        // Prepare data segment to download
        let mut data = [0u8; 7];
        let ix0 = self.data_index;
        let ix1 = (ix0 + 7).min(len);
        let end = self.data_index + 7 >= len;
        
        data.copy_from_slice(&self.data[ix0..ix1]);
        Output(DownloadSegment(t, end, 7, data))
    }
}

/// Finite State Machine implementation
impl<const N: usize, RR, RW> MealyMachine<ServerResponse, ClientOutput<N, RR, RW>> for ClientMachine<N, RR, RW> {
    fn initiate(self: &mut Self) {
        self.state = ClientState::Idle;
        self.data_index = 0;
    }

    fn transit(self: &mut Self, response: ServerResponse) -> ClientOutput<N, RR, RW> {
        use crate::sdo::machines::ClientState::*;
        use crate::sdo::ClientRequest::*;
        use crate::sdo::ServerResponse::*;
        use crate::sdo::machines::ClientOutput::*;
     
        match (&self.state, response) {

            // ---- Upload Handling ----
            
            (InitUploading, UploadSingleSegment(res_index, len, data)) => {
                if res_index != self.current_index {
                    self.state = Idle;
                    Error(SdoError::CanIndexMismatch(res_index, self.current_index))
                } else {
                    self.data[0..4].copy_from_slice(&data);
                    self.data_index += len as usize;

                    self.continue_uploading()
                }
            },

            (InitUploading, ServerResponse::UploadInitMultiples(res_index, _size)) => {
                if res_index != self.current_index {
                    self.state = Idle;
                    Error(SdoError::CanIndexMismatch(res_index, self.current_index))
                } else {
                    self.data_index = 0;
                    let t = ToggleBit(false);
                    self.state = UploadingMultiples(t);
                    Output(UploadSegment(t))                                   
                }
            }

            (UploadingMultiples(toggle), UploadMultiples(res_toggle, end, len, data)) => {
                if res_toggle != *toggle {
                    Error(SdoError::ToggleMismatch)
                } else {
                    let idx = self.data_index;
                    let data_len = len as usize;
                    if idx + data_len > self.data.len() {
                        Error(SdoError::BufferOverflow)
                    } else {
                        self.data[idx..idx + data_len].copy_from_slice(&data[0..data_len]);
                        self.data_index = idx + data_len;
                        if end {
                            self.continue_uploading()
                        } else {
                            let new_toggle = !*toggle;
                            self.state = UploadingMultiples(new_toggle);
                            Output(UploadSegment(new_toggle))
                        }
                    }
                }
            }

            // ---- Download Handling ----
                        
            (InitiateSingleDownload(_len), DownloadInitAck(res_index)) => {
                self.state = Idle;
                if res_index != self.current_index {
                    Error(SdoError::CanIndexMismatch(res_index, self.current_index))
                } else {
                    self.complete_downloading()
                }
            }

            (InitiateMultipleDownload(len), DownloadInitAck(res_index)) => {
                if res_index != self.current_index {
                    self.state = Idle;
                    Error(SdoError::CanIndexMismatch(res_index, self.current_index))
                } else {
                    let t = ToggleBit(false);
                    self.download_segment(t, *len)
                }
            }

            (DownloadingSegments(toggle, n), DownloadSegmentAck(res_toggle)) => {
                if res_toggle != *toggle {
                    self.state = Idle;
                    Error(SdoError::ToggleMismatch)
                } else {
                    if self.data_index + 7 < *n {
                        let new_toggle = !*toggle;
                        self.data_index = self.data_index + 7;
                        self.download_segment(new_toggle, *n)
                    } else {
                        self.continue_downloading()
                    }
                }
            }

            // Default: Unexpected response
            (_state, _response) => {
                Error(SdoError::StateResponseMismatch)
            }

        }
        
    }
} 
  
/// Server states
enum ServerState {
    Idle,
    AwaitingData(bool),
    UploadingSingleSegment,
    UploadingMultipleSegments {
        response_toggle: ToggleBit,
        expected_next_toggle: ToggleBit,
        position: usize,
    },
    DownloadingSingleSegment,
    DownloadingMultipleSegments(ToggleBit, usize),
    ErrorState(Error),
}

/// Server context
pub struct ServerMachine {
    index: CanIndex,
    state: ServerState,
    upload_data: [u8; 1024],
    upload_length: usize,
    download_data: [u8; 1024],
    download_length: usize,
    download_position: usize,
}

impl ServerMachine {
    fn upload_data(self: &mut Self, data: &[u8]) {
        if let ServerState::AwaitingData(b) = self.state {
            let n = data.len();
            self.upload_length = n;
            self.upload_data[0..n].copy_from_slice(data);

            if b {
                self.state = ServerState::UploadingSingleSegment;
            } else {
                self.state = ServerState::UploadingMultipleSegments {
                    response_toggle: ToggleBit(false),
                    expected_next_toggle: ToggleBit(true),
                    position: 0,
                };
            }
        }
    }
}
