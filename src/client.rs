
use crate::heartbeat::*;
use crate::sdo::machines::*;

pub struct Client {
    heartbeat: HeartbeatMachine,
    sdo: ClientMachine,
}


