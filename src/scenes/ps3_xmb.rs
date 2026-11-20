use crate::scenes::{
    create_blend_state, create_buffer, create_rasterizer, create_vs_ps, set_viewport,
    update_cbuffer, update_dynamic_slice, BlendMode, FrameInput, Scene, FULLSCREEN_VS_HLSL,
};
use std::f64::consts::PI;
use std::mem::size_of;
use windows::core::s;
use windows::Win32::Graphics::Direct3D::{
    D3D11_SRV_DIMENSION_TEXTURE2D, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
    D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP,
};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11BlendState, ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, ID3D11InputLayout,
    ID3D11PixelShader, ID3D11RasterizerState, ID3D11RenderTargetView, ID3D11SamplerState,
    ID3D11ShaderResourceView, ID3D11Texture2D, ID3D11VertexShader, D3D11_BIND_CONSTANT_BUFFER,
    D3D11_BIND_INDEX_BUFFER, D3D11_BIND_SHADER_RESOURCE, D3D11_BIND_VERTEX_BUFFER,
    D3D11_COMPARISON_NEVER, D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_INPUT_ELEMENT_DESC,
    D3D11_INPUT_PER_VERTEX_DATA, D3D11_SAMPLER_DESC, D3D11_SHADER_RESOURCE_VIEW_DESC,
    D3D11_SHADER_RESOURCE_VIEW_DESC_0, D3D11_SUBRESOURCE_DATA, D3D11_TEX2D_SRV,
    D3D11_TEXTURE2D_DESC, D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_USAGE_IMMUTABLE,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_R16_UINT, DXGI_FORMAT_R32G32B32A32_FLOAT, DXGI_FORMAT_R32G32_FLOAT,
    DXGI_FORMAT_R8_UNORM, DXGI_SAMPLE_DESC,
};

// =================================================================================================
// Direct port of PS3 spline.elf simulation from bungholesurfers/Nagare (native/scene/src/wave.rs)
// =================================================================================================

const N: usize = 19;
const COUNT: usize = N * N;
const STEP_HZ: f64 = 60.0;
const MAX_STEPS_PER_FRAME: f64 = 4.0;
const FRAME_HZ: f64 = 60.0;
const CLOCK_RATE: f64 = 0.0001;
const CLOCK_WRAP: f64 = 10.0;
const SMOOTHING: f64 = 0.1;
const PULL: f64 = 0.02;
const END_Y_FREQ: f64 = 11.0;
const END_Z_FREQ: f64 = 15.0;
const FAR_LENGTH: f64 = 2.0;
const FAR_TENSION: f64 = 10.0;
const SHAPE_REST: f64 = 1.3;
const SHAPE_REACH: f64 = 5.0;
const SHAPE_BELOW: f64 = 0.3;

const LATTICE: [usize; 3] = [8, 4, 4];
const LX: usize = 11;
const LYZ: usize = 7;
const LATTICE_TIME: f64 = 10.0;
const CROSS_STEP: f64 = 0.005;
const CELL_LIMIT: f64 = 0.999;

const SPANS: usize = 16;
const SAMPLES: usize = 8;
const MESH: usize = SPANS * SAMPLES; // 128
const VERTICES: usize = MESH * MESH; // 16,384
const RECORD: usize = 8;
const RESTART: u16 = 0xFFFF;

const CAM_EYE: [f64; 3] = [0.0, 0.0, 2.0];
const CAM_FOVY: f64 = 0.925025;
const CAM_NEAR: f64 = 0.1;
const CAM_FAR: f64 = 1000.0;

#[derive(Clone, Copy)]
struct LineParams {
    damping: f64,
    length: f64,
    tension: f64,
    timestep: f64,
    perturbation: f64,
    end_y: f64,
    end_z: f64,
}

// LINE1.mnu base parameters from nag_settings.rs
const BASE_LINE: LineParams = LineParams {
    damping: 0.0001,
    length: 0.306001,
    tension: 0.25,
    timestep: 4.0,
    perturbation: 0.0998587,
    end_y: 0.4,
    end_z: 0.2,
};

const START_LINE: LineParams = LineParams {
    timestep: 7.4,
    ..BASE_LINE
};

const START_SETTLE_STEPS: usize = 600;
const START_SETTLE_DAMPING: f64 = 100.0;
const START_NOISE: i32 = 2_000_006;
const START_CLOCK: f64 = 4.37234;
const START_MEAN_PLACE: [f64; 3] = [2.5976, 0.1683, 0.0947];
const START_MEAN_VELOCITY: [f64; 3] = [-0.512, -0.1, 0.198];

const POS_X: f64 = -7.67933;
const POS_Y: f64 = -1.08844;
const POS_Z: f64 = -6.40287;
const ANG_X: f64 = -0.994157;
const ANG_Y: f64 = 0.0867576;
const ANG_Z: f64 = 0.065364;
const ANG_ROT: f64 = 18.1208;
const FFD_SCALE1: [f64; 3] = [5.67726, 1.00077, 1.0];
const FFD_OFFSET: [f64; 3] = [0.0, -0.469999, 0.0];
const FFD_SCALE2: [f64; 3] = [2.82755, 1.27579, 2.88782];
const FFD_PARAM1: f64 = -1.33509;

fn noise(n: i32) -> f64 {
    let x = n.wrapping_shl(13) ^ n;
    let y = x
        .wrapping_mul(x.wrapping_mul(x).wrapping_mul(15731).wrapping_add(789221))
        .wrapping_add(1376312589)
        & 0x7FFF_FFFF;
    1.0 - (y as f32) as f64 / 1_073_741_824.0
}

#[derive(Clone, Default)]
struct Ffd {
    time: f64,
    from: f64,
    share: f64,
}

#[derive(Clone)]
struct Lines {
    p: Vec<f32>,
    prev: Vec<f32>,
    v: Vec<f32>,
    clock: f64,
    smoothed: f64,
    counter: i32,
    acc: f64,
    ffd: Ffd,
}

impl Lines {
    fn new() -> Self {
        Self {
            p: vec![0.0; COUNT * 4],
            prev: vec![0.0; COUNT * 4],
            v: vec![0.0; COUNT * 4],
            clock: 0.0,
            smoothed: 0.0,
            counter: 0,
            acc: 0.0,
            ffd: Ffd::default(),
        }
    }
}

fn set_clock(lines: &mut Lines, value: f64) {
    lines.clock = value;
    lines.ffd.from = lines.ffd.time;
    lines.ffd.time = value;
    lines.ffd.share = 1.0;
}

fn cross_frame(ffd: &mut Ffd, t: f64, dt_sec: f64) -> f64 {
    ffd.from += t - ffd.time;
    ffd.time = t;
    let a = ffd.share;
    if a <= 0.0 {
        return 0.0;
    }
    ffd.share = (a - CROSS_STEP * dt_sec.max(0.0) * FRAME_HZ).max(0.0);
    a * a * (3.0 - 2.0 * a)
}

fn spring(p: &[f32], v: &mut [f32], i: usize, j: usize, rest: f64, k: f64) {
    let dx = p[j] as f64 - p[i] as f64;
    let dy = p[j + 1] as f64 - p[i + 1] as f64;
    let dz = p[j + 2] as f64 - p[i + 2] as f64;
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    if len == 0.0 {
        return;
    }
    let f = (len - rest) * k / len;
    v[i] = (v[i] as f64 + dx * f) as f32;
    v[i + 1] = (v[i + 1] as f64 + dy * f) as f32;
    v[i + 2] = (v[i + 2] as f64 + dz * f) as f32;
    v[j] = (v[j] as f64 - dx * f) as f32;
    v[j + 1] = (v[j + 1] as f64 - dy * f) as f32;
    v[j + 2] = (v[j + 2] as f64 - dz * f) as f32;
}

