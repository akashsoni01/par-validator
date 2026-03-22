use std::{borrow::Cow, fmt::Debug};

/// Outcome of a single validation rule in a [`crate::RuleBuilder`].
#[derive(Debug, PartialEq, Eq)]
pub enum RuleBuilderError<E: PartialEq + Eq + Send + Sync> {
    /// Reserved for generic / framework-level messages.
    ExampleError(Cow<'static, String>),
    /// Domain validation failure carrying your error type `E`.
    Fail(E),
    /// Rule passed.
    Success,
}
