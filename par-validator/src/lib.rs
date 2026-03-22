//! Parallel validation helpers for **string-like** fields (CPU / Rayon) and **fixed-point
//! numerics** (GPU / wgpu).
//!
//! ## String rules
//! Use [`RuleBuilder`] with key paths from [`rust_key_paths`] (typically via `key-paths-derive`).
//! Mandatory rules run **sequentially** and short-circuit on the first failure; remaining rules run
//! in parallel with Rayon inside [`RuleBuilder::apply`].
//!
//! ## Numeric rules
//! See [`gpu_numeric`] for [`GpuNumericEngine`](gpu_numeric::GpuNumericEngine), which batches
//! [`NumericRule`](gpu_numeric::NumericRule) rows in a single compute dispatch. Values cross the
//! CPU/GPU boundary as **i32 × 100** (no `f32`/`f64` in the shader).
//!
//! ## Examples
//! - `cargo run --example basics` — CPU-only starter  
//! - `cargo run --example hybrid_gpu` — small hybrid demo  
//! - `cargo run --example fintech_rayon_nested` — large nested batch, CPU only  
//! - `cargo run --example fintech_gpu_batch` — large numeric batch, GPU only  
//! - `cargo run --example fintech_hybrid_batch` — both layers on a trade batch  

#![forbid(unsafe_code)]

use std::fmt::Debug;

use rayon::prelude::*;
use rust_key_paths::{AccessorTrait, KpType};

pub mod errors;
pub mod gpu_numeric;

pub use errors::RuleBuilderError;

/// Fluent wrapper around a [`KpType`] (key path) and a set of validation functions.
///
/// `R` is the **root** type you pass to [`RuleBuilder::with_root`]; `V` is the **value** type at
/// the end of the path (often `String`). `E` is your error payload (e.g. `String`).
///
/// Rule functions must be `fn` pointers (not closures that capture state) so they can be stored
/// in a [`Vec`] and shared across Rayon threads.
pub struct RuleBuilder<'a, R, V, E: PartialEq + Eq + Send + Sync> {
    root: Option<&'a R>,
    kp: KpType<'a, R, V>,
    mandatory_rules: Vec<fn(Option<&'a V>) -> RuleBuilderError<E>>,
    rules: Vec<fn(Option<&'a V>) -> RuleBuilderError<E>>,
}

impl<'a, R, V, E> RuleBuilder<'a, R, V, E>
where
    E: Debug + Clone + 'static + PartialEq + Eq + Send + Sync,
    R: Sync,
    V: Sync,
{
    /// Starts a builder for the given statically dispatched key path (`#[derive(Kp)]` fields).
    pub fn new(kp: KpType<'a, R, V>) -> Self {
        Self {
            root: None,
            kp,
            rules: vec![],
            mandatory_rules: vec![]
        }
    }

    /// Supply the struct instance `root` that the key path reads from.
    pub fn with_root(mut self, root: &'a R) -> Self {
        self.root = Some(root);
        self
    }

    /// Append a rule executed **in parallel** with other non-mandatory rules when you call
    /// [`apply`](Self::apply).
    pub fn rule(mut self, f: fn(Option<&'a V>) -> RuleBuilderError<E>) -> Self {
        self.rules.push(f);
        self
    }

    /// Rule that runs **before** parallel rules, in order. On the first non-[`Success`](RuleBuilderError::Success),
    /// [`apply`](Self::apply) returns immediately with that single outcome.
    pub fn mandatory_rule(mut self, f: fn(Option<&'a V>) -> RuleBuilderError<E>) -> Self {
        self.mandatory_rules.push(f);
        self
    }

    /// Deprecated typo; use [`Self::mandatory_rule`].
    #[deprecated(note = "use mandatory_rule")]
    pub fn madatory_rule(self, f: fn(Option<&'a V>) -> RuleBuilderError<E>) -> Self {
        self.mandatory_rule(f)
    }

    /// Resolves `Option<&V>` via the key path, runs mandatory rules, then runs remaining rules on
    /// a Rayon thread pool.
    pub fn apply(&self) -> Vec<RuleBuilderError<E>> {
        let val = self.kp.get_optional(self.root);
        for rule in self.mandatory_rules.iter() {
            let result = rule(val);
            if RuleBuilderError::Success != result {
                return vec![result];
            }
        }
        self.rules.par_iter().map(|f| f(val)).collect()
    }
}


