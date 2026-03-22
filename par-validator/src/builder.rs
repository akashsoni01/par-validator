//! Key-path–driven validation builder ([`Rule`]).
//!
//! Each rule is a **`fn` predicate** paired with an error value `E`. The predicate returns
//! **`true` if the field is valid**; `false` means failure and the paired `E` is returned from
//! [`Rule::apply`].

use std::fmt::Debug;

use rayon::prelude::*;
use rust_key_paths::{AccessorTrait, KpType};

/// Binds a [`KpType`] (from `#[derive(Kp)]`) to a root value and a list of validation predicates.
///
/// Compared to returning `RuleBuilderError` from each rule, this type stores **`bool` predicates**
/// and attaches a fixed **`E`** per rule when validation fails.
///
/// # Semantics
///
/// - **Mandatory** rules run in order. The first predicate that returns **`false`** stops the run;
///   [`apply`](Self::apply) returns `vec![that rule’s E]`.
/// - **Non-mandatory** rules run **in parallel** (Rayon). Each predicate that returns **`false`**
///   contributes its `E`; passing rules produce no entry. The result may be **empty** if all pass.
///
/// # Examples
///
/// ```
/// use key_paths_derive::Kp;
/// use par_validator::builder::Rule;
///
/// #[derive(Kp)]
/// struct Invoice {
///     reference: String,
/// }
///
/// fn non_blank(r: Option<&String>) -> bool {
///     r.map(|s| !s.trim().is_empty()).unwrap_or(false)
/// }
///
/// fn len_at_most_8(r: Option<&String>) -> bool {
///     r.map(|s| s.len() <= 8).unwrap_or(false)
/// }
///
/// let inv = Invoice {
///     reference: "INV-01".into(),
/// };
///
/// let failures: Vec<&'static str> = Rule::new(Invoice::reference())
///     .with_root(&inv)
///     .mandatory_rule(non_blank, "reference_blank")
///     .rule(len_at_most_8, "reference_too_long")
///     .apply();
///
/// assert!(failures.is_empty());
///
/// let bad = Invoice {
///     reference: "TOO_LONG_REFERENCE".into(),
/// };
/// let failures = Rule::new(Invoice::reference())
///     .with_root(&bad)
///     .mandatory_rule(non_blank, "reference_blank")
///     .rule(len_at_most_8, "reference_too_long")
///     .apply();
///
/// assert_eq!(failures, vec!["reference_too_long"]);
/// ```
pub struct Rule<'a, R, V, E: PartialEq + Eq + Send + Sync> {
    root:            Option<&'a R>,
    kp:              KpType<'a, R, V>,
    mandatory_rules: Vec<(fn(Option<&'a V>) -> bool, E)>,
    rules:           Vec<(fn(Option<&'a V>) -> bool, E)>,
}

impl<'a, R, V, E> Rule<'a, R, V, E>
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
            mandatory_rules: vec![],
        }
    }

    /// Supply the struct instance `root` that the key path reads from.
    pub fn with_root(mut self, root: &'a R) -> Self {
        self.root = Some(root);
        self
    }

    /// Append a rule executed **in parallel** with other non-mandatory rules when you call
    /// [`apply`](Self::apply).
    pub fn rule(mut self, f: fn(Option<&'a V>) -> bool, e: E) -> Self {
        self.rules.push((f, e));
        self
    }

    /// Rule that runs **before** parallel rules, in order. On the first predicate that returns
    /// **`false`**, [`apply`](Self::apply) returns `vec![e]` for that rule’s error value `e`.
    pub fn mandatory_rule(mut self, f: fn(Option<&'a V>) -> bool, e: E) -> Self {
        self.mandatory_rules.push((f, e));
        self
    }

    /// Resolves `Option<&V>` via the key path, runs mandatory rules, then runs remaining rules on
    /// a Rayon thread pool.
    pub fn apply(&self) -> Vec<E> {
        let val = self.kp.get_optional(self.root);
        for rule in self.mandatory_rules.iter() {
            if !rule.0(val) {
                return vec![rule.1.clone()];
            }
        }
        self.rules
            .par_iter()
            .filter_map(|rule| if rule.0(val) { None } else { Some(rule.1.clone()) })
            .collect()
    }
}
