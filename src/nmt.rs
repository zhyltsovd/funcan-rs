use core::time::Duration;
use heapless::index_map::FnvIndexMap;
use heapless::vec::Vec;

use crate::interfaces::ClockInstant;
use crate::machine::*;
use crate::raw::*;
use crate::cobid::*;

/// The possible NMT states of a node.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum NmtState {
    Initialization,
    PreOperational,
    Operational,
    Stopped,
}

impl NmtState {
    fn to_code(self) -> u8 {
        match self {
            NmtState::Initialization   => 0x00,
            NmtState::Stopped          => 0x04,
            NmtState::Operational      => 0x05,
            NmtState::PreOperational   => 0x7F,
        }
    }

    fn from_code(c: u8) -> Self {
        match c {
            0x00 => NmtState::Initialization,
            0x04 => NmtState::Stopped,
            0x05 => NmtState::Operational,
            0x7F => NmtState::PreOperational,
            _    => unreachable!(),
        }
    }
}

impl From<CANFrame> for NmtEvent {
    fn from(frame: CANFrame) -> NmtEvent {
        let node = (frame.can_cobid & NODE_MASK) as u8;
        let code = frame.can_data[0];
        let state = NmtState::from_code(code);
        
        NmtEvent::Response {
            node_id:   node,
            new_state: state,
        }
    }
}

/// High‐level NMT command request.
#[derive(Debug, Copy, Clone)]
pub struct NmtRequest {
    pub cmd: NmtCommand,
    pub target: NodeTarget,
}

impl Into<CANFrame> for NmtRequest {
    fn into(self) -> CANFrame {
        
        // Translate our high‐level command into the 1‐byte NMT command specifier
        let specifier: u8 = self.cmd as u8;

        // Target node‐ID: 0 = all nodes, otherwise 1..127
        let node_id_byte = self.target.into();
        
        // Build the 8‐byte CAN frame (only first two bytes are used)
        let mut data = [0u8; 8];
        data[0] = specifier;
        data[1] = node_id_byte;

        CANFrame {
            can_cobid: 0x000, // NMT uses COB‐ID = 0
            can_len: 2,       // only 2 bytes valid
            can_data: data,
        }
    }
}

impl From<CANFrame> for NmtRequest {
    fn from(frame: CANFrame) -> Self {
        let cmd =
            match frame.can_data[0] {
                0x01 => NmtCommand::StartRemoteNode,
                0x02 => NmtCommand::StopRemoteNode,
                0x80 => NmtCommand::EnterPreOperational,
                0x81 => NmtCommand::ResetNode,
                0x82 => NmtCommand::ResetCommunication,
                _ => unreachable!()
            };
        
        let target =
            if frame.can_data[1] == 0 {
                NodeTarget::All
            } else {
                NodeTarget::Node(frame.can_data[1])
            };
        
        NmtRequest { cmd , target }
    }
        
}

#[derive(Debug, Copy, Clone)]
pub enum NmtMasterState {
    Idle,
    Execute(NmtRequest),
    Error(NmtError),
}

#[derive(Debug, Copy, Clone)]
pub enum NmtMasterStateTag {
    Idle,
    Execute,
    Error,
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
    Timeout {
        node_id: u8,
    },
    StateMismatch {
        node_id: u8,
        node_state: NmtState,
        expected_state: NmtState,
    },
    ResponseMismatch {
        master_state: NmtMasterStateTag,
        node_id: u8,
        node_state: NmtState,
    },
}

#[derive(Debug)]
pub enum NmtOutput {
    Ready,
    Command(NmtRequest),
    Error(NmtError),
}

// ============= Pending Request Tracker =============

struct Pending<I: ClockInstant> {
    expected: NmtState,
    sent_at: I,
    retries: u8,
}

