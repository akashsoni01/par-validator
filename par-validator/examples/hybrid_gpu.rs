//! Minimal end-to-end demo: **Rayon** for string rules, **wgpu** for fixed-point numerics.
//!
//! String checks use [`par_validator::Rule`] from [`par_validator::builder`].
//!
//! Run: `cargo run --example hybrid_gpu`
//!
//! See also `fintech_rayon_nested`, `fintech_gpu_batch`, and `fintech_hybrid_batch` for
//! larger payloads.

use key_paths_derive::Kp;
use par_validator::gpu_numeric::{
    GpuErrorCode, GpuNumericEngine, NumericOutput, NumericRule, NumericRuleKind,
};
use par_validator::Rule;
use rayon::prelude::*;

mod iso_pain {
    pub fn iso123_ok(r: Option<&String>) -> bool {
        !r.map_or(true, |s| s.trim().is_empty())
    }

    pub fn max_len_35_ok(r: Option<&String>) -> bool {
        r.map(|s| s.len() <= 35).unwrap_or(false)
    }

    pub fn alpha_num_only_ok(r: Option<&String>) -> bool {
        r.map(|s| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric()))
            .unwrap_or(false)
    }
}

#[derive(Kp)]
struct Payment {
    reference: String,
    currency:  String,
}

#[derive(Kp)]
struct Transaction {
    amount:    f64,
    rate:      f64,
    principal: f64,
    tax_rate:  f64,
    days:      f64,
}

fn to_fixed(v: f64) -> i32 {
    (v * 100.0).round() as i32
}

fn fmt_fixed(v: i32) -> String {
    format!("{:.2}", v as f64 / 100.0)
}

fn describe_rule(r: &NumericRule) -> &'static str {
    match r.rule_kind {
        k if k == NumericRuleKind::RangeCheck as u32 => "range_check",
        k if k == NumericRuleKind::MustBePositive as u32 => "must_be_positive",
        k if k == NumericRuleKind::MaxPrecision as u32 => "max_precision",
        k if k == NumericRuleKind::Clamp as u32 => "clamp",
        k if k == NumericRuleKind::Percentage as u32 => "percentage",
        k if k == NumericRuleKind::TaxCalc as u32 => "tax_calc",
        k if k == NumericRuleKind::FxConvert as u32 => "fx_convert",
        k if k == NumericRuleKind::InterestCalc as u32 => "interest_calc",
        _ => "unknown",
    }
}

fn print_numeric_results(labels: &[&str], rules: &[NumericRule], results: &[NumericOutput]) {
    for ((label, rule), out) in labels.iter().zip(rules).zip(results) {
        let code  = GpuErrorCode::from(out.error_code);
        let input = fmt_fixed(rule.value);
        if out.calc_value != 0 {
            println!(
                "  [{label}] {} | input={input} → calc={} | {:?}",
                describe_rule(rule),
                fmt_fixed(out.calc_value),
                code
            );
        } else {
            println!("  [{label}] {} | input={input} → {:?}", describe_rule(rule), code);
        }
    }
}

fn main() {
    pollster::block_on(async {
        println!("=== String Validation (rayon par_iter) — builder::Rule ===\n");

        let p = Payment {
            reference: "REF001".into(),
            currency:  "  ".into(),
        };

        let string_builders = [
            Rule::<Payment, String, String>::new(Payment::reference())
                .with_root(&p)
                .mandatory_rule(iso_pain::iso123_ok, "iso123: blank or missing".into())
                .rule(iso_pain::max_len_35_ok, "max_len_35".into())
                .rule(iso_pain::alpha_num_only_ok, "alpha_num_only".into()),
            Rule::<Payment, String, String>::new(Payment::currency())
                .with_root(&p)
                .mandatory_rule(iso_pain::iso123_ok, "iso123: blank or missing".into())
                .rule(iso_pain::max_len_35_ok, "max_len_35".into())
                .rule(iso_pain::alpha_num_only_ok, "alpha_num_only".into()),
        ];

        let string_failures: Vec<String> =
            string_builders.par_iter().flat_map(|b| b.apply()).collect();

        if string_failures.is_empty() {
            println!("  (no string failures)");
        } else {
            for msg in &string_failures {
                println!("  {msg}");
            }
        }

        println!("\n=== Numeric Validation + Calculation (WebGPU) ===\n");

        let engine = GpuNumericEngine::new().await;

        let tx = Transaction {
            amount:    1250.00,
            rate:      83.25,
            principal: 10_000.00,
            tax_rate:  18.00,
            days:      365.0,
        };

        let rules: Vec<NumericRule> = vec![
            NumericRule {
                value:     to_fixed(tx.amount),
                rule_kind: NumericRuleKind::RangeCheck as u32,
                param_a:   1,
                param_b:   99_999_999,
            },
            NumericRule {
                value:     to_fixed(tx.amount),
                rule_kind: NumericRuleKind::MustBePositive as u32,
                param_a:   0,
                param_b:   0,
            },
            NumericRule {
                value:     to_fixed(tx.amount),
                rule_kind: NumericRuleKind::MaxPrecision as u32,
                param_a:   2,
                param_b:   0,
            },
            NumericRule {
                value:     to_fixed(tx.tax_rate),
                rule_kind: NumericRuleKind::Percentage as u32,
                param_a:   0,
                param_b:   0,
            },
            NumericRule {
                value:     to_fixed(tx.rate),
                rule_kind: NumericRuleKind::MustBePositive as u32,
                param_a:   0,
                param_b:   0,
            },
            NumericRule {
                value:     to_fixed(tx.amount),
                rule_kind: NumericRuleKind::Clamp as u32,
                param_a:   10_000,
                param_b:   50_000_000,
            },
            NumericRule {
                value:     to_fixed(tx.amount),
                rule_kind: NumericRuleKind::TaxCalc as u32,
                param_a:   1800,
                param_b:   0,
            },
            NumericRule {
                value:     to_fixed(tx.amount),
                rule_kind: NumericRuleKind::FxConvert as u32,
                param_a:   (tx.rate * 1000.0).round() as i32,
                param_b:   0,
            },
            NumericRule {
                value:     to_fixed(tx.principal),
                rule_kind: NumericRuleKind::InterestCalc as u32,
                param_a:   650,
                param_b:   tx.days as i32,
            },
            NumericRule {
                value:     (tx.amount * tx.rate * 100.0).round() as i32,
                rule_kind: NumericRuleKind::TaxCalc as u32,
                param_a:   1800,
                param_b:   0,
            },
        ];

        let labels = [
            "amount range",
            "amount > 0",
            "amount precision",
            "tax_rate %",
            "fx_rate > 0",
            "amount clamp",
            "GST calc",
            "FX USD→INR",
            "simple interest",
            "GST on INR amt",
        ];

        let results = engine.run(&rules);
        print_numeric_results(&labels, &rules, &results);
    });
}
