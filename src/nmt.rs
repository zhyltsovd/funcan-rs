use core::time::Duration;
use heapless::index_map::FnvIndexMap;

use crate::interfaces::ClockInstant;

/// The possible NMT states of a node.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum NmtState {
    Initialization,
    PreOperational,
    Operational,
    Stopped,
}

/// The low‐level NMT control service (no node‐ID, no framing).
#[derive(Debug, Copy, Clone)]
pub enum NmtControlCommand {
    Start,
    Stop,
    ResetNode,
    ResetCommunication,
}

/// Target of a command: a single node or all nodes.
#[derive(Debug, Copy, Clone)]
pub enum NodeTarget {
    Node(u8),
    All,
}

/// High‐level NMT command request.
#[derive(Debug, Copy, Clone)]
pub struct NmtCommand {
    pub cmd: NmtControlCommand,
    pub target: NodeTarget,
}

/// Events fed into the state machine.
#[derive(Debug)]
pub enum NmtEvent {
    /// A user‐ or timer‐driven request to change state.
    Command(NmtCommand),
    /// A (decoded) NMT response from a node: "I am now in this state".
    Response { node_id: u8, new_state: NmtState },
    /// A periodic tick for timeout checking.
    Tick,
}

/// Errors the NMT master can encounter.
#[derive(Debug)]
pub enum NmtError {
    Timeout { node_id: u8 },
    UnexpectedResponse { node_id: u8, state: NmtState },
}

// ============= Pending Request Tracker =============

struct Pending<I: ClockInstant> {
    expected: NmtState,
    sent_at: I,
    retries: u8,
}

pub struct NmtMaster<const N: usize, I: ClockInstant> {
    /// Current state of each known node.
    node_states: FnvIndexMap<u8, NmtState, N>,
    /// Pending requests by node waiting for a response.
    pendings: FnvIndexMap<u8, Pending<I>, N>,
    /// Configuration
    timeout: Duration,
    max_retries: u8,
}

