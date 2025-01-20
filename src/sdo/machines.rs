
use crate::sdo::*;
use crate::machine::*;


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    StateResponseMismatch,
    IndexMismatch(Index, Index),
    TransferAborted(AbortCode),
}

enum ClientState {
    Idle,
    InitiateUpload(Index),
    SingleSegmentUploaded(Index),
    InitMultipleSegments(Index, u32),
    UploadingMultipleSegments(ToggleBit, usize), // toggle bit, current index
    MultipleSegmentsUploaded,
    InitiateDownload(Index, Option<u32>), // index, length if known
    DownloadingSegments(ToggleBit, usize), // toggle bit, current index
    MultipleSegmentDownloadCompleted,
    ErrorState(Error),
    AbortInProgress,
}

pub struct ClientMachine {
    state: ClientState,
    data_index: usize,
    data: [u8; 1024],
}

pub enum ClientResult {
    UploadCompleted([u8; 1024]),
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
    
    fn initial(&mut self) {
        self.state = ClientState::Idle;
    }
    
    fn transit(&mut self, response: ServerResponse) {
        match (&self.state, response) {
            // Handle Initiate Upload responses
            (ClientState::InitiateUpload(ix), ServerResponse::UploadSingleSegment(iy, data, len)) => {
                if *ix == iy {
                    self.data[..4].copy_from_slice(&data);
                    self.data_index = 4;
                    self.state = ClientState::SingleSegmentUploaded(*ix);
                } else {
                    self.state = ClientState::ErrorState(Error::IndexMismatch(*ix, iy));
                }
            }

            (ClientState::InitiateUpload(ix), ServerResponse::InitMultipleSegments(iy, len)) => {
                if *ix == iy {
                    self.state = ClientState::InitMultipleSegments(*ix, len);
                } else {
                    self.state = ClientState::ErrorState(Error::IndexMismatch(*ix, iy));
                }
            }

            // Handle Single Segment Upload completion
            (ClientState::SingleSegmentUploaded(_ix), _) => {
                self.state = ClientState::Idle;
                // Assuming upload is done, emit Done
                // Data is already collected in self.data
                // You might want to trim the data based on self.data_index
            }

            // Handle Init Multiple Segments response
            (ClientState::InitMultipleSegments(ix, len), ServerResponse::UploadMultipleSegments(toggle, data, n, end)) => {
                if *ix == ix {
                    // Store received data
                    self.data[self.data_index..self.data_index + 7].copy_from_slice(data);
                    self.data_index += 7;
                    self.state = ClientState::UploadingMultipleSegments(*toggle, self.data_index);
                } else {
                    self.state = ClientState::ErrorState(Error::IndexMismatch(*ix, *ix));
                }
            }

            // Handle Uploading Multiple Segments
(ClientState::UploadingMultipleSegments(toggle, current_idx), ServerResponse::UploadMultipleSegments(new_toggle, data, n, end)) => {
                if *toggle == new_toggle {
                    // Toggle bit should alternate
                    self.state = ClientState::ErrorState(Error::StateResponseMismatch);
                } else {
                    self.data[self.data_index..self.data_index + 7].copy_from_slice(data);
                    self.data_index += 7;
                    if end {
                        self.state = ClientState::MultipleSegmentsUploaded;
                    } else {
                        self.state = ClientState::UploadingMultipleSegments(new_toggle, self.data_index);
                    }
                }
            }

            // Handle Initiate Download responses
            (ClientState::InitiateDownload(ix, Some(len)), ServerResponse::DownloadSingleSegmentAck(iy)) => {
                if *ix == iy {
                    self.state = ClientState::Idle;
                    // Emit Done
                } else {
                    self.state = ClientState::ErrorState(Error::IndexMismatch(*ix, iy));
                }
            }

            (ClientState::InitiateDownload(ix, Some(len)), ServerResponse::DownloadSegmentAck(toggle, n)) => {
                if *ix == ix {
                    self.state = ClientState::DownloadingSegments(*toggle, 0);
                } else {
                    self.state = ClientState::ErrorState(Error::IndexMismatch(*ix, ix));
                }
            }

            // Handle Downloading Segments acknowledgments
            (ClientState::DownloadingSegments(toggle, current_idx), ServerResponse::DownloadSegmentAck(new_toggle, n)) => {
                if *toggle == new_toggle {
                    self.state = ClientState::ErrorState(Error::StateResponseMismatch);
                } else {
                    self.data_index += n as usize;
                    // Check if more data to send
                    if self.data_index < self.data.len() {
                        self.state = ClientState::DownloadingSegments(new_toggle, self.data_index);
                    } else {
                        self.state = ClientState::MultipleSegmentDownloadCompleted;
                    }
                }
            }

            // Handle Abort Transfer
            (_, ServerResponse::AbortTransferResponse(code)) => {
                self.state = ClientState::ErrorState(Error::TransferAborted(code));
            }

            // Handle unexpected responses based on current state
            _ => {
                self.state = ClientState::ErrorState(Error::UnexpectedResponse);
            }
        }
    }

    fn observe(&self) -> Self::Observation {
        match &self.state {
            ClientState::Idle => {
                None
            }

            ClientState::InitiateUpload(ix) => {
                Some(ClientOutput::Output(ClientRequest::InitiateUpload(*ix)))
            }

            ClientState::SingleSegmentUploaded(ix) => {
                Some(ClientOutput::Done(ClientResult::UploadCompleted(
                    self.data[..self.data_index].clone()
                )))
            }

            ClientState::InitMultipleSegments(ix, len) => {
                Some(ClientOutput::Output(ClientRequest::InitiateMultipleSegmentDownload(*ix, *len)))
            }

            ClientState::UploadingMultipleSegments(toggle, _) => {
                Some(ClientOutput::Output(ClientRequest::UploadSegment))
            }

            ClientState::MultipleSegmentsUploaded => {
                Some(ClientOutput::Done(ClientResult::UploadCompleted(
                    self.data[..self.data_index].clone()
                )))
            }

            ClientState::InitiateDownload(ix, len_opt) => {
                if let Some(len) = len_opt {
                    Some(ClientOutput::Output(ClientRequest::InitiateMultipleSegmentDownload(*ix, *len)))
                } else {
Some(ClientOutput::Output(ClientRequest::InitiateSingleSegmentDownload(*ix, [0;4], 0)))
                }
            }

            ClientState::DownloadingSegments(toggle, current_idx) => {
                // Prepare data segment to download
                let mut data = [0u8; 7];
                data.copy_from_slice(&self.data[*current_idx..*current_idx + 7]);
                Some(ClientOutput::Output(ClientRequest::DownloadSegment(*toggle, false, 7, data)))
            }

            ClientState::MultipleSegmentDownloadCompleted => {
                Some(ClientOutput::Done(ClientResult::DownloadCompleted))
            }

            ClientState::ErrorState(err) => {
                Some(ClientOutput::Error(*err))
            }

            ClientState::AbortInProgress => {
                Some(ClientOutput::Output(ClientRequest::AbortTransfer(AbortCode::Generic)))
            }
        }
    }
}
