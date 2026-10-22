//! The particle field: a few thousand points that spring between shapes,
//! one shape per route. Positions are simulated here and handed to WebGL as
//! a flat `[x, y, z, alpha]` buffer, so the browser only draws.

use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Shape {
    /// Home: a breathing sphere.
    Sphere = 0,
    /// Writing: one sheet per post, stacked like paper.
    Sheets = 1,
    /// Events: time as a helix; events are knots on it, now is a ring.
    Helix = 2,
    /// Reading: a ring that closes as you read.
    Ring = 3,
    /// Calendar: this month; past days are flat, future days have volume.
    Grid = 4,
    /// Not found: scattered.
    Scatter = 5,
    /// Searching: a flat disc.
    Disc = 6,
}

impl Shape {
    pub fn from_u32(v: u32) -> Shape {
        match v {
            1 => Shape::Sheets,
            2 => Shape::Helix,
            3 => Shape::Ring,
            4 => Shape::Grid,
            5 => Shape::Scatter,
            6 => Shape::Disc,
            _ => Shape::Sphere,
        }
    }

    /// Preferred pitch/yaw, and whether the shape slowly turns on its own.
    fn pose(self) -> (f32, f32, bool) {
        match self {
            Shape::Sphere => (0.2, 0.0, true),
            Shape::Sheets => (0.5, -0.5, true),
            Shape::Helix => (0.12, 0.0, true),
            Shape::Ring => (0.0, 0.0, false),
            Shape::Grid => (-0.42, 0.32, false),
            Shape::Scatter => (0.0, 0.0, true),
            Shape::Disc => (0.62, 0.0, true),
        }
    }
}

/// What the shapes need to know about the content.
#[derive(Clone, Default)]
pub struct Params {
    pub sheets: usize,
    /// Event positions on the time axis, 0..1, soonest first.
    pub events: Vec<f32>,
    /// Where now sits on the time axis, 0..1.
    pub now: f32,
    /// Calendar month: weekday of the 1st (Monday = 0), length, and today.
    pub first_weekday: u32,
    pub month_days: u32,
    pub today: u32,
}

pub struct Field {
    pub n: usize,
    params: Params,
    pos: Vec<f32>,
    vel: Vec<f32>,
    tgt: Vec<f32>,
    /// Per-point anchor: cluster centre for knots, resting spot for haze.
    aux: Vec<f32>,
    group: Vec<u16>,
    alpha_base: Vec<f32>,
    alpha: Vec<f32>,
    out: Vec<f32>,
    mvp: [f32; 16],

    base: Shape,
    shape: Shape,
    highlight: u16,
    progress: f32,
    energy: f32,
    time: f32,

    rot: (f32, f32),
    spin: (f32, f32),
    drag: (f32, f32),
    dragging: bool,
    offset: (f32, f32),

    size: (f32, f32),
    /// Camera distance and the visible half-extent at the origin plane.
    dist: f32,
    half: (f32, f32),
    pointer: (f32, f32),
    pointer_on: f32,
}

