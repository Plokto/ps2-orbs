use crate::config::OrbMode;
use std::f32::consts::{PI, TAU};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub params: [f32; 2], // [glow_pow, core_intensity]
}

pub const MAX_VERTICES: usize = 8192;

#[derive(Clone, Copy)]
struct Vec2 {
    x: f32,
    y: f32,
}

impl Vec2 {
    #[inline(always)]
    fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    #[inline(always)]
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y)
    }
    #[inline(always)]
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y)
    }
    #[inline(always)]
    fn scale(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s)
    }
    #[inline(always)]
    fn perp_norm(self) -> Self {
        let len = (self.x * self.x + self.y * self.y).sqrt();
        if len < 1e-6 {
            Self::new(0.0, 1.0)
        } else {
            Self::new(-self.y / len, self.x / len)
        }
    }
}

#[derive(Clone, Copy)]
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

impl Vec3 {
    #[inline(always)]
    fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    #[inline(always)]
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    #[inline(always)]
    fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    #[inline(always)]
    fn cross(self, o: Self) -> Self {
        Self::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    #[inline(always)]
    fn normalize(self) -> Self {
        let len = self.dot(self).sqrt();
        if len < 1e-6 {
            Self::new(0.0, 0.0, 1.0)
        } else {
            let inv = 1.0 / len;
            Self::new(self.x * inv, self.y * inv, self.z * inv)
        }
    }
    #[inline(always)]
    fn rot_x(self, a: f32) -> Self {
        let (s, c) = a.sin_cos();
        Self::new(self.x, self.y * c - self.z * s, self.y * s + self.z * c)
    }
    #[inline(always)]
    fn rot_y(self, a: f32) -> Self {
        let (s, c) = a.sin_cos();
        Self::new(self.x * c + self.z * s, self.y, -self.x * s + self.z * c)
    }
    #[inline(always)]
    fn rot_z(self, a: f32) -> Self {
        let (s, c) = a.sin_cos();
        Self::new(self.x * c - self.y * s, self.x * s + self.y * c, self.z)
    }
}

#[inline(always)]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline(always)]
fn lerp_rgb(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]
}

#[inline(always)]
fn scale_rgb(c: [f32; 3], s: f32) -> [f32; 4] {
    [c[0] * s, c[1] * s, c[2] * s, 1.0]
}

#[inline(always)]
fn to_ndc(p: Vec2, inv_aspect: f32) -> [f32; 2] {
    [p.x * inv_aspect, p.y]
}

fn push_sprite(
    verts: &mut Vec<Vertex>,
    center: Vec2,
    radius: f32,
    color: [f32; 4],
    glow_pow: f32,
    core_intensity: f32,
    inv_aspect: f32,
) {
    let x0 = (center.x - radius) * inv_aspect;
    let x1 = (center.x + radius) * inv_aspect;
    let y0 = center.y - radius;
    let y1 = center.y + radius;
    let params = [glow_pow, core_intensity];

    let v00 = Vertex { pos: [x0, y0], uv: [-1.0, -1.0], color, params };
    let v10 = Vertex { pos: [x1, y0], uv: [1.0, -1.0], color, params };
    let v11 = Vertex { pos: [x1, y1], uv: [1.0, 1.0], color, params };
    let v01 = Vertex { pos: [x0, y1], uv: [-1.0, 1.0], color, params };

    verts.extend_from_slice(&[v00, v10, v11, v00, v11, v01]);
}

