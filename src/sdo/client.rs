
use crate::cobid::*;
use crate::dictionary::*;
use crate::machine::*;
use crate::raw::*;
use crate::sdo::*;
use crate::sdo::machines::*;

pub struct SDOClient<R, W> {
    pub sdo: ClientMachine<R, W>,
}

pub enum SDOConfig<D: Dictionary, R, W> {
    Read(u8, D::Index, R),
    Write(u8, D::Index, D::Object, W),
}

impl<R, W> MachineTrans<CANFrame> for SDOClient<R, W> {
    type Observation = Option<CANFrame>;

    fn initial(self: &mut Self) {
        self.sdo.initial();
    }
    
    fn transit(self: &mut Self, frame: CANFrame) {
        let response = ServerResponse::try_from(frame.can_data)?;
        self.sdo.transit(response);
    }

    fn observe(self: &mut Self) -> Self::Observation {

        match self.sdo.observe() {
            None => {}
            Some(r) => {
                match r {
                    ClientOutput::Output(out) => {
                        let data_out: [u8; 8] = out.into();
                        let fun_code = FunCode::Node(NodeCmd::SdoReq, node);
                        let frame_out = CANFrame {
                            can_cobid: fun_code.into(),
                            can_len: 8,
                            can_data: data_out,
                        };

                        Some(frame_out)
                    }
                    
                    ClientOutput::Done(res) => {
                        None // self.handle_sdo_result::<E>(res)?;
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
    }
}
