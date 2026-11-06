//! Collision sandbox: hundreds of circles with mass ∝ r², impulse-based
//! restitution, positional correction and a uniform-grid broad phase.

use super::*;

const STEP: f32 = 1.0 / 120.0;
const BASE_MIN_R: f32 = 4.0;
const BASE_MAX_R: f32 = 13.0;

/// shadcn chart palette (dark), as HSL.
pub const CHART: [(f32, f32, f32); 5] = [
    (220.0, 70.0, 50.0),
    (160.0, 60.0, 45.0),
    (30.0, 80.0, 55.0),
    (280.0, 65.0, 60.0),
    (340.0, 75.0, 55.0),
];

pub struct Balls {
    p: Vec<V2>,
    v: Vec<V2>,
    base_r: Vec<f32>,
    hue: Vec<u8>,
    rng: Rng,
    acc: f32,
    w: f32,
    h: f32,
    grabbed: Option<usize>,
    contacts: usize,
    // broad phase (counting sort into cells)
    cell: f32,
    cols: usize,
    rows: usize,
    cell_start: Vec<u32>,
    cell_items: Vec<u32>,
    ball_cell: Vec<u32>,
    // params
    count: f32,
    gravity: f32,
    restitution: f32,
    scale: f32,
    // toggles
    show_grid: bool,
    show_vel: bool,
    zero_g: bool,
}

impl Default for Balls {
    fn default() -> Self {
        Self {
            p: Vec::new(),
            v: Vec::new(),
            base_r: Vec::new(),
            hue: Vec::new(),
            rng: Rng::new(0xC0FF_EE11),
            acc: 0.0,
            w: 800.0,
            h: 600.0,
            grabbed: None,
            contacts: 0,
            cell: 32.0,
            cols: 1,
            rows: 1,
            cell_start: Vec::new(),
            cell_items: Vec::new(),
            ball_cell: Vec::new(),
            count: 700.0,
            gravity: 1400.0,
            restitution: 0.6,
            scale: 1.0,
            show_grid: true,
            show_vel: false,
            zero_g: false,
        }
    }
}

impl Balls {
    #[inline]
    fn r(&self, i: usize) -> f32 {
        self.base_r[i] * self.scale
    }

    fn spawn_one(&mut self) {
        let r = self.rng.range(BASE_MIN_R, BASE_MAX_R);
        let rs = r * self.scale;
        let x = self.rng.range(rs, (self.w - rs).max(rs + 1.0));
        let y = self.rng.range(rs, (self.h * 0.6).max(rs + 1.0));
        self.p.push(v2(x, y));
        self.v.push(v2(self.rng.range(-150.0, 150.0), self.rng.range(-50.0, 50.0)));
        self.base_r.push(r);
        self.hue.push((self.rng.next_u32() % CHART.len() as u32) as u8);
    }

    fn sync_count(&mut self) {
        let n = self.count.round() as usize;
        if self.p.len() > n {
            self.p.truncate(n);
            self.v.truncate(n);
            self.base_r.truncate(n);
            self.hue.truncate(n);
            if self.grabbed.is_some_and(|g| g >= n) {
                self.grabbed = None;
            }
        }
        while self.p.len() < n {
            self.spawn_one();
        }
    }

    fn build_grid(&mut self) {
        self.cell = (BASE_MAX_R * self.scale * 2.0).max(8.0);
        self.cols = ((self.w / self.cell).ceil() as usize).max(1);
        self.rows = ((self.h / self.cell).ceil() as usize).max(1);
        let n_cells = self.cols * self.rows;
        self.cell_start.clear();
        self.cell_start.resize(n_cells + 1, 0);
        self.ball_cell.clear();
        for p in &self.p {
            let cx = ((p.x / self.cell) as isize).clamp(0, self.cols as isize - 1) as usize;
            let cy = ((p.y / self.cell) as isize).clamp(0, self.rows as isize - 1) as usize;
            let c = cy * self.cols + cx;
            self.ball_cell.push(c as u32);
            self.cell_start[c + 1] += 1;
        }
        for c in 0..n_cells {
            self.cell_start[c + 1] += self.cell_start[c];
        }
        self.cell_items.clear();
        self.cell_items.resize(self.p.len(), 0);
        let mut fill = self.cell_start.clone();
        for (i, &c) in self.ball_cell.iter().enumerate() {
            let slot = &mut fill[c as usize];
            self.cell_items[*slot as usize] = i as u32;
            *slot += 1;
        }
    }