#[allow(clippy::too_many_arguments)]
fn push_ribbon_segment(
    verts: &mut Vec<Vertex>,
    p0: Vec2,
    n0: Vec2,
    w0: f32,
    c0: [f32; 4],
    p1: Vec2,
    n1: Vec2,
    w1: f32,
    c1: [f32; 4],
    glow_pow: f32,
    inv_aspect: f32,
) {
    let l0 = p0.sub(n0.scale(w0));
    let r0 = p0.add(n0.scale(w0));
    let l1 = p1.sub(n1.scale(w1));
    let r1 = p1.add(n1.scale(w1));
    let params = [glow_pow, 0.0];

    let vl0 = Vertex { pos: to_ndc(l0, inv_aspect), uv: [0.0, -1.0], color: c0, params };
    let vr0 = Vertex { pos: to_ndc(r0, inv_aspect), uv: [0.0, 1.0], color: c0, params };
    let vr1 = Vertex { pos: to_ndc(r1, inv_aspect), uv: [0.0, 1.0], color: c1, params };
    let vl1 = Vertex { pos: to_ndc(l1, inv_aspect), uv: [0.0, -1.0], color: c1, params };

    verts.extend_from_slice(&[vl0, vr0, vr1, vl0, vr1, vl1]);
}

struct Ps2Camera {
    pos: Vec3,
    bx: Vec3,
    by: Vec3,
    bz: Vec3,
    focal: f32,
    base_dist: f32,
}

impl Ps2Camera {
    /// Builds the exact MenuCameraMatrix (0x22ED20) from osdbits/menu.c:
    /// fixed Euler rotation {0.031, 0.145, 0.0} applied to pos/fwd/up,
    /// with startup fly-in easing at 0.97^(60 * fly_in_sec).
    fn new(base_dist: f32, fly_in_sec: f32) -> Self {
        let z_offset = -59.0 * 0.97f32.powf((fly_in_sec * 60.0).max(0.0));
        let cam_local = Vec3::new(0.0, 0.0, -base_dist + z_offset);

        let rot_vec = |v: Vec3| -> Vec3 { v.rot_x(0.031).rot_y(0.145).rot_z(0.0) };
        let zd = rot_vec(Vec3::new(0.0, 0.0, 1.0));
        let yd = rot_vec(Vec3::new(0.0, 1.0, 0.0));
        let p = rot_vec(cam_local);

        let bx = yd.cross(zd).normalize();
        let bz = zd.normalize();
        let by = bz.cross(bx);

        Self {
            pos: p,
            bx,
            by,
            bz,
            focal: 2.15,
            base_dist,
        }
    }

    #[inline]
    fn project(&self, world: Vec3) -> (Vec2, f32) {
        let d = world.sub(self.pos);
        let cx = self.bx.dot(d);
        let cy = self.by.dot(d);
        let cz = self.bz.dot(d).max(1.0);
        let sx = cx * (self.focal / cz);
        let sy = cy * (self.focal / cz);
        let depth = (self.base_dist / cz).clamp(0.55, 1.85);
        (Vec2::new(sx, sy), depth)
    }
}

/// Evaluates the exact decompiled PS2 OSDSYS orbit (0x2261B8) for orb i in 0..7
/// driven by local clock seconds (sec, min, hour).
///
/// Post-multiplied chain from osdbits/menu.c:
///   RotZ(tiltZ) * RotZ(PI) * RotY(rateY * 1100.0) * RotX((i + 21) * rateX) * [0, radius, 0]
#[inline]
fn eval_ps2_orb(i: usize, clock_sec: f64, cam: &Ps2Camera) -> (Vec2, f32) {
    let sec = clock_sec.rem_euclid(60.0) as f32;
    let min = (clock_sec / 60.0).rem_euclid(60.0) as f32;
    let hour = (clock_sec / 3600.0).rem_euclid(12.0) as f32;

    let rate_x = sec * (TAU / 60.0);
    let rate_y = min * (TAU / 60.0);
    let tilt_z = hour * (TAU / 12.0);
    let hour_frac = if min > 59.92 {
        let t = ((60.0 - min) / 0.08).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    } else {
        min / 59.92
    };
    let radius = 12.5 + 4.75 * hour_frac;

    let ax = (i as f32 + 21.0) * rate_x;
    let ay = rate_y * 1100.0;

    let world = Vec3::new(0.0, radius, 0.0)
        .rot_x(ax)
        .rot_y(ay)
        .rot_z(PI)
        .rot_z(tilt_z);

    cam.project(world)
}

