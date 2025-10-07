
pub trait MealyMachine<X, Y> {
    fn initiate(self: &mut Self);
    fn transit(self: &mut Self, x: X) -> Y;
}
