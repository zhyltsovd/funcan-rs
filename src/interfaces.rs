
pub trait Responder<X> {
    fn respond(self, x: X) -> Result<(), X>;
}
