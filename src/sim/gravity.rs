//! Gravity wells: thousands of massless particles orbiting a handful of
//! moving point masses. The cursor becomes a well (left) or a repulsor (right).

use super::*;

const MAX_SPEED: f32 = 1600.0;
const SOFTENING2: f32 = 28.0 * 28.0;
const G: f32 = 2.4e6;
const SPEED_BUCKETS: usize = 10;

pub struct Gravity {
    pos: Vec<V2>,
    vel: Vec<V2>,
    wells: Vec<V2>,
    rng: Rng,
    t: f32,
    w: f32,
    h: f32,
    // parameters
    count: f32,
    strength: f32,
    damping: f32,
    well_count: f32,
    // toggles
    streaks: bool,
    orbit: bool,
    heat: bool,
}

impl Default for Gravity {
    fn default() -> Self {
        Self {
            pos: Vec::new(),
            vel: Vec::new(),
            wells: Vec::new(),
            rng: Rng::new(0x9E37_79B9),
            t: 0.0,
            w: 800.0,
            h: 600.0,
            count: 8000.0,
            strength: 1.0,
            damping: 0.12,
            well_count: 3.0,
            streaks: true,
            orbit: true,
            heat: true,
        }
    }
}

impl Gravity {
    fn center(&self) -> V2 {
        v2(self.w * 0.5, self.h * 0.5)
    }

    fn update_wells(&mut self) {
        let n = self.well_count.round() as usize;
        let c = self.center();
        let rx = self.w * 0.24;
        let ry = self.h * 0.22;
        let t = if self.orbit { self.t } else { 0.0 };
        self.wells.clear();
        for i in 0..n {
            let phase = i as f32 / n.max(1) as f32 * std::f32::consts::TAU;
            if n == 1 {
                self.wells.push(c);
                continue;
            }
            let a = t * 0.35 + phase;
            self.wells
                .push(c + v2(a.cos() * rx, (a * 1.0 + 0.6 * (t * 0.21 + phase).sin()).sin() * ry));
        }
    }

    fn accel(&self, p: V2, input: &Input) -> V2 {
        let g = G * self.strength;
        let mut a = V2::default();
        for &w in &self.wells {
            let d = w - p;
            let r2 = d.len2() + SOFTENING2;
            a += d * (g / (r2 * r2.sqrt()));
        }
        if input.inside && (input.left || input.right) {
            let d = input.pos - p;
            let r2 = d.len2() + SOFTENING2;
            let k = if input.left { 2.5 } else { -3.5 };
            a += d * (k * g / (r2 * r2.sqrt()));
        }
        a
    }

    fn spawn(&mut self) -> (V2, V2) {
        let c = self.center();
        let max_r = self.w.min(self.h) * 0.46;
        // Bias toward the center for a galaxy-ish density profile.
        let r = 30.0 + max_r * self.rng.f().powf(0.7);
        let theta = self.rng.f() * std::f32::consts::TAU;
        let p = c + v2(theta.cos() * r, theta.sin() * r);
        // Circular orbit speed around the combined wells: v = sqrt(|a| r)
        let a = self.accel(p, &Input::default()).len();
        let speed = (a * r).sqrt() * self.rng.range(0.85, 1.1);
        let dir = (p - c).perp() * (1.0 / r);
        (p, dir * speed.min(MAX_SPEED))
    }

    fn sync_count(&mut self) {
        let n = self.count.round() as usize;
        if self.pos.len() > n {
            self.pos.truncate(n);
            self.vel.truncate(n);
        }
        while self.pos.len() < n {
            let (p, v) = self.spawn();
            self.pos.push(p);
            self.vel.push(v);
        }
    }
}

