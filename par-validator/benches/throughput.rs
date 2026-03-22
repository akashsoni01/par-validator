//! Criterion throughput benches (CPU Rayon + GPU numeric batch).
//!
//! Run: `cargo bench --bench throughput`
//!
//! Extra **large GPU** benchmark (`nvidia_gpu_*`): **98 304** rules in one dispatch — useful when
//! you re-run benches on an **NVIDIA** machine (Vulkan/DX12) and compare to Apple Metal.
//! Filter with Criterion’s substring match, e.g. `cargo bench --bench throughput -- nvidia_gpu`.
// cd par-validator && cargo bench --bench throughput

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use key_paths_derive::Kp;
use par_validator::gpu_numeric::{GpuNumericEngine, NumericRule, NumericRuleKind};
use par_validator::{RuleBuilder, RuleBuilderError};
use rayon::prelude::*;

type StrErr = RuleBuilderError<String>;

fn non_blank(r: Option<&String>) -> StrErr {
    if r.map_or(true, |s| s.trim().is_empty()) {
        StrErr::Fail("blank".into())
    } else {
        StrErr::Success
    }
}

fn max_len_35(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("missing".into()),
        Some(s) if s.len() > 35 => StrErr::Fail("long".into()),
        Some(_) => StrErr::Success,
    }
}

fn max_len_10(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("missing".into()),
        Some(s) if s.len() > 10 => StrErr::Fail("long".into()),
        Some(_) => StrErr::Success,
    }
}

fn max_len_16(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("missing".into()),
        Some(s) if s.len() > 16 => StrErr::Fail("long".into()),
        Some(_) => StrErr::Success,
    }
}

fn max_len_20(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("missing".into()),
        Some(s) if s.len() > 20 => StrErr::Fail("long".into()),
        Some(_) => StrErr::Success,
    }
}

fn bic11(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("missing".into()),
        Some(s) => {
            let t = s.trim();
            if t.len() != 11 {
                StrErr::Fail("bic len".into())
            } else if !t.chars().all(|c| c.is_ascii_alphanumeric()) {
                StrErr::Fail("bic charset".into())
            } else {
                StrErr::Success
            }
        },
    }
}

fn iban_like(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("missing".into()),
        Some(s) => {
            let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
            if !(15..=34).contains(&t.len()) {
                StrErr::Fail("iban len".into())
            } else if !t.chars().all(|c| c.is_ascii_alphanumeric()) {
                StrErr::Fail("iban charset".into())
            } else {
                StrErr::Success
            }
        },
    }
}

fn uetr_shape(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("missing".into()),
        Some(s) if s.trim().len() != 36 => StrErr::Fail("uetr len".into()),
        Some(_) => StrErr::Success,
    }
}

fn charge_code(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("missing".into()),
        Some(s) => {
            let u = s.to_ascii_uppercase();
            if matches!(u.as_str(), "DEBT" | "CRED" | "SHAR" | "SLEV") {
                StrErr::Success
            } else {
                StrErr::Fail("charge".into())
            }
        },
    }
}

#[derive(Kp, Clone)]
struct CreditTransferFlat {
    instruction_id: String,
    uetr:             String,
    value_date:       String,
    debtor_bic11:     String,
    debtor_iban:      String,
    debtor_lei:       String,
    creditor_bic11:   String,
    creditor_iban:    String,
    creditor_lei:     String,
    charge_code:      String,
    rtrn:             String,
    fx_contract_id:   String,
}

fn synthetic_flat(idx: usize) -> CreditTransferFlat {
    let base = 1000 + (idx % 9000);
    CreditTransferFlat {
        instruction_id: format!("INSTR-{:08}", idx),
        uetr:             format!("{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}", idx, idx, idx % 1000, idx % 65535, idx),
        value_date:       "2025-03-22".into(),
        debtor_bic11:     format!("DEUTDEFF{base:03}"),
        debtor_iban:      format!("DE893704004405320{}00", base % 10000),
        debtor_lei:       format!("529900{base:010}"),
        creditor_bic11:   format!("DEUTDEFF{:03}", (base + 1) % 1000),
        creditor_iban:    format!("DE893704004405320{}00", (base + 3) % 10000),
        creditor_lei:     format!("529900{:010}", base + 7),
        charge_code:      "SHAR".into(),
        rtrn:             format!("{:02}/CRED", idx % 100),
        fx_contract_id:   format!("FX-{:06}", idx % 100_000),
    }
}

