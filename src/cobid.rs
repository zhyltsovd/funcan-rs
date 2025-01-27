const NODE_MASK: u32 = 0x7F; // 7 bits for node ID
const FUN_MASK: u32 = 0x780; // 4 bits for function code (shifted << 7)

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BroadcastCmd {
    Nmt,  // Network Management
    Sync, // Synchronization
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NodeCmd {
    Emergency,
    Time,
    Pdo1Tx,
    Pdo1Rx,
    Pdo2Tx,
    Pdo2Rx,
    Pdo3Tx,
    Pdo3Rx,
    Pdo4Tx,
    Pdo4Rx,
    SdoResp,
    SdoReq,
    Heartbeat,
    Unused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FunCode {
    Broadcast(BroadcastCmd),
    Node(NodeCmd, u8),
}

// Decoding implementation
fn decode_broadcast(cob_id: u32) -> Option<BroadcastCmd> {
    let fun = cob_id & FUN_MASK;
    let node = cob_id & NODE_MASK;

    match (fun, node) {
        (0x000, _) => Some(BroadcastCmd::Nmt),
        (0x080, 0x00) => Some(BroadcastCmd::Sync),
        _ => None,
    }
}

fn decode_node_code(func_part: u32) -> NodeCmd {
    match func_part {
        0x080 => NodeCmd::Emergency,
        0x100 => NodeCmd::Time,
        0x180 => NodeCmd::Pdo1Tx,
        0x200 => NodeCmd::Pdo1Rx,
        0x280 => NodeCmd::Pdo2Tx,
        0x300 => NodeCmd::Pdo2Rx,
        0x380 => NodeCmd::Pdo3Tx,
        0x400 => NodeCmd::Pdo3Rx,
        0x480 => NodeCmd::Pdo4Tx,
        0x500 => NodeCmd::Pdo4Rx,
        0x580 => NodeCmd::SdoResp,
        0x600 => NodeCmd::SdoReq,
        0x700 => NodeCmd::Heartbeat,
        _ => NodeCmd::Unused,
    }
}

impl From<u32> for FunCode {
    fn from(cob_id: u32) -> Self {
        if let Some(broadcast) = decode_broadcast(cob_id) {
            FunCode::Broadcast(broadcast)
        } else {
            let func_part = cob_id & FUN_MASK;
            let node = (cob_id & NODE_MASK) as u8;
            let cmd = decode_node_code(func_part);
            FunCode::Node(cmd, node)
        }
    }
}

// Encoding implementation
fn encode_node_code(cmd: NodeCmd) -> u32 {
    match cmd {
        NodeCmd::Emergency => 0x080,
        NodeCmd::Time => 0x100,
        NodeCmd::Pdo1Tx => 0x180,
        NodeCmd::Pdo1Rx => 0x200,
        NodeCmd::Pdo2Tx => 0x280,
        NodeCmd::Pdo2Rx => 0x300,
        NodeCmd::Pdo3Tx => 0x380,
        NodeCmd::Pdo3Rx => 0x400,
        NodeCmd::Pdo4Tx => 0x480,
        NodeCmd::Pdo4Rx => 0x500,
        NodeCmd::SdoResp => 0x580,
        NodeCmd::SdoReq => 0x600,
        NodeCmd::Heartbeat => 0x700,
        NodeCmd::Unused => 0x000,
    }
}

impl From<FunCode> for u32 {
    fn from(code: FunCode) -> u32 {
        match code {
            FunCode::Broadcast(BroadcastCmd::Nmt) => 0x000,
            FunCode::Broadcast(BroadcastCmd::Sync) => 0x080,
            FunCode::Node(cmd, node) => {
                let func_part = encode_node_code(cmd);
                let node_part = (node as u32) & NODE_MASK;
                func_part | node_part
            }
        }
    }
}
