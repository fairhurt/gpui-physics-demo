//! Simulation core: tiny vector math, input, draw lists and the `Sim` trait
//! every scene implements. Nothing in here knows about GPUI layout — scenes
//! emit a `DrawList` in local canvas coordinates and the view paints it.

pub mod balls;
pub mod cloth;
pub mod gravity;

use gpui::Hsla;
use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

// ---------------------------------------------------------------- math ----

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V2 {
    pub x: f32,
    pub y: f32,
}

pub const fn v2(x: f32, y: f32) -> V2 {
    V2 { x, y }
}

impl V2 {
    #[inline]
    pub fn len2(self) -> f32 {
        self.x * self.x + self.y * self.y
    }
    #[inline]
    pub fn len(self) -> f32 {
        self.len2().sqrt()
    }
    #[inline]
    pub fn dot(self, o: V2) -> f32 {
        self.x * o.x + self.y * o.y
    }
    #[inline]
    pub fn perp(self) -> V2 {
        v2(-self.y, self.x)
    }
}

impl Add for V2 {
    type Output = V2;
    #[inline]
    fn add(self, o: V2) -> V2 {
        v2(self.x + o.x, self.y + o.y)
    }
}
impl Sub for V2 {
    type Output = V2;
    #[inline]
    fn sub(self, o: V2) -> V2 {
        v2(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f32> for V2 {
    type Output = V2;
    #[inline]
    fn mul(self, s: f32) -> V2 {
        v2(self.x * s, self.y * s)
    }
}
impl Neg for V2 {
    type Output = V2;
    #[inline]
    fn neg(self) -> V2 {
        v2(-self.x, -self.y)
    }
}
impl AddAssign for V2 {
    #[inline]
    fn add_assign(&mut self, o: V2) {
        self.x += o.x;
        self.y += o.y;
    }
}
impl SubAssign for V2 {
    #[inline]
    fn sub_assign(&mut self, o: V2) {
        self.x -= o.x;
        self.y -= o.y;
    }
}

/// Small, fast, deterministic PRNG (xorshift32) — no external crates needed.
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(seed.max(1))
    }
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    /// Uniform in [0, 1).
    #[inline]
    pub fn f(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    #[inline]
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f()
    }
}

// --------------------------------------------------------------- input ----

/// Pointer state in canvas-local coordinates.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub pos: V2,
    pub prev_pos: V2,
    pub inside: bool,
    pub left: bool,
    pub right: bool,
    /// True only on the first step after the button went down.
    pub left_pressed: bool,
    pub right_pressed: bool,
}

// ---------------------------------------------------------- draw lists ----

pub struct Dot {
    pub p: V2,
    pub r: f32,
    pub color: Hsla,
    pub border: Option<Hsla>,
}

pub struct Lines {
    pub color: Hsla,
    pub width: f32,
    pub segs: Vec<[f32; 4]>,
}

pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub fill: Hsla,
    pub border: Hsla,
}

/// Everything a scene wants painted this frame, in back-to-front layers.
#[derive(Default)]
pub struct DrawList {
    pub rects: Vec<Rect>,
    pub under: Vec<Lines>,
    pub dots: Vec<Dot>,
    pub over: Vec<Lines>,
}

impl DrawList {
    pub fn primitive_count(&self) -> usize {
        self.rects.len()
            + self.dots.len()
            + self.under.iter().map(|l| l.segs.len()).sum::<usize>()
            + self.over.iter().map(|l| l.segs.len()).sum::<usize>()
    }
}

// --------------------------------------------------------- scene trait ----

#[derive(Clone, Copy)]
pub struct SliderSpec {
    pub label: &'static str,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub unit: &'static str,
    pub decimals: usize,
}

#[derive(Clone, Copy)]
pub struct ToggleSpec {
    pub label: &'static str,
    pub description: &'static str,
    pub value: bool,
}

pub struct Stat {
    pub label: &'static str,
    pub value: String,
    pub caption: String,
}

pub trait Sim {
    fn title(&self) -> &'static str;
    fn tab_label(&self) -> &'static str;
    fn description(&self) -> &'static str;

    /// Rebuild the scene for a canvas of the given size.
    fn reset(&mut self, w: f32, h: f32);
    /// Advance by `dt` seconds of simulated time.
    fn step(&mut self, dt: f32, w: f32, h: f32, input: &Input);
    fn draw(&self, out: &mut DrawList, input: &Input);

    fn sliders(&self) -> Vec<SliderSpec>;
    fn set_slider(&mut self, ix: usize, value: f32);
    fn toggles(&self) -> Vec<ToggleSpec>;
    fn set_toggle(&mut self, ix: usize, value: bool);

    /// The scene's big red button.
    fn action_label(&self) -> &'static str;
    fn action(&mut self);

    fn bodies(&self) -> Stat;
    fn extra_stat(&self) -> Stat;
    /// (key, description) pairs shown as keyboard/mouse hints.
    fn hints(&self) -> Vec<(&'static str, &'static str)>;
}

// ------------------------------------------------------------- colors ----

/// Linear interpolation across a list of HSL stops (hue in degrees).
pub fn ramp(stops: &[(f32, f32, f32)], t: f32, alpha: f32) -> Hsla {
    let t = t.clamp(0.0, 1.0) * (stops.len() - 1) as f32;
    let i = (t.floor() as usize).min(stops.len() - 2);
    let f = t - i as f32;
    let (h0, s0, l0) = stops[i];
    let (h1, s1, l1) = stops[i + 1];
    let mut dh = h1 - h0;
    if dh > 180.0 {
        dh -= 360.0;
    } else if dh < -180.0 {
        dh += 360.0;
    }
    let h = (h0 + dh * f).rem_euclid(360.0);
    gpui::hsla(
        h / 360.0,
        (s0 + (s1 - s0) * f) / 100.0,
        (l0 + (l1 - l0) * f) / 100.0,
        alpha,
    )
}

/// The "heat" ramp used for speed / strain colouring.
pub const HEAT: [(f32, f32, f32); 5] = [
    (222.0, 70.0, 48.0),
    (265.0, 75.0, 62.0),
    (330.0, 82.0, 62.0),
    (28.0, 92.0, 60.0),
    (48.0, 100.0, 88.0),
];

/// Groups segments into `buckets` color bins along a ramp.
pub fn bucketed_lines(buckets: usize, width: f32, alpha: f32, stops: &[(f32, f32, f32)]) -> Vec<Lines> {
    (0..buckets)
        .map(|i| Lines {
            color: ramp(stops, i as f32 / (buckets - 1).max(1) as f32, alpha),
            width,
            segs: Vec::new(),
        })
        .collect()
}
