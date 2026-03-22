//! Load a **validation catalog** from CSV and build [`par_validator::builder::Rule`] chains.
//!
//! Column mapping (see `examples/data/validation_catalog.csv`):
//!
//! | Column | Maps to |
//! |--------|---------|
//! | `table` | Logical **root** type you validate (here: `Payment`) |
//! | `field` | **Key-path field** name from `#[derive(Kp)]` (e.g. `reference` → `Payment::reference()`) |
//! | `rule_kind` | `mandatory` (sequential, short-circuit) or `parallel` (Rayon in `apply`) |
//! | `rule_id` | **Registry key** your code maps to a Rust `fn(Option<&V>) -> bool` |
//! | `error_code` | Stable **error code** (or message) stored as `E` when that predicate returns `false` |
//!
//! Rust cannot load function pointers from CSV at runtime; you **register** `rule_id` → `fn` in a
//! match or map, then attach the CSV’s `error_code` per row.
//!
//! Run: `cargo run --example rule_csv_catalog`

use std::fs;
use std::path::Path;

use key_paths_derive::Kp;
use par_validator::builder::Rule;

#[derive(Debug, Clone)]
struct CatalogRow {
    table:     String,
    field:     String,
    rule_kind: String,
    rule_id:   String,
    error_code: String,
}

fn parse_catalog_csv(text: &str) -> Vec<CatalogRow> {
    let mut rows = Vec::new();
    let mut header = true;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if header {
            header = false;
            continue;
        }
        let cols: Vec<&str> = line.split(',').map(|c| c.trim()).collect();
        if cols.len() != 5 {
            continue;
        }
        rows.push(CatalogRow {
            table:      cols[0].to_string(),
            field:      cols[1].to_string(),
            rule_kind:  cols[2].to_string(),
            rule_id:    cols[3].to_string(),
            error_code: cols[4].to_string(),
        });
    }
    rows
}

// ── Registered predicates (`rule_id` in CSV) ───────────────────────────────

fn non_blank(r: Option<&String>) -> bool {
    r.map(|s| !s.trim().is_empty()).unwrap_or(false)
}

fn max_len_16(r: Option<&String>) -> bool {
    r.map(|s| s.len() <= 16).unwrap_or(false)
}

fn alpha_upper(r: Option<&String>) -> bool {
    r.map(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphabetic() && !c.is_lowercase()))
        .unwrap_or(false)
}

fn resolve_string_rule(rule_id: &str) -> Option<fn(Option<&String>) -> bool> {
    match rule_id {
        "non_blank"   => Some(non_blank),
        "max_len_16"   => Some(max_len_16),
        "alpha_upper" => Some(alpha_upper),
        _             => None,
    }
}

#[derive(Kp)]
struct Payment {
    reference: String,
    currency:  String,
}

/// Apply every catalog row for `Payment.reference` as one [`Rule`] chain.
fn build_rule_for_payment_reference<'a>(
    payment: &'a Payment,
    catalog: &[CatalogRow],
) -> Rule<'a, Payment, String, String> {
    let mut rule = Rule::new(Payment::reference()).with_root(payment);
    for row in catalog {
        if row.table != "Payment" || row.field != "reference" {
            continue;
        }
        let Some(pred) = resolve_string_rule(&row.rule_id) else {
            eprintln!("skip unknown rule_id: {}", row.rule_id);
            continue;
        };
        let err = row.error_code.clone();
        rule = match row.rule_kind.as_str() {
            "mandatory" => rule.mandatory_rule(pred, err),
            "parallel"  => rule.rule(pred, err),
            _           => {
                eprintln!("skip unknown rule_kind: {}", row.rule_kind);
                rule
            },
        };
    }
    rule
}

fn build_rule_for_payment_currency<'a>(
    payment: &'a Payment,
    catalog: &[CatalogRow],
) -> Rule<'a, Payment, String, String> {
    let mut rule = Rule::new(Payment::currency()).with_root(payment);
    for row in catalog {
        if row.table != "Payment" || row.field != "currency" {
            continue;
        }
        let Some(pred) = resolve_string_rule(&row.rule_id) else {
            continue;
        };
        let err = row.error_code.clone();
        rule = match row.rule_kind.as_str() {
            "mandatory" => rule.mandatory_rule(pred, err),
            "parallel"  => rule.rule(pred, err),
            _           => rule,
        };
    }
    rule
}

fn main() {
    let csv_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("data")
        .join("validation_catalog.csv");
    let text = fs::read_to_string(&csv_path).unwrap_or_else(|e| {
        panic!("read {}: {e}", csv_path.display());
    });
    let catalog = parse_catalog_csv(&text);

    println!("Loaded {} catalog rows from {}\n", catalog.len(), csv_path.display());

    let good = Payment {
        reference: "REF-OK".into(),
        currency:  "USD".into(),
    };

    let bad_ref = Payment {
        reference: "THIS_REFERENCE_IS_TOO_LONG".into(),
        currency:  "USD".into(),
    };

    let bad_ccy = Payment {
        reference: "REF-OK".into(),
        currency:  "usd".into(),
    };

    for (label, p) in [
        ("valid", &good),
        ("bad reference length", &bad_ref),
        ("bad currency (not A-Z)", &bad_ccy),
    ] {
        let ref_failures = build_rule_for_payment_reference(p, &catalog).apply();
        let ccy_failures = build_rule_for_payment_currency(p, &catalog).apply();
        println!("{label}:");
        println!("  reference failures: {ref_failures:?}");
        println!("  currency failures:  {ccy_failures:?}");
        println!();
    }
}
