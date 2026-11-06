// Hide the console window in release builds on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod sim;
mod theme;
mod ui;

use gpui::{
    App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1480.), px(920.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Physics Lab — GPUI".into()),
                    ..Default::default()
                }),
                window_min_size: Some(size(px(1080.), px(700.))),
                focus: true,
                ..Default::default()
            },
            |window, cx| cx.new(|cx| app::PhysicsLab::new(window, cx)),
        )
        .expect("failed to open window");

        cx.on_window_closed(|cx| cx.quit()).detach();
        cx.activate(true);
    });
}
