
use crate::sdo::*;
use crate::machine::*;

enum ClientState {
    Idle,
    InitiateUpload(Index),
    SingleSegmentUploaded(Index),
    UploadingMultipleSigments(ToggleBit),
    MultipleSigmentsUploaded
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
    
} 

impl MachineTrans<ServerResponse> for ClientMachine {

    type Observation = ClientOutput;
    
    fn initial(self: &mut Self) {
        self.state = ClientState::Idle;
    }
    
    fn transit(self: &mut Self, x: ServerResponse) {
        todo!()
    }

    fn observe(self: &Self) -> Self::Observation {
        todo!()
    }

    
}
