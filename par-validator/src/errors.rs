use std::{borrow::Cow, fmt::Debug};

#[derive(Debug, PartialEq, Eq)]
pub enum RuleBuilderError<E: PartialEq + Eq> {
    ExampleError(Cow<'static, String>),
    Fail(E),
    Success
}