pub struct NmtMasterMachine<const N: usize, I: ClockInstant> {
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

impl<const N: usize, I> NmtMasterMachine<N, I>
where
    I: ClockInstant,
{
    pub fn new<Nodes: IntoIterator<Item = u8>>(nodes: Nodes, timeout: u64) -> Self {
        let node_states = nodes
            .into_iter()
            .map(|id| (id, NmtState::Initialization))
            .collect();
        NmtMasterMachine {
            state: NmtMasterState::Idle,
            node_states,
            pendings: FnvIndexMap::new(),
            timeout: Duration::from_millis(timeout),
            max_retries: 3,
        }
    }

    /// Start a single node.
    pub fn start_node(&mut self, node_id: u8) {
        self.state = NmtMasterState::Execute(NmtRequest {
            cmd: NmtCommand::StartRemoteNode,
            target: NodeTarget::Node(node_id),
        });
    }

    /// Stop a single node.
    pub fn stop_node(&mut self, node_id: u8) {
        self.state = NmtMasterState::Execute(NmtRequest {
            cmd: NmtCommand::StopRemoteNode,
            target: NodeTarget::Node(node_id),
        });
    }

    /// Reset communication on a single node.
    pub fn enter_preoperational_comm_node(&mut self, node_id: u8) {
        self.state = NmtMasterState::Execute(NmtRequest {
            cmd: NmtCommand::EnterPreOperational,
            target: NodeTarget::Node(node_id),
        });
    }

    /// Reset communication on a single node.
    pub fn reset_comm_node(&mut self, node_id: u8) {
        self.state = NmtMasterState::Execute(NmtRequest {
            cmd: NmtCommand::ResetCommunication,
            target: NodeTarget::Node(node_id),
        });
    }

    /// Reset the application on a single node.
    pub fn reset_node(&mut self, node_id: u8) {
        self.state = NmtMasterState::Execute(NmtRequest {
            cmd: NmtCommand::ResetNode,
            target: NodeTarget::Node(node_id),
        });
    }

    fn handle_response(self: &mut Self, cmd: NmtCommand, node_id: u8, new_state: NmtState) {
        let expected = match cmd {
            NmtCommand::StartRemoteNode => NmtState::Operational,
            NmtCommand::StopRemoteNode => NmtState::Stopped,
            NmtCommand::EnterPreOperational => NmtState::PreOperational,
            NmtCommand::ResetNode => NmtState::Initialization,
            NmtCommand::ResetCommunication => NmtState::PreOperational,
        };

        if new_state == expected {
            // success!  back to idle
            self.state = NmtMasterState::Idle;
        } else {
            // wrong state came back
            self.state = NmtMasterState::Error(NmtError::StateMismatch {
                node_id,
                node_state: new_state,
                expected_state: expected,
            });
        };

        self.node_states.insert(node_id, new_state);
    }

    fn handle_tick(self: &mut Self) {
        let now = I::now();
        let timeout = self.timeout;
        let max_retries = self.max_retries;

        // collect timed-out nodes
        let timed_out: Vec<u8, N> = self
            .pendings
            .iter()
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
                        NmtState::Operational => NmtCommand::StartRemoteNode,
                        NmtState::Stopped => NmtCommand::StopRemoteNode,
                        NmtState::Initialization => NmtCommand::ResetNode,
                        NmtState::PreOperational => NmtCommand::ResetCommunication,
                    };

                    pend.sent_at = I::now();
                    pend.retries += 1;
                    self.pendings.insert(node_id, pend);

                    let x = NmtRequest { cmd, target };
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

impl<const N: usize, I> MachineTrans<NmtEvent> for NmtMasterMachine<N, I>
where
    I: ClockInstant,
{
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

            (NmtMasterState::Execute(cmd), NmtEvent::Response { node_id, new_state }) => {
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

            (NmtMasterState::Idle, NmtEvent::Response { node_id, new_state }) => {
                let e = NmtError::ResponseMismatch {
                    master_state: NmtMasterStateTag::Idle,
                    node_id: node_id,
                    node_state: new_state,
                };
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


pub struct NmtMaster<const N: usize, I: ClockInstant>(pub NmtMasterMachine<N, I>);

impl<const N: usize, I: ClockInstant> MachineTrans<CANFrame> for NmtMaster<N, I>
{
    type Observation = Option<CANFrame>;
   
    fn initial(self: &mut Self) {
        self.0.initial();
    }

    fn transit(self: &mut Self, frame: CANFrame) {
        let r: NmtEvent = frame.into();
        self.0.transit(r);
    }
    
}

