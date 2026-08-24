use core::marker::PhantomData;

use crate::dictionary::*;
use crate::interfaces::*;
use crate::raw::*;
use crate::sdo::machines::*;
use crate::sdo::*;

pub struct SdoClient<const N: usize, F: CanFrame, R, W, D> {
    pub node: u8,
    pub sdo: ClientMachine<N, R, W>,
    _phantom: PhantomData<(F, D)>,
}

pub enum SdoInput<F: CanFrame, R, W, D: Dictionary> {
    Read(D::Index, R),
    Write(D::Index, D::Object, W),
    /// Block upload (CiA 301 7.2.4.8): read the object using the block
    /// transfer protocol.
    BlockRead(D::Index, R),
    /// Block download (CiA 301 7.2.4.7): write the object using the block
    /// transfer protocol.
    BlockWrite(D::Index, D::Object, W),
    Frame(F)
}

impl<F: CanFrame, R, W, D: Dictionary> core::fmt::Debug for SdoInput<F, R, W, D>
where
    D::Index: core::fmt::Debug
{
    fn fmt(self: &Self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
//            SdoInput::Reset => write!(f, "Сброс"),
            SdoInput::Read(ix, _) => write!(f, "Чтение объекта {:?}", ix),
            SdoInput::Write(ix, _, _) => write!(f, "Запись объекта {:?}", ix),
            SdoInput::BlockRead(ix, _) => write!(f, "Блочное чтение объекта {:?}", ix),
            SdoInput::BlockWrite(ix, _, _) => write!(f, "Блочная запись объекта {:?}", ix),
            SdoInput::Frame(_) => write!(f, "Обработчка SDO фрейма"),
        } 
    }
}

impl<const N: usize, F: CanFrame, R, W, D: Dictionary> SdoClient<N, F, R, W, D>
where
    D::Index: TryFrom<CanIndex> + Into<CanIndex>,
    D::Object: for<'a> TryFrom<(D::Index, &'a [u8])> + IntoBuf,
    R: OneshotResponder<<D as Dictionary>::Object>,
    W: OneshotResponder<()>,
{
    pub fn new(node: u8) -> Self {
        let sdo = ClientMachine::default();
        SdoClient {
            node,
            sdo,
            _phantom: PhantomData,
        }
    }

    pub fn reset(self: &mut Self) {
        self.sdo.reset();
    }
    
    pub fn input(self: &mut Self, input: SdoInput<F, R, W, D>) -> ClientOutput<N, R, W> {
        use crate::sdo::machines::ClientOutput::*;
        use crate::sdo::client::SdoInput::*;
        
        match input {
            Read(ix, r) => {
                if self.sdo.is_ready() {
                    self.sdo.read(ix.into(), r)
                } else {
                    Error(SdoError::Busy)
                }
            }

            Write(ix, x, r) => {
                if self.sdo.is_ready() {
                    self.sdo.write(ix.into(), x, r)
                
                } else {
                    Error(SdoError::Busy)
                }
            }

            BlockRead(ix, r) => {
                if self.sdo.is_ready() {
                    self.sdo.read_block(ix.into(), r)
                } else {
                    Error(SdoError::Busy)
                }
            }

            BlockWrite(ix, x, r) => {
                if self.sdo.is_ready() {
                    self.sdo.write_block(ix.into(), x, r)
                } else {
                    Error(SdoError::Busy)
                }
            }

            Frame(frame) => {
                let result = self.sdo.transit_frame(frame.data());

                if let Done(r) = result {
                    self.handle_sdo_result(r)
                } else {
                    result
                }
            }
        }
    }

    /// Emits the next pending segment of the active block download sub-block,
    /// if any. Returns `None` when the sub-block has been fully transmitted
    /// (the machine is then waiting for the server's block response) or when
    /// no block download is in progress.
    pub fn pump(self: &mut Self) -> Option<ClientRequest> {
        self.sdo.pump_block()
    }

    
    #[inline]
    fn handle_sdo_result(self: &mut Self, r: ClientResult<N, R, W>) -> ClientOutput<N, R, W> {
        match r {
            ClientResult::UploadCompleted(ix, data, len, maybe_r) => {
                if let Ok(index) = <D as Dictionary>::Index::try_from(ix) {
                    if let Ok(x) = <D as Dictionary>::Object::try_from((index, &data[0..len])) {
                        if let Some(r) = maybe_r {
                            let _ = r.respond(x);
                            ClientOutput::TransferCompleted
                        } else {
                            ClientOutput::Error(SdoError::NoResponder)
                        }
                    } else {
                        ClientOutput::Error(SdoError::DictionaryDecodingFailure(ix))
                    }                
                } else {
                    ClientOutput::Error(SdoError::DictionaryUnsupportedIndex(ix))
                }
            }
            ClientResult::DownloadCompleted(maybe_r) => {
                if let Some(r) = maybe_r {
                    let _ = r.respond(());
                    ClientOutput::TransferCompleted
                } else {
                    ClientOutput::Error(SdoError::NoResponder)
                }
            }
        }
    }

}


/*

}

impl<const N: usize, R, W, D> MachineTrans<CanFrame> for SdoClient<N, R, W, D>
where
    D: Dictionary,
    D::Index: TryFrom<CanIndex> + Into<CanIndex>,
    D::Object: for<'a> TryFrom<(D::Index, &'a [u8])> + IntoBuf,
    R: Responder<<D as Dictionary>::Object>,
    W: Responder<()>,
{
    type Observation = Option<CanFrame>;

    fn initial(self: &mut Self) {
        self.sdo.initial();
    }

    fn transit(self: &mut Self, frame: CanFrame) {
        if let Ok(response) = ServerResponse::try_from(frame.can_data) {
            self.sdo.transit(response);
        }
    }

    fn observe(self: &mut Self) -> Self::Observation {
        let r = self.sdo.observe()?;

        match r {
            ClientOutput::Output(out) => {
                let data_out: [u8; 8] = out.into();
                let fun_code = CobId::SdoRequest(self.node);
                let frame_out = CanFrame {
                    can_cobid: fun_code.into(),
                    can_len: 8,
                    can_data: data_out,
                };

                Some(frame_out)
            }

            ClientOutput::Done(res) => {
                self.handle_sdo_result(res);
                None
            }

            ClientOutput::Error(err) => {
                None // handle error
            }

            ClientOutput::Ready => {
                None // should not happen
            }
        }
    }
}

*/
