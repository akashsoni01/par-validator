//! Smallest **CPU-only** demo: one struct, one string field, [`RuleBuilder`].
//! No GPU — works on any machine with Rust alone.
//!
//! Run: `cargo run --example basics`

use key_paths_derive::Kp;
use par_validator::{RuleBuilder, RuleBuilderError};

type StrErr = RuleBuilderError<String>;

fn not_empty(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("reference is missing".into()),
        Some(s) if s.trim().is_empty() => StrErr::Fail("reference is blank".into()),
        Some(_) => StrErr::Success,
    }
}

fn max_len_16(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("missing".into()),
        Some(s) if s.len() > 16 => StrErr::Fail(format!("reference too long: {} chars", s.len())),
        Some(_) => StrErr::Success,
    }
}

#[derive(Kp)]
struct Payment {
    reference: String,
}

fn main() {
    let ok = Payment {
        reference: "REF-001".into(),
    };

    let bad = Payment {
        reference: "THIS_REFERENCE_IS_TOO_LONG".into(),
    };

    for (label, p) in [("valid", &ok), ("invalid length", &bad)] {
        let out = RuleBuilder::<Payment, String, String>::new(Payment::reference())
            .with_root(p)
            .mandatory_rule(not_empty)
            .rule(max_len_16)
            .apply();

        println!("{label}: {out:?}");
    }
}
