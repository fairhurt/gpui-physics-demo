//! Position-based (Verlet) cloth with distance constraints, wind, grabbing,
//! cutting and strain-based tearing.

use super::*;

const STEP: f32 = 1.0 / 120.0;
const STRAIN_BUCKETS: usize = 9;
const GRAB_RADIUS: f32 = 38.0;
const CUT_RADIUS: f32 = 9.0;

const STRAIN_RAMP: [(f32, f32, f32); 5] = [
    (240.0, 6.0, 62.0),
    (220.0, 70.0, 58.0),
    (280.0, 65.0, 64.0),
    (340.0, 78.0, 60.0),
    (32.0, 92.0, 62.0),
];

#[derive(Clone, Copy)]
struct Stick {
    a: u32,
    b: u32,
    rest: f32,
}

pub struct Cloth {
    pos: Vec<V2>,
    prev: Vec<V2>,
    pinned: Vec<bool>,
    sticks: Vec<Stick>,
    grabbed: Vec<(u32, V2)>,
    acc: f32,
    t: f32,
    w: f32,
    h: f32,
    gust: f32,
    torn: usize,
    dirty: bool,
    // params
    resolution: f32,
    iterations: f32,
    gravity: f32,
    wind: f32,
    tear: f32,
    // toggles
    wind_on: bool,
    heat: bool,
    show_points: bool,
}

impl Default for Cloth {
    fn default() -> Self {
        Self {
            pos: Vec::new(),
            prev: Vec::new(),
            pinned: Vec::new(),
            sticks: Vec::new(),
            grabbed: Vec::new(),
            acc: 0.0,
            t: 0.0,
            w: 800.0,
            h: 600.0,
            gust: 0.0,
            torn: 0,
            dirty: false,
            resolution: 56.0,
            iterations: 14.0,
            gravity: 900.0,
            wind: 0.35,
            tear: 2.6,
            wind_on: true,
            heat: true,
            show_points: false,
        }
    }
}

impl Cloth {
    fn build(&mut self) {
        self.pos.clear();
        self.prev.clear();
        self.pinned.clear();
        self.sticks.clear();
        self.grabbed.clear();
        self.torn = 0;
        self.t = 0.0;
        self.gust = 0.0;

        let cols = (self.resolution.round() as usize).max(4);
        let width = self.w * 0.64;
        let spacing = width / (cols - 1) as f32;
        let rows = (((self.h * 0.60) / spacing).floor() as usize + 1).max(3);
        let x0 = (self.w - width) * 0.5;
        let y0 = self.h * 0.09;
        let pin_every = (cols / 10).max(2);

        for r in 0..rows {
            for c in 0..cols {
                let p = v2(x0 + c as f32 * spacing, y0 + r as f32 * spacing);
                self.pos.push(p);
                self.prev.push(p);
                self.pinned.push(r == 0 && (c % pin_every == 0 || c == cols - 1));
            }
        }
        let idx = |r: usize, c: usize| (r * cols + c) as u32;
        for r in 0..rows {
            for c in 0..cols {
                if c + 1 < cols {
                    self.sticks.push(Stick { a: idx(r, c), b: idx(r, c + 1), rest: spacing });
                }
                if r + 1 < rows {
                    self.sticks.push(Stick { a: idx(r, c), b: idx(r + 1, c), rest: spacing });
                }
            }
        }
    }

    fn wind_at(&self, p: V2) -> V2 {
        if !self.wind_on {
            return V2::default();
        }
        let t = self.t;
        let s = self.wind * 1100.0 * (1.0 + self.gust);
        let fx = s * (0.5 + 0.5 * (t * 0.9 + p.y * 0.012).sin() * (t * 0.37 + p.x * 0.004).cos());
        let fy = s * 0.22 * (t * 1.7 + p.x * 0.02).sin();
        v2(fx, fy)
    }

