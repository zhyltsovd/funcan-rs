//! # Machine Module
//!
//! This module offers traits for working with abstract finite state machines.

/// A trait that represents a finite state machine (FSM).
///
/// # Type Parameters
///
/// - `X`: The type of the input or event that causes state transitions.
///
/// # Associated Types
///
/// - `Observation`: The type that represents abstract observations of machine states or outputs.
pub trait MachineTrans<X> {
    /// An associated type for capturing the machine's state or output observations.
    type Observation;

    /// Transitions the state machine to a new state based on the input.
    ///
    /// This method modifies the machine's current state according to
    /// the specified input `x`. The exact details of the transition
    /// mechanism are determined by the implementer.
    ///
    /// # Parameters
    ///
    /// - `x`: An input value of type `X` that influences the state change.
    fn transit(self: &mut Self, x: X);

    /// Makes an observation of the machine's current state.
    ///
    /// This method returns an abstract representation of the state or output
    /// of the machine as defined by the `Observation` associated type.
    fn observe(self: &mut Self) -> Self::Observation;

    /// Resets the machine's state to its initial state.
    ///
    /// This method should bring the machine back to its starting condition.
    fn initial(self: &mut Self);
}

/// A trait for machines that have some final states and an associated value.
///
/// This is useful for cases where a machine can complete its execution
/// and produce a final value outcome.
pub trait Final {
    /// An associated type representing the value corresponding to a final state.
    type FinalValue;

    /// Determines if the current state is a final state.
    ///
    /// # Returns
    ///
    /// - `None` if not in a final state.
    /// - `Some(val)` with `val` of type `FinalValue` if in a final state.
    fn is_final(self: Self) -> Option<Self::FinalValue>;
}

/// Represents the composition of two finite state machines,
/// where the output of the first machine (`M0`) serves as the input to the second machine (`M1`).
pub struct Comp<M0, M1> {
    /// The first state machine.
    pub m0: M0,
    /// The second state machine.
    pub m1: M1,
}

/// Implementation of the `MachineTrans` trait for the composition of two finite state machines, `M0` and `M1`.
///
/// This is applicable when the final values of machine `M0` can be used as inputs to machine `M1`.
///
/// A common use case is where `M0` processes and decodes some low-level input
/// to generate higher-level inputs for machine `M1`.
impl<X, M0, M1> MachineTrans<X> for Comp<M0, M1>
where
    M0: MachineTrans<X>,
    <M0 as MachineTrans<X>>::Observation: Final,
    M1: MachineTrans<<<M0 as MachineTrans<X>>::Observation as Final>::FinalValue>,
{
    /// Observable values of the composed machines derived from `M1`.
    type Observation = <M1 as MachineTrans<
        <<M0 as MachineTrans<X>>::Observation as Final>::FinalValue,
    >>::Observation;

    /// Processes an input `x` by passing it through machine `M0`.
    ///
    /// If `M0` reaches a final state, its output is utilized as input for machine `M1`.
    fn transit(self: &mut Self, x: X) {
        self.m0.transit(x);
        if let Some(y) = self.m0.observe().is_final() {
            // Reset `m0` to initial state
            self.m0.initial();
            // Transition `m1` with the final state's value of `m0`
            self.m1.transit(y);
        }
    }

    /// Observes and returns the current state of the composed machine.
    ///
    /// The observation is based on `M1`.
    fn observe(self: &mut Self) -> Self::Observation {
        self.m1.observe()
    }

    /// Resets both `M0` and `M1` to their initial states.
    ///
    /// Ensures that the entire composite machine starts at its initial configuration.
    fn initial(self: &mut Self) {
        self.m0.initial();
        self.m1.initial();
    }
}
