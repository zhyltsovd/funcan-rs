
use crate::sdo::*;
use crate::machine::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    StateResponseMismatch,
    IndexMismatch(Index, Index),
    TransferAborted(AbortCode),
    ToggleMismatch,
    BufferOverflow,
    ProtocolError,
}

enum ClientState {
    Idle,
    InitUpload(Index),
    SingleSegmentUploaded,
    UploadingMultiples(ToggleBit), 
    MultiplesUploaded,

    InitSingleDownload(Index, usize),          
    InitMultipleDownload(Index, usize),          
    DownloadingSegments(ToggleBit, usize),      
    DownloadCompleted,

    ErrorState(Error),
}

pub struct ClientMachine {
    state: ClientState,
    data_index: usize,
    data: [u8; 1024],
}

pub enum ClientResult {
    UploadCompleted([u8; 1024], usize),
    DownloadCompleted,
    TransferAborted(AbortCode),
}

pub enum ClientOutput {
    Output(ClientRequest),
    Done(ClientResult),
    Error(Error),
}

impl ClientOutput {
    pub fn is_ready(self: &Self) -> bool {
        match self {
            ClientOutput::Output(_) => false,
            _ => true,
        }
    }
}

impl Default for ClientMachine {
    fn default() -> Self {
        ClientMachine {
            state: ClientState::Idle,
            data_index: 0,
            data: [0; 1024],
        }
    }
}

impl ClientMachine {

    pub fn read(self: &mut Self, index: Index) {
        self.state = ClientState::InitUpload(index);
    } 
}

impl MachineTrans<ServerResponse> for ClientMachine {

    type Observation = Option<ClientOutput>;
    
    fn initial(self: &mut Self) {
        self.state = ClientState::Idle;
        self.data_index = 0;
    }

    fn transit(self: &mut Self, response: ServerResponse) {
        
        match (&self.state, response) {
            // ---- Upload Handling ----
            // InitUpload -> UploadSingleSegment
            (ClientState::InitUpload(index), ServerResponse::UploadSingleSegment(res_index, len, data)) => {
                if res_index != * index {
                    self.state = ClientState::ErrorState(Error::IndexMismatch(res_index, *index));
                } else {
                    self.data[0 .. 4].copy_from_slice(&data);
                    self.data_index = len as usize;
                    self.state = ClientState::SingleSegmentUploaded;
                }
            }

            // InitUpload -> InitMupltipleSegments
            (ClientState::InitUpload(index), ServerResponse::UploadInitMultiples(res_index, _size)) => {
                if res_index != * index {
                    self.state = ClientState::ErrorState(Error::IndexMismatch(res_index, *index));
                } else {
                    self.data_index = 0;
                    self.state = ClientState::UploadingMultiples(ToggleBit(false));
                }
            }
            

            // UploadingMultiples -> UploadMupltipleSegments
            (ClientState::UploadingMultiples(toggle), ServerResponse::UploadMultiples(res_toggle, end, len, data)) => {
                if res_toggle != *toggle {
                    self.state = ClientState::ErrorState(Error::ToggleMismatch);
                } else {
                    let idx = self.data_index;
                    let data_len = len as usize;
                    if idx + data_len > self.data.len() {
                        self.state = ClientState::ErrorState(Error::BufferOverflow);
                    } else {
                        self.data[idx..idx + data_len].copy_from_slice(&data[0..data_len]);
                        self.data_index = idx + data_len;
                        if end {
                            self.state = ClientState::MultiplesUploaded;
                        } else {
                            let new_toggle = !*toggle;
                            self.state = ClientState::UploadingMultiples(new_toggle);
                        }
                    }
                }
            }

            // ---- Download Handling ----
            // InitDownload -> DownloadInitAck (single segment)
            (ClientState::InitSingleDownload(index, _len), ServerResponse::DownloadInitAck(res_index)) => {
                if res_index != * index {
                    self.state = ClientState::ErrorState(Error::IndexMismatch(res_index, *index));
                } else {
                    self.state = ClientState::DownloadCompleted
                }
            }

            // InitDownload -> DownloadInitAck (multi-segment)
            (ClientState::InitMultipleDownload(index, len), ServerResponse::DownloadInitAck(res_index)) => {
                if res_index != * index {
                    self.state = ClientState::ErrorState(Error::IndexMismatch(res_index, *index));
                } else {
                    self.state = ClientState::DownloadingSegments(ToggleBit(false), *len);
                }
            }

            (ClientState::DownloadingSegments(toggle, n), ServerResponse::DownloadSegmentAck(res_toggle)) => {
                if res_toggle != *toggle {
                    self.state = ClientState::ErrorState(Error::ToggleMismatch);
                } else {
                    if self.data_index + 7 < *n {
                        self.state = ClientState::DownloadingSegments(!*toggle, *n);
                        self.data_index = self.data_index + 7; 
                    } else {
                        self.state = ClientState::DownloadCompleted;
                    }
                }
            }

            // Default: Unexpected response
            (_state, _response) => {
                self.state = ClientState::ErrorState(Error::StateResponseMismatch);
            }
        };
    }

