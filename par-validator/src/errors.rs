use std::{borrow::Cow, fmt::Debug};

#[derive(Debug, PartialEq, Eq)]
pub enum RuleBuilderError<E: PartialEq + Eq + Send + Sync> {
    ExampleError(Cow<'static, String>),
    Fail(E),
    Success
}