/// Deterministic per-point noise in 0..1.
pub fn rnd(i: usize, k: u32) -> f32 {
    let mut x = (i as u32).wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

fn unit_sphere(u: f32, v: f32) -> [f32; 3] {
    let y = 1.0 - 2.0 * u;
    let r = (1.0 - y * y).max(0.0).sqrt();
    let a = v * TAU;
    [r * a.cos(), y, r * a.sin()]
}

impl Field {
    pub fn new(n: usize, params: Params) -> Field {
        let mut f = Field {
            n,
            params,
            pos: vec![0.0; n * 3],
            vel: vec![0.0; n * 3],
            tgt: vec![0.0; n * 3],
            aux: vec![0.0; n * 3],
            group: vec![0; n],
            alpha_base: vec![0.0; n],
            alpha: vec![0.0; n],
            out: vec![0.0; n * 4],
            mvp: [0.0; 16],
            base: Shape::Sphere,
            shape: Shape::Sphere,
            highlight: 0,
            progress: 0.0,
            energy: 1.0,
            time: 0.0,
            rot: (0.2, -1.2),
            spin: (0.0, 0.0),
            drag: (0.0, 0.0),
            dragging: false,
            offset: (0.0, 0.0),
            size: (1280.0, 800.0),
            dist: 4.2,
            half: (2.1, 1.32),
            pointer: (-1e4, -1e4),
            pointer_on: 0.0,
        };
        // Start as a loose cloud far out, so the first frame assembles.
        for i in 0..n {
            let d = unit_sphere(rnd(i, 11), rnd(i, 12));
            let r = 3.0 + 3.0 * rnd(i, 13);
            for (slot, v) in f.pos[i * 3..i * 3 + 3].iter_mut().zip(d) {
                *slot = v * r;
            }
        }
        f.build();
        f
    }

    pub fn shape(&self) -> Shape {
        self.shape
    }

    /// The route's own shape.
    pub fn set_base(&mut self, s: Shape) {
        self.base = s;
        self.progress = 0.0;
        self.set_shape(s, 0);
    }

    /// A temporary shape (hover preview, search); `None` returns to base.
    pub fn preview(&mut self, s: Option<Shape>, group: u16) {
        self.set_shape(s.unwrap_or(self.base), group);
    }

    fn set_shape(&mut self, s: Shape, group: u16) {
        self.highlight = group;
        if s != self.shape {
            self.shape = s;
            self.build();
        }
    }

    pub fn set_progress(&mut self, p: f32) {
        self.progress = p.clamp(0.0, 1.0);
    }

    /// 1 is awake, lower is drowsy, 0 is still (reduced motion).
    pub fn set_energy(&mut self, e: f32) {
        self.energy = e.clamp(0.0, 1.0);
    }

    pub fn resize(&mut self, w: f32, h: f32) {
        self.size = (w.max(1.0), h.max(1.0));
    }

    pub fn pointer(&mut self, x: f32, y: f32, on: bool) {
        self.pointer = (x, y);
        self.pointer_on = if on { 1.0 } else { 0.0 };
    }

    pub fn drag(&mut self, dx: f32, dy: f32, held: bool) {
        self.drag.0 += dx;
        self.drag.1 += dy;
        self.dragging = held;
    }

    pub fn spin(&mut self, d: f32) {
        self.spin.1 += d.clamp(-200.0, 200.0) * 0.004;
    }

    pub fn positions(&self) -> &[f32] {
        &self.out
    }

    pub fn mvp(&self) -> &[f32; 16] {
        &self.mvp
    }

    pub fn distance(&self) -> f32 {
        self.dist
    }

    /// The reading ring is an ellipse that frames the text column.
    fn ring_axes(&self) -> (f32, f32) {
        let (hw, hh) = self.half;
        ((hw * 0.72).max(1.15).min(hw * 0.92), hh * 0.92)
    }

    /// Lays out targets, groups and base brightness for the current shape.
    fn build(&mut self) {
        let n = self.n;
        let p = self.params.clone();
        for i in 0..n {
            let (t, g, a, aux) = match self.shape {
                Shape::Sphere => {
                    let y = 1.0 - 2.0 * (i as f32 + 0.5) / n as f32;
                    let r = (1.0 - y * y).sqrt();
                    let phi = i as f32 * PI * (3.0 - 5f32.sqrt());
                    ([r * phi.cos(), y, r * phi.sin()], 0, 0.8, [0.0; 3])
                }
                Shape::Sheets => sheet(i, n, p.sheets.clamp(1, 14)),
                Shape::Helix => helix(i, n, &p),
                Shape::Ring => {
                    let a = rnd(i, 21) * TAU;
                    let r = 1.8 + 0.9 * rnd(i, 22);
                    let haze = [r * a.cos(), r * a.sin(), -0.6 - 1.4 * rnd(i, 23)];
                    (haze, 0, 0.16, haze)
                }
                Shape::Grid => grid(i, n, &p),
                Shape::Scatter => {
                    let d = unit_sphere(rnd(i, 31), rnd(i, 32));
                    let r = 2.4 * rnd(i, 33).cbrt();
                    ([d[0] * r, d[1] * r, d[2] * r], 0, 0.55, [0.0; 3])
                }
                Shape::Disc => {
                    let a = rnd(i, 41) * TAU;
                    let r = 0.5 + 0.65 * rnd(i, 42).sqrt();
                    ([r * a.cos(), (rnd(i, 43) - 0.5) * 0.02, r * a.sin()], 0, 0.6, [0.0; 3])
                }
            };
            self.tgt[i * 3..i * 3 + 3].copy_from_slice(&t);
            self.aux[i * 3..i * 3 + 3].copy_from_slice(&aux);
            self.group[i] = g;
            self.alpha_base[i] = a;
        }
    }

    /// The target for point `i` this frame, after animation and highlight.
    fn target(&self, i: usize) -> ([f32; 3], f32) {
        let mut t = [self.tgt[i * 3], self.tgt[i * 3 + 1], self.tgt[i * 3 + 2]];
        let mut a = self.alpha_base[i];
        let g = self.group[i];
        let lit = self.highlight != 0 && g == self.highlight;
        let dim = self.highlight != 0 && g != 0 && !lit;
        match self.shape {
            Shape::Sphere => {
                let b = 1.0 + 0.04 * self.energy * (self.time * 0.9 + rnd(i, 1) * TAU + t[1] * 2.5).sin();
                t = [t[0] * b, t[1] * b, t[2] * b];
            }
            Shape::Sheets if lit => {
                // The hovered page slides out of the stack.
                t[0] += 0.32;
                t[1] += 0.04;
            }
            Shape::Helix if lit => {
                let c = &self.aux[i * 3..i * 3 + 3];
                for k in 0..3 {
                    t[k] = c[k] + (t[k] - c[k]) * 2.4;
                }
            }
            Shape::Ring => {
                let f = (i as f32 + 0.5) / self.n as f32;
                if f <= self.progress {
                    let ang = PI / 2.0 - f * TAU;
                    let (rx, ry) = self.ring_axes();
                    let j = 1.0 + (rnd(i, 24) - 0.5) * 0.03;
                    t = [rx * j * ang.cos(), ry * j * ang.sin(), (rnd(i, 25) - 0.5) * 0.05];
                    a = 0.85;
                }
            }
            Shape::Disc => {
                let r = (t[0] * t[0] + t[2] * t[2]).sqrt();
                t[1] += 0.03 * self.energy * (self.time * 2.0 - r * 9.0).sin();
            }
            _ => {}
        }
        if lit {
            a = 1.0;
        } else if dim {
            a *= 0.3;
        }
        (t, a)
    }

    /// Advances the simulation by `dt` seconds and refreshes the outputs.
    pub fn step(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, 1.0 / 20.0);
        self.time += dt * (0.25 + 0.75 * self.energy);
        self.turn(dt);
        self.camera(dt);

        let (w, h) = self.size;
        let (px, py) = self.pointer;
        let reach = 0.13 * w.min(h).max(500.0);
        let push = self.pointer_on * self.energy.min(1.0);
        let m = self.mvp;
        let (sx, cx) = self.rot.0.sin_cos();
        let (sy, cy) = self.rot.1.sin_cos();
        let damp = (-dt * 4.8).exp();
        let fade = (dt * 6.0).min(1.0);

        for i in 0..self.n {
            let (t, want) = self.target(i);
            let o = i * 3;
            let k = 7.0 + 8.0 * rnd(i, 2);
            let mut v = [self.vel[o], self.vel[o + 1], self.vel[o + 2]];
            let p = [self.pos[o], self.pos[o + 1], self.pos[o + 2]];
            for j in 0..3 {
                v[j] += (t[j] - p[j]) * k * dt;
            }
            if push > 0.0 {
                // Project, compare with the pointer on screen, and push
                // away along the screen plane, mapped back into the object.
                let cw = m[3] * p[0] + m[7] * p[1] + m[11] * p[2] + m[15];
                if cw > 0.0 {
                    let nx = (m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12]) / cw;
                    let ny = (m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13]) / cw;
                    let dx = (nx * 0.5 + 0.5) * w - px;
                    let dy = (0.5 - ny * 0.5) * h - py;
                    let d2 = dx * dx + dy * dy;
                    if d2 < reach * reach && d2 > 1e-4 {
                        let d = d2.sqrt();
                        let f = (1.0 - d / reach).powi(2) * 9.0 * push * dt;
                        let (ux, uy) = (dx / d * f, -dy / d * f);
                        // Inverse of RotX(pitch) * RotY(yaw), applied to (ux, uy, 0).
                        let (x1, y1, z1) = (ux, cx * uy, -sx * uy);
                        v[0] += cy * x1 - sy * z1;
                        v[1] += y1;
                        v[2] += sy * x1 + cy * z1;
                    }
                }
            }
            for j in 0..3 {
                v[j] *= damp;
                self.vel[o + j] = v[j];
                self.pos[o + j] = p[j] + v[j] * dt;
            }
            self.alpha[i] += (want - self.alpha[i]) * fade;
            let q = i * 4;
            self.out[q..q + 3].copy_from_slice(&self.pos[o..o + 3]);
            self.out[q + 3] = self.alpha[i];
        }
    }

    fn turn(&mut self, dt: f32) {
        let (pitch, yaw, auto) = self.shape.pose();
        let ease = (dt * 2.2).min(1.0);
        if dt > 0.0 && (self.drag.0 != 0.0 || self.drag.1 != 0.0) {
            let (dx, dy) = (self.drag.0 * 0.006, self.drag.1 * 0.006);
            self.rot.1 += dx;
            self.rot.0 += dy;
            self.spin = (dy / dt * 0.5 + self.spin.0 * 0.5, dx / dt * 0.5 + self.spin.1 * 0.5);
            self.drag = (0.0, 0.0);
        } else {
            self.rot.0 += self.spin.0 * dt;
            self.rot.1 += self.spin.1 * dt;
        }
        let decay = (-dt * 2.6).exp();
        self.spin = (self.spin.0 * decay, self.spin.1 * decay);
        if !self.dragging {
            self.rot.0 += (pitch - self.rot.0) * ease;
            if auto {
                self.rot.1 += 0.11 * self.energy * dt;
            } else {
                let goal = yaw + ((self.rot.1 - yaw) / TAU).round() * TAU;
                self.rot.1 += (goal - self.rot.1) * ease;
            }
        }
        self.rot.0 = self.rot.0.clamp(-1.3, 1.3);
    }

    fn camera(&mut self, dt: f32) {
        let (w, h) = self.size;
        let aspect = w / h;
        let half_fov = 17.5f32.to_radians();
        let dist = (4.2f32).max(1.45 / (half_fov.tan() * aspect));
        let half_h = dist * half_fov.tan();
        let half_w = half_h * aspect;
        self.dist = dist;
        self.half = (half_w, half_h);
        let centred = matches!(self.shape, Shape::Ring | Shape::Disc | Shape::Scatter);
        let goal = if centred {
            (0.0, 0.0)
        } else if aspect > 1.15 {
            (half_w * 0.37, 0.0)
        } else {
            (0.0, half_h * 0.3)
        };
        let ease = (dt * 2.0).min(1.0);
        self.offset.0 += (goal.0 - self.offset.0) * ease;
        self.offset.1 += (goal.1 - self.offset.1) * ease;

        let f = 1.0 / half_fov.tan();
        let (near, far) = (0.1, 50.0);
        let nf = 1.0 / (near - far);
        #[rustfmt::skip]
        let proj = [
            f / aspect, 0.0, 0.0, 0.0,
            0.0, f, 0.0, 0.0,
            0.0, 0.0, (far + near) * nf, -1.0,
            0.0, 0.0, 2.0 * far * near * nf, 0.0,
        ];
        let (sx, cx) = self.rot.0.sin_cos();
        let (sy, cy) = self.rot.1.sin_cos();
        // Model-view: translate(offset, -dist) * RotX(pitch) * RotY(yaw), column-major.
        #[rustfmt::skip]
        let mv = [
            cy, sx * sy, -cx * sy, 0.0,
            0.0, cx, sx, 0.0,
            sy, -sx * cy, cx * cy, 0.0,
            self.offset.0, self.offset.1, -dist, 1.0,
        ];
        self.mvp = mul(&proj, &mv);
    }
}

