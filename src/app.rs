//! The Physics Lab view: layout, input routing, the frame loop and painting
//! of the simulation draw list onto a GPUI canvas.

use std::{
    cell::Cell,
    collections::VecDeque,
    rc::Rc,
    time::Instant,
};

use gpui::{
    BoxShadow, Bounds, Context, Div, FocusHandle, FontWeight, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathBuilder, Pixels, SharedString, Window,
    canvas, div, point, prelude::*, px, quad, relative, size,
};

use crate::sim::{self, DrawList, Input, Sim, SliderSpec, V2, v2};
use crate::theme::*;
use crate::ui::{self, BadgeVariant, Icon, Variant};

const HISTORY: usize = 120;
const MAX_SLIDERS: usize = 16;
const TIME_SLIDER: usize = MAX_SLIDERS - 1;
const MAX_SEGS_PER_PATH: usize = 3000;

pub struct PhysicsLab {
    sims: Vec<Box<dyn Sim>>,
    active: usize,
    needs_reset: Vec<bool>,
    paused: bool,
    time_scale: f32,

    focus: FocusHandle,
    canvas_bounds: Rc<Cell<Bounds<Pixels>>>,
    slider_bounds: Vec<Rc<Cell<Bounds<Pixels>>>>,
    dragging_slider: Option<usize>,
    input: Input,

    last_frame: Instant,
    frame_ms: f32,
    step_ms: f32,
    fps_history: VecDeque<f32>,
    step_history: VecDeque<f32>,
    last_primitives: usize,
}