fn step(lines: &mut Lines, s: &LineParams) {
    lines.prev.copy_from_slice(&lines.p);
    let dt = s.timestep * CLOCK_RATE;
    lines.clock += dt;
    if lines.clock > CLOCK_WRAP {
        set_clock(lines, 0.0);
    }
    let near = s.length;
    let far = s.length * FAR_LENGTH;
    let k = s.tension;
    let k_far = s.tension * FAR_TENSION;
    let Lines { p, v, counter, .. } = lines;
    for r in 0..N {
        for c in 0..N {
            let i = 4 * (N * r + c);
            if c > 0 {
                spring(p, v, i, i - 4, near, k);
            }
            if c > 1 {
                spring(p, v, i, i - 8, far, k_far);
            }
            if r > 0 {
                spring(p, v, i, i - 4 * N, near, k);
            }
            if r > 1 {
                spring(p, v, i, i - 8 * N, far, k_far);
            }
            for a in 0..3 {
                *counter = counter.wrapping_add(1);
                v[i + a] = (v[i + a] as f64 + s.perturbation * noise(*counter)) as f32;
            }
        }
    }
    let damp = s.damping * s.timestep;
    for i in (0..4 * COUNT).step_by(4) {
        for a in 0..3 {
            p[i + a] = (p[i + a] as f64 + v[i + a] as f64 * dt) as f32;
            v[i + a] = (v[i + a] as f64 - v[i + a] as f64 * damp) as f32;
        }
    }
    lines.smoothed += (lines.clock - lines.smoothed) * SMOOTHING;
    let pull = s.timestep * PULL;
    let (p, v) = (&mut lines.p, &mut lines.v);
    for r in 0..N {
        let t = r as f64 / N as f64 + lines.smoothed;
        v[4 * N * r] = (v[4 * N * r] as f64 + pull) as f32;
        let e = 4 * (N * r + N - 1);
        p[e] = 0.0;
        p[e + 1] = (s.end_y * (0.5 * (END_Y_FREQ * t).sin() + 0.5)) as f32;
        p[e + 2] = (s.end_z * 0.5 * ((END_Z_FREQ * t).cos() + 1.0)) as f32;
        p[e + 3] = 1.0;
        v[e..e + 4].fill(0.0);
    }
}

fn shape(x: f64, p1: f64) -> f64 {
    if x < p1 {
        return SHAPE_BELOW;
    }
    let m = ((x - p1) / SHAPE_REACH).clamp(0.0, 1.0);
    SHAPE_REST - ((x - p1) * PI / 2.0).cos() * (1.0 - m * m * (3.0 - 2.0 * m))
}

fn receive(lines: &Lines, p1: f64, out: &mut [f32]) {
    let (p, prev, a) = (&lines.p, &lines.prev, lines.acc);
    for i in (0..4 * COUNT).step_by(4) {
        let lerp = |k: usize| prev[i + k] as f64 + (p[i + k] as f64 - prev[i + k] as f64) * a;
        let x = lerp(0);
        let sc = shape(x, p1);
        out[i] = x as f32;
        out[i + 1] = (lerp(1) * sc) as f32;
        out[i + 2] = (lerp(2) * sc) as f32;
        out[i + 3] = 1.0;
    }
}

#[allow(clippy::approx_constant)]
fn build_lattice(t: f64, ctrl: &mut [f64], out: &mut [f64]) {
    let [nx, ny, nz] = LATTICE;
    let wobble = 0.24 * ((2.0 * t).sin() + 3.0) * (-0.0001 * t).exp();
    let bump_at = 0.833333 * ((0.1 * t).sin() + 0.2);
    for k in 0..nz {
        let z = k as f64 / nz as f64;
        for j in 0..ny {
            for i in 0..nx {
                let x0 = i as f64 / nx as f64;
                let x = 1.3 * x0 * x0 + 0.2 * x0 - 0.15;
                let y = wobble * ((x0 - 0.5).tanh() + 1.0) * (7.85 * x0 - 2.5 * t - 1.25).sin()
                    + (0.25 * t).sin() / 2.0
                    + (-50.0 * (x0 - bump_at) * (x0 - bump_at)).exp()
                    + (t - 6.28 * z).sin() / 8.0;
                let c = 3 * (i + nx * (j + ny * k));
                ctrl[c] = x * FFD_SCALE1[0] + FFD_OFFSET[0];
                ctrl[c + 1] = y * FFD_SCALE1[1] + FFD_OFFSET[1];
                ctrl[c + 2] = FFD_OFFSET[2];
            }
        }
    }
    let clampi = |v: isize, hi: usize| v.clamp(0, hi as isize - 1) as usize;
    for kk in 0..LYZ {
        let k = clampi(kk as isize - 1, nz);
        for jj in 0..LYZ {
            let j = clampi(jj as isize - 1, ny);
            for ii in 0..LX {
                let i = clampi(ii as isize - 1, nx);
                let c = 3 * (i + nx * (j + ny * k));
                let o = 3 * (ii + LX * (jj + LYZ * kk));
                out[o..o + 3].copy_from_slice(&ctrl[c..c + 3]);
            }
        }
    }
}

fn basis(t: f64, out: &mut [f64]) {
    let u = 1.0 - t;
    out[0] = u * u * u / 6.0;
    out[1] = (3.0 * t * t * t - 6.0 * t * t + 4.0) / 6.0;
    out[2] = (-3.0 * t * t * t + 3.0 * t * t + 3.0 * t + 1.0) / 6.0;
    out[3] = t * t * t / 6.0;
}

fn basis_slope(t: f64, out: &mut [f64]) {
    out[0] = -(1.0 - t) * (1.0 - t) / 2.0;
    out[1] = (3.0 * t * t - 4.0 * t) / 2.0;
    out[2] = (-3.0 * t * t + 2.0 * t + 1.0) / 2.0;
    out[3] = t * t / 2.0;
}

fn deform_point(src: &[f32], n: usize, lattice: &[f64], w: &mut [f64; 16]) -> [f32; 4] {
    let strides = [1, LX, LX * LYZ];
    let i = 4 * n;
    let mut base = 0;
    for a in 0..3 {
        let u = (src[i + a] as f64 - FFD_OFFSET[a]) / FFD_SCALE1[a];
        let q = u.clamp(0.0, CELL_LIMIT) * LATTICE[a] as f64;
        let cell = q.floor();
        basis(q - cell, &mut w[4 * a..4 * a + 4]);
        w[12 + a] = u;
        base += cell as usize * strides[a];
    }
    let (mut dx, mut dy, mut dz) = (0.0, 0.0, 0.0);
    for c in 0..4 {
        for b in 0..4 {
            let wbc = w[4 + b] * w[8 + c];
            let row = 3 * (base + LX * (b + LYZ * c));
            for (a, &wa) in w[..4].iter().enumerate() {
                let wt = wa * wbc;
                let o = row + 3 * a;
                dx += wt * lattice[o];
                dy += wt * lattice[o + 1];
                dz += wt * lattice[o + 2];
            }
        }
    }
    [
        ((w[12] + dx) * FFD_SCALE2[0]) as f32,
        ((w[13] + dy) * FFD_SCALE2[1]) as f32,
        ((w[14] + dz) * FFD_SCALE2[2]) as f32,
        1.0,
    ]
}