fn compute_ribbon_normals(pts: &[Vec2], norms: &mut [Vec2]) {
    let last = pts.len() - 1;
    for k in 0..=last {
        let dir = if k == 0 {
            pts[0].sub(pts[1])
        } else if k == last {
            pts[last - 1].sub(pts[last])
        } else {
            pts[k - 1].sub(pts[k + 1])
        };
        norms[k] = dir.perp_norm();
    }
}

/// Mode 1: Authentic decompiled PS2 BIOS Clock Orbs (0x2261B8 + 0x22EFF0)
/// - 7 orbs driven by exact (i + 21) * sec and 1100 * min rotations
/// - 2.2-second trails with asymmetric chromatic decay (R*u^4, G*u^2, B*u)
/// - orbBaseColor {0x30, 0x62, 0x80} = (0.375, 0.766, 1.0) steel-aqua halo + white core
fn build_ps2_original(verts: &mut Vec<Vertex>, clock_sec: f64, fly_in_sec: f32, inv_aspect: f32) {
    const TRAIL_STEPS: usize = 72;
    const TRAIL_SPAN: f64 = 2.20;
    const BASE_RGB: [f32; 3] = [0.375, 0.766, 1.0];

    let cam = Ps2Camera::new(76.0, fly_in_sec);
    let mut pts = [Vec2::new(0.0, 0.0); TRAIL_STEPS + 1];
    let mut depths = [1.0f32; TRAIL_STEPS + 1];
    let mut norms = [Vec2::new(0.0, 1.0); TRAIL_STEPS + 1];

    for i in 0..7 {
        for k in 0..=TRAIL_STEPS {
            let frac = (k as f64) / (TRAIL_STEPS as f64);
            let (p, d) = eval_ps2_orb(i, clock_sec - frac * TRAIL_SPAN, &cam);
            pts[k] = p;
            depths[k] = d;
        }
        compute_ribbon_normals(&pts, &mut norms);

        // 1a. Delicate gossamer outer trail glow
        for k in 0..TRAIL_STEPS {
            let u0 = 1.0 - (k as f32) / (TRAIL_STEPS as f32);
            let u1 = 1.0 - ((k + 1) as f32) / (TRAIL_STEPS as f32);
            let col_at = |u: f32, d: f32| -> [f32; 4] {
                let s = 0.16 * u.powf(1.3) * d.clamp(0.75, 1.25);
                [
                    BASE_RGB[0] * u.powi(4) * s,
                    BASE_RGB[1] * u.powi(2) * s,
                    BASE_RGB[2] * u * s,
                    1.0,
                ]
            };
            let w0 = lerp(0.005, 0.024, u0) * depths[k];
            let w1 = lerp(0.005, 0.024, u1) * depths[k + 1];
            push_ribbon_segment(
                verts,
                pts[k],
                norms[k],
                w0,
                col_at(u0, depths[k]),
                pts[k + 1],
                norms[k + 1],
                w1,
                col_at(u1, depths[k + 1]),
                2.6,
                inv_aspect,
            );
        }

        // 1b. Slender crisp PS2 line-strip core with exact (u^4, u^2, u) decay
        for k in 0..TRAIL_STEPS {
            let u0 = 1.0 - (k as f32) / (TRAIL_STEPS as f32);
            let u1 = 1.0 - ((k + 1) as f32) / (TRAIL_STEPS as f32);
            let col_at = |u: f32, d: f32| -> [f32; 4] {
                let s = 0.52 * d.clamp(0.75, 1.25);
                [
                    BASE_RGB[0] * u.powi(4) * s,
                    BASE_RGB[1] * u.powi(2) * s,
                    BASE_RGB[2] * u * s,
                    1.0,
                ]
            };
            let w0 = lerp(0.0018, 0.0075, u0) * depths[k];
            let w1 = lerp(0.0018, 0.0075, u1) * depths[k + 1];
            push_ribbon_segment(
                verts,
                pts[k],
                norms[k],
                w0,
                col_at(u0, depths[k]),
                pts[k + 1],
                norms[k + 1],
                w1,
                col_at(u1, depths[k + 1]),
                2.0,
                inv_aspect,
            );
        }

        // 2. Authentic jewel-like orb head: soft TEXCBLUR halo + inner aqua + crisp TEXCNAVI core
        let head = pts[0];
        let d = depths[0];

        push_sprite(
            verts,
            head,
            0.115 * d,
            scale_rgb(BASE_RGB, 0.48 * d.clamp(0.75, 1.25)),
            2.8,
            0.0,
            inv_aspect,
        );
        push_sprite(
            verts,
            head,
            0.048 * d,
            scale_rgb([0.58, 0.86, 1.0], 0.62 * d.clamp(0.75, 1.25)),
            3.0,
            0.0,
            inv_aspect,
        );
        push_sprite(
            verts,
            head,
            0.026 * d,
            scale_rgb([0.90, 0.96, 1.0], 0.65),
            3.4,
            0.95,
            inv_aspect,
        );
    }
}

