use core::time::Duration;
use heapless::index_map::FnvIndexMap;

use crate::machine::*;
// use crate::raw::*;
use crate::interfaces::*;
use crate::nmt::*;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ObservableNodeState {
    NotFound,
    Alive(NmtState),
    Lost,
}

pub struct NodeState<I: ClockInstant> {
    pub state: NmtState,
    pub beat: I,
}

pub struct HeartbeatMachine<const N: usize, I: ClockInstant> {
    /// Current state of each known node.
    node_states: FnvIndexMap<u8, NodeState<I>, N>,
    /// Timeout
    timeout: Duration,
}

impl<const N: usize, I: ClockInstant> HeartbeatMachine<N, I> {
    pub fn new(timeout: u64) -> Self {
        Self {
            node_states: FnvIndexMap::new(),
            timeout: Duration::from_millis(timeout),
        }
    }

    pub fn check_state<R>(self: &Self, node_id: u8, r: R)
    where
        R: Responder<ObservableNodeState>,
    {
        match self.node_states.get(&node_id) {
            None => {
                r.respond(ObservableNodeState::NotFound);
            }

            Some(node) => {
                let now = I::now();
                if now.duration_since(&node.beat) > self.timeout {
                    r.respond(ObservableNodeState::Lost);
                } else {
                    r.respond(ObservableNodeState::Alive(node.state));
                }
            }
        }
    }
}

impl<const N: usize, I: ClockInstant> MealyMachine<(u8, NmtState), ()> for HeartbeatMachine<N, I> {
    
    fn transit(self: &mut Self, x: (u8, NmtState)) {
        let (node_id, state) = x;

        let now = I::now();

        match self.node_states.get_mut(&node_id) {
            None => {
                let node_state = NodeState {
                    state: state,
                    beat: now,
                };
                self.node_states.insert(node_id, node_state);
            }

            Some(node) => {
                node.state = state;
                node.beat = now;
            }
        };
    }

    fn initiate(self: &mut Self) {
        self.node_states.clear();
    }
}
