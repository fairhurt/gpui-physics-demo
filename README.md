# Physics Lab — a GPUI tech demo

Real-time physics in [GPUI](https://www.gpui.rs/) (Zed's GPU-accelerated Rust UI framework), with a shadcn/ui-style dark interface built from plain GPUI divs.

## Scenes

| Scene | What it shows | Interact |
|---|---|---|
| **Gravity Wells** | Up to 20,000 particles on softened inverse-square gravity around moving point masses, with motion streaks and speed colouring | Left mouse button attracts, right repels, `E` triggers a supernova |
| **Collision Sandbox** | Up to 1,800 rigid circles with mass ∝ r², restitution impulses and a visible uniform-grid broad phase | Left mouse button grabs and flings, right blasts, `E` explodes |
| **Verlet Cloth** | Position-based cloth with iterative distance constraints, wind turbulence and strain tearing | Left mouse button grabs, right cuts, `E` sends a gust |

Shortcuts: `Space` pauses, `R` resets, `1`–`3` switch scenes.

## Run

On Windows, double-click **`run.cmd`**. It finds `fxc.exe` in your Windows SDK, builds a release binary (log in `build.log`), and launches it. Or run it yourself:

```sh
cargo run --release
```

The first build compiles about 400 crates and takes several minutes. Later builds are quick.

**Windows note:** GPUI compiles its HLSL shaders with `fxc.exe` from the Windows SDK, but its build script only checks one SDK version (`10.0.26100.0`). If you have a different one, `cargo` fails with `Failed to find fxc.exe`. Either use `run.cmd`, or set `GPUI_FXC_PATH` to the full path of `fxc.exe`, for example `C:\Program Files (x86)\Windows Kits\10\bin\10.0.22000.0\x64\fxc.exe`.

## Layout

```
src/
  main.rs        window setup
  app.rs         the view: layout, input, frame loop, canvas painting
  ui.rs          shadcn-style primitives (Card, Button, Badge, Switch, Tabs, Kbd, Sparkline, icons)
  theme.rs       zinc dark theme tokens and chart colours
  sim/
    mod.rs       vector math, RNG, draw lists, the `Sim` trait
    gravity.rs   gravity wells
    balls.rs     collision sandbox
    cloth.rs     Verlet cloth
```

Each scene emits a `DrawList` (dots, line batches, rects) in canvas-local coordinates. `app.rs` paints it with `window.paint_quad` (circles are rounded quads) and `window.paint_path` (line segments are batched into stroked paths, tessellated by lyon). The view calls `window.request_animation_frame()` every frame to keep the animation running.