    fn substep(&mut self, input: &Input) {
        self.t += STEP;
        let g = v2(0.0, self.gravity);
        let dt2 = STEP * STEP;

        // Integrate.
        for i in 0..self.pos.len() {
            if self.pinned[i] {
                continue;
            }
            let p = self.pos[i];
            let vel = (p - self.prev[i]) * 0.994;
            self.prev[i] = p;
            let a = g + self.wind_at(p);
            self.pos[i] = p + vel + a * dt2;
        }

        // Grabbed points follow the cursor.
        if input.left {
            for &(i, off) in &self.grabbed {
                let i = i as usize;
                self.prev[i] = self.pos[i];
                self.pos[i] = input.pos + off;
            }
        }

        // Satisfy constraints; tear over-stretched sticks on the last pass.
        let iters = self.iterations.round().max(1.0) as usize;
        let tear = self.tear;
        let mut torn_any = false;
        for it in 0..iters {
            let last = it + 1 == iters;
            for s in self.sticks.iter_mut() {
                let (a, b) = (s.a as usize, s.b as usize);
                let d = self.pos[b] - self.pos[a];
                let len = d.len().max(1e-4);
                if last && len > s.rest * tear {
                    s.rest = -1.0; // mark torn
                    torn_any = true;
                    continue;
                }
                let diff = (len - s.rest) / len;
                let pa = self.pinned[a];
                let pb = self.pinned[b];
                match (pa, pb) {
                    (true, true) => {}
                    (true, false) => self.pos[b] -= d * diff,
                    (false, true) => self.pos[a] += d * diff,
                    (false, false) => {
                        let off = d * (0.5 * diff);
                        self.pos[a] += off;
                        self.pos[b] -= off;
                    }
                }
            }
        }
        if torn_any {
            let before = self.sticks.len();
            self.sticks.retain(|s| s.rest > 0.0);
            self.torn += before - self.sticks.len();
        }

        // Keep inside the canvas.
        for i in 0..self.pos.len() {
            let p = &mut self.pos[i];
            if p.x < 0.0 || p.x > self.w || p.y < 0.0 || p.y > self.h {
                p.x = p.x.clamp(0.0, self.w);
                p.y = p.y.clamp(0.0, self.h);
                // Friction against the walls.
                self.prev[i] = self.prev[i] + (*p - self.prev[i]) * 0.2;
            }
        }
    }

    fn cut(&mut self, a: V2, b: V2) {
        let ab = b - a;
        let l2 = ab.len2().max(1e-6);
        let before = self.sticks.len();
        let pos = &self.pos;
        self.sticks.retain(|s| {
            let m = (pos[s.a as usize] + pos[s.b as usize]) * 0.5;
            let t = ((m - a).dot(ab) / l2).clamp(0.0, 1.0);
            let q = a + ab * t;
            (m - q).len2() > CUT_RADIUS * CUT_RADIUS
        });
        self.torn += before - self.sticks.len();
    }
}

