
use core::future::Future;
use core::pin::Pin;
use heapless::{LinearMap, Vec};
use futures::future::BoxFuture;

use crate::raw::*;
use crate::machine::*;

pub trait CANMachine: MachineTrans<CANFrame, Observation = CANFrame> {}

pub struct Dispatcher<'a>
{
    /// Map from COB-ID -> machine index
    table: LinearMap<u16, usize, 16>,
    /// Stored boxed handlers
    machines: Vec<&'a mut dyn CANMachine, 16>,
}
