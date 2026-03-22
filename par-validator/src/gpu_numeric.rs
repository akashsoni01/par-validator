//! Fixed-point numeric validation and calculation on the GPU via wgpu.
//!
//! WGSL uses portable `u32` wide multiply/divide (no `i64`), which Metal’s Naga
//! path does not accept. **Interest** uses divisor `365 * 10_000` so that values
//! scaled ×100 with rate in basis points match simple interest in the same scale
//! as `principal * rate_bp * days / (365 * 10000)` in fixed-point arithmetic.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Error codes returned per row from the compute shader (mirrors WGSL constants).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuErrorCode {
    /// Rule passed (calculation rows may still set [`NumericOutput::calc_value`]).
    Success        = 0,
    /// Value outside an inclusive `[param_a, param_b]` range (scaled ×100).
    RangeError     = 1,
    /// Failed [`NumericRuleKind::MustBePositive`].
    NotPositive    = 2,
    /// Reserved for overflow signalling in future rule kinds.
    Overflow       = 3,
    /// FX path with `param_a == 0` (rate ×1000).
    DivByZero      = 4,
    /// [`NumericRuleKind::MaxPrecision`] — extra decimal places vs allowed scale.
    PrecisionError = 5,
}

impl From<u32> for GpuErrorCode {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::Success,
            1 => Self::RangeError,
            2 => Self::NotPositive,
            3 => Self::Overflow,
            4 => Self::DivByZero,
            5 => Self::PrecisionError,
            _ => Self::RangeError,
        }
    }
}

/// Discriminant for [`NumericRule::rule_kind`] (must stay in sync with WGSL).
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum NumericRuleKind {
    /// Inclusive bounds: `param_a`..=`param_b` (all ×100).
    RangeCheck     = 0,
    /// `param_a == 0` → strict `> 0`; `param_a == 1` → `>= 0`.
    MustBePositive = 1,
    /// `param_a` = max decimal places (0, 1, or 2) for a value already scaled ×100.
    MaxPrecision   = 2,
    /// Writes `clamp(value, param_a, param_b)` to [`NumericOutput::calc_value`]; [`GpuErrorCode::RangeError`] if out of range.
    Clamp          = 3,
    /// Percentage 0.00–100.00 → value in `0..=10000`.
    Percentage     = 4,
    /// `calc = value * param_a / 10000` with `param_a` in basis points.
    TaxCalc        = 5,
    /// `calc = value * param_a / 1000` with `param_a` = rate×1000.
    FxConvert      = 6,
    /// `principal×100 * rate_bp * days / (365 * 10000)` → [`NumericOutput::calc_value`] ×100.
    InterestCalc   = 7,
}

/// One row of input for the numeric compute shader (fixed-point ×100 on the wire).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct NumericRule {
    /// Input scalar ×100 (e.g. `12.50` → `1250`).
    pub value:     i32,
    pub rule_kind: u32,
    /// Meaning depends on [`NumericRuleKind`].
    pub param_a:   i32,
    pub param_b:   i32,
}

/// One output row per [`NumericRule`] (16-byte stride for WGSL `storage` arrays).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct NumericOutput {
    /// [`GpuErrorCode`] as `u32`.
    pub error_code: u32,
    /// Computed value ×100 for calculation rules; `0` for pure validation rows.
    pub calc_value: i32,
    _pad0:          u32,
    _pad1:          u32,
}

/// Owns a wgpu device, queue, and compiled numeric compute pipeline.
///
/// Create once (async), then call [`GpuNumericEngine::run`] for each batch of [`NumericRule`].
pub struct GpuNumericEngine {
    device:   wgpu::Device,
    queue:    wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
}

impl GpuNumericEngine {
    /// Requests the default adapter and builds the numeric compute pipeline.
    ///
    /// # Panics
    /// Panics if no adapter or device is available (appropriate for examples; production code
    /// should handle errors).
    pub async fn new() -> Self {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("no GPU adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .expect("device creation failed");

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("numeric_shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label:               Some("numeric_pipeline"),
            layout:              None,
            module:              &shader,
            entry_point:         "main",
            compilation_options: Default::default(),
            cache:               None,
        });

        Self { device, queue, pipeline }
    }

