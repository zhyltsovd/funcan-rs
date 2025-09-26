use paste::paste;

use crate::sdo::client::*;

#[macro_export]
macro_rules! build_sdo_dispatcher {
    // accept a comma‐separated list of (node_id, DictType) pairs
    (
        $( ($id:expr, $dict:ident) ),* $(,)?
    ) => {
        paste! {
            /// The generated dispatcher struct
            pub struct SDODispatcher<R, W> {
                $(
                    pub [<node_ $id>]: SDOClient<R, W, $dict>,
                )*
            }

            impl<R, W> SDODispatcher<R, W> {
                /// constructor: builds one client per node
                pub fn new() -> Self {
                    SDODispatcher {
                        $(
                            [<node_ $id>]: $SDOClient::new(),
                        )*
                    }
                }

                /// dispatch incoming 8‐byte data to the correct client
                pub fn dispatch(&mut self, node: u8, data: [u8; 8]) {
                    match node {
                        $(
                            $id => self.[<node_ $id>].handle(data),
                        )*
                        other => panic!("unknown node id {}", other),
                    }
                }
            }
        }
    };
}

#[derive(Debug)]
pub struct D0;
#[derive(Debug)]
pub struct D1;
#[derive(Debug)]
pub struct D2;

build_sdo_dispatcher!((0, D0), (1, D1), (2, D2),);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_forwards_to_each_node() {
        let mut disp: SDODispatcher<(), ()> = SDODispatcher::new();

        // these will print to stdout when running `cargo test -- --nocapture`
        disp.dispatch(0, [0; 8]);
        disp.dispatch(1, [1; 8]);
        disp.dispatch(2, [2; 8]);
    }
}

/*

// use core::future::Future;
use core::pin::Pin;
use heapless::{LinearMap, Vec};
use futures::future::BoxFuture;

use crate::raw::*;
use crate::machine::*;

pub trait Configurable {
    type Config;
    fn configure(&mut self, config: Self::Config);
}

pub trait CANMachine: MachineTrans<CANFrame, Observation = Option<CANFrame>> {}

pub struct Dispatcher<'a>
{
    /// Map from COB-ID -> machine index
    table: LinearMap<u32, usize, 16>,
    /// Stored boxed handlers
    machines: Vec<&'a mut dyn CANMachine, 16>,
}

impl<'a> Dispatcher<'a> {
    pub const fn new() -> Self {
        Dispatcher {
            table: LinearMap::new(),
            machines: Vec::new(),
        }
    }

    pub fn register<E>(
        &mut self,
        cobid: u32,
        m: &'a mut dyn CANMachine,
    ) -> Result<(), E> {
        let idx = self.machines.len();
        self.table.insert(cobid, idx);
        self.machines.push(m);
        Ok(())
    }

    pub fn dispatch(&mut self, frame: CANFrame) {
        if let Some(&idx) = self.table.get(&frame.can_cobid) {
            let m = &mut self.machines[idx];
            m.transit(frame);
            let obs: CANFrame = m.observe();

            todo!(); // write if not None
        }
    }
}

*/