fn mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    out
}

type Point = ([f32; 3], u16, f32, [f32; 3]);

fn sheet(i: usize, n: usize, k: usize) -> Point {
    let per = n / k;
    let s = (i / per).min(k - 1);
    let j = i - s * per;
    let m = if s == k - 1 { n - s * per } else { per };
    let cols = ((m as f32 * 1.55).sqrt().ceil() as usize).max(2);
    let rows = m.div_ceil(cols).max(2);
    let x = ((j % cols) as f32 / (cols - 1) as f32 - 0.5) * 1.75;
    let z = ((j / cols) as f32 / (rows - 1) as f32 - 0.5) * 1.15;
    let gap = (1.9 / k as f32).min(0.2);
    let y = ((k - 1) as f32 / 2.0 - s as f32) * gap + (rnd(i, 3) - 0.5) * 0.006;
    ([x, y, z], s as u16 + 1, 0.8, [0.0; 3])
}

fn helix_at(s: f32) -> [f32; 3] {
    let a = s * 2.6 * TAU;
    [0.62 * a.cos(), (s - 0.5) * 2.3, 0.62 * a.sin()]
}

fn helix(i: usize, n: usize, p: &Params) -> Point {
    let e = p.events.len();
    let per = (n * 30 / 100).checked_div(e).unwrap_or(0);
    let ring = n * 8 / 100;
    let thread = n - per * e - ring;
    if i < thread {
        let s = i as f32 / thread as f32;
        let mut t = helix_at(s);
        for (k, v) in t.iter_mut().enumerate() {
            *v += (rnd(i, 4 + k as u32) - 0.5) * 0.02;
        }
        // The future is drawn lighter than the past.
        (t, 0, if s > p.now { 0.28 } else { 0.62 }, [0.0; 3])
    } else if i < thread + per * e {
        let j = i - thread;
        let ev = j / per;
        let s = p.events[ev];
        let c = helix_at(s);
        let d = unit_sphere((j % per) as f32 / per as f32, rnd(i, 7));
        let r = 0.075 * (0.6 + 0.4 * rnd(i, 8));
        let t = [c[0] + d[0] * r, c[1] + d[1] * r, c[2] + d[2] * r];
        (t, ev as u16 + 1, if s < p.now { 0.45 } else { 0.95 }, c)
    } else {
        let j = i - thread - per * e;
        let a = j as f32 / ring as f32 * TAU;
        let y = (p.now - 0.5) * 2.3;
        ([0.92 * a.cos(), y, 0.92 * a.sin()], 0, 0.7, [0.0; 3])
    }
}

