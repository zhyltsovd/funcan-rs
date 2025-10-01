/// 7-bit CANopen Node ID (1..127). 0 is valid for "all nodes" in NMT,
/// but rarely used elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeId(pub u8);

impl NodeId {
    pub fn new(id: u8) -> Option<Self> {
        if id <= 0x7F {
            Some(NodeId(id))
        } else {
            None
        }
    }
    pub fn get(&self) -> u8 { self.0 }
}

/// Target of a command: a single node or all nodes.
#[derive(PartialEq, Eq, Hash, Debug, Copy, Clone)]
pub enum NodeTarget {
    Node(u8),
    All,
}

impl Into<u8> for NodeTarget {
    fn into(self: Self) -> u8 {
        match self {
            NodeTarget::All => 0,
            NodeTarget::Node(n) => n,
        }
    }
}
     
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NmtCommand {
    StartRemoteNode        = 0x01,
    StopRemoteNode         = 0x02,
    EnterPreOperational    = 0x80,
    ResetNode              = 0x81,
    ResetCommunication     = 0x82,
}

/// Our “master” enum for every CANopen‐defined COB-ID
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CobId {
    /// NMT management service (always uses COB-ID 0x000).  Data[0] = cmd, Data[1] = node.
    NmtService,

    /// Synchronization object (COB-ID = 0x080).  Data may carry SYNC counter (optional).
    Sync,

    /// Time stamp object (COB-ID = 0x100).
    TimeStamp,

    /// Emergency message (COB-ID = 0x080 + nodeid)
    Emergency(NodeId),

    /// PDO Tx: 1..4
    PdoTx { pdo_number: u8 /*1..4*/, node: NodeId },

    /// PDO Rx: 1..4
    PdoRx { pdo_number: u8 /*1..4*/, node: NodeId },

    /// SDO Response  (COB-ID = 0x580 + node)
    SdoResponse(NodeId),

    /// SDO Request   (COB-ID = 0x600 + node)
    SdoRequest(NodeId),

    /// Heartbeat or Node-Guard (COB-ID = 0x700 + node)
    Heartbeat(NodeId),

    /// Everything else: manufacturer-specific or reserved
    ManufacturerSpecific(u16),
}

pub const NODE_MASK:  u32 = 0x7F;    // lower 7 bits
const FUNC_MASK:  u32 = 0x780;   // next 4 bits << 7

impl From<CobId> for u32 {
    fn from(c: CobId) -> u32 {
        match c {
            CobId::NmtService         => 0x000,
            CobId::Sync               => 0x080,
            CobId::TimeStamp          => 0x100,
            CobId::Emergency(n)       => 0x080 | (n.get() as u32),
            CobId::PdoTx { pdo_number, node } => {
                let base = 0x100 * pdo_number as u32 + 0x080;
                base | (node.get() as u32)
            }
            CobId::PdoRx { pdo_number, node } => {
                let base = 0x100 * pdo_number as u32 + 0x100;
                base | (node.get() as u32)
            }
            CobId::SdoResponse(n)    => 0x580 | (n.get() as u32),
            CobId::SdoRequest(n)     => 0x600 | (n.get() as u32),
            CobId::Heartbeat(n)      => 0x700 | (n.get() as u32),
            CobId::ManufacturerSpecific(id) => id as u32,
        }
    }
}

impl From<u32> for CobId {
    fn from(raw: u32) -> CobId {
        let func = raw & FUNC_MASK;
        let node = (raw & NODE_MASK) as u8;
        match (func, node) {
            // NMT service is always COB-ID = 0x000
            (0x000, _) => {
                // data[0] and data[1] must be examined by caller
                // to figure out the actual NmtCommand and target node,
                // so we just return a placeholder here.
                // Application code can then decode actual bytes.
                CobId::NmtService 
            }

            // Sync object
            (0x080, 0x00) => CobId::Sync,

            // timestamp
            (0x100, 0x00) => CobId::TimeStamp,

            // Emergency
            (0x080, n) if n != 0 => {
                CobId::Emergency(NodeId(n))
            }

            // PDO Tx [1..4]
            (fp, n) if (0x180..=0x480).contains(&fp) && (fp - 0x080) % 0x100 == 0 => {
                let pdo = ((fp - 0x080) / 0x100) as u8;
                CobId::PdoTx {
                    pdo_number: pdo,
                    node: NodeId(n),
                }
            }
            // PDO Rx [1..4]
            (fp, n) if (0x200..=0x500).contains(&fp) && (fp - 0x100) % 0x100 == 0 => {
                let pdo = ((fp - 0x100) / 0x100) as u8;
                CobId::PdoRx {
                    pdo_number: pdo,
                    node: NodeId(n),
                }
            }

            // SDO Resp
            (0x580, n) => CobId::SdoResponse(NodeId(n)),
            // SDO Req
            (0x600, n) => CobId::SdoRequest(NodeId(n)),
            // Heartbeat / Guard
            (0x700, n) => CobId::Heartbeat(NodeId(n)),

            // Anything else
            _ => CobId::ManufacturerSpecific(raw as u16),
        }
    }
}

