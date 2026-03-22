use std::{borrow::Cow, fmt::Debug};

/// Optional helper enum when you want **success / fail** payloads outside of [`crate::Rule`]’s
/// `Vec<E>` failure list (this crate’s string validation path uses [`crate::Rule`] + `E` per rule).
#[derive(Debug, PartialEq, Eq)]
pub enum RuleBuilderError<E: PartialEq + Eq + Send + Sync> {
    /// Reserved for generic / framework-level messages.
    ExampleError(Cow<'static, String>),
    /// Domain validation failure carrying your error type `E`.
    Fail(E),
    /// Rule passed.
    Success,
}