fn grid(i: usize, n: usize, p: &Params) -> Point {
    let rows = (p.first_weekday + p.month_days).div_ceil(7).max(4) as usize;
    let cells = rows * 7;
    let per = n / cells;
    let cell = (i / per.max(1)).min(cells - 1);
    let day = cell as i32 - p.first_weekday as i32 + 1;
    let (size, gap) = (0.2, 0.065);
    let col = (cell % 7) as f32;
    let row = (cell / 7) as f32;
    let cx = (col - 3.0) * (size + gap);
    let cy = ((rows as f32 - 1.0) / 2.0 - row) * (size + gap);
    let inside = day >= 1 && day <= p.month_days as i32;
    let (depth, a) = if !inside {
        (0.0, 0.1)
    } else if (day as u32) < p.today {
        (0.012, 0.35)
    } else if day as u32 == p.today {
        (0.42, 1.0)
    } else {
        (0.2, 0.82)
    };
    let half = if inside { size / 2.0 } else { size / 6.0 };
    let (hx, hy, hz) = (half, half, depth / 2.0);
    // Sample the box surface, weighted by face area.
    let (axy, axz, ayz) = (hx * hy, hx * hz, hy * hz);
    let pick = rnd(i, 51) * (axy + axz + ayz);
    let (u, v) = (rnd(i, 52) * 2.0 - 1.0, rnd(i, 53) * 2.0 - 1.0);
    let side = if rnd(i, 54) < 0.5 { -1.0 } else { 1.0 };
    let local = if pick < axy {
        [u * hx, v * hy, side * hz]
    } else if pick < axy + axz {
        [u * hx, side * hy, v * hz]
    } else {
        [side * hx, u * hy, v * hz]
    };
    ([cx + local[0], cy + local[1], local[2] + hz], 0, a, [0.0; 3])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> Params {
        Params { sheets: 3, events: vec![0.2, 0.7], now: 0.5, first_weekday: 3, month_days: 31, today: 22 }
    }

    #[test]
    fn every_shape_settles_to_finite_positions() {
        let mut f = Field::new(2000, params());
        f.resize(1200.0, 800.0);
        f.pointer(600.0, 400.0, true);
        for s in 0..7 {
            f.set_base(Shape::from_u32(s));
            f.set_progress(0.5);
            for _ in 0..240 {
                f.step(1.0 / 60.0);
            }
            assert!(f.positions().iter().all(|v| v.is_finite()), "shape {s}");
            assert!(f.mvp().iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn highlight_brightens_one_sheet() {
        let mut f = Field::new(900, params());
        f.set_base(Shape::Sheets);
        f.preview(Some(Shape::Sheets), 2);
        for _ in 0..120 {
            f.step(1.0 / 60.0);
        }
        let out = f.positions();
        let lit = (0..900).filter(|&i| f.group[i] == 2).map(|i| out[i * 4 + 3]).fold(0.0, f32::max);
        let dim = (0..900).filter(|&i| f.group[i] == 1).map(|i| out[i * 4 + 3]).fold(0.0, f32::max);
        assert!(lit > 0.95 && dim < 0.3, "lit {lit} dim {dim}");
    }

    #[test]
    fn ring_closes_with_progress() {
        let mut f = Field::new(1000, params());
        f.set_base(Shape::Ring);
        f.set_progress(1.0);
        for _ in 0..400 {
            f.step(1.0 / 60.0);
        }
        let out = f.positions();
        let (rx, ry) = f.ring_axes();
        let on_ring = (0..1000)
            .filter(|&i| ((out[i * 4] / rx).powi(2) + (out[i * 4 + 1] / ry).powi(2) - 1.0).abs() < 0.08)
            .count();
        assert!(on_ring > 950, "{on_ring}");
    }
}
