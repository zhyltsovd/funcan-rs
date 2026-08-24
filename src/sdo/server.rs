use core::marker::PhantomData;

use crate::interfaces::*;
use crate::raw::*;
use crate::sdo::*;
use crate::sdo::machines::*;

pub struct SdoServer<const N: usize, F: CanFrame, D> {
    pub sdo: ServerMachine<N>,
    pub dictionary: D,
    _frame: PhantomData<F>,
}

impl<const N: usize, F: CanFrame, D: Dictionary> SdoServer<N, F, D>
where
    D: Default,
    D::Index: TryFrom<CanIndex>,
    D::Object: for<'a> TryFrom<(D::Index, &'a [u8])> + IntoBuf
{
    pub fn new() -> Self { 
        Self {
            sdo: ServerMachine::default(),
            dictionary: D::default(),
            _frame: PhantomData,
        }
    }

    pub fn handle_frame(self: &mut Self, frame: F) -> ServerOutput<N> {
        use crate::sdo::machines::ServerOutput::*;

        let server_out = self.sdo.transit_frame(frame.data());
        match server_out {

            ServerOutput::Data(sindex) => {
                if let Ok(index) = <D as Dictionary>::Index::try_from(sindex) {
                    let data = self.dictionary.get(&index);
                    self.sdo.upload_data(&data)
                } else {
                    // send error
                    todo!()
                }
            }

            ServerOutput::FinalOutput(resp, result) => {
                if let ServerResult::DownloadCompleted(dindex, data, n) = result {
                    if let Ok(index) = <D as Dictionary>::Index::try_from(dindex) {
                        if let Ok(downloaded_value) = <D as Dictionary>::Object::try_from((index, &data[0..n])) {
                            self.dictionary.set(downloaded_value);
                            ServerOutput::FinalOutput(resp, result)
                        } else {
                            Error(SdoError::DictionaryDecodingFailure(dindex))
                        }                
                    } else {
                        Error(SdoError::DictionaryUnsupportedIndex(dindex))
                    }
                    
                } else {
                    ServerOutput::FinalOutput(resp, result)
                }
            }

            out => out,
        }   
    }

    /// Emits the next pending segment of the active block upload sub-block,
    /// if any. Returns `None` when the sub-block has been fully transmitted
    /// (the machine is then waiting for the client's block response) or when
    /// no block upload is in progress.
    pub fn pump(self: &mut Self) -> Option<ServerResponse> {
        self.sdo.pump_block()
    }
}



