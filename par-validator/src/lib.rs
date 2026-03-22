use std::fmt::Debug;

use rayon::prelude::*;
use rust_key_paths::{AccessorTrait, KpType};

pub mod errors;
pub mod gpu_numeric;

pub use errors::RuleBuilderError;

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
    pub fn new(kp: KpType<'a, R, V>) -> Self {
        Self {
            root: None,
            kp,
            rules: vec![],
            mandatory_rules: vec![]
        }
    }

    pub fn with_root(mut self, root: &'a R) -> Self {
        self.root = Some(root);
        self
    }

    pub fn rule(mut self, f: fn(Option<&'a V>) -> RuleBuilderError<E>) -> Self {
        self.rules.push(f);
        self
    }

    pub fn mandatory_rule(mut self, f: fn(Option<&'a V>) -> RuleBuilderError<E>) -> Self {
        self.mandatory_rules.push(f);
        self
    }

    /// Deprecated typo; use [`Self::mandatory_rule`].
    #[deprecated(note = "use mandatory_rule")]
    pub fn madatory_rule(self, f: fn(Option<&'a V>) -> RuleBuilderError<E>) -> Self {
        self.mandatory_rule(f)
    }

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


