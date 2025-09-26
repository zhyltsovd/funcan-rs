
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
