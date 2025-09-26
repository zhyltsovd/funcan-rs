use core::marker::PhantomData;

use crate::cobid::*;
use crate::dictionary::*;
use crate::interfaces::*;
use crate::machine::*;
use crate::raw::*;
use crate::sdo::*;
use crate::sdo::machines::*;

pub struct SDOClient<R, W, D> {
    pub node: u8,
    pub sdo: ClientMachine<R, W>,
    _phantom: PhantomData<D>,
}

pub enum SDOConfig<D: Dictionary, R, W> {
    Read(u8, D::Index, R),
    Write(u8, D::Index, D::Object, W),
}

impl<R, W, D: Dictionary> SDOClient<R, W, D>
where
    D::Index: TryFrom<Index>,
    D::Object: for<'a> TryFrom<(D::Index, &'a [u8])>,
    R: Responder<<D as Dictionary>::Object>,
    W: Responder<()>
{
    
    #[inline]
    fn handle_sdo_result(self: &mut Self, r: ClientResult<R, W>)
    {
        match r {
            ClientResult::UploadCompleted(ix, data, len, maybe_r) => {
                if let Ok(index) = <D as Dictionary>::Index::try_from(ix) {
                    if let Ok(x) = <D as Dictionary>::Object::try_from((index, &data[0..len])) {
                        if let Some(r) = maybe_r {
                            let _ = r.respond(x);
                        }
                    }
                }
            }
            ClientResult::DownloadCompleted(maybe_r) => {
                if let Some(r) = maybe_r {
                    let _ = r.respond(());
                }
            }
            ClientResult::TransferAborted(_) => {}
        }
    }
}

impl<R, W, D> MachineTrans<CANFrame> for SDOClient<R, W, D>
where
    D: Dictionary,
    D::Index: TryFrom<Index>,
    D::Object: for<'a> TryFrom<(D::Index, &'a [u8])>,
    R: Responder<<D as Dictionary>::Object>,
    W: Responder<()>
{
    type Observation = Option<CANFrame>;

    fn initial(self: &mut Self) {
        self.sdo.initial();
    }
    
    fn transit(self: &mut Self, frame: CANFrame) {
        if let Ok(response) = ServerResponse::try_from(frame.can_data) {
            self.sdo.transit(response);
        }
    }

    fn observe(self: &mut Self) -> Self::Observation {

        let r = self.sdo.observe()?;
            
        match r {
            ClientOutput::Output(out) => {
                let data_out: [u8; 8] = out.into();
                let fun_code = FunCode::Node(NodeCmd::SdoReq, self.node);
                let frame_out = CANFrame {
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
  
