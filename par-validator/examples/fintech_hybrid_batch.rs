//! **Hybrid** — Rayon over a large batch of nested transfers, then **one** GPU pass for all numerics.
//!
//! Pattern: string / identifier checks scale with **CPU parallelism**; amounts, rates, and derived tax
//! lines are packed into a single [`Vec<NumericRule>`] and validated in one
//! [`GpuNumericEngine::run`](par_validator::gpu_numeric::GpuNumericEngine::run).
//!
//! Run: `cargo run --release --example fintech_hybrid_batch`

use key_paths_derive::Kp;
use par_validator::gpu_numeric::{GpuErrorCode, GpuNumericEngine, NumericRule, NumericRuleKind};
use par_validator::Rule;
use rayon::prelude::*;

fn non_blank_ok(r: Option<&String>) -> bool {
    r.map(|s| !s.trim().is_empty()).unwrap_or(false)
}

fn alnum_id_ok(r: Option<&String>) -> bool {
    match r {
        None => false,
        Some(s) => s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
    }
}

/// Nested trade: identifiers + numeric economics (often deserialized from JSON / ISO XML).
#[derive(Kp, Clone)]
struct TradeLeg {
    client_ref:     String,
    product_code:   String,
    notional_usd:   f64,
    fx_rate:        f64,
    withholding_bp: i32,
}

#[derive(Clone)]
struct TradeBatch {
    legs: Vec<TradeLeg>,
}

fn to_fixed(v: f64) -> i32 {
    (v * 100.0).round() as i32
}

fn string_rules<'a>(t: &'a TradeLeg) -> Vec<Rule<'a, TradeLeg, String, String>> {
    vec![
        Rule::new(TradeLeg::client_ref())
            .with_root(t)
            .mandatory_rule(non_blank_ok, "blank".into())
            .rule(alnum_id_ok, "id must be alphanumeric / hyphen".into()),
        Rule::new(TradeLeg::product_code())
            .with_root(t)
            .mandatory_rule(non_blank_ok, "blank".into())
            .rule(alnum_id_ok, "id must be alphanumeric / hyphen".into()),
    ]
}

fn numeric_rules_for_leg(t: &TradeLeg) -> Vec<NumericRule> {
    let n = to_fixed(t.notional_usd);
    let r = (t.fx_rate * 1000.0).round() as i32;
    vec![
        NumericRule {
            value:     n,
            rule_kind: NumericRuleKind::MustBePositive as u32,
            param_a:   0,
            param_b:   0,
        },
        NumericRule {
            value:     n,
            rule_kind: NumericRuleKind::RangeCheck as u32,
            param_a:   100,
            param_b:   1_000_000_000,
        },
        NumericRule {
            value:     r,
            rule_kind: NumericRuleKind::MustBePositive as u32,
            param_a:   0,
            param_b:   0,
        },
        NumericRule {
            value:     n,
            rule_kind: NumericRuleKind::TaxCalc as u32,
            param_a:   t.withholding_bp,
            param_b:   0,
        },
        NumericRule {
            value:     n,
            rule_kind: NumericRuleKind::FxConvert as u32,
            param_a:   r,
            param_b:   0,
        },
    ]
}

fn main() {
    pollster::block_on(async {
        const N: usize = 1024;
        let batch = TradeBatch {
            legs: (0..N)
                .map(|i| TradeLeg {
                    client_ref:     format!("CL-{:06}", i),
                    product_code:   if i % 211 == 0 {
                        "BAD CODE!".into()
                    } else {
                        format!("FXSP{:04}", i % 10000)
                    },
                    notional_usd:   25_000.0 + (i % 200) as f64 * 500.0,
                    fx_rate:        1.05 + (i % 50) as f64 * 0.002,
                    withholding_bp: 1500,
                })
                .collect(),
        };

        println!("=== Rayon: {N} legs × string rules ===\n");
        let t_strings = std::time::Instant::now();
        let str_errors: Vec<String> = batch
            .legs
            .par_iter()
            .flat_map_iter(|leg| string_rules(leg).into_iter().flat_map(|b| b.apply()))
            .collect();
        println!("String checks in {:?}: {} failures", t_strings.elapsed(), str_errors.len());

        println!("\n=== GPU: one batch for all numeric rules ===\n");
        let mut all_numeric: Vec<NumericRule> = Vec::with_capacity(N * 5);
        for leg in &batch.legs {
            all_numeric.extend(numeric_rules_for_leg(leg));
        }

        let engine = GpuNumericEngine::new().await;
        let t_gpu = std::time::Instant::now();
        let outs = engine.run(&all_numeric);
        println!("GPU {:?} for {} rules", t_gpu.elapsed(), outs.len());

        let gpu_bad = outs
            .iter()
            .filter(|o| GpuErrorCode::from(o.error_code) != GpuErrorCode::Success)
            .count();
        println!("GPU non-success rows: {gpu_bad}");
    });
}