impl Sim for Cloth {
    fn title(&self) -> &'static str {
        "Verlet Cloth"
    }
    fn tab_label(&self) -> &'static str {
        "Cloth"
    }
    fn description(&self) -> &'static str {
        "Position-based dynamics: Verlet integration, iterative distance constraints and strain tearing."
    }

    fn reset(&mut self, w: f32, h: f32) {
        self.w = w;
        self.h = h;
        self.acc = 0.0;
        self.build();
    }

    fn step(&mut self, dt: f32, w: f32, h: f32, input: &Input) {
        self.w = w;
        self.h = h;
        if self.dirty {
            self.dirty = false;
            self.build();
        }
        self.gust *= (-dt * 1.2).exp();

        if input.left_pressed && input.inside {
            self.grabbed.clear();
            for (i, p) in self.pos.iter().enumerate() {
                let d = *p - input.pos;
                if d.len2() < GRAB_RADIUS * GRAB_RADIUS && !self.pinned[i] {
                    self.grabbed.push((i as u32, d * 0.6));
                }
            }
        }
        if !input.left {
            self.grabbed.clear();
        }
        if input.right && input.inside {
            let from = if input.right_pressed { input.pos } else { input.prev_pos };
            self.cut(from, input.pos);
        }

        self.acc = (self.acc + dt).min(STEP * 5.0);
        while self.acc >= STEP {
            self.substep(input);
            self.acc -= STEP;
        }
    }

    fn draw(&self, out: &mut DrawList, input: &Input) {
        let mut lines = if self.heat {
            bucketed_lines(STRAIN_BUCKETS, 1.15, 0.9, &STRAIN_RAMP)
        } else {
            vec![Lines { color: gpui::hsla(0.0, 0.0, 0.95, 0.7), width: 1.15, segs: Vec::new() }]
        };
        let max_strain = 0.22;
        for s in &self.sticks {
            let a = self.pos[s.a as usize];
            let b = self.pos[s.b as usize];
            let bucket = if self.heat {
                let strain = ((b - a).len() / s.rest - 1.0).max(0.0);
                ((strain / max_strain).min(1.0) * (STRAIN_BUCKETS - 1) as f32).round() as usize
            } else {
                0
            };
            lines[bucket].segs.push([a.x, a.y, b.x, b.y]);
        }
        out.under = lines;

        if self.show_points {
            out.dots.reserve(self.pos.len());
            for p in &self.pos {
                out.dots.push(Dot { p: *p, r: 1.4, color: gpui::hsla(0.0, 0.0, 1.0, 0.55), border: None });
            }
        }
        for (i, p) in self.pos.iter().enumerate() {
            if self.pinned[i] {
                out.dots.push(Dot {
                    p: *p,
                    r: 3.5,
                    color: gpui::hsla(0.0, 0.0, 0.98, 1.0),
                    border: Some(gpui::hsla(0.0, 0.0, 0.4, 1.0)),
                });
            }
        }
        if input.inside && input.left && !self.grabbed.is_empty() {
            out.dots.push(Dot {
                p: input.pos,
                r: GRAB_RADIUS,
                color: gpui::hsla(160.0 / 360.0, 0.6, 0.45, 0.06),
                border: Some(gpui::hsla(160.0 / 360.0, 0.6, 0.55, 0.6)),
            });
        }
        if input.inside && input.right {
            out.dots.push(Dot {
                p: input.pos,
                r: CUT_RADIUS + 3.0,
                color: gpui::hsla(345.0 / 360.0, 0.75, 0.55, 0.2),
                border: Some(gpui::hsla(345.0 / 360.0, 0.8, 0.62, 0.9)),
            });
        }
    }

    fn sliders(&self) -> Vec<SliderSpec> {
        vec![
            SliderSpec { label: "Resolution", value: self.resolution, min: 16.0, max: 96.0, step: 2.0, unit: " cols", decimals: 0 },
            SliderSpec { label: "Solver iterations", value: self.iterations, min: 1.0, max: 40.0, step: 1.0, unit: "", decimals: 0 },
            SliderSpec { label: "Gravity", value: self.gravity, min: 0.0, max: 2500.0, step: 50.0, unit: " px/s²", decimals: 0 },
            SliderSpec { label: "Wind", value: self.wind, min: 0.0, max: 1.5, step: 0.01, unit: "", decimals: 2 },
            SliderSpec { label: "Tear threshold", value: self.tear, min: 1.3, max: 6.0, step: 0.1, unit: "×", decimals: 1 },
        ]
    }

    fn set_slider(&mut self, ix: usize, v: f32) {
        match ix {
            0 => {
                if (self.resolution - v).abs() > f32::EPSILON {
                    self.resolution = v;
                    self.dirty = true;
                }
            }
            1 => self.iterations = v,
            2 => self.gravity = v,
            3 => self.wind = v,
            4 => self.tear = v,
            _ => {}
        }
    }

    fn toggles(&self) -> Vec<ToggleSpec> {
        vec![
            ToggleSpec { label: "Wind field", description: "Spatio-temporal sine turbulence", value: self.wind_on },
            ToggleSpec { label: "Strain heatmap", description: "Colour constraints by stretch", value: self.heat },
            ToggleSpec { label: "Show particles", description: "Render every point mass", value: self.show_points },
        ]
    }

    fn set_toggle(&mut self, ix: usize, v: bool) {
        match ix {
            0 => self.wind_on = v,
            1 => self.heat = v,
            2 => self.show_points = v,
            _ => {}
        }
    }

    fn action_label(&self) -> &'static str {
        "Gust"
    }

    fn action(&mut self) {
        self.wind_on = true;
        self.gust = 4.0;
    }

    fn bodies(&self) -> Stat {
        Stat { label: "Point masses", value: super::gravity::fmt_int(self.pos.len()), caption: "Verlet @ 120 Hz".into() }
    }

    fn extra_stat(&self) -> Stat {
        Stat {
            label: "Constraints",
            value: super::gravity::fmt_int(self.sticks.len()),
            caption: format!("{} torn · {}× solver", super::gravity::fmt_int(self.torn), self.iterations.round()),
        }
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![("LMB", "grab"), ("RMB", "cut"), ("E", "gust")]
    }
}
