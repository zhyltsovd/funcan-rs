//use heapless::vec::*;
use heapless::*;
//use heapless::index_map::*;

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

pub struct Consumer<D: Dictionary>
{
    // id: PdoId,
    map: FnvIndexMap<D::Index, usize, 8>,
    /// One entry per mapped object: (byte offset within the PDO payload,
    /// size in bytes). Offsets accumulate across pushes.
    objs: Vec<(usize, usize), 8>,
}

impl<D: Dictionary> Consumer<D>
where
    D::Index: core::hash::Hash + Eq + Copy + CanSize,
//    D::Object: IntoBuf
{
    pub fn new() -> Self {
        let objs = Vec::new();
        let map = FnvIndexMap::new();
        Self { objs, map }
    }

    pub fn push(self: &mut Self, index: D::Index) {
        let size = index.can_size();
        let offset = self.objs.last().map(|(o, s)| o + s).unwrap_or(0);
        let ix = self.objs.len();
        
        self.map.insert(index, ix);
        self.objs.push((offset, size));
    }

    pub fn deserialize<T, E>(self: &Self, index: D::Index, data_in: [u8; 8]) -> Result<T, E>
    where
        D::Object: for<'a> TryFrom<(D::Index, &'a [u8])>,
        T: TryFrom<D::Object>,
        E: for<'a> From<<D::Object as TryFrom<(D::Index, &'a [u8])>>::Error> + From<<T as TryFrom<D::Object>>::Error>,
        
    {
        match self.map.get(&index) {
            None => { panic!("PDO Consumer deserialize: Handle miss case!") }
            Some(p) => {
                let (off, len) = self.objs[*p];
                let obj = D::Object::try_from((index, &data_in[off .. off + len]))?;
                let t = T::try_from(obj)?;
                Ok(t)
            }
        } 
    }    
}


pub struct Producer<D: Dictionary, F: CanFrame> {
    id: PdoId,
    map: FnvIndexMap<D::Index, usize, 8>,
    objs: Vec<(D::Object, usize), 8>,
    _frame: core::marker::PhantomData<F>,
}

impl<D: Dictionary, F: CanFrame> Producer<D, F>
where
    D::Index: core::hash::Hash + Eq + Copy + CanSize,
    D::Object: IntoBuf
{
    pub fn new(id: PdoId) -> Self {
        let objs = Vec::new();
        let map = FnvIndexMap::new();
        Self {
            id,
            objs,
            map,
            _frame: core::marker::PhantomData,
        }
    }

    pub fn push<T>(self: &mut Self, t: T)
    where
        T: DictionaryValue<D> + Into<D::Object> {
        let index: D::Index = T::index();
        let size = index.can_size();
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
    }
    
    pub fn serialize(self: &Self, node_id: u8) -> F {
        let mut data_out = [0; 8];
        let mut ix = 0;
        for (o, len) in self.objs.iter() {
            o.into_buf(&mut data_out[ix .. ix + len]);
            ix += len;
        }

        let cobid: CobId = (self.id, node_id).into();
        F::from_parts(cobid, ix, data_out)
    }
}

pub enum PdoMap<'a, D: Dictionary, F: CanFrame> {
    None,
    TxMapped(&'a Producer<D, F>)
}

pub fn pdo_map_param<D, T>() -> u32
where
    D: Dictionary,
    D::Index: Copy + CanSize + Into<CanIndex>,
    T: DictionaryValue<D>
{
    let index: D::Index = T::index();
    let can_index: CanIndex = index.into();
    let size = index.can_size() as u32;
    ((can_index.base as u32) << 16) | ((can_index.sub as u32) << 8) | (size * 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test object dictionary index; its CANopen size defines the PDO mapping.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct Idx(u16);

    impl CanSize for Idx {
        fn can_size(&self) -> usize {
            match self.0 {
                0x6041 => 2, // Statusword
                0x603F => 2, // Error code
                0x01 => 1,
                0x02 => 2,
                0x03 => 3,
                _ => 0,
            }
        }
    }

    /// Test dictionary object decoded from the mapped slice.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Obj(u32);

    impl TryFrom<(Idx, &[u8])> for Obj {
        type Error = ();

        fn try_from((_ix, data): (Idx, &[u8])) -> Result<Self, Self::Error> {
            let mut v = 0u32;
            for (i, &b) in data.iter().enumerate() {
                v |= (b as u32) << (8 * i);
            }
            Ok(Obj(v))
        }
    }

    struct Out(u32);

    impl TryFrom<Obj> for Out {
        type Error = ();

        fn try_from(o: Obj) -> Result<Self, Self::Error> {
            Ok(Out(o.0))
        }
    }

    struct TestDict;

    impl Dictionary for TestDict {
        type Index = Idx;
        type Object = Obj;

        fn set(&mut self, _x: Self::Object) {}
        fn get(&self, _ix: &Self::Index) -> Self::Object {
            Obj(0)
        }
    }

    #[test]
    fn consumer_deserialize_two_objects() {
        // TxPDO1 = [Statusword(2B), Errorcode(2B)] — the reported repro:
        // the second object must be read at offset 2, not ordinal 1.
        let mut consumer = Consumer::<TestDict>::new();
        consumer.push(Idx(0x6041)); // Statusword, offset 0
        consumer.push(Idx(0x603F)); // Error code, offset 2

        // payload: statusword = 0x1234, errorcode = 0x5678
        let payload: [u8; 8] = [0x34, 0x12, 0x78, 0x56, 0, 0, 0, 0];

        let status: Out = consumer.deserialize::<Out, ()>(Idx(0x6041), payload).unwrap();
        assert_eq!(status.0, 0x1234);

        let err: Out = consumer.deserialize::<Out, ()>(Idx(0x603F), payload).unwrap();
        assert_eq!(err.0, 0x5678);
    }

    #[test]
    fn consumer_deserialize_three_objects_mixed_sizes() {
        // mapping with sizes 1, 2, 3 -> offsets 0, 1, 3
        let mut consumer = Consumer::<TestDict>::new();
        consumer.push(Idx(0x01));
        consumer.push(Idx(0x02));
        consumer.push(Idx(0x03));

        let payload: [u8; 8] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0, 0];

        let a: Out = consumer.deserialize::<Out, ()>(Idx(0x01), payload).unwrap();
        assert_eq!(a.0, 0x11);

        let b: Out = consumer.deserialize::<Out, ()>(Idx(0x02), payload).unwrap();
        assert_eq!(b.0, 0x3322);

        let c: Out = consumer.deserialize::<Out, ()>(Idx(0x03), payload).unwrap();
        assert_eq!(c.0, 0x665544);
    }

    #[test]
    fn consumer_deserialize_single_object() {
        // single-object PDOs were unaffected, but keep them covered
        let mut consumer = Consumer::<TestDict>::new();
        consumer.push(Idx(0x6041));

        let payload: [u8; 8] = [0x34, 0x12, 0, 0, 0, 0, 0, 0];
        let v: Out = consumer.deserialize::<Out, ()>(Idx(0x6041), payload).unwrap();
        assert_eq!(v.0, 0x1234);
    }
}
