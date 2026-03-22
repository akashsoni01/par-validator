# par-validator

Parallel validation for **payment / trading-style payloads**: run **string and identifier checks** on the CPU with **Rayon**, and pack **fixed-point numerics** (ranges, GST, FX conversion, simple interest) into **one WebGPU compute dispatch** per batch.

The crate is built around [`rust-key-paths`](https://crates.io/crates/rust-key-paths) for type-safe field access and [`wgpu`](https://crates.io/crates/wgpu) for portable GPU execution (Vulkan, Metal, DX12, etc.).

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
| [`hybrid_gpu`](par-validator/examples/hybrid_gpu.rs) | Small walkthrough: Rayon + GPU together |
| [`fintech_rayon_nested`](par-validator/examples/fintech_rayon_nested.rs) | **~4k** nested ISO-20022–style transfers → flatten to key paths → CPU-only validation |
| [`fintech_gpu_batch`](par-validator/examples/fintech_gpu_batch.rs) | **~12k** numeric rules (2k legs × 6 checks) in **one** GPU `run` |
| [`fintech_hybrid_batch`](par-validator/examples/fintech_hybrid_batch.rs) | **~1k** trade legs: Rayon on strings, single GPU batch for notionals / tax / FX |

The nested Rayon example keeps a **deep domain struct** (`BankParty`, charges, FX block) and projects it to a **flat** `#[derive(Kp)]` view for validation. That matches how `rust-key-paths` `KpType` integrates with this crate today; composed `.then()` chains use a different internal representation and are better handled via flattening or dynamic key paths upstream.

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
