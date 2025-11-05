use crate::sdo::machines::*;
use crate::sdo::*;
use crate::raw::*;


pub struct SdoServer<const N: usize, D> {
    pub sdo: ServerMachine<N>,
    dictionary: D,
}

impl<const N: usize, D: Dictionary> SdoServer<N, D>
where
    D: Default
{
    pub fn new() -> Self { 
        Self {
            sdo: ServerMachine::default(),
            dictionary: D::default()
        }
    }

    pub fn handle_frame(self: &mut Self, frame: CanFrame) -> ServerOutput<N> {
        use crate::sdo::machines::ServerOutput::*;

        match ClientRequest::try_from(frame.data) {
            Ok(req) => {
                todo!()
            }

            Err(err) => {
                Error(SdoError::DecodingFailure(err))
            }
        }   
    }
}



