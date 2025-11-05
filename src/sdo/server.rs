use crate::sdo::machines::*;
use crate::sdo::*;


pub struct SdoServer<const N: usize, D> {
    pub sdo: ServerMachine<N>,
    dictionary: D,
}

impl<const N: usize, D: Dictionary> SdoServer<N, D>
where
    D: Default
{
    pub fn new() -> Self { 
        Self {
            sdo: ServerMachine::default(),
            dictionary: D::default()
        }
    }
}