fn flat_builders<'a>(f: &'a CreditTransferFlat) -> Vec<RuleBuilder<'a, CreditTransferFlat, String, String>> {
    vec![
        RuleBuilder::new(CreditTransferFlat::instruction_id())
            .with_root(f)
            .mandatory_rule(non_blank)
            .rule(max_len_35),
        RuleBuilder::new(CreditTransferFlat::uetr())
            .with_root(f)
            .mandatory_rule(non_blank)
            .rule(uetr_shape),
        RuleBuilder::new(CreditTransferFlat::value_date())
            .with_root(f)
            .mandatory_rule(non_blank)
            .rule(max_len_10),
        RuleBuilder::new(CreditTransferFlat::debtor_bic11())
            .with_root(f)
            .mandatory_rule(non_blank)
            .rule(bic11),
        RuleBuilder::new(CreditTransferFlat::debtor_iban())
            .with_root(f)
            .mandatory_rule(non_blank)
            .rule(iban_like),
        RuleBuilder::new(CreditTransferFlat::debtor_lei())
            .with_root(f)
            .mandatory_rule(non_blank)
            .rule(max_len_20),
        RuleBuilder::new(CreditTransferFlat::creditor_bic11())
            .with_root(f)
            .mandatory_rule(non_blank)
            .rule(bic11),
        RuleBuilder::new(CreditTransferFlat::creditor_iban())
            .with_root(f)
            .mandatory_rule(non_blank)
            .rule(iban_like),
        RuleBuilder::new(CreditTransferFlat::creditor_lei())
            .with_root(f)
            .mandatory_rule(non_blank)
            .rule(max_len_20),
        RuleBuilder::new(CreditTransferFlat::charge_code())
            .with_root(f)
            .mandatory_rule(non_blank)
            .rule(charge_code),
        RuleBuilder::new(CreditTransferFlat::rtrn())
            .with_root(f)
            .mandatory_rule(non_blank)
            .rule(max_len_35),
        RuleBuilder::new(CreditTransferFlat::fx_contract_id())
            .with_root(f)
            .mandatory_rule(non_blank)
            .rule(max_len_16),
    ]
}

fn run_rayon_batch(rows: &[CreditTransferFlat]) {
    let _: Vec<StrErr> = rows
        .par_iter()
        .flat_map_iter(|row| {
            flat_builders(row)
                .into_iter()
                .flat_map(|b| b.apply())
                .collect::<Vec<_>>()
        })
        .collect();
}

fn bench_rayon_fintech_4k(c: &mut Criterion) {
    const N: usize = 4096;
    let rows: Vec<CreditTransferFlat> = (0..N).map(synthetic_flat).collect();

    c.bench_function("rayon_cpu_4096_transfers_x12_builders", |b| {
        b.iter(|| run_rayon_batch(black_box(rows.as_slice())))
    });
}

fn to_fixed(v: f64) -> i32 {
    (v * 100.0).round() as i32
}

fn build_gpu_rules(legs: usize) -> Vec<NumericRule> {
    let mut rules = Vec::with_capacity(legs * 6);
    for i in 0..legs {
        let usd = 10_000.0 + (i % 500) as f64 * 250.0;
        let rate = 83.0 + (i % 20) as f64 * 0.05;
        let amt = to_fixed(usd);
        let rate_i = (rate * 1000.0).round() as i32;
        rules.extend_from_slice(&[
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
                param_a:   1800,
                param_b:   0,
            },
            NumericRule {
                value:     amt,
                rule_kind: NumericRuleKind::FxConvert as u32,
                param_a:   rate_i,
                param_b:   0,
            },
        ]);
    }
    rules
}

fn bench_gpu_numeric_12k(c: &mut Criterion) {
    let rules = build_gpu_rules(2048);
    assert_eq!(rules.len(), 2048 * 6);

    let engine = pollster::block_on(GpuNumericEngine::new());

    let mut group = c.benchmark_group("wgpu_gpu");
    group.sample_size(30);
    group.bench_function("12288_numeric_rules_one_dispatch", |b| {
        b.iter(|| engine.run(black_box(rules.as_slice())))
    });
    group.finish();
}

/// Larger batch for **NVIDIA** (or any discrete GPU) throughput comparison — same rule mix, more rows.
const NVIDIA_STRESS_LEGS: usize = 16_384;

fn bench_gpu_numeric_nvidia_large(c: &mut Criterion) {
    let rules = build_gpu_rules(NVIDIA_STRESS_LEGS);
    assert_eq!(rules.len(), NVIDIA_STRESS_LEGS * 6);

    let engine = pollster::block_on(GpuNumericEngine::new());

    let mut group = c.benchmark_group("wgpu_gpu_nvidia");
    group.sample_size(15);
    group.measurement_time(std::time::Duration::from_secs(5));
    group.bench_function(
        "nvidia_gpu_98304_numeric_rules_one_dispatch",
        |b| b.iter(|| engine.run(black_box(rules.as_slice()))),
    );
    group.finish();
}

criterion_group!(
    benches,
    bench_rayon_fintech_4k,
    bench_gpu_numeric_12k,
    bench_gpu_numeric_nvidia_large
);
criterion_main!(benches);
