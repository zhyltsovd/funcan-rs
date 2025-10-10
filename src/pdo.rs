use heapless::vec::*;

use crate::raw::*;
use crate::dictionary::*;
use crate::interfaces::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdoId {
    Tx(u8),
    Rx(u8)
}

impl Into<CobId> for (PdoId, u8) {
    fn into(self: Self) -> CobId {
        let (pdo, node_id) = self;
        match pdo {
            PdoId::Tx(pdo_id) => CobId::PdoTx { pdo_id, node_id},
            PdoId::Rx(pdo_id) => CobId::PdoRx { pdo_id, node_id},
        }
    }
}

pub struct Producer<D: Dictionary> {
    id: PdoId,
    objs: Vec<(D::Object, usize), 12>,
}

impl<D: Dictionary> Producer<D>
where
    D::Object: IntoBuf
{
    pub fn new(id: PdoId, objs: Vec<(D::Object, usize), 12>) -> Self {
        Self { id, objs }
    }

    pub fn serialize(self: &Self, node_id: u8) -> CanFrame {
        let mut data_out = [0; 8];
        let mut ix = 0;
        for (o, len) in self.objs.iter() {
            o.into_buf(&mut data_out[ix .. ix + len]);
            ix += len;
        }

        let cobid: CobId = (self.id, node_id).into();
        CanFrame { cobid: cobid, len: ix, data: data_out} 
    }
    
}
