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
    D::Object: for<'a> TryFrom<(D::Index, &'a [u8])> + IntoBuf
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
                        if let ServerResult::DownloadCompleted(dindex, data, n) = result {
                            if let Ok(index) = <D as Dictionary>::Index::try_from(dindex) {
                                if let Ok(downloaded_value) = <D as Dictionary>::Object::try_from((index, &data[0..n])) {
                                    self.dictionary.set(downloaded_value);
                                    Output(resp)
                                } else {
                                    Error(SdoError::DictionaryDecodingFailure(dindex))
                                }                
                            } else {
                                Error(SdoError::DictionaryUnsupportedIndex(dindex))
                            }
                            
                        } else {
                            todo!()
                        }
                            //assert_eq!(index, dindex);
                            //assert_eq!(n, 2);
                            //let  =
                            //    u16::from_le_bytes([data[0], data[1]]);
                            //assert_eq!(downloaded_value, value);
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