impl PhysicsLab {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus);
        let sims: Vec<Box<dyn Sim>> = vec![
            Box::new(sim::gravity::Gravity::default()),
            Box::new(sim::balls::Balls::default()),
            Box::new(sim::cloth::Cloth::default()),
        ];
        let n = sims.len();
        Self {
            sims,
            active: 0,
            needs_reset: vec![true; n],
            paused: false,
            time_scale: 1.0,
            focus,
            canvas_bounds: Rc::new(Cell::new(Bounds::default())),
            slider_bounds: (0..MAX_SLIDERS).map(|_| Rc::new(Cell::new(Bounds::default()))).collect(),
            dragging_slider: None,
            input: Input::default(),
            last_frame: Instant::now(),
            frame_ms: 16.6,
            step_ms: 0.0,
            fps_history: VecDeque::with_capacity(HISTORY),
            step_history: VecDeque::with_capacity(HISTORY),
            last_primitives: 0,
        }
    }

    fn sim(&self) -> &dyn Sim {
        self.sims[self.active].as_ref()
    }

    fn sim_mut(&mut self) -> &mut dyn Sim {
        self.sims[self.active].as_mut()
    }

    fn reset_active(&mut self) {
        self.needs_reset[self.active] = true;
    }

    fn select(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix < self.sims.len() {
            self.active = ix;
            self.dragging_slider = None;
            cx.notify();
        }
    }

    // ------------------------------------------------------------ frame ----

    fn tick(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;
        let dt_ms = dt * 1000.0;
        self.frame_ms += (dt_ms - self.frame_ms) * 0.1;
        push_hist(&mut self.fps_history, 1000.0 / dt_ms.max(0.1));

        let b = self.canvas_bounds.get();
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        if w < 16.0 || h < 16.0 {
            return;
        }
        let active = self.active;
        if self.needs_reset[active] {
            self.needs_reset[active] = false;
            self.sims[active].reset(w, h);
        }
        if !self.paused {
            let t0 = Instant::now();
            let sim_dt = dt.min(1.0 / 30.0) * self.time_scale;
            let input = self.input;
            self.sims[active].step(sim_dt, w, h, &input);
            let ms = t0.elapsed().as_secs_f32() * 1000.0;
            self.step_ms += (ms - self.step_ms) * 0.1;
            push_hist(&mut self.step_history, ms);
        }
        self.input.left_pressed = false;
        self.input.right_pressed = false;
        self.input.prev_pos = self.input.pos;
    }

    fn local_mouse(&self, p: gpui::Point<Pixels>) -> (V2, bool) {
        let b = self.canvas_bounds.get();
        let local = v2(f32::from(p.x - b.origin.x), f32::from(p.y - b.origin.y));
        (local, b.contains(&p))
    }

    fn set_slider_from_x(&mut self, ix: usize, x: Pixels) {
        let b = self.slider_bounds[ix].get();
        let width = f32::from(b.size.width);
        if width <= 0.0 {
            return;
        }
        let frac = (f32::from(x - b.origin.x) / width).clamp(0.0, 1.0);
        if ix == TIME_SLIDER {
            let spec = self.time_spec();
            self.time_scale = snap(&spec, frac);
        } else if let Some(spec) = self.sim().sliders().get(ix).copied() {
            let v = snap(&spec, frac);
            self.sim_mut().set_slider(ix, v);
        }
    }

    fn time_spec(&self) -> SliderSpec {
        SliderSpec {
            label: "Time scale",
            value: self.time_scale,
            min: 0.05,
            max: 2.0,
            step: 0.05,
            unit: "×",
            decimals: 2,
        }
    }

    // ------------------------------------------------------- components ----

    fn slider(&self, ix: usize, spec: SliderSpec, cx: &mut Context<Self>) -> Div {
        let frac = ((spec.value - spec.min) / (spec.max - spec.min)).clamp(0.0, 1.0);
        let cell = self.slider_bounds[ix].clone();
        let active = self.dragging_slider == Some(ix);
        let value = format!("{:.*}{}", spec.decimals, spec.value, spec.unit);
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_sm()
                    .child(div().text_color(foreground()).child(spec.label))
                    .child(
                        div()
                            .font_family(MONO)
                            .text_xs()
                            .text_color(if active { foreground() } else { muted_foreground() })
                            .child(value),
                    ),
            )
            .child(
                div()
                    .id(("slider", ix))
                    .relative()
                    .h(px(16.))
                    .w_full()
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .child(
                        canvas(move |b, _, _| cell.set(b), |_, _, _, _| {})
                            .absolute()
                            .size_full(),
                    )
                    .child(
                        div()
                            .h(px(6.))
                            .w_full()
                            .rounded_full()
                            .bg(muted())
                            .child(div().h_full().w(relative(frac)).rounded_full().bg(primary())),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(0.))
                            .left(relative(frac))
                            .ml(px(-8.))
                            .size(px(16.))
                            .rounded_full()
                            .border_1()
                            .border_color(primary())
                            .bg(background())
                            .shadow(vec![BoxShadow {
                                color: alpha(ring(), if active { 0.35 } else { 0.0 }),
                                offset: point(px(0.), px(0.)),
                                blur_radius: px(0.),
                                spread_radius: px(4.),
                            }])
                            .hover(|s| {
                                s.shadow(vec![BoxShadow {
                                    color: alpha(ring(), 0.25),
                                    offset: point(px(0.), px(0.)),
                                    blur_radius: px(0.),
                                    spread_radius: px(4.),
                                }])
                            }),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                            this.dragging_slider = Some(ix);
                            this.set_slider_from_x(ix, ev.position.x);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            )
    }

    fn toggle_row(&self, ix: usize, label: &'static str, desc: &'static str, on: bool, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(("toggle-row", ix))
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .py(px(2.))
            .cursor_pointer()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .min_w(px(0.))
                    .child(div().text_sm().text_color(foreground()).child(label))
                    .child(div().text_xs().text_color(muted_foreground()).truncate().child(desc)),
            )
            .child(ui::switch(("switch", ix), on))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.sim_mut().set_toggle(ix, !on);
                cx.notify();
            }))
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let fps = 1000.0 / self.frame_ms.max(0.01);
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .h(px(56.))
            .px_5()
            .border_b_1()
            .border_color(border())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(logo())
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(foreground())
                            .child("Physics Lab"),
                    )
                    .child(ui::vseparator())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_sm()
                            .text_color(muted_foreground())
                            .child("gpui")
                            .child("/")
                            .child("scenes")
                            .child("/")
                            .child(div().text_color(foreground()).child(self.sim().title())),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        ui::badge(SharedString::from(format!("{fps:>3.0} fps")), BadgeVariant::Outline)
                            .font_family(MONO)
                            .child(ui::status_dot(if self.paused { chart(2) } else { chart(1) })),
                    )
                    .child(ui::badge(RENDERER, BadgeVariant::Secondary))
                    .child(div().w(px(4.)))
                    .child(
                        ui::button("pause", Variant::Outline)
                            .child(ui::icon(if self.paused { Icon::Play } else { Icon::Pause }, foreground()))
                            .child(if self.paused { "Resume" } else { "Pause" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.paused = !this.paused;
                                cx.notify();
                            })),
                    )
                    .child(
                        ui::button("reset", Variant::Default)
                            .child(ui::icon(Icon::Reset, primary_foreground()))
                            .child("Reset")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reset_active();
                                cx.notify();
                            })),
                    ),
            )
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let sim = self.sim();
        let tabs = ui::tabs_list().children(self.sims.iter().enumerate().map(|(i, s)| {
            ui::tab_trigger(("tab", i), s.tab_label(), i == self.active)
                .on_click(cx.listener(move |this, _, _, cx| this.select(i, cx)))
        }));

        let sliders: Vec<_> = sim
            .sliders()
            .into_iter()
            .enumerate()
            .map(|(i, s)| self.slider(i, s, cx))
            .collect();
        let toggles: Vec<_> = sim
            .toggles()
            .into_iter()
            .enumerate()
            .map(|(i, t)| self.toggle_row(i, t.label, t.description, t.value, cx).into_any_element())
            .collect();

        div()
            .id("sidebar")
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap_4()
            .w(px(312.))
            .h_full()
            .overflow_y_scroll()
            // Scene picker
            .child(
                ui::card()
                    .child(ui::card_header("Scene", Some(sim.description().into())))
                    .child(div().p_5().pt_4().child(tabs)),
            )
            // Parameters
            .child(
                ui::card()
                    .child(ui::card_header("Parameters", Some("Drag to tweak live — no restart needed.".into())))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_5()
                            .p_5()
                            .children(sliders)
                            .child(ui::separator())
                            .child(self.slider(TIME_SLIDER, self.time_spec(), cx)),
                    ),
            )
            // Display toggles
            .child(
                ui::card()
                    .child(ui::card_header("Display", None))
                    .child(div().flex().flex_col().gap_3().p_5().pt_4().children(toggles)),
            )
            // Action
            .child(
                ui::card().child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .p_4()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Impulse"))
                                .child(div().text_xs().text_color(muted_foreground()).child("Kick the system")),
                        )
                        .child(
                            ui::button("action", Variant::Secondary)
                                .child(ui::icon(Icon::Zap, chart(2)))
                                .child(sim.action_label())
                                .child(ui::kbd("E"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.sim_mut().action();
                                    cx.notify();
                                })),
                        ),
                ),
            )
    }

    fn viewport(&self, draw: DrawList, cx: &mut Context<Self>) -> impl IntoElement {
        let bounds_cell = self.canvas_bounds.clone();
        let sim = self.sim();
        let hints = sim.hints();
        let paused = self.paused;

        div()
            .id("viewport")
            .relative()
            .flex_1()
            .min_h(px(0.))
            .rounded_xl()
            .border_1()
            .border_color(border())
            .bg(canvas_bg())
            .overflow_hidden()
            .cursor_crosshair()
            .child(
                canvas(
                    move |b, _, _| bounds_cell.set(b),
                    move |b, _, window, _| paint_scene(b, draw, window),
                )
                .size_full(),
            )
            // Top-left overlay
            .child(
                div()
                    .absolute()
                    .top_3()
                    .left_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        ui::badge(sim.title(), BadgeVariant::Outline)
                            .bg(alpha(background(), 0.7))
                            .child(ui::status_dot(if paused { chart(2) } else { chart(1) })),
                    )
                    .when(paused, |d| d.child(ui::badge("Paused", BadgeVariant::Default))),
            )
            // Top-right overlay
            .child(
                div()
                    .absolute()
                    .top_3()
                    .right_3()
                    .flex()
                    .gap_2()
                    .child(
                        ui::badge(
                            SharedString::from(format!("{} prims", sim::gravity::fmt_int(self.last_primitives))),
                            BadgeVariant::Outline,
                        )
                        .bg(alpha(background(), 0.7))
                        .font_family(MONO),
                    ),
            )
            // Bottom-left hints
            .child(
                div()
                    .absolute()
                    .bottom_3()
                    .left_3()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_2()
                    .py(px(5.))
                    .rounded_lg()
                    .border_1()
                    .border_color(border())
                    .bg(alpha(background(), 0.75))
                    .children(hints.into_iter().map(|(k, d)| {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_xs()
                            .text_color(muted_foreground())
                            .child(ui::kbd(k))
                            .child(d)
                    }))
                    .child(ui::vseparator())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_xs()
                            .text_color(muted_foreground())
                            .child(ui::kbd("Space"))
                            .child("pause")
                            .child(ui::kbd("R"))
                            .child("reset")
                            .child(ui::kbd("1-3"))
                            .child("scene"),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                    let (p, inside) = this.local_mouse(ev.position);
                    this.input.pos = p;
                    this.input.inside = inside;
                    this.input.left = true;
                    this.input.left_pressed = true;
                    window.focus(&this.focus);
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                    let (p, inside) = this.local_mouse(ev.position);
                    this.input.pos = p;
                    this.input.prev_pos = p;
                    this.input.inside = inside;
                    this.input.right = true;
                    this.input.right_pressed = true;
                    cx.notify();
                }),
            )
    }

    fn stats(&self) -> impl IntoElement {
        let sim = self.sim();
        let bodies = sim.bodies();
        let extra = sim.extra_stat();
        let fps = 1000.0 / self.frame_ms.max(0.01);
        let fps_vals: Vec<f32> = self.fps_history.iter().copied().collect();
        let fps_max = fps_vals.iter().copied().fold(60.0f32, f32::max) * 1.15;
        let step_vals: Vec<f32> = self.step_history.iter().copied().collect();
        let step_max = step_vals.iter().copied().fold(2.0f32, f32::max) * 1.2;

        div()
            .flex()
            .flex_shrink_0()
            .gap_4()
            .h(px(128.))
            .child(stat_card(
                "Frame rate",
                format!("{fps:.0}"),
                "fps",
                format!("{:.2} ms / frame", self.frame_ms),
                Some(ui::sparkline(fps_vals, fps_max, chart(1)).into_any_element()),
            ))
            .child(stat_card(
                "Simulation",
                format!("{:.2}", self.step_ms),
                "ms",
                format!("{:.0}% of frame budget", (self.step_ms / self.frame_ms.max(0.01) * 100.0).min(999.0)),
                Some(ui::sparkline(step_vals, step_max, chart(0)).into_any_element()),
            ))
            .child(stat_card(bodies.label, bodies.value, "", bodies.caption, None))
            .child(stat_card(extra.label, extra.value, "", extra.caption, None))
    }
}

