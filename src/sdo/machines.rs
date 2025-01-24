
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
    SingleSegmentUploaded(Index),
    UploadingMultiples(ToggleBit), 
    MultiplesUploaded,

    InitSingleDownload(Index, usize),          
    InitMultipleDownload(Index, usize),          
    DownloadingSegments(ToggleBit),      
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

impl ClientMachine {
    pub fn new() -> Self {
        ClientMachine {
            state: ClientState::Idle,
            data_index: 0,
            data: [0; 1024],
        }
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
                    self.state = ClientState::SingleSegmentUploaded(*index);
                }
            }

            // InitUpload -> InitMupltipleSegments
            (ClientState::InitUpload(index), ServerResponse::UploadInitMultiples(res_index, size)) => {
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
            (ClientState::InitMultipleDownload(index, _len), ServerResponse::DownloadInitAck(res_index)) => {
                if res_index != * index {
                    self.state = ClientState::ErrorState(Error::IndexMismatch(res_index, *index));
                } else {
                    self.state = ClientState::DownloadingSegments(ToggleBit(false))
                }
            }

            
            
            /*
            

                    let end = len <= 7;
                    let seg_len = len;
                    let mut data = [0; 7];
                    data[0..seg_len as usize].copy_from_slice(&self.data[0..seg_len as usize]);
                    self.data_index = seg_len as usize;
                    let req = ClientRequest::DownloadSegment(ToggleBit::First, end, seg_len, data);
                    let next_state = ClientState::DownloadingSegments(ToggleBit::First, self.data_index);
                    (next_state, ClientOutput::Output(req))

            // DownloadingSegments -> DownloadSegmentAck
            (ClientState::DownloadingSegments(toggle, idx), ServerResponse::DownloadSegmentAck(res_toggle)) => {
                if res_toggle != toggle {
                    (ClientState::ErrorState(Error::ToggleMismatch), ClientOutput::Error(Error::ToggleMismatch))
                } else {
                    let remaining = self.data.len() - idx;
                    if remaining == 0 {
                        (ClientState::MultipleDownloadCompleted, ClientOutput::Done(ClientResult::DownloadCompleted))
                    } else {
                        let new_toggle = toggle.toggle();
                        let end = remaining <= 7;
                        let seg_len = remaining.min(7) as u8;
                        let mut data = [0; 7];
                        data[0..seg_len as usize].copy_from_slice(&self.data[idx..idx + seg_len as usize]);
                        let req = ClientRequest::DownloadSegment(new_toggle, end, seg_len, data);
                        let next_state = ClientState::DownloadingSegments(new_toggle, idx + seg_len as usize);
                        (next_state, ClientOutput::Output(req))
                    }
                }
            }

            // Default: Unexpected response
            (state, response) => {
                self.state = ClientState::ErrorState(Error::StateResponseMismatch);
            }

            */
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
            
            ClientState::SingleSegmentUploaded(ix) => {
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
                let ix1 = n.min(ix0 + 7);
                
                data.copy_from_slice(&self.data[ix0..ix1]);
                Some(ClientOutput::Output(ClientRequest::DownloadSegment(*toggle, false, 7, data)))
            }

            
            ClientState::ErrorState(err) => {
                Some(ClientOutput::Error(*err))
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
