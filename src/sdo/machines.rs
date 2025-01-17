
use crate::sdo::*;
use crate::machine::*;


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    StateResponseMismatch,
    IndexMismatch(Index, Index)
}
    
enum ClientState {
    Idle,
    InitiateUpload(Index),
    SingleSegmentUploaded(Index),
    UploadingMultipleSigments(ToggleBit),
    MultipleSigmentsUploaded,
    ErrorState(Error)
        
}

pub struct ClientMachine {
    state: ClientState,
    data_index: usize,
    data: [u8; 1024],
}

pub struct ClientResult;

pub enum ClientOutput {
    Output(ClientRequest),
    Done(ClientResult),
    Error(Error),
} 

impl MachineTrans<ServerResponse> for ClientMachine {

    type Observation = Option<ClientOutput>;
    
    fn initial(self: &mut Self) {
        self.state = ClientState::Idle;
    }
    
    fn transit(self: &mut Self, x: ServerResponse) {
        match (&self.state, x) {
            (ClientState::InitiateUpload(ix), ServerResponse::UploadSingleSegment(iy, data, len)) => {
                if * ix == iy {
                    todo!()
                } else {
                    self.state = ClientState::ErrorState(Error::IndexMismatch(* ix, iy));
                }
            }

            
            _ => {
                self.state = ClientState::ErrorState(Error::StateResponseMismatch);
            }
        }
    }

    fn observe(self: &Self) -> Self::Observation {
        match &self.state {
            ClientState::Idle => {
                todo!()
            }

            ClientState::InitiateUpload(_) => {
                todo!()
            }

            ClientState::SingleSegmentUploaded(_) => {
                todo!()
            }

            ClientState::UploadingMultipleSigments(_) => {
                todo!()
            }

            ClientState::MultipleSigmentsUploaded => {
                todo!()
            }

            ClientState::ErrorState(err) => {
                Some(ClientOutput::Error(err.clone()))
            }
        }
    }

    
}