impl Render for PhysicsLab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Continuous animation: ask for another frame every frame.
        window.request_animation_frame();
        self.tick();

        let mut draw = DrawList::default();
        let input = self.input;
        self.sim().draw(&mut draw, &input);
        self.last_primitives = draw.primitive_count();

        div()
            .id("root")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(background())
            .text_color(foreground())
            .child(self.header(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.))
                    .gap_4()
                    .p_4()
                    .child(self.sidebar(cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.))
                            .gap_4()
                            .child(self.viewport(draw, cx))
                            .child(self.stats()),
                    ),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                let (p, inside) = this.local_mouse(ev.position);
                this.input.pos = p;
                this.input.inside = inside;
                if let Some(ix) = this.dragging_slider {
                    if ev.pressed_button == Some(MouseButton::Left) {
                        this.set_slider_from_x(ix, ev.position.x);
                        cx.notify();
                    } else {
                        this.dragging_slider = None;
                    }
                }
                if ev.pressed_button.is_none() {
                    this.input.left = false;
                    this.input.right = false;
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| {
                    this.input.left = false;
                    this.dragging_slider = None;
                }),
            )
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(|this, _: &MouseUpEvent, _, _| {
                    this.input.right = false;
                }),
            )
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                match ev.keystroke.key.as_str() {
                    "space" => this.paused = !this.paused,
                    "r" => this.reset_active(),
                    "e" => this.sim_mut().action(),
                    "1" => this.select(0, cx),
                    "2" => this.select(1, cx),
                    "3" => this.select(2, cx),
                    _ => return,
                }
                cx.notify();
            }))
    }
}