impl Sim for Gravity {
    fn title(&self) -> &'static str {
        "Gravity Wells"
    }
    fn tab_label(&self) -> &'static str {
        "Gravity"
    }
    fn description(&self) -> &'static str {
        "Massless particles integrated against moving point masses with softened inverse-square gravity."
    }

    fn reset(&mut self, w: f32, h: f32) {
        self.w = w;
        self.h = h;
        self.t = 0.0;
        self.pos.clear();
        self.vel.clear();
        self.update_wells();
        self.sync_count();
    }

    fn step(&mut self, dt: f32, w: f32, h: f32, input: &Input) {
        self.w = w;
        self.h = h;
        self.sync_count();
        // Two substeps per frame keeps close fly-bys stable.
        let substeps = 2;
        let h_dt = dt / substeps as f32;
        let damp = (1.0 - self.damping * h_dt).max(0.0);
        for _ in 0..substeps {
            self.t += h_dt;
            self.update_wells();
            for i in 0..self.pos.len() {
                let p = self.pos[i];
                let a = self.accel(p, input);
                let mut v = (self.vel[i] + a * h_dt) * damp;
                let s2 = v.len2();
                if s2 > MAX_SPEED * MAX_SPEED {
                    v = v * (MAX_SPEED / s2.sqrt());
                }
                let mut np = p + v * h_dt;
                // Soft walls.
                if np.x < 0.0 || np.x > w {
                    v.x = -v.x * 0.5;
                    np.x = np.x.clamp(0.0, w);
                }
                if np.y < 0.0 || np.y > h {
                    v.y = -v.y * 0.5;
                    np.y = np.y.clamp(0.0, h);
                }
                self.vel[i] = v;
                self.pos[i] = np;
            }
        }
    }

    fn draw(&self, out: &mut DrawList, input: &Input) {
        let mut streaks = bucketed_lines(SPEED_BUCKETS, 1.0, 0.45, &HEAT);
        out.dots.reserve(self.pos.len() + 16);
        let plain = gpui::hsla(0.0, 0.0, 0.92, 0.75);
        for (p, v) in self.pos.iter().zip(&self.vel) {
            let s = v.len() / 900.0;
            let color = if self.heat { ramp(&HEAT, s, 0.9) } else { plain };
            if self.streaks {
                let b = ((s.min(1.0)) * (SPEED_BUCKETS - 1) as f32).round() as usize;
                let tail = *p - *v * 0.022;
                streaks[b].segs.push([p.x, p.y, tail.x, tail.y]);
            }
            out.dots.push(Dot { p: *p, r: 1.3, color, border: None });
        }
        if self.streaks {
            if !self.heat {
                for l in &mut streaks {
                    l.color = gpui::hsla(0.0, 0.0, 0.9, 0.3);
                }
            }
            out.under = streaks;
        }
        // Wells: a bright core with soft halos.
        for w in &self.wells {
            for (r, a) in [(30.0, 0.04), (18.0, 0.08), (10.0, 0.18)] {
                out.dots.push(Dot {
                    p: *w,
                    r,
                    color: gpui::hsla(0.0, 0.0, 1.0, a),
                    border: Some(gpui::hsla(0.0, 0.0, 1.0, a * 1.6)),
                });
            }
            out.dots.push(Dot { p: *w, r: 4.0, color: gpui::hsla(0.0, 0.0, 1.0, 1.0), border: None });
        }
        if input.inside && (input.left || input.right) {
            let hue = if input.left { 160.0 } else { 345.0 };
            for (r, a) in [(44.0, 0.06), (24.0, 0.14)] {
                out.dots.push(Dot {
                    p: input.pos,
                    r,
                    color: gpui::hsla(hue / 360.0, 0.7, 0.55, a),
                    border: Some(gpui::hsla(hue / 360.0, 0.7, 0.6, 0.5)),
                });
            }
        }
    }

    fn sliders(&self) -> Vec<SliderSpec> {
        vec![
            SliderSpec { label: "Particles", value: self.count, min: 500.0, max: 20000.0, step: 500.0, unit: "", decimals: 0 },
            SliderSpec { label: "Gravity", value: self.strength, min: 0.1, max: 3.0, step: 0.05, unit: "×", decimals: 2 },
            SliderSpec { label: "Damping", value: self.damping, min: 0.0, max: 1.5, step: 0.01, unit: "", decimals: 2 },
            SliderSpec { label: "Wells", value: self.well_count, min: 1.0, max: 6.0, step: 1.0, unit: "", decimals: 0 },
        ]
    }

    fn set_slider(&mut self, ix: usize, v: f32) {
        match ix {
            0 => self.count = v,
            1 => self.strength = v,
            2 => self.damping = v,
            3 => {
                self.well_count = v;
                self.update_wells();
            }
            _ => {}
        }
    }

    fn toggles(&self) -> Vec<ToggleSpec> {
        vec![
            ToggleSpec { label: "Motion streaks", description: "Velocity tails, tessellated paths", value: self.streaks },
            ToggleSpec { label: "Orbiting wells", description: "Animate the point masses", value: self.orbit },
            ToggleSpec { label: "Speed heatmap", description: "Colour particles by |v|", value: self.heat },
        ]
    }

    fn set_toggle(&mut self, ix: usize, v: bool) {
        match ix {
            0 => self.streaks = v,
            1 => self.orbit = v,
            2 => self.heat = v,
            _ => {}
        }
    }

    fn action_label(&self) -> &'static str {
        "Supernova"
    }

    fn action(&mut self) {
        let c = self.center();
        for (p, v) in self.pos.iter().zip(self.vel.iter_mut()) {
            let d = *p - c;
            let r = d.len().max(1.0);
            *v += d * (900.0 / r) + d.perp() * (200.0 / r);
        }
    }

    fn bodies(&self) -> Stat {
        Stat { label: "Particles", value: fmt_int(self.pos.len()), caption: "integrated per frame ×2".into() }
    }

    fn extra_stat(&self) -> Stat {
        Stat {
            label: "Force evals",
            value: fmt_int(self.pos.len() * (self.wells.len() + 1) * 2),
            caption: "point-mass interactions / frame".into(),
        }
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![("LMB", "attract"), ("RMB", "repel"), ("E", "supernova")]
    }
}

pub fn fmt_int(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}
