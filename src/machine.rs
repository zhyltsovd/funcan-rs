//! # Machine Module
//!
//! This module offers traits for creating and managing a finite state machine.

/// A trait that represents a finite state machine (FSM).
///
/// # Type Parameters
///
/// - `X`: The type of the input or event that causes state transitions.
///
/// # Associated Types
///
/// - `FinalValue`: The type that represents the final value or result 
///   produced by the state machine once it reaches a terminal state.
pub trait MachineTrans<X> {
    type FinalValue;

    /// Transitions the state machine to a new state based on the input.
    ///
    /// This method modifies the machine's current state according to 
    /// the specified input `x`. The exact details of the transition 
    /// mechanism are determined by the implementer.
    ///
    /// # Parameters
    ///
    /// - `x`: An input value of type `X` that influences the state change.
    fn transition(self: &mut Self, x: X);

    /// Checks if the state machine has reached a final state.
    ///
    /// This method returns an `Option` containing a final value if the 
    /// machine is in a terminal state, otherwise it returns `None`.
    ///
    /// # Return Value
    ///
    /// An `Option<Self::FinalValue>` that contains the output if the 
    /// machine is finished; otherwise, `None`.
    fn is_final(self: &Self) -> Option<Self::FinalValue>;
}

/// A trait for finite state machines that can produce output during operation.
///
/// This trait adds the capability to generate output from the current state 
/// of the machine, even if it hasn't reached a terminal state yet. It provides 
/// flexibility in managing intermediate results or signals from the machine.
///
/// # Associated Types
///
/// - `Output`: The type of data that the machine can output based on its 
///   current state.
pub trait MachineWithOutput {
    type Output;

    /// Retrieves the current output of the state machine.
    ///
    /// This method returns an `Option` containing an output value if 
    /// it can be determined from the machine's current state; otherwise, 
    /// it returns `None`.
    ///
    /// # Return Value
    ///
    /// An `Option<Self::Output>` that contains the output if available; 
    /// otherwise, `None`.
    fn output(self: &Self) -> Option<Self::Output>;
}

/// Represents the composition of two finite state machines,
/// where the output of the first machine (`M0`) serves as the input to the second machine (`M1`).
pub struct Comp<M0, M1> {
    /// The first state machine.
    pub m0: M0,
    /// The second state machine.
    pub m1: M1,
}

/// Allows for the composition of two finite state machines `M0` and `M1`.
/// This trait implementation is applicable when the final values of machine `M0`
/// can be used as inputs to machine `M1`. 
/// 
/// A common use case is where `M0` processes and decodes some low-level input
/// to generate higher-level inputs for machine `M1`.
impl<X, M0, M1> MachineTrans<X> for Comp<M0, M1>
where
    M0: MachineTrans<X>,
    M1: MachineTrans<<M0 as MachineTrans<X>>::FinalValue>
{
    /// The final value type produced by the composed machines.
    type FinalValue = <M1 as MachineTrans<<M0 as MachineTrans<X>>::FinalValue>>::FinalValue;
    
    /// Processes an input `x` by passing it through machine `M0`.
    /// If `M0` reaches a final state, its output is passed as input to machine `M1`.
    fn transition(self: &mut Self, x: X) {
        self.m0.transition(x);
        match self.m0.is_final() {
            Some(y) => {
                self.m1.transition(y);
            }
            None => {}
        }
    }

    /// Checks if the composed machines have reached a final state.
    /// Returns the final value of machine `M1` if it has reached a final state.
    fn is_final(self: &Self) -> Option<Self::FinalValue> {
        self.m1.is_final()
    }
}
