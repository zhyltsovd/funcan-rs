use crate::machine::*;

pub struct HeartbeatMachine {
//    last: Instant
}

impl Default for HeartbeatMachine {
    fn default() -> Self {
        Self {
//            last: Instant::now()
        }
    }
}

impl MachineTrans<[u8; 8]> for HeartbeatMachine {
    type Observation = ();

    fn transit(self: &mut Self, _x: [u8; 8]) {
        // do nothing
    }

    fn observe(self: &Self) -> Self::Observation {
        ()
    }

    fn initial(self: &mut Self) {
        * self = Default::default();
    }
}
