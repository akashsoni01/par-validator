//! **CPU / Rayon** — validate thousands of nested ISO-20022–style credit transfers.
//!
//! The domain model is **nested** (`BankParty` inside `CreditTransfer`, charges, reporting,
//! FX metadata). For [`RuleBuilder`] we use a **flat key-path view** (`CreditTransferFlat`)
//! derived from each nested value: `rust-key-paths`’ [`KpType`] (used by this crate) only covers
//! `derive(Kp)` fields; composed `.then()` paths use a different internal representation. Flattening
//! is a common pattern before static validation.
//!
//! Parallelism: Rayon over the batch, and each [`RuleBuilder::apply`] parallelizes non-mandatory
//! rules internally.
//!
//! Run: `cargo run --release --example fintech_rayon_nested`

use key_paths_derive::Kp;
use par_validator::{RuleBuilder, RuleBuilderError};
use rayon::prelude::*;

type StrErr = RuleBuilderError<String>;

fn non_blank(r: Option<&String>) -> StrErr {
    if r.map_or(true, |s| s.trim().is_empty()) {
        StrErr::Fail("must not be blank".into())
    } else {
        StrErr::Success
    }
}

fn max_len_35(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("missing".into()),
        Some(s) if s.len() > 35 => StrErr::Fail(format!("len {} > 35", s.len())),
        Some(_) => StrErr::Success,
    }
}

fn max_len_10(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("missing".into()),
        Some(s) if s.len() > 10 => StrErr::Fail(format!("len {} > 10", s.len())),
        Some(_) => StrErr::Success,
    }
}

fn max_len_16(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("missing".into()),
        Some(s) if s.len() > 16 => StrErr::Fail(format!("len {} > 16", s.len())),
        Some(_) => StrErr::Success,
    }
}

fn max_len_20(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("missing".into()),
        Some(s) if s.len() > 20 => StrErr::Fail(format!("len {} > 20", s.len())),
        Some(_) => StrErr::Success,
    }
}

fn bic11(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("BIC missing".into()),
        Some(s) => {
            let t = s.trim();
            if t.len() != 11 {
                StrErr::Fail(format!("BIC must be 11 chars, got {}", t.len()))
            } else if !t.chars().take(8).all(|c| c.is_ascii_alphanumeric())
                || !t.chars().skip(8).take(3).all(|c| c.is_ascii_alphanumeric())
            {
                StrErr::Fail("BIC must be alphanumeric".into())
            } else {
                StrErr::Success
            }
        },
    }
}

fn iban_like(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("IBAN missing".into()),
        Some(s) => {
            let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
            let n = t.len();
            if !(15..=34).contains(&n) {
                StrErr::Fail(format!("IBAN length {n} not in 15..=34"))
            } else if !t.chars().all(|c| c.is_ascii_alphanumeric()) {
                StrErr::Fail("IBAN must be alphanumeric".into())
            } else {
                StrErr::Success
            }
        },
    }
}

fn uetr_shape(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("UETR missing".into()),
        Some(s) => {
            let t = s.trim();
            if t.len() != 36 {
                StrErr::Fail(format!("UETR must be 36 chars (UUID), got {}", t.len()))
            } else {
                StrErr::Success
            }
        },
    }
}

fn charge_code(r: Option<&String>) -> StrErr {
    match r {
        None => StrErr::Fail("charge bearer missing".into()),
        Some(s) => {
            let u = s.to_ascii_uppercase();
            if matches!(u.as_str(), "DEBT" | "CRED" | "SHAR" | "SLEV") {
                StrErr::Success
            } else {
                StrErr::Fail(format!("invalid charge bearer '{s}'"))
            }
        },
    }
}

// ── Nested domain payload (not `Kp` — mirrors ISO 20022 grouping) ───────────

#[derive(Clone)]
struct BankParty {
    bic11: String,
    iban:  String,
    lei:   String,
}

#[derive(Clone)]
struct RegulatoryReporting {
    rtrn: String,
}

#[derive(Clone)]
struct FxLegDetails {
    contract_id: String,
    /// Carried for realism; numeric checks run in GPU examples.
    #[allow(dead_code)]
    rate_scaled: f64,
    #[allow(dead_code)]
    notional:    f64,
}

#[derive(Clone)]
struct ChargeBearerBlock {
    code: String,
}

