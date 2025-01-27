use futures::future::BoxFuture;

use crate::cobid::*;
use crate::dictionary::*;
use crate::heartbeat::*;
use crate::machine::*;
use crate::raw::*;
use crate::sdo::machines::*;
use crate::sdo::Error as SdoError;
use crate::sdo::*;

pub enum ClientCmd<Ix> {
    Read(u8, Ix),
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

pub struct ClientConfig<C, D> {
    pub dictionary: D,
    pub physical: C,
}

struct ClientInterface<D> {
    heartbeat: HeartbeatMachine,
    sdo: ClientMachine,
    dictionary: D,
}

pub struct ClientCtx<C, D> {
    interface: ClientInterface<D>,
    physical: C,
}

impl<D: Dictionary, C: CANInterface<ClientCmd<D::Index>>> ClientCtx<C, D>
where
    D::Index: TryFrom<Index> + Into<Index>,
    D::Object: for<'a> TryFrom<(Index, &'a [u8])>,
{
    pub fn new(config: ClientConfig<C, D>) -> Self {
        let interface = ClientInterface {
            heartbeat: HeartbeatMachine::default(),
            sdo: ClientMachine::default(),
            dictionary: config.dictionary,
        };

        let ctx = Self {
            interface: interface,
            physical: config.physical,
        };

        ctx
    }

    #[inline]
    async fn handle_cmd<E>(self: &mut Self, cmd: ClientCmd<D::Index>) -> Result<(), E>
    where
        E: From<<C as CANInterface<ClientCmd<D::Index>>>::Error>
    {
        match cmd {
            ClientCmd::Read(node, ix) => {
                if let Some(st) = self.interface.sdo.observe() {
                    if st.is_ready() {
                        self.interface.sdo.read(ix.into());
                        if let Some(ClientOutput::Output(out)) = self.interface.sdo.observe() {
                            self.handle_sdo_request::<E>(node, out).await?;
                        }
                    }
                }
            }
        };

        Ok(())
    }

    #[inline]
    fn handle_broadcast<E>(self: &mut Self, cmd: BroadcastCmd) -> Result<(), E>
    where
        E: From<<C as CANInterface<ClientCmd<D::Index>>>::Error>,
    {
        // todo
        Ok(())
    }

    // <D::Object as TryFrom<(Index, &'a [u8])>>

    #[inline]
    fn handle_sdo_result<E>(self: &mut Self, r: ClientResult) -> Result<(), E>
    where
        E: for<'a> From<<D::Object as TryFrom<(Index, &'a [u8])>>::Error>,
    {
        match r {
            ClientResult::UploadCompleted(ix, data, len) => {
                let x = D::Object::try_from((ix, &data[0..len]))?;
                self.interface.dictionary.set(x);
            }
            ClientResult::DownloadCompleted => {}
            ClientResult::TransferAborted(_) => {}
        };

        Ok(())
    }

    async fn handle_sdo_request<E>(self: &mut Self, node: u8, out: ClientRequest) -> Result<(), E>
    where
        E: From<<C as CANInterface<ClientCmd<D::Index>>>::Error> 
    {
        let data_out: [u8; 8] = out.into();
        let fun_code = FunCode::Node(NodeCmd::SdoReq, node);
        let frame_out = CANFrame {
            can_cobid: fun_code.into(),
            can_len: 8,
            can_data: data_out,
        };
        self.physical.send_frame(frame_out).await?;
        Ok(())
    }
    
    async fn handle_sdo_rx<E>(self: &mut Self, node: u8, data: [u8; 8]) -> Result<(), E>
    where
        E: From<<C as CANInterface<ClientCmd<D::Index>>>::Error>
            + From<SdoError>
            + for<'a> From<<D::Object as TryFrom<(Index, &'a [u8])>>::Error>,
    {
        let response = ServerResponse::try_from(data)?;

        self.interface.sdo.transit(response);
        match self.interface.sdo.observe() {
            None => {}
            Some(r) => {
                match r {
                    ClientOutput::Output(out) => {
                        self.handle_sdo_request::<E>(node, out).await?;
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
    async fn handle_node_cmd<E>(
        self: &mut Self,
        cmd: NodeCmd,
        node: u8,
        data: [u8; 8],
    ) -> Result<(), E>
    where
        E: From<<C as CANInterface<ClientCmd<D::Index>>>::Error>
            + From<SdoError>
            + for<'a> From<<D::Object as TryFrom<(Index, &'a [u8])>>::Error>,
    {
        match cmd {
            NodeCmd::Emergency => {}
            NodeCmd::Time => {}
            NodeCmd::Pdo1Tx => {}
            NodeCmd::Pdo1Rx => {}
            NodeCmd::Pdo2Tx => {}
            NodeCmd::Pdo2Rx => {}
            NodeCmd::Pdo3Tx => {}
            NodeCmd::Pdo3Rx => {}
            NodeCmd::Pdo4Tx => {}
            NodeCmd::Pdo4Rx => {}
            NodeCmd::SdoResp => self.handle_sdo_rx::<E>(node, data).await?,
            NodeCmd::SdoReq => {}
            NodeCmd::Heartbeat => self.interface.heartbeat.transit(data),
            NodeCmd::Unused => {}
        }

        Ok(())
    }

    #[inline]
    async fn handle_rx<E>(self: &mut Self, frame: CANFrame) -> Result<(), E>
    where
        E: From<<C as CANInterface<ClientCmd<D::Index>>>::Error>
            + From<SdoError>
            + for<'a> From<<D::Object as TryFrom<(Index, &'a [u8])>>::Error>,
    {
        let fun_code: FunCode = FunCode::from(frame.can_cobid);

        match fun_code {
            FunCode::Broadcast(cmd) => self.handle_broadcast(cmd),
            FunCode::Node(cmd, node) => self.handle_node_cmd(cmd, node, frame.can_data).await,
        }
    }

    pub async fn run<E>(mut self: Self) -> Result<(), E>
    where
        E: From<<C as CANInterface<ClientCmd<D::Index>>>::Error>
            + From<SdoError>
            + for<'a> From<<D::Object as TryFrom<(Index, &'a [u8])>>::Error>,
    {
        loop {
            let event = self.physical.wait_can_event().await?;

            match event {
                CANEvent::Cmd(cmd) => {
                    self.handle_cmd::<E>(cmd).await?;
                }

                CANEvent::Rx(frame) => {
                    self.handle_rx::<E>(frame).await?;
                }
            }
        }
    }
}