    fn solve_pair(&mut self, i: usize, j: usize, apply_impulse: bool) -> bool {
        let ri = self.r(i);
        let rj = self.r(j);
        let d = self.p[j] - self.p[i];
        let dist2 = d.len2();
        let min = ri + rj;
        if dist2 >= min * min || dist2 < 1e-6 {
            return false;
        }
        let dist = dist2.sqrt();
        let n = d * (1.0 / dist);
        let mi = ri * ri;
        let mj = rj * rj;
        let inv_i = if Some(i) == self.grabbed { 0.0 } else { 1.0 / mi };
        let inv_j = if Some(j) == self.grabbed { 0.0 } else { 1.0 / mj };
        let inv_sum = inv_i + inv_j;
        if inv_sum == 0.0 {
            return true;
        }
        // Positional correction (split by inverse mass).
        let overlap = min - dist;
        let corr = n * (overlap * 0.8 / inv_sum);
        self.p[i] -= corr * inv_i;
        self.p[j] += corr * inv_j;
        if apply_impulse {
            let rel = self.v[j] - self.v[i];
            let vn = rel.dot(n);
            if vn < 0.0 {
                let jmag = -(1.0 + self.restitution) * vn / inv_sum;
                let imp = n * jmag;
                self.v[i] -= imp * inv_i;
                self.v[j] += imp * inv_j;
            }
        }
        true
    }

    fn substep(&mut self, input: &Input) {
        let g = if self.zero_g { 0.0 } else { self.gravity };
        let n = self.p.len();
        // Grab / release.
        if input.left_pressed && input.inside {
            let mut best = None;
            let mut best_d = f32::MAX;
            for i in 0..n {
                let d = (self.p[i] - input.pos).len() - self.r(i);
                if d < best_d && d < 24.0 {
                    best_d = d;
                    best = Some(i);
                }
            }
            self.grabbed = best;
        }
        if !input.left {
            self.grabbed = None;
        }

        for i in 0..n {
            let mut v = self.v[i];
            v.y += g * STEP;
            if input.right && input.inside {
                let d = self.p[i] - input.pos;
                let dist = d.len().max(1.0);
                if dist < 260.0 {
                    let k = (1.0 - dist / 260.0) * 9000.0;
                    v += d * (k * STEP / dist);
                }
            }
            v = v * 0.9995;
            self.v[i] = v;
        }
        if let Some(gi) = self.grabbed {
            // Critically-damped spring to the cursor.
            let target = input.pos;
            let vel = (target - self.p[gi]) * 18.0;
            self.v[gi] = vel;
        }
        for i in 0..n {
            let v = self.v[i];
            self.p[i] += v * STEP;
        }

        self.build_grid();
        self.contacts = 0;
        let iterations = 3;
        for it in 0..iterations {
            let first = it == 0;
            for i in 0..n {
                let c = self.ball_cell[i] as usize;
                let cx = (c % self.cols) as isize;
                let cy = (c / self.cols) as isize;
                for oy in -1..=1isize {
                    let y = cy + oy;
                    if y < 0 || y >= self.rows as isize {
                        continue;
                    }
                    for ox in -1..=1isize {
                        let x = cx + ox;
                        if x < 0 || x >= self.cols as isize {
                            continue;
                        }
                        let cell = y as usize * self.cols + x as usize;
                        let (s, e) = (self.cell_start[cell] as usize, self.cell_start[cell + 1] as usize);
                        for k in s..e {
                            let j = self.cell_items[k] as usize;
                            if j > i && self.solve_pair(i, j, first) && first {
                                self.contacts += 1;
                            }
                        }
                    }
                }
            }
            // Walls.
            for i in 0..n {
                let r = self.r(i);
                let e = self.restitution;
                let p = &mut self.p[i];
                let v = &mut self.v[i];
                if p.x < r {
                    p.x = r;
                    if v.x < 0.0 {
                        v.x = -v.x * e;
                    }
                } else if p.x > self.w - r {
                    p.x = self.w - r;
                    if v.x > 0.0 {
                        v.x = -v.x * e;
                    }
                }
                if p.y < r {
                    p.y = r;
                    if v.y < 0.0 {
                        v.y = -v.y * e;
                    }
                } else if p.y > self.h - r {
                    p.y = self.h - r;
                    if v.y > 0.0 {
                        v.y = -v.y * e;
                    }
                    v.x *= 0.995; // floor friction
                }
            }
        }
    }
}

