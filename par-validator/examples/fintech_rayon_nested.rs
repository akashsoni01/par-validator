//! **CPU / Rayon** — validate thousands of nested ISO-20022–style credit transfers.
//!
//! The domain model is **nested** (`BankParty` inside `CreditTransfer`, charges, reporting,
//! FX metadata). For validation we use a **flat key-path view** (`CreditTransferFlat`) with
//! [`par_validator::Rule`] (`bool` predicates + error strings).
//!
//! Parallelism: Rayon over the batch, and each [`Rule::apply`] parallelizes non-mandatory rules.
//!
//! Run: `cargo run --release --example fintech_rayon_nested`

use key_paths_derive::Kp;
use par_validator::Rule;
use rayon::prelude::*;

fn non_blank_ok(r: Option<&String>) -> bool {
    !r.map_or(true, |s| s.trim().is_empty())
}

fn max_len_35_ok(r: Option<&String>) -> bool {
    r.map(|s| s.len() <= 35).unwrap_or(false)
}

fn max_len_10_ok(r: Option<&String>) -> bool {
    r.map(|s| s.len() <= 10).unwrap_or(false)
}

fn max_len_16_ok(r: Option<&String>) -> bool {
    r.map(|s| s.len() <= 16).unwrap_or(false)
}

fn max_len_20_ok(r: Option<&String>) -> bool {
    r.map(|s| s.len() <= 20).unwrap_or(false)
}

fn bic11_ok(r: Option<&String>) -> bool {
    match r {
        None => false,
        Some(s) => {
            let t = s.trim();
            t.len() == 11
                && t.chars().take(8).all(|c| c.is_ascii_alphanumeric())
                && t.chars().skip(8).take(3).all(|c| c.is_ascii_alphanumeric())
        },
    }
}

fn iban_like_ok(r: Option<&String>) -> bool {
    match r {
        None => false,
        Some(s) => {
            let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
            (15..=34).contains(&t.len()) && t.chars().all(|c| c.is_ascii_alphanumeric())
        },
    }
}

fn uetr_shape_ok(r: Option<&String>) -> bool {
    r.map(|s| s.trim().len() == 36).unwrap_or(false)
}

fn charge_code_ok(r: Option<&String>) -> bool {
    match r {
        None => false,
        Some(s) => {
            let u = s.to_ascii_uppercase();
            matches!(u.as_str(), "DEBT" | "CRED" | "SHAR" | "SLEV")
        },
    }
}

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
) -> Vec<Rule<'a, CreditTransferFlat, String, String>> {
    vec![
        Rule::new(CreditTransferFlat::instruction_id())
            .with_root(f)
            .mandatory_rule(non_blank_ok, "instruction_id: blank".into())
            .rule(max_len_35_ok, "instruction_id: max_len_35".into()),
        Rule::new(CreditTransferFlat::uetr())
            .with_root(f)
            .mandatory_rule(non_blank_ok, "uetr: blank".into())
            .rule(uetr_shape_ok, "uetr: shape".into()),
        Rule::new(CreditTransferFlat::value_date())
            .with_root(f)
            .mandatory_rule(non_blank_ok, "value_date: blank".into())
            .rule(max_len_10_ok, "value_date: max_len_10".into()),
        Rule::new(CreditTransferFlat::debtor_bic11())
            .with_root(f)
            .mandatory_rule(non_blank_ok, "debtor_bic11: blank".into())
            .rule(bic11_ok, "debtor_bic11: bic11".into()),
        Rule::new(CreditTransferFlat::debtor_iban())
            .with_root(f)
            .mandatory_rule(non_blank_ok, "debtor_iban: blank".into())
            .rule(iban_like_ok, "debtor_iban: iban".into()),
        Rule::new(CreditTransferFlat::debtor_lei())
            .with_root(f)
            .mandatory_rule(non_blank_ok, "debtor_lei: blank".into())
            .rule(max_len_20_ok, "debtor_lei: max_len_20".into()),
        Rule::new(CreditTransferFlat::creditor_bic11())
            .with_root(f)
            .mandatory_rule(non_blank_ok, "creditor_bic11: blank".into())
            .rule(bic11_ok, "creditor_bic11: bic11".into()),
        Rule::new(CreditTransferFlat::creditor_iban())
            .with_root(f)
            .mandatory_rule(non_blank_ok, "creditor_iban: blank".into())
            .rule(iban_like_ok, "creditor_iban: iban".into()),
        Rule::new(CreditTransferFlat::creditor_lei())
            .with_root(f)
            .mandatory_rule(non_blank_ok, "creditor_lei: blank".into())
            .rule(max_len_20_ok, "creditor_lei: max_len_20".into()),
        Rule::new(CreditTransferFlat::charge_code())
            .with_root(f)
            .mandatory_rule(non_blank_ok, "charge_code: blank".into())
            .rule(charge_code_ok, "charge_code: invalid".into()),
        Rule::new(CreditTransferFlat::rtrn())
            .with_root(f)
            .mandatory_rule(non_blank_ok, "rtrn: blank".into())
            .rule(max_len_35_ok, "rtrn: max_len_35".into()),
        Rule::new(CreditTransferFlat::fx_contract_id())
            .with_root(f)
            .mandatory_rule(non_blank_ok, "fx_contract_id: blank".into())
            .rule(max_len_16_ok, "fx_contract_id: max_len_16".into()),
    ]
}

fn main() {
    const N: usize = 4096;
    let batch = SettlementBatch {
        grphdr_msg_id: "BATCH-2025-03-22-001".into(),
        transfers:     (0..N).map(synthetic_transfer).collect(),
    };

    println!("Rayon validation of {N} nested credit transfers (flattened to key paths, 12 Rules each)…\n");

    let t0 = std::time::Instant::now();
    let failures: Vec<String> = batch
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

    println!("Finished in {elapsed:?}");
    println!("Total failure messages: {}", failures.len());
    println!("(Each message is one failed predicate; empty means all checks passed for that path.)");

    println!("\nSample failure messages:");
    for msg in failures.iter().take(8) {
        println!("  {msg}");
    }
}