// ------------------------------------------------------------- helpers ----

fn push_hist(h: &mut VecDeque<f32>, v: f32) {
    if h.len() == HISTORY {
        h.pop_front();
    }
    h.push_back(v);
}

fn snap(spec: &SliderSpec, frac: f32) -> f32 {
    let raw = spec.min + frac * (spec.max - spec.min);
    let v = (raw / spec.step).round() * spec.step;
    v.clamp(spec.min, spec.max)
}

fn logo() -> impl IntoElement {
    div()
        .size(px(28.))
        .rounded_md()
        .bg(primary())
        .flex()
        .items_center()
        .justify_center()
        .child(
            canvas(
                |_, _, _| {},
                |b, _, window, _| {
                    let c = b.center();
                    // Tiny orbit diagram.
                    let r = px(7.5);
                    let mut ring = PathBuilder::stroke(px(1.6));
                    ring.move_to(point(c.x - r, c.y));
                    ring.arc_to(point(r, r), px(0.), false, true, point(c.x + r, c.y));
                    ring.arc_to(point(r, r), px(0.), false, true, point(c.x - r, c.y));
                    if let Ok(p) = ring.build() {
                        window.paint_path(p, primary_foreground());
                    }
                    let dot = |x: Pixels, y: Pixels, s: f32| {
                        quad(
                            Bounds::new(point(x - px(s / 2.), y - px(s / 2.)), size(px(s), px(s))),
                            px(s / 2.),
                            primary_foreground(),
                            px(0.),
                            gpui::transparent_black(),
                            Default::default(),
                        )
                    };
                    window.paint_quad(dot(c.x, c.y, 4.0));
                    window.paint_quad(dot(c.x + px(5.3), c.y - px(5.3), 3.5));
                },
            )
            .size(px(20.)),
        )
}

