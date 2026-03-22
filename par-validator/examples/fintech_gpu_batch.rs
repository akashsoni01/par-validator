//! **GPU / wgpu** — one dispatch over tens of thousands of fixed-point numeric rules.
//!
//! Simulates a **treasury blotter**: many FX legs, each contributing several
//! [`NumericRule`](par_validator::gpu_numeric::NumericRule) rows (range, positivity, tax, conversion).
//! All rules share a single [`GpuNumericEngine::run`](par_validator::gpu_numeric::GpuNumericEngine::run)
//! call to amortize GPU submission and readback.
//!
//! Run: `cargo run --release --example fintech_gpu_batch`

use par_validator::gpu_numeric::{
    GpuErrorCode, GpuNumericEngine, NumericRule, NumericRuleKind,
};

fn to_fixed(v: f64) -> i32 {
    (v * 100.0).round() as i32
}

/// One synthetic deal leg (nested business object); expanded to GPU rows below.
#[derive(Clone)]
struct FxLeg {
    usd_notional: f64,
    rate_inr_per_usd: f64,
    gst_bp: i32,
}

fn rules_for_leg(leg: &FxLeg) -> [NumericRule; 6] {
    let amt = to_fixed(leg.usd_notional);
    let rate_i = (leg.rate_inr_per_usd * 1000.0).round() as i32;
    [
        NumericRule {
            value:     amt,
            rule_kind: NumericRuleKind::RangeCheck as u32,
            param_a:   1,
            param_b:   500_000_000,
        },
        NumericRule {
            value:     amt,
            rule_kind: NumericRuleKind::MustBePositive as u32,
            param_a:   0,
            param_b:   0,
        },
        NumericRule {
            value:     amt,
            rule_kind: NumericRuleKind::MaxPrecision as u32,
            param_a:   2,
            param_b:   0,
        },
        NumericRule {
            value:     rate_i,
            rule_kind: NumericRuleKind::MustBePositive as u32,
            param_a:   0,
            param_b:   0,
        },
        NumericRule {
            value:     amt,
            rule_kind: NumericRuleKind::TaxCalc as u32,
            param_a:   leg.gst_bp as i32,
            param_b:   0,
        },
        NumericRule {
            value:     amt,
            rule_kind: NumericRuleKind::FxConvert as u32,
            param_a:   rate_i,
            param_b:   0,
        },
    ]
}

fn main() {
    pollster::block_on(async {
        const LEGS: usize = 2048;
        let legs: Vec<FxLeg> = (0..LEGS)
            .map(|i| FxLeg {
                usd_notional:    10_000.0 + (i % 500) as f64 * 250.0,
                rate_inr_per_usd: 83.0 + (i % 20) as f64 * 0.05,
                gst_bp:          1800,
            })
            .collect();

        let mut rules: Vec<NumericRule> = Vec::with_capacity(LEGS * 6);
        for leg in &legs {
            rules.extend_from_slice(&rules_for_leg(leg));
        }

        println!(
            "GPU batch: {} legs × 6 rules = {} numeric checks in one dispatch…\n",
            LEGS,
            rules.len()
        );

        let engine = GpuNumericEngine::new().await;
        let t0 = std::time::Instant::now();
        let outs = engine.run(&rules);
        let elapsed = t0.elapsed();

        let ok = outs
            .iter()
            .filter(|o| GpuErrorCode::from(o.error_code) == GpuErrorCode::Success)
            .count();
        let bad = outs.len() - ok;

        println!("Finished in {elapsed:?}");
        println!("Outcomes: {} success, {} non-success", ok, bad);

        // Spot-check first leg’s GST + FX calc slots (indices 4 and 5)
        if outs.len() >= 6 {
            let gst = outs[4].calc_value as f64 / 100.0;
            let inr = outs[5].calc_value as f64 / 100.0;
            println!("\nFirst leg sample: GST calc ≈ {gst:.2}, INR notional ≈ {inr:.2}");
        }
    });
}