/// Mode 2: Vivid Violet Trails on the authentic PS2 3D clock-harmonic orbit
fn build_ps2_vivid_trails(
    verts: &mut Vec<Vertex>,
    clock_sec: f64,
    fly_in_sec: f32,
    inv_aspect: f32,
) {
    const TRAIL_STEPS: usize = 72;
    const TRAIL_SPAN: f64 = 2.00;
    const BEAD_COUNT: usize = 14;

    let c_head = [0.46, 0.92, 1.0];
    let c_blue = [0.18, 0.45, 1.0];
    let c_violet = [0.54, 0.16, 0.98];
    let c_deep = [0.22, 0.04, 0.62];

    let cam = Ps2Camera::new(68.0, fly_in_sec);
    let mut pts = [Vec2::new(0.0, 0.0); TRAIL_STEPS + 1];
    let mut depths = [1.0f32; TRAIL_STEPS + 1];
    let mut norms = [Vec2::new(0.0, 1.0); TRAIL_STEPS + 1];

    for i in 0..7 {
        for k in 0..=TRAIL_STEPS {
            let frac = (k as f64) / (TRAIL_STEPS as f64);
            let (p, d) = eval_ps2_orb(i, clock_sec - frac * TRAIL_SPAN, &cam);
            pts[k] = p;
            depths[k] = d;
        }
        compute_ribbon_normals(&pts, &mut norms);

        // 1a. Wide lush cyan -> blue -> indigo-violet nebula ribbon
        for k in 0..TRAIL_STEPS {
            let f0 = (k as f32) / (TRAIL_STEPS as f32);
            let f1 = ((k + 1) as f32) / (TRAIL_STEPS as f32);
            let style = |frac: f32, d: f32| -> (f32, [f32; 4]) {
                let rgb = if frac < 0.28 {
                    lerp_rgb(c_blue, c_violet, frac / 0.28)
                } else if frac < 0.72 {
                    lerp_rgb(c_violet, [0.38, 0.08, 0.85], (frac - 0.28) / 0.44)
                } else {
                    lerp_rgb([0.38, 0.08, 0.85], c_deep, (frac - 0.72) / 0.28)
                };
                let fade = (1.0 - frac).powf(1.35);
                let w = lerp(0.045, 0.010, frac.powf(0.8)) * d;
                (w, scale_rgb(rgb, 0.42 * fade * d.clamp(0.75, 1.25)))
            };
            let (w0, c0) = style(f0, depths[k]);
            let (w1, c1) = style(f1, depths[k + 1]);
            push_ribbon_segment(
                verts,
                pts[k],
                norms[k],
                w0,
                c0,
                pts[k + 1],
                norms[k + 1],
                w1,
                c1,
                2.2,
                inv_aspect,
            );
        }

        // 1b. Crisp bright cyan-blue inner thread
        for k in 0..TRAIL_STEPS {
            let f0 = (k as f32) / (TRAIL_STEPS as f32);
            let f1 = ((k + 1) as f32) / (TRAIL_STEPS as f32);
            let style = |frac: f32, d: f32| -> (f32, [f32; 4]) {
                let rgb = if frac < 0.35 {
                    lerp_rgb(c_head, c_blue, frac / 0.35)
                } else {
                    lerp_rgb(c_blue, c_violet, (frac - 0.35) / 0.65)
                };
                let fade = (1.0 - frac).powf(1.7);
                let w = lerp(0.014, 0.003, frac) * d;
                (w, scale_rgb(rgb, 0.65 * fade * d.clamp(0.75, 1.25)))
            };
            let (w0, c0) = style(f0, depths[k]);
            let (w1, c1) = style(f1, depths[k + 1]);
            push_ribbon_segment(
                verts,
                pts[k],
                norms[k],
                w0,
                c0,
                pts[k + 1],
                norms[k + 1],
                w1,
                c1,
                2.5,
                inv_aspect,
            );
        }

        // 2. Subtle fading plasma-echo beads along the first 1.2s of the trail
        for b in 1..=BEAD_COUNT {
            let frac = (b as f32) / ((BEAD_COUNT + 1) as f32);
            let lag = (frac as f64) * 1.20;
            let (bp, bd) = eval_ps2_orb(i, clock_sec - lag, &cam);

            let rgb = if frac < 0.28 {
                lerp_rgb([0.65, 0.96, 1.0], c_blue, frac / 0.28)
            } else {
                lerp_rgb(c_blue, c_violet, (frac - 0.28) / 0.72)
            };
            let fade = (1.0 - frac).powf(1.55);
            let radius = lerp(0.036, 0.018, frac) * bd;
            let glow = 0.42 * fade * bd.clamp(0.75, 1.25);
            let core = if b <= 3 {
                0.24 * (1.0 - (b as f32) / 4.0)
            } else {
                0.0
            };

            push_sprite(
                verts,
                bp,
                radius,
                scale_rgb(rgb, glow),
                2.6,
                core,
                inv_aspect,
            );
        }

        // 3. Luminous white-cyan orb head
        let head = pts[0];
        let d = depths[0];

        push_sprite(
            verts,
            head,
            0.125 * d,
            scale_rgb([0.22, 0.56, 1.0], 0.56 * d.clamp(0.75, 1.25)),
            2.6,
            0.0,
            inv_aspect,
        );
        push_sprite(
            verts,
            head,
            0.056 * d,
            scale_rgb([0.52, 0.92, 1.0], 0.76 * d.clamp(0.75, 1.25)),
            2.9,
            0.0,
            inv_aspect,
        );
        push_sprite(
            verts,
            head,
            0.030 * d,
            scale_rgb([0.88, 0.98, 1.0], 0.72),
            3.3,
            0.96,
            inv_aspect,
        );
    }
}

pub fn build_frame_vertices(
    verts: &mut Vec<Vertex>,
    mode: OrbMode,
    clock_sec: f64,
    fly_in_sec: f32,
    width: u32,
    height: u32,
) {
    verts.clear();
    let h = height.max(1) as f32;
    let w = width.max(1) as f32;
    let inv_aspect = h / w;

    match mode {
        OrbMode::Menu => build_ps2_original(verts, clock_sec, fly_in_sec, inv_aspect),
        OrbMode::Trail => build_ps2_vivid_trails(verts, clock_sec, fly_in_sec, inv_aspect),
    }
}
