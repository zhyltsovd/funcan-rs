use core::time::Duration;
use heapless::index_map::FnvIndexMap;
use heapless::vec::Vec;

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

#[derive(Debug, Copy, Clone)]
enum NmtMasterState {
    Idle,
    Execute(NmtCommand),
    Error(NmtError)
}

#[derive(Debug, Copy, Clone)]
enum NmtMasterStateTag {
    Idle,
    Execute,
    Error
}

impl From<NmtMasterState> for NmtMasterStateTag {
    fn from(state: NmtMasterState) -> Self {
        match state {
            NmtMasterState::Idle => NmtMasterStateTag::Idle,
            NmtMasterState::Execute(_) => NmtMasterStateTag::Execute,
            NmtMasterState::Error(_) => NmtMasterStateTag::Error,
        }
    }
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
    StateMismatch { node_id: u8, node_state: NmtState, expected_state: NmtState },
    ResponseMismatch { master_state: NmtMasterStateTag, node_id: u8, node_state: NmtState }
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
    pub fn new<Nodes: IntoIterator<Item=u8>>(nodes: Nodes, timeout: u64) -> Self {
        let node_states = nodes.into_iter()
            .map(|id| (id, NmtState::Initialization))
            .collect();
        NmtMaster {
            state: NmtMasterState::Idle,
            node_states,
            pendings: FnvIndexMap::new(),
            timeout: Duration::from_millis(timeout),
            max_retries: 3,
        }
    }

    

    fn handle_response(self: &mut Self, cmd: NmtControlCommand, node_id: u8, new_state: NmtState) {
        let expected = match cmd {
            NmtControlCommand::Start => NmtState::Operational,
            NmtControlCommand::Stop => NmtState::Stopped,
            NmtControlCommand::ResetNode => NmtState::Initialization,
            NmtControlCommand::ResetCommunication => NmtState::PreOperational,
        };
        
        todo!()
    }


    fn handle_tick(self: &mut Self) {
        let now = I::now();
        let timeout = self.timeout;
        let max_retries = self.max_retries;
        
        // collect timed-out nodes
        let timed_out: Vec<u8, N> = self.pendings.iter()
            .filter_map(|(&node_id, pend)| {
                if now.duration_since(&pend.sent_at) >= timeout {
                    Some(node_id)
                } else {
                    None
                }
            })
            .collect();
        
        for node_id in timed_out {
            if let Some(mut pend) = self.pendings.remove(&node_id) {
                if pend.retries < max_retries {
                    // retry
                    let target = NodeTarget::Node(node_id);
                    
                    let cmd = match pend.expected {
                        NmtState::Operational => NmtControlCommand::Start,
                        NmtState::Stopped => NmtControlCommand::Stop,
                        NmtState::Initialization => NmtControlCommand::ResetNode,
                        NmtState::PreOperational => NmtControlCommand::ResetCommunication,
                    };
                    
                    pend.sent_at = I::now();
                    pend.retries += 1;
                    self.pendings.insert(node_id, pend);
                    
                    let x = NmtCommand {cmd, target};
                    self.state = NmtMasterState::Execute(x);
                } else {
                    // permanent timeout
                    // eprintln!("Error: NMT timeout on node {}", node_id);
                    // drop this pending and notify user or log
                    let err = NmtError::Timeout { node_id };
                    self.state = NmtMasterState::Error(err);
                }
            }
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

            (NmtMasterState::Idle, NmtEvent::Tick) => {
                self.handle_tick();
            }

            (_, NmtEvent::Tick) => {
                // do nothing
            }
            
            (NmtMasterState::Execute(cmd), NmtEvent::Response {node_id, new_state}) => {
                match &cmd.target {
                    NodeTarget::Node(target_id) => {
                        if *target_id == node_id {
                            self.handle_response(cmd.cmd, node_id, new_state);
                        } else {
                            // raise error?
                        }
                    }

                    NodeTarget::All => {
                        self.handle_response(cmd.cmd, node_id, new_state);
                    }
                }
            }

            (NmtMasterState::Idle, NmtEvent::Response {node_id, new_state}) => {
                let e = NmtError::ResponseMismatch { master_state: NmtMasterStateTag::Idle, node_id: node_id, node_state: new_state };
                self.state = NmtMasterState::Error(e)
            }

            (NmtMasterState::Error(_e), _) => {
                // do nothing? 
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
