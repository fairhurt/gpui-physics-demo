//! A handful of shadcn/ui-flavoured primitives built from plain GPUI divs:
//! Card, Button, Badge, Switch, Kbd, Separator, Tabs trigger, Sparkline, Icons.

use crate::theme::*;
use gpui::{
    BoxShadow, Div, ElementId, FontWeight, Hsla, PathBuilder, SharedString, Stateful, canvas, div,
    linear_color_stop, linear_gradient, point, prelude::*, px, quad, size, Bounds,
};

// ---------------------------------------------------------------- card ----

pub fn card() -> Div {
    div()
        .flex()
        .flex_col()
        .rounded_xl()
        .border_1()
        .border_color(border())
        .bg(card_bg())
        .shadow_sm()
}

fn card_bg() -> Hsla {
    crate::theme::card()
}

pub fn card_header(title: impl Into<SharedString>, description: Option<SharedString>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .px_5()
        .pt_5()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(foreground())
                .child(title.into()),
        )
        .when_some(description, |d, s| {
            d.child(div().text_xs().text_color(muted_foreground()).child(s))
        })
}

pub fn separator() -> Div {
    div().h(px(1.)).w_full().flex_shrink_0().bg(border())
}

pub fn vseparator() -> Div {
    div().w(px(1.)).h(px(16.)).flex_shrink_0().bg(border())
}

// -------------------------------------------------------------- button ----

#[derive(Clone, Copy, PartialEq)]
pub enum Variant {
    Default,
    Secondary,
    Outline,

}

pub fn button(id: impl Into<ElementId>, variant: Variant) -> Stateful<Div> {
    let base = div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap_2()
        .h(px(32.))
        .px_3()
        .rounded_md()
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .whitespace_nowrap();
    match variant {
        Variant::Default => base
            .bg(primary())
            .text_color(primary_foreground())
            .shadow_sm()
            .hover(|s| s.bg(alpha(primary(), 0.88)))
            .active(|s| s.bg(alpha(primary(), 0.78))),
        Variant::Secondary => base
            .bg(muted())
            .text_color(foreground())
            .hover(|s| s.bg(alpha(muted(), 0.8))),
        Variant::Outline => base
            .border_1()
            .border_color(input())
            .bg(alpha(input(), 0.3))
            .text_color(foreground())
            .shadow_sm()
            .hover(|s| s.bg(alpha(input(), 0.6))),
    }
}

// --------------------------------------------------------------- badge ----

#[derive(Clone, Copy, PartialEq)]
pub enum BadgeVariant {
    Default,
    Secondary,
    Outline,
}

pub fn badge(label: impl Into<SharedString>, variant: BadgeVariant) -> Div {
    let base = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.))
        .px(px(8.))
        .py(px(2.))
        .rounded_md()
        .border_1()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap();
    let base = match variant {
        BadgeVariant::Default => base
            .border_color(gpui::transparent_black())
            .bg(primary())
            .text_color(primary_foreground()),
        BadgeVariant::Secondary => base
            .border_color(gpui::transparent_black())
            .bg(muted())
            .text_color(foreground()),
        BadgeVariant::Outline => base.border_color(border()).text_color(foreground()),
    };
    base.child(label.into())
}

pub fn status_dot(color: Hsla) -> Div {
    div()
        .size(px(6.))
        .rounded_full()
        .bg(color)
        .shadow(vec![BoxShadow {
            color: alpha(color, 0.6),
            offset: point(px(0.), px(0.)),
            blur_radius: px(6.),
            spread_radius: px(0.),
        }])
}

// ----------------------------------------------------------------- kbd ----

pub fn kbd(label: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .h(px(20.))
        .min_w(px(20.))
        .px(px(5.))
        .rounded(px(4.))
        .border_1()
        .border_color(border())
        .bg(alpha(muted(), 0.5))
        .text_xs()
        .font_family(MONO)
        .text_color(muted_foreground())
        .child(label.into())
}

// -------------------------------------------------------------- switch ----

pub fn switch(id: impl Into<ElementId>, on: bool) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .w(px(34.))
        .h(px(19.))
        .p(px(2.))
        .rounded_full()
        .border_1()
        .border_color(gpui::transparent_black())
        .cursor_pointer()
        .bg(if on { primary() } else { alpha(input(), 0.9) })
        .when(on, |d| d.justify_end())
        .child(
            div()
                .size(px(15.))
                .rounded_full()
                .bg(if on { primary_foreground() } else { foreground() })
                .shadow_sm(),
        )
}

// ---------------------------------------------------------------- tabs ----

pub fn tabs_list() -> Div {
    div()
        .flex()
        .items_center()
        .h(px(36.))
        .w_full()
        .p(px(3.))
        .gap(px(2.))
        .rounded_lg()
        .bg(muted())
}

