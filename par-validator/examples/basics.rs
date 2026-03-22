//! Smallest **CPU-only** demo for [`Rule`](par_validator::builder::Rule) in the
//! [`builder`](par_validator::builder) module (`src/builder.rs`): one `#[derive(Kp)]` struct, one
//! string field, mandatory + parallel predicates.
//! No GPU — works on any machine with Rust alone.
//!
//! Run: `cargo run --example basics`

use key_paths_derive::Kp;
use par_validator::builder::Rule;

fn not_empty_ok(r: Option<&String>) -> bool {
    r.map(|s| !s.trim().is_empty()).unwrap_or(false)
}

fn max_len_16_ok(r: Option<&String>) -> bool {
    r.map(|s| s.len() <= 16).unwrap_or(false)
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
        let out = Rule::<Payment, String, String>::new(Payment::reference())
            .with_root(p)
            .mandatory_rule(not_empty_ok, "reference is missing or blank".into())
            .rule(max_len_16_ok, "reference too long (max 16 chars)".into())
            .apply();

        println!("{label}: {out:?}");
    }
}