fn build_matrix(aspect: f64, m: &mut [f64; 16]) {
    let al = (ANG_X * ANG_X + ANG_Y * ANG_Y + ANG_Z * ANG_Z).sqrt();
    let (ax, ay, az) = (ANG_X / al, ANG_Y / al, ANG_Z / al);
    let th = ANG_ROT * PI / 180.0;
    let (c, sn) = (th.cos(), th.sin());
    let t = 1.0 - c;
    let r = [
        [t * ax * ax + c, t * ax * ay + sn * az, t * ax * az - sn * ay],
        [t * ax * ay - sn * az, t * ay * ay + c, t * ay * az + sn * ax],
        [t * ax * az + sn * ay, t * ay * az - sn * ax, t * az * az + c],
    ];
    let tx = POS_X - CAM_EYE[0];
    let ty = POS_Y - CAM_EYE[1];
    let tz = POS_Z - CAM_EYE[2];
    let f = 1.0 / (CAM_FOVY / 2.0).tan();
    let p00 = f / aspect;
    let p11 = f;
    let p22 = (CAM_FAR + CAM_NEAR) / (CAM_NEAR - CAM_FAR);
    let p23 = 2.0 * CAM_FAR * CAM_NEAR / (CAM_NEAR - CAM_FAR);
    for col in 0..3 {
        m[4 * col] = p00 * r[col][0];
        m[4 * col + 1] = p11 * r[col][1];
        m[4 * col + 2] = p22 * r[col][2];
        m[4 * col + 3] = -r[col][2];
    }
    m[12] = p00 * tx;
    m[13] = p11 * ty;
    m[14] = p22 * tz + p23;
    m[15] = -tz;
}

fn transform_point(m: &[f64; 16], p: [f32; 4]) -> [f32; 4] {
    let (x, y, z, w) = (p[0] as f64, p[1] as f64, p[2] as f64, p[3] as f64);
    std::array::from_fn(|r| (m[r] * x + m[4 + r] * y + m[8 + r] * z + m[12 + r] * w) as f32)
}

struct SampleBasis {
    pos: [f64; SAMPLES * 4],
    slope: [f64; SAMPLES * 4],
}

impl SampleBasis {
    fn new() -> Self {
        let mut s = Self {
            pos: [0.0; SAMPLES * 4],
            slope: [0.0; SAMPLES * 4],
        };
        for j in 0..SAMPLES {
            let t = (j * SPANS) as f64 / (MESH - 1) as f64;
            basis(t, &mut s.pos[4 * j..4 * j + 4]);
            basis_slope(t, &mut s.slope[4 * j..4 * j + 4]);
        }
        s
    }
}

