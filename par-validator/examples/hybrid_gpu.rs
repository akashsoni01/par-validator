//! String rules on Rayon; numeric rules on wgpu (see `gpu_numeric`).
// cargo run --example hybrid_gpu
use key_paths_derive::Kp;
use par_validator::gpu_numeric::{
    GpuErrorCode, GpuNumericEngine, NumericOutput, NumericRule, NumericRuleKind,
};
use par_validator::{RuleBuilder, RuleBuilderError};
use rayon::prelude::*;

mod iso_pain {
    use par_validator::RuleBuilderError;
    type IsoError = RuleBuilderError<String>;

    pub fn iso123rule(r: Option<&String>) -> IsoError {
        if r.map_or(true, |s| s.trim().is_empty()) {
            IsoError::Fail("iso123: blank or missing".into())
        } else {
            IsoError::Success
        }
    }

    pub fn max_len_35(r: Option<&String>) -> IsoError {
        match r {
            None    => IsoError::Fail("max_len_35: missing".into()),
            Some(s) => {
                if s.len() > 35 {
                    IsoError::Fail(format!("max_len_35: len={} > 35", s.len()))
                } else {
                    IsoError::Success
                }
            },
        }
    }

    pub fn alpha_num_only(r: Option<&String>) -> IsoError {
        match r {
            None => IsoError::Fail("alpha_num_only: missing".into()),
            Some(s) => {
                if s.chars().all(|c| c.is_alphanumeric()) {
                    IsoError::Success
                } else {
                    IsoError::Fail(format!("alpha_num_only: invalid chars in '{s}'"))
                }
            },
        }
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
        println!("=== String Validation (rayon par_iter) ===\n");

        let p = Payment {
            reference: "REF001".into(),
            currency:  "  ".into(),
        };

        let string_builders = [
            RuleBuilder::<Payment, String, String>::new(Payment::reference())
                .with_root(&p)
                .mandatory_rule(iso_pain::iso123rule)
                .rule(iso_pain::max_len_35)
                .rule(iso_pain::alpha_num_only),
            RuleBuilder::<Payment, String, String>::new(Payment::currency())
                .with_root(&p)
                .mandatory_rule(iso_pain::iso123rule)
                .rule(iso_pain::max_len_35)
                .rule(iso_pain::alpha_num_only),
        ];

        let string_errors: Vec<RuleBuilderError<String>> =
            string_builders.par_iter().flat_map(|b| b.apply()).collect();

        for e in &string_errors {
            println!("  {e:?}");
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