    /// Uploads `rules`, dispatches `ceil(n / 64)` workgroups, copies results back, and returns one
    /// [`NumericOutput`] per input row (blocking until the GPU finishes).
    pub fn run(&self, rules: &[NumericRule]) -> Vec<NumericOutput> {
        if rules.is_empty() {
            return vec![];
        }
        let device = &self.device;
        let queue  = &self.queue;
        let n      = rules.len() as u64;

        let rule_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("numeric_rules"),
            contents: bytemuck::cast_slice(rules),
            usage:    wgpu::BufferUsages::STORAGE,
        });

        let out_stride = std::mem::size_of::<NumericOutput>() as u64;
        let out_size   = n * out_stride;
        let output_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("numeric_output"),
            size:               out_size,
            usage:              wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("numeric_readback"),
            size:               out_size,
            usage:              wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let bgl = self.pipeline.get_bind_group_layout(0);
        let bg  = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:  Some("numeric_bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: rule_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: output_buf.as_entire_binding() },
            ],
        });

        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups((n as u32).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output_buf, 0, &readback_buf, 0, out_size);
        queue.submit(Some(encoder.finish()));

        let slice = readback_buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::Maintain::Wait);

        let data   = slice.get_mapped_range();
        let result = bytemuck::cast_slice::<u8, NumericOutput>(&data).to_vec();
        drop(data);
        readback_buf.unmap();
        result
    }
}

const SHADER_SRC: &str = r#"
const SUCCESS        : u32 = 0u;
const RANGE_ERROR    : u32 = 1u;
const NOT_POSITIVE   : u32 = 2u;
const OVERFLOW       : u32 = 3u;
const DIV_BY_ZERO    : u32 = 4u;
const PRECISION_ERROR: u32 = 5u;

const RANGE_CHECK    : u32 = 0u;
const MUST_BE_POS    : u32 = 1u;
const MAX_PRECISION  : u32 = 2u;
const CLAMP          : u32 = 3u;
const PERCENTAGE     : u32 = 4u;
const TAX_CALC       : u32 = 5u;
const FX_CONVERT     : u32 = 6u;
const INTEREST_CALC  : u32 = 7u;

struct NumericRule   { value: i32, rule_kind: u32, param_a: i32, param_b: i32 }
struct NumericOutput { error_code: u32, calc_value: i32, _pad0: u32, _pad1: u32 }

@group(0) @binding(0) var<storage, read>       rules  : array<NumericRule>;
@group(0) @binding(1) var<storage, read_write> output : array<NumericOutput>;

// Portable wide math (Metal / Naga do not support i64 in WGSL here).
fn umul32(a: u32, b: u32) -> vec2<u32> {
    let a0 = a & 0xFFFFu;
    let a1 = a >> 16u;
    let b0 = b & 0xFFFFu;
    let b1 = b >> 16u;
    let p00 = a0 * b0;
    let p01 = a0 * b1;
    let p10 = a1 * b0;
    let p11 = a1 * b1;
    let mid = (p00 >> 16u) + (p01 & 0xFFFFu) + (p10 & 0xFFFFu);
    let lo = (p00 & 0xFFFFu) | ((mid & 0xFFFFu) << 16u);
    let hi = p11 + (p01 >> 16u) + (p10 >> 16u) + (mid >> 16u);
    return vec2(lo, hi);
}

fn abs_i32_u32(x: i32) -> u32 {
    return u32(select(x, -x, x < 0));
}

fn div_u64_u32(n_lo: u32, n_hi: u32, den: u32) -> u32 {
    if n_hi == 0u {
        return n_lo / den;
    }
    var r_lo = 0u;
    var r_hi = 0u;
    var q_lo = 0u;
    var q_hi = 0u;
    for (var i = 0u; i < 64u; i++) {
        let k = 63u - i;
        let bit = select((n_hi >> (k - 32u)) & 1u, (n_lo >> k) & 1u, k < 32u);
        let nl = (r_lo << 1u) | bit;
        let nh = (r_hi << 1u) | (r_lo >> 31u);
        r_lo = nl;
        r_hi = nh;
        q_lo = (q_lo << 1u) | (q_hi >> 31u);
        q_hi = q_hi << 1u;
        if r_hi > 0u || r_lo >= den {
            if r_lo >= den {
                r_lo = r_lo - den;
            } else {
                r_hi = r_hi - 1u;
                r_lo = r_lo - den;
            }
            q_lo = q_lo | 1u;
        }
    }
    return q_lo;
}

