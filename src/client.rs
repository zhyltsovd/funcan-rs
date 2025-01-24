// use futures::future::BoxFuture;

use crate::heartbeat::*;
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

    fn handle_broadcast<E>(self: &mut Self, cmd: BroadcastCmd) -> Result<(), E> {
        // todo
        Ok(())
    }

    fn handle_node_cmd<E>(self: &mut Self, cmd: NodeCmd, node: u8, data: [u8; 8]) -> Result<(), E> {
        
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
    
    fn handle_rx<E>(self: &mut Self, frame: CANFrame) -> Result<(), E> {
        let fun_code: FunCode = FunCode::from(frame.can_cobid);

        match fun_code {
            FunCode::Broadcast(cmd) => self.handle_broadcast(cmd),
            FunCode::Node(cmd, node) => self.handle_node_cmd(cmd, node, frame.can_data),
        }
    }
    
    pub async fn run<E>(mut self: Self) -> Result<(), E>
    where
        E: From<<C as CANInterface>::Error>
    {

        loop {
            let event = self.physical.wait_can_event().await?;

            match event {
                CANEvent::Tx(frame) => {
                    self.physical.send_frame(frame).await?;
                }

                CANEvent::Rx(frame) => {
                    todo!()
                }
            }
        }
    }
}


