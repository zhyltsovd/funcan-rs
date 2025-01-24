// use futures::future::BoxFuture;

use crate::heartbeat::*;
use crate::sdo::machines::*;
use crate::raw::*;


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
    async fn run<E>(mut self: Self) -> Result<(), E>
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