fn mul64xu32(lo: u32, hi: u32, m: u32) -> vec3<u32> {
    let t = umul32(lo, m);
    let u = umul32(hi, m);
    let mid = t.y + u.x;
    let c1 = u32(mid < t.y);
    return vec3(t.x, mid, u.y + c1);
}

fn sub96_u32(a: vec3<u32>, b: vec3<u32>) -> vec3<u32> {
    let l = a.x - b.x;
    let c0 = u32(a.x < b.x);
    let m = a.y - b.y - c0;
    let c1 = u32(a.y < b.y) | u32(a.y == b.y && c0 == 1u);
    let h = a.z - b.z - c1;
    return vec3(l, m, h);
}

fn ge96_u32(a: vec3<u32>, b: vec3<u32>) -> bool {
    if a.z != b.z { return a.z > b.z; }
    if a.y != b.y { return a.y > b.y; }
    return a.x >= b.x;
}

fn div_u96_u32(l: u32, m: u32, h: u32, den: u32) -> u32 {
    var r = vec3(0u, 0u, 0u);
    var q_lo = 0u;
    var q_hi = 0u;
    for (var i = 0u; i < 96u; i++) {
        let k = 95u - i;
        var bit = 0u;
        if k < 32u { bit = (l >> k) & 1u; }
        else if k < 64u { bit = (m >> (k - 32u)) & 1u; }
        else { bit = (h >> (k - 64u)) & 1u; }
        let nx = (r.x << 1u) | bit;
        let ny = (r.y << 1u) | (r.x >> 31u);
        let nz = (r.z << 1u) | (r.y >> 31u);
        r = vec3(nx, ny, nz);
        q_lo = (q_lo << 1u) | (q_hi >> 31u);
        q_hi = q_hi << 1u;
        let d3 = vec3(den, 0u, 0u);
        if ge96_u32(r, d3) {
            r = sub96_u32(r, d3);
            q_lo = q_lo | 1u;
        }
    }
    return q_lo;
}

fn signed_mul_div2(a: i32, b: i32, den: u32) -> i32 {
    let neg = (a < 0) != (b < 0);
    let ua = abs_i32_u32(a);
    let ub = abs_i32_u32(b);
    let p = umul32(ua, ub);
    let q = div_u64_u32(p.x, p.y, den);
    return select(i32(q), -i32(q), neg);
}

fn signed_mul_div3_nonneg(a: i32, b: i32, c: i32, den: u32) -> i32 {
    let ua = abs_i32_u32(a);
    let ub = abs_i32_u32(b);
    let uc = abs_i32_u32(c);
    let p = umul32(ua, ub);
    let t = mul64xu32(p.x, p.y, uc);
    return i32(div_u96_u32(t.x, t.y, t.z, den));
}

const INTEREST_DENOM: u32 = 3650000u;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if idx >= arrayLength(&rules) { return; }

    let r = rules[idx];
    var err  : u32 = SUCCESS;
    var calc : i32 = 0;

    switch r.rule_kind {

        case RANGE_CHECK: {
            if r.value < r.param_a || r.value > r.param_b {
                err = RANGE_ERROR;
            }
        }

        case MUST_BE_POS: {
            let ok = select(r.value > 0, r.value >= 0, r.param_a == 1);
            if !ok { err = NOT_POSITIVE; }
        }

        case MAX_PRECISION: {
            let divisor = select(select(100, 10, r.param_a == 1), 1, r.param_a >= 2);
            if r.value % divisor != 0 { err = PRECISION_ERROR; }
        }

        case CLAMP: {
            calc = clamp(r.value, r.param_a, r.param_b);
            if r.value != calc { err = RANGE_ERROR; }
        }

        case PERCENTAGE: {
            if r.value < 0 || r.value > 10000 { err = RANGE_ERROR; }
        }

        case TAX_CALC: {
            calc = signed_mul_div2(r.value, r.param_a, 10000u);
        }

        case FX_CONVERT: {
            if r.param_a == 0 { err = DIV_BY_ZERO; }
            else {
                calc = signed_mul_div2(r.value, r.param_a, 1000u);
            }
        }

        case INTEREST_CALC: {
            calc = signed_mul_div3_nonneg(r.value, r.param_a, r.param_b, INTEREST_DENOM);
        }

        default: { err = RANGE_ERROR; }
    }

    output[idx] = NumericOutput(err, calc, 0u, 0u);
}
"#;
