use crate::raw::*;

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
    StartRemoteNode = 0x01,
    StopRemoteNode = 0x02,
    EnterPreOperational = 0x80,
    ResetNode = 0x81,
    ResetCommunication = 0x82,
}

/// The possible NMT states of a node.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum NmtState {
    Initialization,
    PreOperational,
    Operational,
    Stopped,
}

impl Into<u8> for NmtState {
    fn into(self) -> u8 {
        match self {
            NmtState::Initialization => 0x00,
            NmtState::Stopped => 0x04,
            NmtState::Operational => 0x05,
            NmtState::PreOperational => 0x7F,
        }
    }
}

impl From<u8> for NmtState {
    fn from(c: u8) -> Self {
        match c {
            0x00 => NmtState::Initialization,
            0x04 => NmtState::Stopped,
            0x05 => NmtState::Operational,
            0x7F => NmtState::PreOperational,
            _ => unreachable!(),
        }
    }
}

impl Into<NmtCommand> for NmtState {
    fn into(self: Self) -> NmtCommand {
        match self {
            NmtState::Operational => NmtCommand::StartRemoteNode,
            NmtState::Stopped => NmtCommand::StopRemoteNode,
            NmtState::Initialization => NmtCommand::ResetNode,
            NmtState::PreOperational => NmtCommand::EnterPreOperational,
        }
    }
}

/// High‐level NMT command request.
#[derive(Debug, Copy, Clone)]
pub struct NmtRequest {
    pub cmd: NmtCommand,
    pub target: NodeTarget,
}

impl From<NmtRequest> for CanFrame16 {
    fn from(req: NmtRequest) -> CanFrame16 {
        let (cobid, data) = req.into_parts();
        CanFrame16::from_parts(cobid, 2, data)
    }
}

impl From<NmtRequest> for CanFrame13 {
    fn from(req: NmtRequest) -> CanFrame13 {
        let (cobid, data) = req.into_parts();
        CanFrame13::from_parts(cobid, 2, data)
    }
}

impl From<CanFrame16> for NmtRequest {
    fn from(frame: CanFrame16) -> Self {
        Self::from_frame_payload(frame.data())
    }
}

impl From<CanFrame13> for NmtRequest {
    fn from(frame: CanFrame13) -> Self {
        Self::from_frame_payload(frame.data())
    }
}

impl NmtRequest {
    /// Splits the request into the NMT COB-ID and the 2-byte payload
    /// (command specifier, target node).
    fn into_parts(self) -> (CobId, [u8; 8]) {
        // Translate our high‐level command into the 1‐byte NMT command specifier
        let specifier: u8 = self.cmd as u8;

        // Target node‐ID: 0 = all nodes, otherwise 1..127
        let node_id_byte = self.target.into();

        // Build the 8‐byte CAN frame (only first two bytes are used)
        let mut data = [0u8; 8];
        data[0] = specifier;
        data[1] = node_id_byte;

        (CobId::NmtService(specifier, node_id_byte), data)
    }

    fn from_frame_payload(data: [u8; 8]) -> Self {
        let cmd = match data[0] {
            0x01 => NmtCommand::StartRemoteNode,
            0x02 => NmtCommand::StopRemoteNode,
            0x80 => NmtCommand::EnterPreOperational,
            0x81 => NmtCommand::ResetNode,
            0x82 => NmtCommand::ResetCommunication,
            _ => unreachable!(),
        };

        let target = if data[1] == 0 {
            NodeTarget::All
        } else {
            NodeTarget::Node(data[1])
        };

        NmtRequest { cmd, target }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nmt_request_to_from_16() {
        let req = NmtRequest {
            cmd: NmtCommand::StartRemoteNode,
            target: NodeTarget::All,
        };

        let frame: CanFrame16 = req.into();
        assert_eq!(frame.cobid(), CobId::NmtService(0x01, 0));
        assert_eq!(frame.len(), 2);
        let data = frame.data();
        assert_eq!(data[0], 0x01);
        assert_eq!(data[1], 0x00);

        let back = NmtRequest::from(frame);
        assert_eq!(back.cmd, NmtCommand::StartRemoteNode);
        assert_eq!(back.target, NodeTarget::All);
    }

    #[test]
    fn nmt_request_to_from_13() {
        let req = NmtRequest {
            cmd: NmtCommand::EnterPreOperational,
            target: NodeTarget::Node(7),
        };

        let frame: CanFrame13 = req.into();
        assert_eq!(frame.cobid(), CobId::NmtService(0x80, 7));
        assert_eq!(frame.len(), 2);
        let data = frame.data();
        assert_eq!(data[0], 0x80);
        assert_eq!(data[1], 0x07);

        let back = NmtRequest::from(frame);
        assert_eq!(back.cmd, NmtCommand::EnterPreOperational);
        assert_eq!(back.target, NodeTarget::Node(7));
    }
}
