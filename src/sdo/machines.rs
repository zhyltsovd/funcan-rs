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

    InitSingleDownload(usize),
    InitMultipleDownload(usize),
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
    fn init_upload(self: &mut Self) -> ClientOutput<N, RR, RW> {
        self.state = ClientState::InitUploading;
        ClientOutput::Output(ClientRequest::InitUpload(self.current_index))
    }

    
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
        
        let req = if n <= 4 {
            self.state = ClientState::InitSingleDownload(n);
            let mut data = [0; 4];
            data.copy_from_slice(&self.data[0..n]);
            
            ClientRequest::InitSingleSegmentDownload(self.current_index, n as u8, data)
        } else {
            self.state = ClientState::InitMultipleDownload(n);
            ClientRequest::InitMultipleDownload(self.current_index, n as u32)        
        };

        ClientOutput::Output(req)
    }

    pub fn output_data(self: &mut Self) -> ClientOutput<N, RR, RW> {
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
}

/// Finite State Machine implementation
impl<const N: usize, RR, RW> MealyMachine<ServerResponse, ClientOutput<N, RR, RW>> for ClientMachine<N, RR, RW> {

    
    fn initiate(self: &mut Self) {
        self.state = ClientState::Idle;
        self.data_index = 0;
    }

    fn transit(self: &mut Self, response: ServerResponse) -> ClientOutput<N, RR, RW> {
        use crate::sdo::machines::ClientState::*;
        use crate::sdo::ServerResponse::*;
        use crate::sdo::machines::ClientOutput::*;
        use crate::sdo::machines::ClientResult::*;
    
        match (&self.state, response) {
            (InitUploading, UploadSingleSegment(res_index, len, data)) => {
                if res_index != self.current_index {
                    self.state = Idle;
                    Error(SdoError::CanIndexMismatch(res_index, self.current_index))
                } else {
                    self.data[0..4].copy_from_slice(&data);
                    self.data_index += len as usize;
                    let _ = self.current_mode.pop();
                    if self.current_mode.len() == 0 {   
                        self.state = Idle;
                        self.output_data()
                    } else {
                        self.current_index.inc_sub();
                        self.init_upload()
                    }
                    
                }
            },
        }
    }
}
