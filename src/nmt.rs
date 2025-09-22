use core::time::Duration;
use heapless::index_map::FnvIndexMap;

use crate::machine::*;
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

enum NmtMasterState {
    Idle,
    Execute(NmtCommand),
    Error(NmtError)
}

/// Events fed into the state machine.
#[derive(Debug)]
pub enum NmtEvent {
    /// A (decoded) NMT response from a node: "I am now in this state".
    Response { node_id: u8, new_state: NmtState },
    /// A periodic tick for timeout checking.
    Tick,
}

/// Errors the NMT master can encounter.
#[derive(Debug, Clone, Copy)]
pub enum NmtError {
    Timeout { node_id: u8 },
    UnexpectedResponse { node_id: u8, state: NmtState },
}

#[derive(Debug)]
pub enum NmtOutput {
    Ready,
    Command(NmtCommand),
    Error(NmtError),
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
    /// State
    state: NmtMasterState,
}

impl<const N: usize, I> NmtMaster<N, I>
where
    I: ClockInstant {
    pub fn new<Nodes: IntoIterator<Item=u8>>(nodes: Nodes) -> Self {
        let node_states = nodes.into_iter()
            .map(|id| (id, NmtState::Initialization))
            .collect();
        NmtMaster {
            state: NmtMasterState::Idle,
            node_states,
            pendings: FnvIndexMap::new(),
            timeout: Duration::from_millis(500),
            max_retries: 3,
        }
    }
}

impl<const N: usize, I> MachineTrans<NmtEvent> for NmtMaster<N, I>
where
    I: ClockInstant {
    type Observation = NmtOutput;
    
    fn initial(self: &mut Self) {
        self.state = NmtMasterState::Idle;
        // self.pendings.clear()
    }

    fn transit(self: &mut Self, response: NmtEvent) {
        match (&self.state, response) {
            (NmtMasterState::Execute(cmd), NmtEvent::Response {node_id, new_state}) => {
                todo!()
            }

            (_, NmtEvent::Tick) => {
                todo!()
            }

            (s, r) => {
                let e = todo!();
                self.state = NmtMasterState::Error(e)
            }
            
        }
    }

    fn observe(&mut self) -> Self::Observation {
        match &self.state {
            NmtMasterState::Idle => NmtOutput::Ready,
            NmtMasterState::Execute(cmd) => NmtOutput::Command(*cmd),
            NmtMasterState::Error(e) => NmtOutput::Error(*e),
        }
    }

}
