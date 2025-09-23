use core::time::Duration;
    
pub trait Responder<X> {
    fn respond(self, x: X) -> Result<(), X>;
}

pub trait ClockInstant {
    fn now() -> Self;
    fn duration_since(self: &Self, i: &Self) -> Duration;
}
