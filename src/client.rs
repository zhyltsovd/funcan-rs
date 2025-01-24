// use futures::future::BoxFuture;

use crate::heartbeat::*;
use crate::sdo::{Error as SdoError};
use crate::sdo::*;
use crate::sdo::machines::*;
use crate::raw::*;
use crate::cobid::*;
use crate::machine::*;

struct ClientInterface {
    heartbeat: HeartbeatMachine,
    sdo: ClientMachine,
}

pub struct ClientCtx<C> {
    interface: ClientInterface,
    physical: C
}

impl<C: CANInterface> ClientCtx<C>
{   
    #[inline]
    fn handle_broadcast<E>(self: &mut Self, cmd: BroadcastCmd) -> Result<(), E>
    where
        E: From<<C as CANInterface>::Error> 
    {
        // todo
        Ok(())
    }

    async fn handle_sdo_rx<E>(self: &mut Self, node: u8, data: [u8; 8]) -> Result<(), E>
    where
        E: From<<C as CANInterface>::Error> + From<SdoError>
    {
        let response = ServerResponse::try_from(data)?;

        self.interface.sdo.transit(response);
        match self.interface.sdo.observe() {
            None => {},
            Some(r) => {
                match r {
                    
                    ClientOutput::Output(out) => {
                        let data_out: [u8; 8] = out.into();
                        let fun_code = FunCode::Node(NodeCmd::SdoReq, node);
                        let frame_out = CANFrame {
                            can_cobid: fun_code.into(),
                            can_len: 8,
                            can_data: data_out
                        };
                        self.physical.send_frame(frame_out).await?;
                    }
                    
                    ClientOutput::Done(res) => {
                        // to something with result
                    }

                    ClientOutput::Error(err) => {
                        // handle error
                    }
                }
            }
        }

        Ok(())
    }   
    
    #[inline]
    async fn handle_node_cmd<E>(self: &mut Self, cmd: NodeCmd, node: u8, data: [u8; 8]) -> Result<(), E>
    where
        E: From<<C as CANInterface>::Error> + From<SdoError>
    {
        match cmd {
            NodeCmd::Emergency => {},
            NodeCmd::Time => {},
            NodeCmd::Pdo1Tx => {},
            NodeCmd::Pdo1Rx => {},
            NodeCmd::Pdo2Tx => {},
            NodeCmd::Pdo2Rx => {},
            NodeCmd::Pdo3Tx => {},
            NodeCmd::Pdo3Rx => {},
            NodeCmd::Pdo4Tx => {},
            NodeCmd::Pdo4Rx => {},
            NodeCmd::SdoResp => {},
            NodeCmd::SdoReq => {},
            NodeCmd::Heartbeat => { self.interface.heartbeat.transit(data) },
            NodeCmd::Unused => {},
        }
        
        Ok(())
    }
    
    #[inline]   
    async fn handle_rx<E>(self: &mut Self, frame: CANFrame) -> Result<(), E>
    where
        E: From<<C as CANInterface>::Error> + From<SdoError>
    {
        let fun_code: FunCode = FunCode::from(frame.can_cobid);

        match fun_code {
            FunCode::Broadcast(cmd) => self.handle_broadcast(cmd),
            FunCode::Node(cmd, node) => self.handle_node_cmd(cmd, node, frame.can_data).await,
        }
    }
    
    pub async fn run<E>(mut self: Self) -> Result<(), E>
    where
        E: From<<C as CANInterface>::Error> + From<SdoError>
    {

        loop {
            let event = self.physical.wait_can_event().await?;

            match event {
                CANEvent::Tx(frame) => {
                    self.physical.send_frame(frame).await?;
                }

                CANEvent::Rx(frame) => {
                    self.handle_rx::<E>(frame).await?;
                }
            }
        }
    }
}


