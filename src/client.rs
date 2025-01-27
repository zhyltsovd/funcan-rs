use futures::future::BoxFuture;

use crate::heartbeat::*;
use crate::sdo::{Error as SdoError};
use crate::sdo::*;
use crate::sdo::machines::*;
use crate::raw::*;
use crate::cobid::*;
use crate::machine::*;
use crate::dictionary::*;


pub enum ClientCmd {
    Read(Index)
}

/// Represents an event in the raw CAN interface.
pub enum CANEvent<Cmd> {
    Cmd(Cmd),
    Rx(CANFrame),
}

/// Abstract interface for Controller Area Network (CAN) communication.
///
/// This trait provides an async-capable abstraction layer for CAN bus operations,
/// suitable for both standard and embedded (no_std) environments. Implementations
/// should handle physical layer details while exposing a hardware-agnostic API.
///
/// # Type Parameters
/// - `Error`: Associated error type for implementation-specific error handling
///
pub trait CANInterface<Cmd> {
    /// Error type returned by CAN interface operations.
    ///
    /// Represents hardware-specific or protocol errors that can occur during
    /// frame transmission/reception. Common errors include:
    /// - Bus-off state
    /// - Arbitration lost
    /// - Form/CRC errors
    /// - TX buffer overflow
    type Error;

    /// Asynchronously wait for the next CAN bus event.
    fn wait_can_event<'a>(self: &'a mut Self) -> BoxFuture<'a, Result<CANEvent<Cmd>, Self::Error>>;

    /// Asynchronously send a raw CAN frame through the physical layer.
    fn send_frame<'a>(
        self: &'a mut Self,
        frame: CANFrame,
    ) -> BoxFuture<'a, Result<(), Self::Error>>;
}

struct ClientInterface<D, Factory> {
    heartbeat: HeartbeatMachine,
    sdo: ClientMachine,
    dictionary: D,
    factory: Factory
}

pub struct ClientCtx<C, D, Factory> {
    interface: ClientInterface<D, Factory>,
    physical: C
}

impl<C: CANInterface<ClientCmd>, D: CANDictionary, Factory: CANFactory> ClientCtx<C, D, Factory>
{
    #[inline]
    fn handle_cmd<E>(self: &mut Self, cmd: ClientCmd) -> Result<(), E> {
        match cmd {
            ClientCmd::Read(ix) => {
                if let Some(st) = self.interface.sdo.observe() { 
                    if st.is_ready() {
                        self.interface.sdo.read(ix);
                    }
                }
            }
        };

        Ok(())
    }
    
    #[inline]
    fn handle_broadcast<E>(self: &mut Self, cmd: BroadcastCmd) -> Result<(), E>
    where
        E: From<<C as CANInterface<ClientCmd>>::Error> 
    {
        // todo
        Ok(())
    }

    #[inline]
    fn handle_sdo_result<E>(self: &mut Self, r: ClientResult) -> Result<(), E> {
        match r {
            ClientResult::UploadCompleted(data, len) => {
                let ix = Index::new(0, 0);
                let x = self.interface.factory.mk_obj(ix, &data[0 .. len]);
                self.interface.dictionary.set(x);
                
            }
            ClientResult::DownloadCompleted => {},
            ClientResult::TransferAborted(_) => {},
        };

        Ok(())
    }
    
    async fn handle_sdo_rx<E>(self: &mut Self, node: u8, data: [u8; 8]) -> Result<(), E>
    where
        E: From<<C as CANInterface<ClientCmd>>::Error> + From<SdoError>
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
                        self.handle_sdo_result::<E>(res)?;
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
        E: From<<C as CANInterface<ClientCmd>>::Error> + From<SdoError>
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
            NodeCmd::SdoResp => self.handle_sdo_rx::<E>(node, data).await?,
            NodeCmd::SdoReq => {},
            NodeCmd::Heartbeat => { self.interface.heartbeat.transit(data) },
            NodeCmd::Unused => {},
        }
        
        Ok(())
    }
    
    #[inline]   
    async fn handle_rx<E>(self: &mut Self, frame: CANFrame) -> Result<(), E>
    where
        E: From<<C as CANInterface<ClientCmd>>::Error> + From<SdoError>
    {
        let fun_code: FunCode = FunCode::from(frame.can_cobid);

        match fun_code {
            FunCode::Broadcast(cmd) => self.handle_broadcast(cmd),
            FunCode::Node(cmd, node) => self.handle_node_cmd(cmd, node, frame.can_data).await,
        }
    }
    
    pub async fn run<E>(mut self: Self) -> Result<(), E>
    where
        E: From<<C as CANInterface<ClientCmd>>::Error> + From<SdoError>
    {

        loop {
            let event = self.physical.wait_can_event().await?;

            match event {
                CANEvent::Cmd(cmd) => {
                    self.handle_cmd::<E>(cmd)?;
                }

                CANEvent::Rx(frame) => {
                    self.handle_rx::<E>(frame).await?;
                }
            }
        }
    }
}