    fn observe(&self) -> Self::Observation {
        match &self.state {
            ClientState::Idle => {
                None
            }

            ClientState::InitUpload(ix) => {
                Some(ClientOutput::Output(ClientRequest::InitUpload(*ix)))
            }
            
            ClientState::SingleSegmentUploaded => {
                Some(ClientOutput::Done(ClientResult::UploadCompleted(
                    self.data.clone(), self.data_index
                )))
            }

            ClientState::UploadingMultiples(toggle) => {
                Some(ClientOutput::Output(ClientRequest::UploadSegment(*toggle)))
            }

            ClientState::MultiplesUploaded => {
                Some(ClientOutput::Done(ClientResult::UploadCompleted(
                    self.data.clone(), self.data_index
                )))
            }
            
            ClientState::InitSingleDownload(ix, len) => {
                let mut data = [0; 4];
                data.copy_from_slice(&self.data[0 .. *len]);
                
                Some(ClientOutput::Output(ClientRequest::InitSingleSegmentDownload(*ix, *len as u8, data)))
            }

            ClientState::InitMultipleDownload(ix, len) => {
                Some(ClientOutput::Output(ClientRequest::InitMultipleDownload(* ix, * len as u32)))          
            }

            ClientState::DownloadingSegments(toggle, n) => {
                // Prepare data segment to download
                let mut data = [0u8; 7];
                let ix0 = self.data_index;
                let ix1 = (ix0 + 7).min(*n);
                let end = self.data_index + 7 >= *n;
                
                data.copy_from_slice(&self.data[ix0..ix1]);
                Some(ClientOutput::Output(ClientRequest::DownloadSegment(*toggle, end, 7, data)))
            }

            ClientState::DownloadCompleted => {
                Some(ClientOutput::Done(ClientResult::DownloadCompleted))
            }
            
            
            ClientState::ErrorState(err) => {
                Some(ClientOutput::Error(err.clone()))
            }
        }
    }
}

            
/*            
            
            ClientState::InitMultiples(ix, len) => {
                Some(ClientOutput::Output(ClientRequest::InitMultipleDownload(*ix, *len)))
            }
            
            
            ClientState::InitDownload(ix, len_opt) => {
                if let Some(len) = len_opt {
                    Some(ClientOutput::Output(ClientRequest::InitMultipleDownload(*ix, *len)))
                } else {
                    
                }
            }


            ClientState::MultipleDownloadCompleted => {
                Some(ClientOutput::Done(ClientResult::DownloadCompleted))
            }


            ClientState::AbortInProgress => {
                Some(ClientOutput::Output(ClientRequest::AbortTransfer(AbortCode::Generic)))
            }*/