pub fn tab_trigger(id: impl Into<ElementId>, label: impl Into<SharedString>, active: bool) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_1()
        .h_full()
        .items_center()
        .justify_center()
        .rounded_md()
        .border_1()
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .when(active, |d| {
            d.bg(background())
                .border_color(alpha(input(), 1.0))
                .text_color(foreground())
                .shadow_sm()
        })
        .when(!active, |d| {
            d.border_color(gpui::transparent_black())
                .text_color(muted_foreground())
                .hover(|s| s.text_color(foreground()))
        })
        .child(label.into())
}

// ----------------------------------------------------------- sparkline ----

/// Area sparkline with a vertical gradient fill.
pub fn sparkline(values: Vec<f32>, max: f32, color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            if values.len() < 2 {
                return;
            }
            let w = f32::from(bounds.size.width);
            let h = f32::from(bounds.size.height);
            let ox = f32::from(bounds.origin.x);
            let oy = f32::from(bounds.origin.y);
            let n = values.len();
            let pt = |i: usize, v: f32| {
                let x = ox + w * i as f32 / (n - 1) as f32;
                let y = oy + h - (v / max).clamp(0.0, 1.0) * (h - 2.0) - 1.0;
                point(px(x), px(y))
            };

            let mut fill = PathBuilder::fill();
            fill.move_to(point(px(ox), px(oy + h)));
            for (i, v) in values.iter().enumerate() {
                fill.line_to(pt(i, *v));
            }
            fill.line_to(point(px(ox + w), px(oy + h)));
            fill.close();
            if let Ok(path) = fill.build() {
                window.paint_path(
                    path,
                    linear_gradient(
                        180.,
                        linear_color_stop(alpha(color, 0.35), 0.),
                        linear_color_stop(alpha(color, 0.0), 1.),
                    ),
                );
            }

            let mut line = PathBuilder::stroke(px(1.5));
            for (i, v) in values.iter().enumerate() {
                if i == 0 {
                    line.move_to(pt(i, *v));
                } else {
                    line.line_to(pt(i, *v));
                }
            }
            if let Ok(path) = line.build() {
                window.paint_path(path, color);
            }
        },
    )
    .w_full()
    .h(px(36.))
}

// --------------------------------------------------------------- icons ----

#[derive(Clone, Copy)]
pub enum Icon {
    Play,
    Pause,
    Reset,
    Zap,
}

pub fn icon(kind: Icon, color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let o = b.origin;
            let s = f32::from(b.size.width);
            let p = |x: f32, y: f32| point(o.x + px(x * s), o.y + px(y * s));
            match kind {
                Icon::Pause => {
                    for x in [0.22, 0.58] {
                        window.paint_quad(quad(
                            Bounds::new(p(x, 0.18), size(px(0.2 * s), px(0.64 * s))),
                            px(1.),
                            color,
                            px(0.),
                            gpui::transparent_black(),
                            Default::default(),
                        ));
                    }
                }
                Icon::Play => {
                    let mut b = PathBuilder::fill();
                    b.add_polygon(&[p(0.26, 0.16), p(0.84, 0.5), p(0.26, 0.84)], true);
                    if let Ok(path) = b.build() {
                        window.paint_path(path, color);
                    }
                }
                Icon::Reset => {
                    let r = 0.32 * s;
                    let c = p(0.5, 0.52);
                    let mut b = PathBuilder::stroke(px(1.6));
                    // ~300° arc
                    let start = point(c.x - px(r), c.y);
                    b.move_to(start);
                    let a: f32 = -1.1;
                    let end = point(c.x + px(r * a.cos()), c.y + px(r * a.sin()));
                    b.arc_to(point(px(r), px(r)), px(0.), true, false, end);
                    if let Ok(path) = b.build() {
                        window.paint_path(path, color);
                    }
                    let mut head = PathBuilder::fill();
                    head.add_polygon(&[p(0.04, 0.38), p(0.36, 0.38), p(0.18, 0.66)], true);
                    if let Ok(path) = head.build() {
                        window.paint_path(path, color);
                    }
                }
                Icon::Zap => {
                    let mut b = PathBuilder::fill();
                    b.add_polygon(
                        &[
                            p(0.56, 0.06),
                            p(0.18, 0.56),
                            p(0.46, 0.56),
                            p(0.40, 0.94),
                            p(0.82, 0.42),
                            p(0.54, 0.42),
                        ],
                        true,
                    );
                    if let Ok(path) = b.build() {
                        window.paint_path(path, color);
                    }
                }
            }
        },
    )
    .size(px(14.))
    .flex_shrink_0()
}