impl Sim for Balls {
    fn title(&self) -> &'static str {
        "Collision Sandbox"
    }
    fn tab_label(&self) -> &'static str {
        "Collide"
    }
    fn description(&self) -> &'static str {
        "Impulse-based rigid circles with mass ∝ r², restitution, and a uniform-grid broad phase."
    }

    fn reset(&mut self, w: f32, h: f32) {
        self.w = w;
        self.h = h;
        self.p.clear();
        self.v.clear();
        self.base_r.clear();
        self.hue.clear();
        self.grabbed = None;
        self.acc = 0.0;
        self.sync_count();
    }

    fn step(&mut self, dt: f32, w: f32, h: f32, input: &Input) {
        self.w = w;
        self.h = h;
        self.sync_count();
        self.acc = (self.acc + dt).min(STEP * 6.0);
        let mut first = true;
        while self.acc >= STEP {
            let mut inp = *input;
            if !first {
                inp.left_pressed = false;
                inp.right_pressed = false;
            }
            self.substep(&inp);
            self.acc -= STEP;
            first = false;
        }
    }

    fn draw(&self, out: &mut DrawList, input: &Input) {
        if self.show_grid && self.cell_start.len() == self.cols * self.rows + 1 {
            // Occupied cells of the broad phase, brighter where crowded.
            for cell in 0..self.cols * self.rows {
                let count = self.cell_start[cell + 1] - self.cell_start[cell];
                if count == 0 {
                    continue;
                }
                let x = (cell % self.cols) as f32 * self.cell;
                let y = (cell / self.cols) as f32 * self.cell;
                let a = (count as f32 / 4.0).min(1.0);
                out.rects.push(Rect {
                    x,
                    y,
                    w: self.cell,
                    h: self.cell,
                    fill: gpui::hsla(220.0 / 360.0, 0.7, 0.55, 0.03 + 0.07 * a),
                    border: gpui::hsla(220.0 / 360.0, 0.7, 0.65, 0.10 + 0.12 * a),
                });
            }
        }
        out.dots.reserve(self.p.len() + 4);
        for i in 0..self.p.len() {
            let (h, s, l) = CHART[self.hue[i] as usize];
            let speed = (self.v[i].len() / 1200.0).min(1.0);
            let l = l + speed * 22.0;
            out.dots.push(Dot {
                p: self.p[i],
                r: self.r(i),
                color: gpui::hsla(h / 360.0, s / 100.0, l / 100.0, 0.92),
                border: Some(gpui::hsla(h / 360.0, s / 100.0, (l + 18.0).min(95.0) / 100.0, 0.9)),
            });
        }
        if let Some(g) = self.grabbed {
            out.dots.push(Dot {
                p: self.p[g],
                r: self.r(g) + 6.0,
                color: gpui::hsla(0.0, 0.0, 1.0, 0.06),
                border: Some(gpui::hsla(0.0, 0.0, 1.0, 0.85)),
            });
            out.over.push(Lines {
                color: gpui::hsla(0.0, 0.0, 1.0, 0.5),
                width: 1.0,
                segs: vec![[self.p[g].x, self.p[g].y, input.pos.x, input.pos.y]],
            });
        }
        if input.right && input.inside {
            out.dots.push(Dot {
                p: input.pos,
                r: 260.0,
                color: gpui::hsla(345.0 / 360.0, 0.75, 0.55, 0.04),
                border: Some(gpui::hsla(345.0 / 360.0, 0.75, 0.6, 0.35)),
            });
        }
        if self.show_vel {
            let mut l = Lines { color: gpui::hsla(0.0, 0.0, 0.98, 0.55), width: 1.0, segs: Vec::with_capacity(self.p.len()) };
            for (p, v) in self.p.iter().zip(&self.v) {
                let e = *p + *v * 0.06;
                l.segs.push([p.x, p.y, e.x, e.y]);
            }
            out.over.push(l);
        }
    }

    fn sliders(&self) -> Vec<SliderSpec> {
        vec![
            SliderSpec { label: "Bodies", value: self.count, min: 50.0, max: 1800.0, step: 10.0, unit: "", decimals: 0 },
            SliderSpec { label: "Gravity", value: self.gravity, min: 0.0, max: 3000.0, step: 50.0, unit: " px/s²", decimals: 0 },
            SliderSpec { label: "Restitution", value: self.restitution, min: 0.0, max: 1.0, step: 0.01, unit: "", decimals: 2 },
            SliderSpec { label: "Radius scale", value: self.scale, min: 0.5, max: 2.0, step: 0.05, unit: "×", decimals: 2 },
        ]
    }

    fn set_slider(&mut self, ix: usize, v: f32) {
        match ix {
            0 => self.count = v,
            1 => self.gravity = v,
            2 => self.restitution = v,
            3 => self.scale = v,
            _ => {}
        }
    }

    fn toggles(&self) -> Vec<ToggleSpec> {
        vec![
            ToggleSpec { label: "Broad-phase grid", description: "Visualise spatial hash occupancy", value: self.show_grid },
            ToggleSpec { label: "Velocity vectors", description: "Draw v·Δt for every body", value: self.show_vel },
            ToggleSpec { label: "Zero-G", description: "Disable gravity entirely", value: self.zero_g },
        ]
    }

    fn set_toggle(&mut self, ix: usize, v: bool) {
        match ix {
            0 => self.show_grid = v,
            1 => self.show_vel = v,
            2 => self.zero_g = v,
            _ => {}
        }
    }

    fn action_label(&self) -> &'static str {
        "Explode"
    }

    fn action(&mut self) {
        let c = v2(self.w * 0.5, self.h * 0.85);
        for i in 0..self.p.len() {
            let d = self.p[i] - c;
            let dist = d.len().max(10.0);
            let k = 2600.0 * (1.0 - (dist / (self.w.max(self.h))).min(1.0));
            self.v[i] += d * (k / dist) + v2(0.0, -400.0);
        }
    }

    fn bodies(&self) -> Stat {
        Stat { label: "Bodies", value: super::gravity::fmt_int(self.p.len()), caption: "rigid circles @ 120 Hz".into() }
    }

    fn extra_stat(&self) -> Stat {
        Stat {
            label: "Contacts",
            value: super::gravity::fmt_int(self.contacts),
            caption: "resolved pairs this step".into(),
        }
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![("LMB", "grab & fling"), ("RMB", "blast"), ("E", "explode")]
    }
}