#[derive(Clone)]
struct CreditTransferNested {
    instruction_id: String,
    uetr:             String,
    value_date:       String,
    debtor:           BankParty,
    creditor:         BankParty,
    charges:          ChargeBearerBlock,
    reg:              RegulatoryReporting,
    fx:               FxLegDetails,
}

/// Flat projection used with `#[derive(Kp)]` and [`RuleBuilder`].
#[derive(Kp, Clone)]
struct CreditTransferFlat {
    instruction_id:   String,
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

impl From<&CreditTransferNested> for CreditTransferFlat {
    fn from(ct: &CreditTransferNested) -> Self {
        Self {
            instruction_id: ct.instruction_id.clone(),
            uetr:             ct.uetr.clone(),
            value_date:       ct.value_date.clone(),
            debtor_bic11:     ct.debtor.bic11.clone(),
            debtor_iban:      ct.debtor.iban.clone(),
            debtor_lei:       ct.debtor.lei.clone(),
            creditor_bic11:   ct.creditor.bic11.clone(),
            creditor_iban:    ct.creditor.iban.clone(),
            creditor_lei:     ct.creditor.lei.clone(),
            charge_code:      ct.charges.code.clone(),
            rtrn:             ct.reg.rtrn.clone(),
            fx_contract_id:   ct.fx.contract_id.clone(),
        }
    }
}

#[derive(Clone)]
struct SettlementBatch {
    #[allow(dead_code)]
    grphdr_msg_id: String,
    transfers:     Vec<CreditTransferNested>,
}

fn synthetic_party(seed: usize, corrupt_bic: bool) -> BankParty {
    let base = 1000 + (seed % 9000);
    let bic = if corrupt_bic {
        format!("BAD{base}")
    } else {
        format!("DEUTDEFF{base:03}")
    };
    BankParty {
        bic11: bic,
        iban:  format!("DE893704004405320{}00", base % 10000),
        lei:   format!("529900{base:010}"),
    }
}

fn synthetic_transfer(idx: usize) -> CreditTransferNested {
    let inject_bic_fail = idx % 317 == 0;
    let inject_uetr_fail = idx % 509 == 0;
    CreditTransferNested {
        instruction_id: format!("INSTR-{:08}", idx),
        uetr: if inject_uetr_fail {
            "short".into()
        } else {
            format!("{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}", idx, idx, idx % 1000, idx % 65535, idx)
        },
        value_date: "2025-03-22".into(),
        debtor:     synthetic_party(idx, false),
        creditor:   synthetic_party(idx + 1, inject_bic_fail),
        charges:    ChargeBearerBlock { code: if idx % 401 == 0 { "FREE".into() } else { "SHAR".into() } },
        reg:        RegulatoryReporting {
            rtrn: format!("{:02}/{}", idx % 100, if idx % 2 == 0 { "CRED" } else { "DEBT" }),
        },
        fx: FxLegDetails {
            contract_id: format!("FX-{:06}", idx % 100_000),
            rate_scaled: 1.0825 * (1.0 + (idx % 7) as f64 * 0.001),
            notional:    50_000.0 + (idx % 1000) as f64 * 125.0,
        },
    }
}

fn flat_builders<'a>(
    f: &'a CreditTransferFlat,
) -> Vec<RuleBuilder<'a, CreditTransferFlat, String, String>> {
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

fn main() {
    const N: usize = 4096;
    let batch = SettlementBatch {
        grphdr_msg_id: "BATCH-2025-03-22-001".into(),
        transfers:     (0..N).map(synthetic_transfer).collect(),
    };

    println!("Rayon validation of {N} nested credit transfers (flattened to key paths, 12 builders each)…\n");

    let t0 = std::time::Instant::now();
    let errors: Vec<StrErr> = batch
        .transfers
        .par_iter()
        .flat_map_iter(|ct| {
            let flat = CreditTransferFlat::from(ct);
            flat_builders(&flat)
                .into_iter()
                .flat_map(|b| b.apply())
                .collect::<Vec<_>>()
        })
        .collect();
    let elapsed = t0.elapsed();

    let fails = errors.iter().filter(|e| **e != StrErr::Success).count();
    println!("Finished in {elapsed:?}");
    println!("Total rule outcomes: {}", errors.len());
    println!("Failures: {fails}");
    println!("Successes: {}", errors.len() - fails);

    println!("\nSample failure messages:");
    for e in errors.iter().filter(|e| **e != StrErr::Success).take(8) {
        println!("  {e:?}");
    }
}
