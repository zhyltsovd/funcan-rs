use crate::interfaces::*;
use crate::machine::*;
use crate::raw::*;
use crate::sdo::*;
use crate::sdo::machines::*;

pub struct SdoServer<const N: usize, D> {
    pub sdo: ServerMachine<N>,
    dictionary: D,
}

impl<const N: usize, D: Dictionary> SdoServer<N, D>
where
    D: Default,
    D::Index: From<CanIndex>,
    D::Object: IntoBuf
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
                let server_out = self.sdo.transit(req);
                match server_out {

                    ServerOutput::Data(sindex) => {
                        let index: D::Index = sindex.into();
                        let data = self.dictionary.get(&index);
                        self.sdo.upload_data(&data)
                    }

                    ServerOutput::FinalOutput(resp, result) => {
                        todo!()
                    }

                    out => out,
                }
            }

            Err(err) => {
                Error(SdoError::DecodingFailure(err))
            }
        }   
    }
}



