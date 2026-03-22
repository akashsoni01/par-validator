# par-validator

Parallel validation for **payment / trading-style payloads**: run **string and identifier checks** on the CPU with **Rayon**, and pack **fixed-point numerics** (ranges, GST, FX conversion, simple interest) into **one WebGPU compute dispatch** per batch.

The crate is built around [`rust-key-paths`](https://crates.io/crates/rust-key-paths) for type-safe field access and [`wgpu`](https://crates.io/crates/wgpu) for portable GPU execution (Vulkan, Metal, DX12, etc.).

---

## Beginner’s guide

This section is for you if you are new to the project or to Rust crates that mix **CPU parallelism** and **GPU compute**.

### What you should already know

- **Rust basics**: `cargo`, `struct`, `enum`, `impl`, and reading example binaries under `examples/`.
- **Optional**: A little exposure to **shaders** or **WebGPU** helps for the numeric path, but you can treat `GpuNumericEngine` as a black box at first.

### What this crate does (in one minute)

1. **String-style validation** — You point at a field on a struct (via **key paths** from `#[derive(Kp)]`), attach rule functions, and call `apply`. **Mandatory** rules run one after another; if one fails, the rest of that builder’s mandatory chain is skipped. **Other** rules for the same field run **in parallel** with [Rayon](https://crates.io/crates/rayon).
2. **Numeric validation and math** — You fill a `Vec<NumericRule>` with **fixed-point** integers (amounts scaled **×100**). The GPU runs many rules in **one** dispatch and returns a `Vec<NumericOutput>` (error code + optional calculated value).

So: **texty things → CPU + Rayon**, **lots of fixed-point number rules → GPU batch**.

### Prerequisites

- **Rust** (stable, recent; edition 2024 is declared in `Cargo.toml` — use a toolchain that supports it).
- A **working GPU stack** for examples that call `GpuNumericEngine` (e.g. **Metal** on Apple Silicon, **Vulkan** on many Linux setups, **DX12** on Windows). If the GPU path fails, check drivers / OS support for wgpu.

### First run (step by step)

1. Clone the repo and enter the **crate** directory (the one that contains `Cargo.toml`):

   ```bash
   cd par-validator
   ```

2. Optional — **smallest** sample (CPU only, no GPU):

   ```bash
   cargo run --example basics
   ```

3. Run the small end-to-end demo (strings on the CPU, numbers on the GPU):

   ```bash
   cargo run --example hybrid_gpu
   ```

4. When that works, try a **CPU-only** large batch:

   ```bash
   cargo run --release --example fintech_rayon_nested
   ```

5. Then a **GPU-heavy** batch:

   ```bash
   cargo run --release --example fintech_gpu_batch
   ```

### Where to read the code (suggested order)

| Order | File | Why |
|-------|------|-----|
| 1 | [`examples/basics.rs`](par-validator/examples/basics.rs) | Minimal `RuleBuilder` only (no GPU) |
| 2 | [`examples/hybrid_gpu.rs`](par-validator/examples/hybrid_gpu.rs) | Short story: `RuleBuilder` + `GpuNumericEngine::run` |
| 3 | [`src/lib.rs`](par-validator/src/lib.rs) | `RuleBuilder` API and docs |
| 4 | [`src/gpu_numeric.rs`](par-validator/src/gpu_numeric.rs) | Rules, outputs, WGSL shader |
| 5 | [`examples/fintech_hybrid_batch.rs`](par-validator/examples/fintech_hybrid_batch.rs) | Bigger payload, both layers |

### Concepts cheat sheet

| Term | Meaning |
|------|---------|
| **Key path (`Kp`)** | Compile-time accessor to a field, e.g. `MyStruct::field_name()`, from `key-paths-derive`. |
| **`RuleBuilder`** | Holds a key path + list of rule `fn`s + optional root reference; `apply()` runs the rules. |
| **Mandatory rule** | Runs first, in order; first failure stops the rest of the mandatory list for that builder. |
| **`NumericRule`** | One row: value ×100, rule kind, two integer parameters (meaning depends on kind). |
| **`GpuNumericEngine::run`** | Uploads all rules, runs compute shader once, downloads results (blocking). |

### Flattening nested structs

Your **business** model can be deeply nested (parties, legs, charges). This crate’s `RuleBuilder::new` expects a **`KpType`** from plain `#[derive(Kp)]` fields. A common pattern (see `fintech_rayon_nested`) is a **flat “view” struct** that mirrors nested data for validation only.

### If something goes wrong

- **`cd` to `par-validator`** — Commands assume the crate root next to `src/` and `examples/`.
- **GPU panic / “no adapter”** — Environment may lack a supported backend; try another machine or update OS/GPU drivers.
- **Slower than expected** — Use `cargo run --release` for large examples and benchmarks.

---

## Features

| Layer | What | How |
|--------|------|-----|
| **Strings / IDs** | BIC-shaped data, IBAN-like lengths, UETR, product codes | `RuleBuilder` + `#[derive(Kp)]` + Rayon |
| **Numerics** | Range, %, tax in bps, FX ×1000 rate, interest | `GpuNumericEngine` + WGSL compute |

- **Mandatory rules** run **in order** and **short-circuit** on the first failure.
- **Other rules** for the same key path run **in parallel** inside `RuleBuilder::apply`.
- **GPU**: all values are **i32 scaled ×100** end-to-end in the shader (no `f32`/`f64` on the boundary), avoiding nondeterministic floats in validation math.

---

## Quick start

```bash
cd par-validator
cargo run --example hybrid_gpu
```

---

## Basic example (CPU only)

The smallest program uses **`RuleBuilder`** on a **`#[derive(Kp)]`** struct — **no GPU**, no `async`, so it is easy to copy into your own crate.

```bash
cargo run --example basics
```

Source: [`examples/basics.rs`](par-validator/examples/basics.rs). Core idea:

```rust
use key_paths_derive::Kp;
use par_validator::{RuleBuilder, RuleBuilderError};

#[derive(Kp)]
struct Payment {
    reference: String,
}

fn not_empty(r: Option<&String>) -> RuleBuilderError<String> {
    match r {
        None => RuleBuilderError::Fail("missing".into()),
        Some(s) if s.trim().is_empty() => RuleBuilderError::Fail("blank".into()),
        Some(_) => RuleBuilderError::Success,
    }
}

fn max_len_16(r: Option<&String>) -> RuleBuilderError<String> {
    match r {
        None => RuleBuilderError::Fail("missing".into()),
        Some(s) if s.len() > 16 => RuleBuilderError::Fail("too long".into()),
        Some(_) => RuleBuilderError::Success,
    }
}

let p = Payment { reference: "REF-001".into() };

let results = RuleBuilder::<Payment, String, String>::new(Payment::reference())
    .with_root(&p)
    .mandatory_rule(not_empty)
    .rule(max_len_16)
    .apply();
```

- **`mandatory_rule`** runs first; if it fails, you get a single-element `Vec` and parallel rules are skipped.
- **`rule`** adds checks that run **in parallel** with each other inside `apply()`.

---

Release builds are recommended for large batches:

```bash
cargo run --release --example fintech_rayon_nested
cargo run --release --example fintech_gpu_batch
cargo run --release --example fintech_hybrid_batch
```

---

## Examples

| Example | Focus |
|---------|--------|
| [`basics`](par-validator/examples/basics.rs) | **Starter:** one field, mandatory + parallel rules, **no GPU** |
| [`hybrid_gpu`](par-validator/examples/hybrid_gpu.rs) | Small walkthrough: Rayon + GPU together |
| [`fintech_rayon_nested`](par-validator/examples/fintech_rayon_nested.rs) | **~4k** nested ISO-20022–style transfers → flatten to key paths → CPU-only validation |
| [`fintech_gpu_batch`](par-validator/examples/fintech_gpu_batch.rs) | **~12k** numeric rules (2k legs × 6 checks) in **one** GPU `run` |
| [`fintech_hybrid_batch`](par-validator/examples/fintech_hybrid_batch.rs) | **~1k** trade legs: Rayon on strings, single GPU batch for notionals / tax / FX |

The nested Rayon example keeps a **deep domain struct** (`BankParty`, charges, FX block) and projects it to a **flat** `#[derive(Kp)]` view for validation. That matches how `rust-key-paths` `KpType` integrates with this crate today; composed `.then()` chains use a different internal representation and are better handled via flattening or dynamic key paths upstream.

---

## Benchmarks

Criterion benchmarks live in [`par-validator/benches/throughput.rs`](par-validator/benches/throughput.rs).

```bash
cd par-validator
cargo bench --bench throughput
```

Run **only** the large GPU case (handy on a Windows/Linux **NVIDIA** box with Vulkan or DX12):

```bash
cargo bench --bench throughput -- nvidia_gpu
```

### Results (reference machine)

| Benchmark | What it measures | Typical time (M1 Air) |
|-----------|------------------|------------------------|
| `rayon_cpu_4096_transfers_x12_builders` | 4 096 flat transfers × 12 `RuleBuilder` runs (Rayon over rows + inner `par_iter` in `apply`) | **~2.86 ms** |
| `wgpu_gpu/12288_numeric_rules_one_dispatch` | 12 288 `NumericRule` rows in one `GpuNumericEngine::run` | **~1.53 ms** |
| `wgpu_gpu_nvidia/nvidia_gpu_98304_numeric_rules_one_dispatch` | **98 304** rules (16 384 legs × 6), one dispatch — **stress size for discrete GPUs** | **~3.44 ms** |

**Testing machine:** MacBook Air **M1**, Apple Silicon (**arm64**), macOS, Rust **1.85**, `cargo bench` release profile. Figures are Criterion medians from a single run; variance and outliers are normal—re-run locally for your hardware.

**NVIDIA:** On a PC with an NVIDIA GPU, install current drivers, use the same `cargo bench --bench throughput -- nvidia_gpu` command, and compare the reported time to the M1 row above (wgpu will typically use **Vulkan** or **DX12**).

---

## Library layout

- **`RuleBuilder`** — CPU validation keyed by [`KpType`](https://docs.rs/rust-key-paths/latest/rust_key_paths/type.KpType.html).
- **`gpu_numeric`** — `NumericRule` / `NumericOutput`, `GpuNumericEngine::run`, WGSL shader (portable wide integer math for Metal).

Full API docs:

```bash
cargo doc --open -p par-validator
```

---

## Dependencies

- [`rayon`](https://crates.io/crates/rayon), [`rust-key-paths`](https://crates.io/crates/rust-key-paths), [`wgpu`](https://crates.io/crates/wgpu), [`bytemuck`](https://crates.io/crates/bytemuck), [`pollster`](https://crates.io/crates/pollster) (examples / blocking on async GPU init).

Dev-only: [`key-paths-derive`](https://crates.io/crates/key-paths-derive) for `#[derive(Kp)]` in examples.

---

## License

See the repository `LICENSE` file.
