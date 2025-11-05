use core::time::Duration;

pub trait OneshotResponder<X> {
    fn respond(self, x: X) -> Result<(), X>;
}

/*
use futures::future::*;

pub trait AsyncResponder<X> {
    type Error;
    fn send(&self, x: X) -> BoxFuture<Result<(), Self::Error>>;
}
*/

pub trait ClockInstant {
    fn now() -> Self;
    fn duration_since(self: &Self, i: &Self) -> Duration;
}

pub trait CanSize {
    fn can_size<'a>(self: &'a Self) -> usize;
}

pub trait IntoBuf {
    fn into_buf<'a>(self: &'a Self, buf: &'a mut [u8]) -> usize;
}

impl IntoBuf for u32 {
    fn into_buf<'a>(self: &'a Self, buf: &'a mut [u8]) -> usize {
        let data = self.to_le_bytes();
        let n = data.len();
        assert!(buf.len() >= n);
        buf[0..n].copy_from_slice(&data);
        n
    }
}

impl IntoBuf for u16 {
    fn into_buf<'a>(self: &'a Self, buf: &'a mut [u8]) -> usize {
        let data = self.to_le_bytes();
        let n = data.len();
        assert!(buf.len() >= n);
        buf[0..n].copy_from_slice(&data);
        n
    }
}

