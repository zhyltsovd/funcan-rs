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