fn stat_card(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    unit: &'static str,
    caption: impl Into<SharedString>,
    chart_el: Option<gpui::AnyElement>,
) -> impl IntoElement {
    ui::card()
        .flex_1()
        .min_w(px(0.))
        .h_full()
        .p_4()
        .gap_1()
        .overflow_hidden()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(muted_foreground())
                .child(label.into()),
        )
        .child(
            div()
                .flex()
                .items_end()
                .gap_1()
                .child(
                    div()
                        .text_2xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .font_family(MONO)
                        .text_color(foreground())
                        .child(value.into()),
                )
                .when(!unit.is_empty(), |d| {
                    d.child(div().pb(px(4.)).text_xs().text_color(muted_foreground()).child(unit))
                }),
        )
        .child(
            div()
                .text_xs()
                .text_color(muted_foreground())
                .truncate()
                .child(caption.into()),
        )
        .when_some(chart_el, |d, el| d.child(div().mt_auto().child(el)))
}

/// Paints a `DrawList` (canvas-local coordinates) into window space.
fn paint_scene(bounds: Bounds<Pixels>, draw: DrawList, window: &mut Window) {
    let ox = f32::from(bounds.origin.x);
    let oy = f32::from(bounds.origin.y);
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);

    // Engineering-paper background grid.
    for (spacing, a) in [(32.0f32, 0.035f32), (160.0, 0.06)] {
        let mut grid = PathBuilder::stroke(px(1.));
        let mut x = spacing;
        while x < w {
            grid.move_to(point(px(ox + x), px(oy)));
            grid.line_to(point(px(ox + x), px(oy + h)));
            x += spacing;
        }
        let mut y = spacing;
        while y < h {
            grid.move_to(point(px(ox), px(oy + y)));
            grid.line_to(point(px(ox + w), px(oy + y)));
            y += spacing;
        }
        if let Ok(p) = grid.build() {
            window.paint_path(p, alpha(foreground(), a));
        }
    }

    for r in &draw.rects {
        window.paint_quad(quad(
            Bounds::new(point(px(ox + r.x), px(oy + r.y)), size(px(r.w), px(r.h))),
            px(0.),
            r.fill,
            px(1.),
            r.border,
            Default::default(),
        ));
    }

    paint_lines(&draw.under, ox, oy, window);

    for d in &draw.dots {
        let r = d.r;
        window.paint_quad(quad(
            Bounds::new(point(px(ox + d.p.x - r), px(oy + d.p.y - r)), size(px(r * 2.), px(r * 2.))),
            px(r),
            d.color,
            px(if d.border.is_some() { 1. } else { 0. }),
            d.border.unwrap_or(gpui::transparent_black()),
            Default::default(),
        ));
    }

    paint_lines(&draw.over, ox, oy, window);
}

/// Batches line segments into as few tessellated paths as possible
/// (chunked to stay well under GPUI's u16 index limit per path).
fn paint_lines(batches: &[sim::Lines], ox: f32, oy: f32, window: &mut Window) {
    for batch in batches {
        for chunk in batch.segs.chunks(MAX_SEGS_PER_PATH) {
            if chunk.is_empty() {
                continue;
            }
            let mut b = PathBuilder::stroke(px(batch.width));
            for s in chunk {
                b.move_to(point(px(ox + s[0]), px(oy + s[1])));
                b.line_to(point(px(ox + s[2]), px(oy + s[3])));
            }
            if let Ok(p) = b.build() {
                window.paint_path(p, batch.color);
            }
        }
    }
}
