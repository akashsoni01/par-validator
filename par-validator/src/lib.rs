//! Parallel validation helpers for **string-like** fields (CPU / Rayon) and **fixed-point
//! numerics** (GPU / wgpu).
//!
//! ## String rules
//! Use [`Rule`](crate::builder::Rule) from [`builder`] with **`bool` predicates** and a paired
//! error value `E` per rule (see [`builder::Rule::mandatory_rule`] / [`builder::Rule::rule`]).
//! Mandatory rules run **sequentially** and short-circuit; remaining rules run in parallel with
//! Rayon inside [`Rule::apply`](crate::builder::Rule::apply).
//!
//! ## Numeric rules
//! See [`gpu_numeric`] for [`GpuNumericEngine`](gpu_numeric::GpuNumericEngine), which batches
//! [`NumericRule`](gpu_numeric::NumericRule) rows in a single compute dispatch. Values cross the
//! CPU/GPU boundary as **i32 × 100** (no `f32`/`f64` in the shader).
//!
//! ## Examples
//! - `cargo run --example basics` — CPU-only starter  
//! - `cargo run --example hybrid_gpu` — small hybrid demo  
//! - `cargo run --example fintech_rayon_nested` — large nested batch, CPU only  
//! - `cargo run --example fintech_gpu_batch` — large numeric batch, GPU only  
//! - `cargo run --example fintech_hybrid_batch` — both layers on a trade batch  
//! - `cargo run --example rule_csv_catalog` — [`builder::Rule`] + CSV catalog  

#![forbid(unsafe_code)]

pub mod builder;
pub mod errors;
pub mod gpu_numeric;

pub use builder::Rule;
pub use errors::RuleBuilderError;
