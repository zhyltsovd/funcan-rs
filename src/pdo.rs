use heapless::vec::*;
use heapless::index_map::*;

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
    map: FnvIndexMap<D::Index, usize, 8>,
    objs: Vec<(D::Object, usize), 8>,
}

impl<D: Dictionary> Producer<D>
where
    D::Index: core::hash::Hash + Eq + Copy + Into<CanIndices>,
    D::Object: IntoBuf
{
    pub fn new(id: PdoId) -> Self {
        let objs = Vec::new();
        let map = FnvIndexMap::new();
        Self { id, objs, map }
    }

    pub fn push<T>(self: &mut Self, t: T)
    where
        T: DictionaryValue<D> + Into<D::Object> {
        let index: D::Index = T::index();
        let ixs: CanIndices = index.into(); 
        let size = ixs.can_type.size();
        let obj: D::Object = t.into();
        let ix = self.objs.len();
        
        self.map.insert(index, ix);
        self.objs.push((obj, size));
    }

    pub fn update<T>(self: &mut Self, t: T)
    where
        T: DictionaryValue<D> + Into<D::Object> {
        let index: D::Index = T::index();
        let obj: D::Object = t.into();

        match self.map.get(&index) {
            None => { panic!("PDO Producer update: Handle miss case!") }
            Some(p) => {
                self.objs[*p].0 = obj;
            }
        }
            
        //self.map.insert(index, ix);
        //self.objs.push((obj, size));
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

pub enum PdoMap<'a, D: Dictionary> {
    None,
    TxMapped(&'a Producer<D>)
}
