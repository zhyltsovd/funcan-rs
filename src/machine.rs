
pub trait MealyMachine<X, Y> {
    fn initiate(self: &mut Self);
    fn transit(self: &mut Self, x: X) -> Y;
}

pub struct MorphMachine<'a, M, U, V, X, Y> {
    pub machine: M,
    pub decode: &'a dyn Fn(X) -> U,
    pub encode: &'a dyn Fn(V) -> Y,
}

impl<'a, M, U, V, X, Y> MealyMachine<X, Y> for MorphMachine<'a, M, U, V, X, Y>
where
    M: MealyMachine<U, V>
{
    fn initiate(self: &mut Self) {
        self.machine.initiate();
    }

    fn transit(self: &mut Self, x: X) -> Y {
        let u = (self.decode)(x);
        let v = self.machine.transit(u);
        (self.encode)(v)
    }
}