fn evaluate_surface(
    sb: &SampleBasis,
    ctrl: &[f32],
    row_pos: &mut [f64],
    row_slope: &mut [f64],
    mesh: &mut [f32],
) {
    for r in 0..N {
        for ix in 0..MESH {
            let span = ix >> 3;
            let wb = 4 * (ix & 7);
            let (mut px, mut py, mut pz, mut pw, mut sx, mut sy, mut sz) =
                (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
            for b in 0..4 {
                let c = 4 * (N * r + span + b);
                let (w, d) = (sb.pos[wb + b], sb.slope[wb + b]);
                let (cx, cy, cz, cw) = (
                    ctrl[c] as f64,
                    ctrl[c + 1] as f64,
                    ctrl[c + 2] as f64,
                    ctrl[c + 3] as f64,
                );
                px += w * cx;
                py += w * cy;
                pz += w * cz;
                pw += w * cw;
                sx += d * cx;
                sy += d * cy;
                sz += d * cz;
            }
            let o = 4 * (MESH * r + ix);
            row_pos[o..o + 4].copy_from_slice(&[px, py, pz, pw]);
            row_slope[o..o + 3].copy_from_slice(&[sx, sy, sz]);
        }
    }
    for iy in 0..MESH {
        let span = iy >> 3;
        let wa = 4 * (iy & 7);
        for ix in 0..MESH {
            let (mut px, mut py, mut pz, mut pw) = (0.0, 0.0, 0.0, 0.0);
            let (mut tx, mut ty, mut tz, mut sx, mut sy, mut sz) =
                (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
            for a in 0..4 {
                let o = 4 * (MESH * (span + a) + ix);
                let (w, d) = (sb.pos[wa + a], sb.slope[wa + a]);
                px += w * row_pos[o];
                py += w * row_pos[o + 1];
                pz += w * row_pos[o + 2];
                pw += w * row_pos[o + 3];
                tx += d * row_pos[o];
                ty += d * row_pos[o + 1];
                tz += d * row_pos[o + 2];
                sx += w * row_slope[o];
                sy += w * row_slope[o + 1];
                sz += w * row_slope[o + 2];
            }
            let v = RECORD * (MESH * iy + ix);
            mesh[v] = px as f32;
            mesh[v + 1] = py as f32;
            mesh[v + 2] = pz as f32;
            mesh[v + 3] = pw as f32;
            mesh[v + 4] = (ty * sz - tz * sy) as f32;
            mesh[v + 5] = (tz * sx - tx * sz) as f32;
            mesh[v + 6] = (tx * sy - ty * sx) as f32;
            mesh[v + 7] = 0.0;
        }
    }
}

fn edge(i: usize) -> f64 {
    (10.0 * i.min(MESH - 1 - i) as f64 / (MESH - 1) as f64).min(1.0)
}

fn texcoords() -> Vec<f32> {
    let mut uv = vec![0.0f32; VERTICES * 2];
    for iy in 0..MESH {
        for ix in 0..MESH {
            let o = 2 * (MESH * iy + ix);
            uv[o] = (iy as f64 / (MESH - 1) as f64) as f32;
            uv[o + 1] = (edge(ix) * edge(iy)) as f32;
        }
    }
    uv
}

fn indices_stitched() -> Vec<u16> {
    let mut raw = Vec::new();
    for iy in 0..MESH - 1 {
        let mut first = 0;
        while first < MESH - 1 {
            let last = (first + SPANS).min(MESH - 1);
            for ix in first..=last {
                raw.push((MESH * iy + ix) as u16);
                raw.push((MESH * (iy + 1) + ix) as u16);
            }
            raw.push(RESTART);
            first += SPANS;
        }
    }
    let mut out = Vec::with_capacity(raw.len() + raw.len() / 8);
    let mut restart = false;
    for &i in &raw {
        if i == RESTART {
            restart = true;
            continue;
        }
        if restart {
            if let Some(&last) = out.last() {
                out.push(last);
                out.push(i);
            }
            restart = false;
        }
        out.push(i);
    }
    out
}

fn make_start() -> Lines {
    let mut start = Lines::new();
    for r in 0..N {
        let t = r as f64 / N as f64;
        let y = START_LINE.end_y * (0.5 * (END_Y_FREQ * t).sin() + 0.5);
        let z = START_LINE.end_z * 0.5 * ((END_Z_FREQ * t).cos() + 1.0);
        for c in 0..N {
            let i = 4 * (N * r + c);
            start.p[i] = ((N - 1 - c) as f64 * START_LINE.length) as f32;
            start.p[i + 1] = y as f32;
            start.p[i + 2] = z as f32;
            start.p[i + 3] = 1.0;
        }
    }
    let settle = LineParams {
        damping: START_LINE.damping * START_SETTLE_DAMPING,
        ..START_LINE
    };
    start.counter = START_NOISE;
    for _ in 0..START_SETTLE_STEPS {
        step(&mut start, &settle);
    }
    start.clock = 0.0;
    start.smoothed = 0.0;
    while start.smoothed < START_CLOCK {
        step(&mut start, &START_LINE);
    }
    let last = 4 * N * (N - 1);
    for i in 0..4 * N {
        let p = ((start.p[i] as f64 + start.p[last + i] as f64) / 2.0) as f32;
        start.p[i] = p;
        start.p[last + i] = p;
        let v = ((start.v[i] as f64 + start.v[last + i] as f64) / 2.0) as f32;
        start.v[i] = v;
        start.v[last + i] = v;
    }
    for k in 0..3 {
        let (mut p, mut v) = (0.0, 0.0);
        for i in 0..COUNT {
            p += start.p[4 * i + k] as f64;
            v += start.v[4 * i + k] as f64;
        }
        let dp = (START_MEAN_PLACE[k] * COUNT as f64 - p) / (COUNT - N) as f64;
        let dv = (START_MEAN_VELOCITY[k] * COUNT as f64 - v) / (COUNT - N) as f64;
        for r in 0..N {
            for c in 0..N - 1 {
                let o = 4 * (N * r + c) + k;
                start.p[o] = (start.p[o] as f64 + dp) as f32;
                start.v[o] = (start.v[o] as f64 + dv) as f32;
            }
        }
    }
    start
}

struct WaveSim {
    lines: Lines,
    received: Vec<f32>,
    lattice: Vec<f64>,
    matrix: [f64; 16],
    control: Vec<f32>,
    deformed: Vec<f32>,
    lattice_ctrl: Vec<f64>,
    lattice_old: Vec<f64>,
    weights: [f64; 16],
    row_pos: Vec<f64>,
    row_slope: Vec<f64>,
    sample_basis: SampleBasis,
    mesh: Vec<f32>,
}

impl WaveSim {
    fn new() -> Self {
        let start = make_start();
        let mut lines = Lines::new();
        lines.p.copy_from_slice(&start.p);
        lines.prev.copy_from_slice(&start.p);
        lines.v.copy_from_slice(&start.v);

        let mut sim = Self {
            lines,
            received: vec![0.0; COUNT * 4],
            lattice: vec![0.0; LX * LYZ * LYZ * 3],
            matrix: [0.0; 16],
            control: vec![0.0; COUNT * 4],
            deformed: vec![0.0; COUNT * 4],
            lattice_ctrl: vec![0.0; LATTICE[0] * LATTICE[1] * LATTICE[2] * 3],
            lattice_old: vec![0.0; LX * LYZ * LYZ * 3],
            weights: [0.0; 16],
            row_pos: vec![0.0; N * MESH * 4],
            row_slope: vec![0.0; N * MESH * 4],
            sample_basis: SampleBasis::new(),
            mesh: vec![0.0; VERTICES * RECORD],
        };
        for _ in 0..16 {
            sim.update(1.0 / 60.0, 16.0 / 9.0);
        }
        sim
    }

    fn update(&mut self, dt_sec: f64, aspect: f64) {
        let t = LATTICE_TIME * self.lines.clock;
        let old = cross_frame(&mut self.lines.ffd, t, dt_sec);
        self.lines.acc = (self.lines.acc + dt_sec.max(0.0) * STEP_HZ).min(MAX_STEPS_PER_FRAME + 1.0);
        while self.lines.acc > 1.0 {
            step(&mut self.lines, &BASE_LINE);
            self.lines.acc -= 1.0;
        }
        receive(&self.lines, FFD_PARAM1, &mut self.received);
        build_lattice(t, &mut self.lattice_ctrl, &mut self.lattice);
        if old > 0.0 {
            build_lattice(self.lines.ffd.from, &mut self.lattice_ctrl, &mut self.lattice_old);
            for (l, o) in self.lattice.iter_mut().zip(&self.lattice_old) {
                *l += (o - *l) * old;
            }
        }
        build_matrix(aspect, &mut self.matrix);
        for n in 0..COUNT {
            let p = deform_point(&self.received, n, &self.lattice, &mut self.weights);
            self.deformed[4 * n..4 * n + 4].copy_from_slice(&p);
        }
        for n in 0..COUNT {
            let p = transform_point(
                &self.matrix,
                self.deformed[4 * n..4 * n + 4].try_into().unwrap(),
            );
            self.control[4 * n..4 * n + 4].copy_from_slice(&p);
        }
        evaluate_surface(
            &self.sample_basis,
            &self.control,
            &mut self.row_pos,
            &mut self.row_slope,
            &mut self.mesh,
        );
    }
}

// =================================================================================================
// PS3 12-month DAY/NIGHT cubic fits from bungholesurfers/Nagare (months.rs)
// =================================================================================================

#[rustfmt::skip]
const MONTH_DAY: [[[f32; 10]; 3]; 12] = [
    [[1.00062, -0.681527, -1.30087, 0.969522, 1.09666, 2.06386, -0.577799, -0.840668, -0.2339, -0.790184],
     [0.999605, -0.677184, -1.29372, 0.956274, 1.0906, 2.05099, -0.575025, -0.833807, -0.227415, -0.78123],
     [0.999495, -0.676622, -1.29308, 0.954841, 1.08976, 2.04996, -0.574634, -0.833303, -0.226625, -0.780375]],
    [[0.852691, -0.243208, -0.229752, 0.241596, 0.414269, 0.463981, -0.158069, -0.227602, -0.152234, -0.11903],
     [0.825854, -0.67821, -0.675999, 0.482457, 0.988194, 1.17363, -0.294681, -0.380072, -0.287599, -0.26936],
     [0.166351, -0.354004, -0.381227, 0.206713, 0.50314, 0.576692, -0.11322, -0.151636, -0.120497, -0.108491]],
    [[0.553515, -0.375042, -0.183133, 0.0640462, 0.90064, 0.016295, -0.382176, 0.190208, -0.269996, -0.131915],
     [0.817807, -0.318504, -0.558659, 0.848388, 0.518366, 0.41319, -0.287337, 0.023755, -0.261305, -0.548594],
     [0.16447, -0.116597, 0.0518542, -0.212489, 0.282873, -0.235411, -0.186401, 0.177991, 0.156895, 0.0154348]],
    [[0.877178, 0.00530029, 0.0173076, -0.0353306, -0.0716996, 0.0477158, 0.0481779, -0.0295483, 0.00216591, 0.0000841475],
     [0.485283, 0.0271399, -0.150458, 0.33448, 0.447491, -0.264413, -0.350082, 0.233481, 0.144856, -0.46562],
     [0.581149, 0.0241323, -0.0502287, 0.240913, 0.199436, -0.168749, -0.140218, 0.129411, 0.0282395, -0.333542]],
    [[0.0723825, 0.0248335, -0.000707391, 0.00695834, -0.00488178, 0.00388717, 0.00336619, -0.00105102, -0.00947518, -0.00443187],
     [0.421156, 0.0703062, 0.0228394, -0.023245, 0.353503, 0.00376766, -0.236596, -0.000733773, -0.0180344, -0.00688382],
     [0.0723825, 0.0248335, -0.000707391, 0.00695834, -0.00488178, 0.00388717, 0.00336619, -0.00105102, -0.00947518, -0.00443187]],
    [[0.536251, 0.458032, -0.0214769, -0.276908, -0.110973, -0.315714, -0.114344, 0.216705, 0.161826, -0.0262087],
     [0.311389, 0.182406, 0.0769172, -0.157233, 0.0963629, -0.0971539, -0.0829741, 0.0444459, -0.0497457, 0.0254398],
     [0.584632, 0.537171, 0.0318778, -0.185551, 0.0393151, 0.0136909, -0.331932, -0.00440812, 0.16264, -0.0117843]],
    [[0.00289851, -0.0279277, -0.0116738, -0.00739229, 0.0736048, 0.0270716, -0.0424723, -0.0195048, 0.0524507, 0.0199744],
     [0.664131, 0.104881, 0.0438681, -0.0265691, 0.368319, 0.0648644, -0.27308, -0.0455345, 0.00915022, -0.00660426],
     [0.550854, 0.127521, 0.119591, -0.00475646, 0.40851, 0.210568, -0.274177, -0.138939, -0.101045, -0.0213409]],
    [[-0.0106758, 0.0519841, -0.0465505, -0.0691563, 0.0939042, 0.388737, -0.114568, -0.0803538, -0.0345419, -0.144852],
     [-0.00124282, 0.00244707, -0.0239846, 0.699487, 0.592412, 1.26327, -0.236204, -0.595974, -0.393937, -0.635434],
     [0.221162, 0.537462, 0.610709, -0.798752, -0.104701, 0.623627, 0.113159, -0.653966, -0.0870921, 0.411653]],
    [[0.816153, -0.341021, -0.130637, 0.379809, 0.0399881, 0.263365, 0.0426405, -0.131185, -0.147323, -0.107621],
     [0.38281, -0.374117, -0.102956, 0.339428, 0.109409, 0.153721, 0.118625, -0.0523856, -0.302025, -0.0181236],
     [0.897325, -0.583303, -0.1842, 0.527866, 0.346481, 0.397753, -0.0485994, -0.203957, -0.246812, -0.134869]],
    [[0.889623, -0.0377875, -0.0355178, 0.017387, 0.0428367, 0.131733, 0.0138891, -0.130399, -0.0908185, 0.0810232],
     [0.701758, -0.175068, -0.0913822, -0.0055047, 0.141131, 0.154949, -0.0810426, 0.0539895, 0.225475, -0.205845],
     [0.0613457, 0.0644032, -0.100769, 0.0369735, -0.163302, 0.0045681, 0.0417915, 0.0275975, 0.178747, -0.0798799]],
    [[0.472339, 0.0, -0.10392, 0.0, 0.0, 0.600845, 0.0, -0.372826, 0.0, 0.0],
     [0.294852, 0.0, -0.106591, 0.0, 0.0, 0.675438, 0.0, -0.409436, 0.0, 0.0],
     [0.0982981, 0.0, -0.119669, 0.0, 0.0, 0.504533, 0.0, -0.304, 0.0, 0.0]],
    [[0.837506, 0.160208, -0.0226315, 0.0420396, -0.065481, -0.0156062, -0.0349245, 0.0136065, -0.0107834, -0.00596771],
     [0.253656, 0.0286501, -0.0193641, 0.0043216, -0.0150513, 0.000049971, -0.00900011, 0.0100312, 0.0286655, -0.0183274],
     [0.171629, 0.0207587, -0.0154692, -0.0157005, -0.00924311, 0.010508, -0.0115469, 0.00164819, 0.0412077, -0.0161435]],
];

#[rustfmt::skip]
const MONTH_NIGHT: [[[f32; 10]; 3]; 12] = [
    [[0.723392, -0.0144026, -0.443064, 0.052593, -0.00249464, -1.15932, 0.0000768861, 0.965388, 0.00490387, -0.0445665],
     [0.724393, -0.0147122, -0.44905, 0.0518757, -0.00111716, -1.14849, -0.000974572, 0.959472, 0.005186, -0.0442247],
     [0.724393, -0.0147122, -0.44905, 0.0518757, -0.00111716, -1.14849, -0.000974572, 0.959472, 0.005186, -0.0442247]],
    [[0.724048, 0.0983777, -0.542541, -0.287062, 0.101613, -0.910273, -0.0773501, 0.806503, -0.0102493, 0.179559],
     [0.694433, 0.107458, -0.633174, -0.329264, 0.112003, -0.590807, -0.0828136, 0.605405, -0.0131147, 0.211266],
     [0.452673, 0.168692, -0.590431, -0.617283, 0.158346, 0.092647, -0.116861, 0.0853782, 0.00472717, 0.41666]],
    [[0.572005, 0.19255, -0.381612, -0.222188, -0.171103, -0.812398, -0.020032, 0.690267, 0.220245, 0.00150092],
     [0.645522, 0.176042, -0.423232, -0.197504, -0.144282, -0.948788, -0.0234453, 0.800561, 0.185387, 0.00235419],
     [0.41818, 0.213334, -0.312507, -0.29036, -0.17044, -0.504087, -0.0367725, 0.454928, 0.258805, 0.0204127]],
    [[0.797736, -0.00362039, -0.290371, 0.00573127, -0.00762114, -1.23487, 0.00568336, 0.785949, 0.00224348, -0.00365583],
     [0.648678, 0.0146837, -0.251112, 0.00149912, -0.00293568, -1.04011, 0.000967245, 0.700106, 0.00222114, -0.0182538],
     [0.677127, 0.00810731, -0.235185, -0.00645865, 0.0036758, -1.11879, -0.00339585, 0.733735, 0.00252472, -0.00487177]],
    [[0.142342, 0.0585711, -0.24337, -0.121627, 0.0446552, 0.383415, -0.0117024, -0.200139, 0.0752295, -0.045459],
     [0.300446, 0.249931, -0.448951, -0.591112, 0.163712, 0.383082, -0.115948, -0.0791264, 0.195338, 0.106489],
     [0.142342, 0.0585711, -0.24337, -0.121627, 0.0446552, 0.383415, -0.0117024, -0.200139, 0.0752295, -0.045459]],
    [[0.807984, 0.0316085, -0.583077, -0.0493082, 0.00458397, -1.04922, -0.0022512, 0.864148, -0.00605094, 0.0210892],
     [0.617337, 0.0346282, -0.378978, -0.0714165, -0.000287114, -0.850586, 0.000448646, 0.647597, 0.000467447, 0.0369159],
     [0.846747, 0.0362344, -0.471096, -0.0476053, -0.00345739, -1.2641, 0.00248285, 0.926646, 0.00253886, 0.00859062]],
    [[0.0470902, -0.122189, 0.0576003, 0.164797, 0.239973, -0.0549125, -0.0764596, 0.0213645, -0.144048, -0.0820324],
     [0.291538, 0.308869, -0.218505, -0.534186, 0.153903, -0.224377, -0.153178, 0.25411, 0.130977, 0.0562724],
     [0.273316, 0.2931, -0.174292, -0.517868, 0.176348, -0.249424, -0.158388, 0.251291, 0.105416, 0.0626737]],
    [[0.015758, 0.315653, -0.372194, -1.04168, 0.0343227, 1.0098, -0.0409354, -0.678401, 0.0519724, 0.721528],
     [0.549207, 0.189234, -0.078509, -1.17172, -0.438553, -0.854232, 0.232733, 0.557152, 0.363697, 0.677371],
     [0.654959, -0.0537107, -0.242594, -0.227715, -0.0464507, -0.447806, 0.0403362, 0.43444, 0.103963, -0.0984211]],
    [[0.425587, -0.212466, 0.231376, -0.269341, 0.0498402, -0.921866, -0.00749811, 0.489051, 0.00937332, 0.310417],
     [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
     [0.592755, -0.494803, 0.250491, -0.393359, 0.45504, -1.18215, -0.189769, 0.671969, 0.0934293, 0.340739]],
    [[0.863889, 0.0014249, -0.674038, -0.00953676, 0.00329461, -1.14886, -0.00020866, 0.983091, -0.00533086, 0.0116001],
     [0.584936, -0.0308945, -0.462034, 0.0563852, 0.00399658, -0.900105, -0.00172477, 0.778734, 0.000161919, -0.0281728],
     [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]],
    [[0.500496, 0.0, -0.290772, 0.0, 0.0, -0.283406, 0.0, 0.126521, 0.0, 0.0],
     [0.331595, 0.0, -0.250471, 0.0, 0.0, 0.0895913, 0.0, -0.11894, 0.0, 0.0],
     [0.146823, 0.0, -0.269676, 0.0, 0.0, 0.481865, 0.0, -0.306595, 0.0, 0.0]],
    [[0.255562, 0.557562, -0.43864, -0.895904, 0.139687, 0.0857301, -0.126476, 0.210475, 0.246172, 0.0372808],
     [0.13702, 0.169026, -0.169225, 0.0154173, -0.143994, -0.0775363, 0.0843923, 0.188204, 0.130736, -0.269015],
     [0.118158, 0.109683, -0.12836, 0.115588, -0.164143, -0.0823676, 0.102249, 0.164562, 0.110408, -0.280686]],
];

const FRESNEL_KNOTS: [[f64; 2]; 19] = [
    [0.0, 0.0], [14.0, 0.0], [17.0, 0.031], [19.0, 0.133], [21.0, 0.349], [22.0, 0.486],
    [24.0, 0.753], [26.0, 0.922], [29.0, 0.988], [32.0, 0.953], [34.0, 0.89], [36.0, 0.796],
    [38.0, 0.698], [43.0, 0.482], [48.0, 0.329], [58.0, 0.153], [77.0, 0.035], [120.0, 0.0],
    [511.0, 0.0],
];

fn build_fresnel_lut() -> [u8; 512] {
    let k = &FRESNEL_KNOTS;
    let mut out = [0u8; 512];
    for (i, dst) in out.iter_mut().enumerate() {
        let x = i as f64;
        let mut j = 0;
        while j < k.len() - 2 && x > k[j + 1][0] {
            j += 1;
        }
        let t = (x - k[j][0]) / (k[j + 1][0] - k[j][0]);
        let p0 = k[j.saturating_sub(1)][1];
        let p1 = k[j][1];
        let p2 = k[j + 1][1];
        let p3 = k[(j + 2).min(k.len() - 1)][1];
        let v = 0.5
            * (2.0 * p1
                + (p2 - p0) * t
                + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
                + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t * t * t);
        *dst = (255.0 * v.clamp(0.0, 1.0)).round() as u8;
    }
    out
}

fn eval_month_cubic(c: &[f32; 10], u: f32, v: f32) -> f32 {
    (c[0]
        + c[1] * u
        + c[2] * v
        + c[3] * u * v
        + c[4] * u * u
        + c[5] * v * v
        + c[6] * u * u * u
        + c[7] * v * v * v
        + c[8] * u * u * v
        + c[9] * u * v * v)
        .clamp(0.0, 1.0)
}

#[repr(C)]
#[derive(Clone, Copy)]
struct BgConsts {
    top_col: [f32; 4],
    mid_col: [f32; 4],
    bot_col: [f32; 4],
    glow_time: [f32; 4], // x = time, y = day_factor
}

#[repr(C)]
#[derive(Clone, Copy)]
struct WaveConsts {
    tint: [f32; 4],       // rgb = tint, a = scale
    params: [f32; 4],     // x = mipmap_bias, y = brightness, z = fresnel, w = unused
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SparkleVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    alpha: f32,
}

const BG_PS_HLSL: &str = r#"
cbuffer BgParams : register(b0) {
    float4 u_top;
    float4 u_mid;
    float4 u_bot;
    float4 u_glow;
};
struct PSIn {
    float4 pos : SV_Position;
    float2 uv  : TEXCOORD0;
};
float4 main(PSIn input) : SV_Target {
    float y = input.uv.y;
    float3 col = (y < 0.5)
        ? lerp(u_top.rgb, u_mid.rgb, y * 2.0)
        : lerp(u_mid.rgb, u_bot.rgb, (y - 0.5) * 2.0);
    // Gentle PS3 daylight ambient glow shift
    float2 p = input.uv - float2(0.5 + 0.12 * sin(u_glow.x * 0.08), 0.35);
    col += u_mid.rgb * (0.14 * exp(-dot(p, p) * 2.4));
    // Smooth dark vignette at bottom/top
    col *= lerp(0.84, 1.05, smoothstep(0.0, 0.5, y) * (1.0 - 0.22 * smoothstep(0.6, 1.0, y)));
    float n = frac(sin(dot(input.pos.xy, float2(12.9898, 78.233))) * 43758.5453);
    col += (n - 0.5) * (1.0 / 255.0);
    return float4(saturate(col), 1.0);
}
"#;

const WAVE_VS_HLSL: &str = r#"
cbuffer WaveParams : register(b0) {
    float4 u_tint;
    float4 u_params; // x = mipmap_bias, y = brightness, z = fresnel
};
struct VSIn {
    float4 pos    : POSITION;
    float4 normal : NORMAL;
    float2 uv     : TEXCOORD0;
};
struct VSOut {
    float4 pos    : SV_Position;
    float4 params : TEXCOORD0; // x = fresnel*m, y = brightness*m, z = |e.n|*m, w = edge_fade
};
VSOut main(VSIn input) {
    VSOut o;
    float inv_len = rsqrt(max(dot(input.pos.xyz, input.pos.xyz), 1e-10));
    float3 e = input.pos.xyz * inv_len;
    float3 n = input.normal.xyz * rsqrt(max(dot(input.normal.xyz, input.normal.xyz), 1e-10));
    float m = abs(dot(e, input.normal.xyz)) * inv_len * u_params.x;
    o.params = float4(u_params.z * m, u_params.y * m, abs(dot(e, n)) * m, input.uv.y);
    // Remap OpenGL clip Z [-w, w] -> D3D11 [0, w]
    o.pos = float4(input.pos.xy, 0.5 * (input.pos.z + input.pos.w), input.pos.w);
    return o;
}
"#;

const WAVE_PS_HLSL: &str = r#"
cbuffer WaveParams : register(b0) {
    float4 u_tint;
    float4 u_params;
};
Texture2D<float> u_fres_lut : register(t0);
SamplerState     u_samp     : register(s0);

struct PSIn {
    float4 pos    : SV_Position;
    float4 params : TEXCOORD0;
};
float4 main(PSIn input) : SV_Target {
    float fres = u_fres_lut.Sample(u_samp, float2(saturate(input.params.z), 0.5));
    float light = (fres * input.params.x + 0.5 * input.params.y) * input.params.w;
    // Soft silky HDR tone roll-off so overlapping folds look translucent, never flat white
    float silk = (1.0 - exp(-light * 1.15)) * u_tint.a;
    float3 col = u_tint.rgb * silk;
    if (silk > 0.002) {
        float n = frac(sin(dot(input.pos.xy, float2(12.9898, 78.233))) * 43758.5453);
        col += (n - 0.5) * (0.6 / 255.0);
    }
    return float4(max(col, 0.0), 1.0);
}
"#;

const SPARKLE_VS_HLSL: &str = r#"
struct VSIn {
    float2 pos   : POSITION;
    float2 uv    : TEXCOORD0;
    float  alpha : TEXCOORD1;
};
struct VSOut {
    float4 pos   : SV_Position;
    float2 uv    : TEXCOORD0;
    float  alpha : TEXCOORD1;
};
VSOut main(VSIn input) {
    VSOut o;
    o.pos = float4(input.pos, 0.0, 1.0);
    o.uv = input.uv;
    o.alpha = input.alpha;
    return o;
}
"#;

const SPARKLE_PS_HLSL: &str = r#"
struct PSIn {
    float4 pos   : SV_Position;
    float2 uv    : TEXCOORD0;
    float  alpha : TEXCOORD1;
};
float4 main(PSIn input) : SV_Target {
    float r2 = dot(input.uv, input.uv);
    float d = saturate(1.0 - r2);
    float glow = exp(-r2 * 9.5) * d;
    float3 col = float3(0.86, 0.92, 1.0) * (glow * input.alpha);
    return float4(col, 1.0);
}
"#;

const SPARKLE_COUNT: usize = 160;

#[derive(Clone, Copy)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    age: f32,
    life: f32,
    size: f32,
    phase: f32,
}

pub struct Ps3XmbScene {
    oled_black: bool,
    sim: WaveSim,
    particles: Vec<Particle>,
    rng_state: u32,
    sparkle_verts: Vec<SparkleVertex>,

    bg_vs: ID3D11VertexShader,
    bg_ps: ID3D11PixelShader,
    bg_cb: ID3D11Buffer,

    wave_vs: ID3D11VertexShader,
    wave_ps: ID3D11PixelShader,
    wave_il: ID3D11InputLayout,
    wave_vb: ID3D11Buffer,
    uv_vb: ID3D11Buffer,
    wave_ib: ID3D11Buffer,
    index_count: u32,
    wave_cb: ID3D11Buffer,
    _fres_tex: ID3D11Texture2D,
    fres_srv: ID3D11ShaderResourceView,
    sampler: ID3D11SamplerState,

    sparkle_vs: ID3D11VertexShader,
    sparkle_ps: ID3D11PixelShader,
    sparkle_il: ID3D11InputLayout,
    sparkle_vb: ID3D11Buffer,

    opaque_blend: ID3D11BlendState,
    add_blend: ID3D11BlendState,
    rast_state: ID3D11RasterizerState,
}

impl Ps3XmbScene {
    pub fn new(device: &ID3D11Device, oled_black: bool) -> Result<Self, String> {
        let (_, bg_vs, bg_ps) = create_vs_ps(device, FULLSCREEN_VS_HLSL, BG_PS_HLSL)?;
        let (wvs_bytes, wave_vs, wave_ps) = create_vs_ps(device, WAVE_VS_HLSL, WAVE_PS_HLSL)?;
        let (svs_bytes, sparkle_vs, sparkle_ps) =
            create_vs_ps(device, SPARKLE_VS_HLSL, SPARKLE_PS_HLSL)?;

        let wave_layout = [
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("POSITION"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32B32A32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 0,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("NORMAL"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32B32A32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 16,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("TEXCOORD"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32_FLOAT,
                InputSlot: 1,
                AlignedByteOffset: 0,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
        ];

        let sparkle_layout = [
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("POSITION"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 0,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("TEXCOORD"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 8,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("TEXCOORD"),
                SemanticIndex: 1,
                Format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 16,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
        ];

        let (wave_il, sparkle_il) = unsafe {
            let mut wil: Option<ID3D11InputLayout> = None;
            device
                .CreateInputLayout(&wave_layout, &wvs_bytes, Some(&mut wil))
                .map_err(|e| format!("wave_il: {e}"))?;
            let mut sil: Option<ID3D11InputLayout> = None;
            device
                .CreateInputLayout(&sparkle_layout, &svs_bytes, Some(&mut sil))
                .map_err(|e| format!("sparkle_il: {e}"))?;
            (wil.ok_or("No wil")?, sil.ok_or("No sil")?)
        };

        let uvs = texcoords();
        let indices = indices_stitched();
        let index_count = indices.len() as u32;

        let wave_vb = create_buffer(
            device,
            (VERTICES * RECORD * size_of::<f32>()) as u32,
            D3D11_BIND_VERTEX_BUFFER.0 as u32,
            true,
            None,
        )?;
        let uv_vb = create_buffer(
            device,
            (uvs.len() * size_of::<f32>()) as u32,
            D3D11_BIND_VERTEX_BUFFER.0 as u32,
            false,
            Some(uvs.as_ptr() as *const _),
        )?;
        let wave_ib = create_buffer(
            device,
            (indices.len() * size_of::<u16>()) as u32,
            D3D11_BIND_INDEX_BUFFER.0 as u32,
            false,
            Some(indices.as_ptr() as *const _),
        )?;
        let bg_cb = create_buffer(
            device,
            size_of::<BgConsts>() as u32,
            D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            true,
            None,
        )?;
        let wave_cb = create_buffer(
            device,
            size_of::<WaveConsts>() as u32,
            D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            true,
            None,
        )?;
        let sparkle_vb = create_buffer(
            device,
            (SPARKLE_COUNT * 6 * size_of::<SparkleVertex>()) as u32,
            D3D11_BIND_VERTEX_BUFFER.0 as u32,
            true,
            None,
        )?;

        let fres_bytes = build_fresnel_lut();
        let tex_desc = D3D11_TEXTURE2D_DESC {
            Width: 512,
            Height: 1,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let sub = D3D11_SUBRESOURCE_DATA {
            pSysMem: fres_bytes.as_ptr() as *const _,
            SysMemPitch: 512,
            SysMemSlicePitch: 512,
        };
        let (fres_tex, fres_srv, sampler) = unsafe {
            let mut tex: Option<ID3D11Texture2D> = None;
            device
                .CreateTexture2D(&tex_desc, Some(&sub), Some(&mut tex))
                .map_err(|e| format!("fres_tex: {e}"))?;
            let tex = tex.ok_or("No fres_tex")?;

            let srv_desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
                Format: DXGI_FORMAT_R8_UNORM,
                ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_SRV {
                        MostDetailedMip: 0,
                        MipLevels: 1,
                    },
                },
            };
            let mut srv: Option<ID3D11ShaderResourceView> = None;
            device
                .CreateShaderResourceView(&tex, Some(&srv_desc), Some(&mut srv))
                .map_err(|e| format!("fres_srv: {e}"))?;

            let samp_desc = D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                ComparisonFunc: D3D11_COMPARISON_NEVER,
                MaxLOD: f32::MAX,
                ..Default::default()
            };
            let mut samp: Option<ID3D11SamplerState> = None;
            device
                .CreateSamplerState(&samp_desc, Some(&mut samp))
                .map_err(|e| format!("fres_sampler: {e}"))?;

            (tex, srv.ok_or("No srv")?, samp.ok_or("No sampler")?)
        };

        let mut sim = WaveSim::new();
        let mut rng_state = 0x243F_6A88u32;
        let mut rng = || -> f32 {
            rng_state = rng_state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((rng_state >> 16) & 0xFFFF) as f32 / 65535.0
        };

        let mut particles = Vec::with_capacity(SPARKLE_COUNT);
        for _ in 0..SPARKLE_COUNT {
            let idx = ((rng() * (VERTICES as f32 - 1.0)) as usize).min(VERTICES - 1);
            let w = sim.mesh[idx * RECORD + 3].max(0.1);
            let x = (sim.mesh[idx * RECORD] / w).clamp(-1.0, 1.0);
            let y = (sim.mesh[idx * RECORD + 1] / w).clamp(-1.0, 1.0);
            particles.push(Particle {
                x,
                y,
                vx: (rng() - 0.5) * 0.018 + 0.008,
                vy: 0.010 + rng() * 0.024,
                age: rng() * 4.5,
                life: 3.0 + rng() * 3.5,
                size: 0.0032 + rng().powi(2) * 0.0068,
                phase: rng() * std::f32::consts::TAU,
            });
        }
        // Keep sim clean on construction
        sim.update(1.0 / 60.0, 16.0 / 9.0);

        Ok(Self {
            oled_black,
            sim,
            particles,
            rng_state,
            sparkle_verts: Vec::with_capacity(SPARKLE_COUNT * 6),
            bg_vs,
            bg_ps,
            bg_cb,
            wave_vs,
            wave_ps,
            wave_il,
            wave_vb,
            uv_vb,
            wave_ib,
            index_count,
            wave_cb,
            _fres_tex: fres_tex,
            fres_srv,
            sampler,
            sparkle_vs,
            sparkle_ps,
            sparkle_il,
            sparkle_vb,
            opaque_blend: create_blend_state(device, BlendMode::Opaque)?,
            add_blend: create_blend_state(device, BlendMode::Additive)?,
            rast_state: create_rasterizer(device, false, false)?,
        })
    }

    #[inline]
    fn next_lcg(state: &mut u32) -> f32 {
        *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((*state >> 16) & 0xFFFF) as f32 / 65535.0
    }

    fn step_particles(&mut self, dt: f32, inv_aspect: f32, t: f32) {
        self.sparkle_verts.clear();
        let mesh = &self.sim.mesh;
        for p in &mut self.particles {
            p.age += dt;
            if p.age >= p.life {
                // Respawn on a live mesh vertex
                let r1 = Self::next_lcg(&mut self.rng_state);
                let r2 = Self::next_lcg(&mut self.rng_state);
                let r3 = Self::next_lcg(&mut self.rng_state);
                let r4 = Self::next_lcg(&mut self.rng_state);
                let v_idx = ((r1 * (VERTICES as f32 - 1.0)) as usize).min(VERTICES - 1);
                let w = mesh[v_idx * RECORD + 3];
                if w > 0.1 {
                    p.x = (mesh[v_idx * RECORD] / w).clamp(-1.05, 1.05);
                    p.y = (mesh[v_idx * RECORD + 1] / w).clamp(-0.95, 0.95);
                } else {
                    p.x = r2 * 2.0 - 1.0;
                    p.y = (r3 - 0.5) * 0.9;
                }
                p.vx = (r2 - 0.5) * 0.018 + 0.008;
                p.vy = 0.010 + r3 * 0.024;
                p.age = 0.0;
                p.life = 3.0 + r4 * 3.5;
                p.size = 0.0032 + r4.powi(2) * 0.0068;
            } else {
                let curl = (t * 0.9 + p.phase + p.y * 4.0).sin() * 0.008;
                p.x += (p.vx + curl) * dt;
                p.y += p.vy * dt;
            }

            let u = (p.age / p.life).clamp(0.0, 1.0);
            let env = (u * std::f32::consts::PI).sin().powf(1.2);
            let twinkle = 0.7 + 0.3 * (t * 2.4 + p.phase).sin();
            let alpha = env * twinkle * if self.oled_black { 0.45 } else { 0.55 };

            let rx = p.size * inv_aspect;
            let ry = p.size;
            let x0 = p.x - rx;
            let x1 = p.x + rx;
            let y0 = p.y - ry;
            let y1 = p.y + ry;
            self.sparkle_verts.extend_from_slice(&[
                SparkleVertex { pos: [x0, y0], uv: [-1.0, -1.0], alpha },
                SparkleVertex { pos: [x1, y0], uv: [1.0, -1.0], alpha },
                SparkleVertex { pos: [x1, y1], uv: [1.0, 1.0], alpha },
                SparkleVertex { pos: [x0, y0], uv: [-1.0, -1.0], alpha },
                SparkleVertex { pos: [x1, y1], uv: [1.0, 1.0], alpha },
                SparkleVertex { pos: [x0, y1], uv: [-1.0, 1.0], alpha },
            ]);
        }
    }
}

impl Scene for Ps3XmbScene {
    fn resize(&mut self, _device: &ID3D11Device, _width: u32, _height: u32) -> Result<(), String> {
        Ok(())
    }

    fn render(
        &mut self,
        context: &ID3D11DeviceContext,
        rtv: &ID3D11RenderTargetView,
        _backbuf_srv: Option<&ID3D11ShaderResourceView>,
        input: &FrameInput,
    ) {
        let aspect = (input.width.max(1) as f64) / (input.height.max(1) as f64);
        let dt = if input.is_capture {
            1.0 / 60.0
        } else {
            (input.dt as f64).clamp(0.0, 0.05)
        };
        self.sim.update(dt, aspect);
        self.step_particles(dt as f32, 1.0 / aspect as f32, input.elapsed);

        update_dynamic_slice(context, &self.wave_vb, &self.sim.mesh);
        update_dynamic_slice(context, &self.sparkle_vb, &self.sparkle_verts);

        let wave_c = if self.oled_black {
            WaveConsts {
                tint: [0.76, 0.80, 0.86, 0.36],
                params: [1.86707, 0.701917, 0.638971, 0.0],
            }
        } else {
            WaveConsts {
                tint: [0.92, 0.95, 1.00, 0.34],
                params: [1.86707, 0.701917, 0.638971, 0.0],
            }
        };
        update_cbuffer(context, &self.wave_cb, &wave_c);

        unsafe {
            let rtv_slice = [Some(rtv.clone())];
            context.OMSetRenderTargets(Some(&rtv_slice), None);
            set_viewport(context, input.width, input.height);
            context.RSSetState(&self.rast_state);

            // Pass 1: Backdrop (OLED pure #000000 OR PS3 Monthly Day/Night Cubic Gradient)
            if self.oled_black {
                context.ClearRenderTargetView(rtv, &[0.0, 0.0, 0.0, 1.0]);
            } else {
                let m_idx = (input.month.clamp(1, 12) - 1) as usize;
                let hour = (input.clock_sec / 3600.0).rem_euclid(24.0) as f32;
                let day_weight = if (6.0..=18.0).contains(&hour) {
                    1.0
                } else if (4.0..6.0).contains(&hour) {
                    (hour - 4.0) / 2.0
                } else if (18.0..=20.0).contains(&hour) {
                    1.0 - (hour - 18.0) / 2.0
                } else {
                    0.0
                };

                let u_day = ((input.day.clamp(1, 31) as f32 - 1.0) / 30.0).clamp(0.05, 0.95);
                let sample_v = |v: f32| -> [f32; 4] {
                    let u = u_day;
                    let r_d = eval_month_cubic(&MONTH_DAY[m_idx][0], u, v);
                    let g_d = eval_month_cubic(&MONTH_DAY[m_idx][1], u, v);
                    let b_d = eval_month_cubic(&MONTH_DAY[m_idx][2], u, v);
                    let r_n = eval_month_cubic(&MONTH_NIGHT[m_idx][0], u, v);
                    let g_n = eval_month_cubic(&MONTH_NIGHT[m_idx][1], u, v);
                    let b_n = eval_month_cubic(&MONTH_NIGHT[m_idx][2], u, v);
                    // Scale down gently to authentic PS3 XMB contrast (so silk wave pops cleanly)
                    let scale = 0.65;
                    [
                        (r_n + (r_d - r_n) * day_weight) * scale,
                        (g_n + (g_d - g_n) * day_weight) * scale,
                        (b_n + (b_d - b_n) * day_weight) * scale,
                        1.0,
                    ]
                };

                let bg_c = BgConsts {
                    top_col: sample_v(0.10),
                    mid_col: sample_v(0.50),
                    bot_col: sample_v(0.90),
                    glow_time: [input.elapsed, day_weight, 0.0, 0.0],
                };
                update_cbuffer(context, &self.bg_cb, &bg_c);

                context.OMSetBlendState(&self.opaque_blend, None, 0xFFFF_FFFF);
                context.IASetInputLayout(None);
                context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                context.VSSetShader(&self.bg_vs, None);
                context.PSSetShader(&self.bg_ps, None);
                let cbs = [Some(self.bg_cb.clone())];
                context.PSSetConstantBuffers(0, Some(&cbs));
                context.Draw(3, 0);
            }

            // Pass 2: PS3 128x128 B-spline Silk Wave (Additive)
            context.OMSetBlendState(&self.add_blend, None, 0xFFFF_FFFF);
            context.IASetInputLayout(&self.wave_il);
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
            context.VSSetShader(&self.wave_vs, None);
            context.PSSetShader(&self.wave_ps, None);
            let wcbs = [Some(self.wave_cb.clone())];
            context.VSSetConstantBuffers(0, Some(&wcbs));
            context.PSSetConstantBuffers(0, Some(&wcbs));
            let srvs = [Some(self.fres_srv.clone())];
            context.PSSetShaderResources(0, Some(&srvs));
            let samps = [Some(self.sampler.clone())];
            context.PSSetSamplers(0, Some(&samps));

            let vbs = [Some(self.wave_vb.clone()), Some(self.uv_vb.clone())];
            let strides = [(RECORD * size_of::<f32>()) as u32, (2 * size_of::<f32>()) as u32];
            let offsets = [0u32, 0u32];
            context.IASetVertexBuffers(0, 2, Some(vbs.as_ptr()), Some(strides.as_ptr()), Some(offsets.as_ptr()));
            context.IASetIndexBuffer(&self.wave_ib, DXGI_FORMAT_R16_UINT, 0);
            context.DrawIndexed(self.index_count, 0, 0);

            // Pass 3: Sparkling Dust Particles
            if !self.sparkle_verts.is_empty() {
                context.IASetInputLayout(&self.sparkle_il);
                context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                context.VSSetShader(&self.sparkle_vs, None);
                context.PSSetShader(&self.sparkle_ps, None);
                let s_vbs = [Some(self.sparkle_vb.clone())];
                let s_stride = size_of::<SparkleVertex>() as u32;
                let s_off = 0u32;
                context.IASetVertexBuffers(0, 1, Some(s_vbs.as_ptr()), Some(&s_stride), Some(&s_off));
                context.Draw(self.sparkle_verts.len() as u32, 0);
            }
        }
    }
}
