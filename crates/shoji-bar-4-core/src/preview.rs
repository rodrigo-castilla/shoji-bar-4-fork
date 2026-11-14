//! Pictures of parts of the shell without a compositor's surfaces, for looking at them while
//! working on them:
//!
//! `shoji-bar-4 preview <part> <out.png> [--width W] [--height H] [--scale S] [--light]
//!  [--bar-position top|left|right] [--pointer X,Y] [--click X,Y] [--right-click X,Y]
//!  [--drag X,Y:X,Y] [--escape] [--key K] [--late] [--ms N]`
//!
//! The pointer and clicks are in logical pixels, and `--escape` presses Escape after the clicks;
//! `--late` opens a panel a second and a half after the start, for pictures of it coming in; `--ms`
//! says how long animations run before the picture is taken.

use crate::placement::{self, BarPlacement};
use anti_webui::app::headless::write_png;
use anti_webui::core::Ui;
use anti_webui::core::event::{Modifiers, PointerButton, WindowEvent};
use anti_webui::core::geometry::{PointF, Scale, SizeI};
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui::wgpu::{Gpu, Renderer};
use std::time::{Duration, Instant};

pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let part = args.first().cloned().unwrap_or_else(|| "bar".into());
    let out = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| format!("{part}.png"));
    let opt = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let point = |s: String| -> Option<(f32, f32)> {
        let (x, y) = s.split_once(',')?;
        Some((x.parse().ok()?, y.parse().ok()?))
    };
    let bar_position = match opt("--bar-position").as_deref() {
        None | Some("top") => crate::settings::BarPosition::Top,
        Some("left") => crate::settings::BarPosition::Left,
        Some("right") => crate::settings::BarPosition::Right,
        Some(other) => return Err(format!("unknown bar position: {other}").into()),
    };
    let at = BarPlacement::new(bar_position);
    let scale: f64 = opt("--scale").and_then(|s| s.parse().ok()).unwrap_or(1.8);
    let (default_w, default_h) = match part.as_str() {
        "bar" if at.is_vertical() => (placement::WIDTH as f32, 1000.0),
        "bar" => (1600.0, placement::HEIGHT as f32),
        "title" if at.is_vertical() => (80.0, 80.0),
        "title" => (480.0, placement::HEIGHT as f32),
        "dock" => (900.0, 136.0),
        "launcher" | "quick-settings" | "calendar" | "clipboard" | "media" | "power" | "lock" => {
            (1600.0, 1000.0)
        }
        _ => (800.0, 600.0),
    };
    let lw: f32 = opt("--width")
        .and_then(|s| s.parse().ok())
        .unwrap_or(default_w);
    let lh: f32 = opt("--height")
        .and_then(|s| s.parse().ok())
        .unwrap_or(default_h);
    let (w, h) = (
        (lw as f64 * scale).round() as u32,
        (lh as f64 * scale).round() as u32,
    );
    let light = args.iter().any(|a| a == "--light");
    let late = args.iter().any(|a| a == "--late");
    let pointer = opt("--pointer").and_then(point);
    // In order, with the button they press.
    let clicks: Vec<((f32, f32), PointerButton)> = args
        .iter()
        .enumerate()
        .filter_map(|(i, a)| match a.as_str() {
            "--click" => Some((i, PointerButton::Primary)),
            "--right-click" => Some((i, PointerButton::Secondary)),
            _ => None,
        })
        .filter_map(|(i, button)| Some((args.get(i + 1).cloned().and_then(point)?, button)))
        .collect();
    // `--drag X1,Y1:X2,Y2`: pressed at the first point, moved to the second over a few frames,
    // let go there, after the clicks.
    let drag: Option<((f32, f32), (f32, f32))> = opt("--drag").and_then(|s| {
        let (a, b) = s.split_once(':')?;
        Some((point(a.to_owned())?, point(b.to_owned())?))
    });
    let ms: u64 = opt("--ms").and_then(|s| s.parse().ok()).unwrap_or(1200);
    // `--frames N --every MS` writes N pictures, MS apart, after the first, as `<out>-<i>.png`.
    let frames: u32 = opt("--frames").and_then(|s| s.parse().ok()).unwrap_or(0);
    let every: u64 = opt("--every").and_then(|s| s.parse().ok()).unwrap_or(50);
    // `--click-frames N`: how many frames to step after each click (to catch what it starts).
    let click_frames: u64 = opt("--click-frames")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    let gpu = Gpu::headless()?;
    let mut renderer = Renderer::new(
        &gpu.device,
        anti_webui::wgpu::wgpu::TextureFormat::Rgba8Unorm,
    );
    let output = outputs().into_iter().next();
    crate::settings::never_save();
    let mut ui = Ui::new(move || {
        if light {
            crate::settings::settings().update(|s| s.mode = crate::settings::ThemeMode::Light);
        }
        crate::settings::settings().update(|s| s.bar_position = bar_position);
        crate::theme::install();
        let backdrop = div()
            .size_full()
            .bg(rgb(if light { 0xc8d0d8 } else { 0x30343a }));
        let Some(output) = output else {
            return backdrop.child("no output");
        };
        if let Some(panel) = crate::panels::Panel::from_name(&part) {
            let name = output.name.clone();
            if late {
                set_timeout(Duration::from_millis(1500), move || {
                    crate::panels::open(&name, panel)
                });
            } else {
                crate::panels::open(&name, panel);
            }
        }
        let view = match part.as_str() {
            "bar" => crate::bar::view(output, at),
            // As ShojiWM lists them: the ones with windows and the active one. At 1.5 s a new,
            // empty one after the last is gone to; at 3 s the first, which leaves the empty one.
            "workspaces" => {
                let ws = |i: u32, active: bool| crate::bar::Ws {
                    id: i.to_string(),
                    name: i.to_string(),
                    active,
                    empty: false,
                    urgent: false,
                };
                let list = signal(vec![ws(1, false), ws(2, true)]);
                set_timeout(Duration::from_millis(1500), move || {
                    list.set(vec![ws(1, false), ws(2, false), ws(3, true)])
                });
                set_timeout(Duration::from_millis(3000), move || {
                    list.set(vec![ws(1, true), ws(2, false)])
                });
                div().p(8.0_f32).flex().child(crate::bar::workspaces_island(
                    memo(move || list.get()),
                    std::rc::Rc::new(|_: &str| {}),
                    at,
                ))
            }
            // The title of the window with the keyboard, going between a long and a short one
            // every `--switch` ms (80 by default) for a second and a half from 1.2 s, then staying
            // on the one `--end` names (`long` by default). With `--windows` they are the titles of
            // two windows that take the keyboard in turn, not two titles of one window. With
            // `--spinner` it is a terminal whose title starts with a spinner, as Claude Code's
            // does, and at the end says what it is doing.
            "title" => {
                let switch: u64 = opt("--switch").and_then(|s| s.parse().ok()).unwrap_or(80);
                let windows = args.iter().any(|a| a == "--windows");
                let spinner = args.iter().any(|a| a == "--spinner");
                let end_long = opt("--end").is_none_or(|e| e != "short");
                let focused = move |step: Option<usize>, long: bool| {
                    let second = windows && !long;
                    let title = match (spinner, step) {
                        (true, Some(i)) => {
                            format!("{} Claude Code", ['·', '✢', '✳', '✶', '✻', '✽'][i % 6])
                        }
                        (true, None) => "✳ Fixing the window title island".into(),
                        (false, _) if long => "A rather long window title - Google Chrome".into(),
                        (false, _) => "New Tab".into(),
                    };
                    crate::bar::Focused {
                        id: if second { 2 } else { 1 },
                        title,
                        app_id: if second || spinner {
                            "kitty".into()
                        } else {
                            "google-chrome".into()
                        },
                    }
                };
                let shown = signal(Some(focused(Some(0), false)));
                // After the preview's own wait at the start.
                let mut elapsed = 1200;
                let mut step = 0;
                while elapsed < 2700 {
                    elapsed += switch;
                    step += 1;
                    let long = step % 2 == 1;
                    set_timeout(Duration::from_millis(elapsed), move || {
                        shown.set(Some(focused(Some(step), long)))
                    });
                }
                set_timeout(Duration::from_millis(elapsed + switch), move || {
                    shown.set(Some(focused(None, end_long)))
                });
                let frame = div()
                    .p(4.0_f32)
                    .flex()
                    .items_center()
                    .dyn_style(|s| s.text_color(crate::theme::colors().on_surface));
                let frame = if at.is_vertical() {
                    frame.h(placement::WIDTH as f32).justify_center()
                } else {
                    frame.h(placement::HEIGHT as f32)
                };
                frame.child(crate::bar::focused_island(memo(move || shown.get()), at))
            }
            "notifications" => match crate::notify::server() {
                Some(server) => crate::notify::popups(server, signal(InputRegion::Empty)),
                None => div().child("not the notification daemon"),
            },
            "dock" => crate::dock::view(output, signal(InputRegion::Full)),
            "lock" => crate::lock::view(output),
            "launcher" | "quick-settings" | "calendar" | "clipboard" | "media" | "power" => {
                let panel = crate::panels::Panel::from_name(&part).unwrap();
                crate::panels::view(output, panel)
            }
            _ => div().child("unknown part"),
        };
        backdrop
            .relative()
            .child(div().absolute().inset(0.0).flex().flex_col().child(view))
    });
    let to_physical = |(x, y): (f32, f32)| PointF::new(x * scale as f32, y * scale as f32);
    ui.handle_event(WindowEvent::Resized {
        size: SizeI::new(w as i32, h as i32),
        scale: Scale::from_f64(scale),
    });
    ui.handle_event(WindowEvent::Focused(true));
    ui.update();
    let mut now = Instant::now();
    let step = |ui: &mut Ui, now: &mut Instant, frames: u64| {
        for _ in 0..frames {
            // In real time, so that what the services send arrives on the way.
            std::thread::sleep(Duration::from_millis(16));
            // Transitions start at the real time, so the clock here has to be the real one.
            *now = Instant::now();
            ui.tick(*now);
            ui.animate(*now);
            ui.update();
            ui.paint(*now);
        }
    };
    // Lets the services and pictures arrive.
    let _ = ui.wait_for_images(Duration::from_secs(5));
    std::thread::sleep(Duration::from_millis(600));
    step(&mut ui, &mut now, 30);
    let m = Modifiers::default();
    for (c, button) in clicks {
        let at = to_physical(c);
        ui.handle_event(WindowEvent::PointerMoved {
            position: at,
            modifiers: m,
        });
        ui.handle_event(WindowEvent::PointerButton {
            position: at,
            button,
            pressed: true,
            modifiers: m,
            time: 0,
        });
        ui.handle_event(WindowEvent::PointerButton {
            position: at,
            button,
            pressed: false,
            modifiers: m,
            time: 10,
        });
        step(&mut ui, &mut now, click_frames);
    }
    if args.iter().any(|a| a == "--escape") {
        for pressed in [true, false] {
            ui.handle_event(WindowEvent::Key(anti_webui::core::event::KeyEvent {
                key: anti_webui::core::event::Key::Named(anti_webui::core::event::NamedKey::Escape),
                text: None,
                modifiers: m,
                pressed,
                repeat: false,
            }));
        }
        step(&mut ui, &mut now, 20);
    }
    if let Some((from, to)) = drag {
        let button = |ui: &mut Ui, at: PointF, pressed: bool| {
            ui.handle_event(WindowEvent::PointerButton {
                position: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: m,
                time: 0,
            });
        };
        let start = to_physical(from);
        ui.handle_event(WindowEvent::PointerMoved {
            position: start,
            modifiers: m,
        });
        button(&mut ui, start, true);
        for i in 1..=10 {
            let t = i as f32 / 10.0;
            let at = to_physical((from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t));
            ui.handle_event(WindowEvent::PointerMoved {
                position: at,
                modifiers: m,
            });
            step(&mut ui, &mut now, 1);
        }
        button(&mut ui, to_physical(to), false);
        step(&mut ui, &mut now, click_frames);
    }
    // `--key K`, any number of times, after the clicks: ArrowDown, ArrowUp, Enter, Delete, Escape,
    // or a character to type.
    for (i, _) in args.iter().enumerate().filter(|(_, a)| *a == "--key") {
        use anti_webui::core::event::{Key, KeyEvent, NamedKey};
        let Some(name) = args.get(i + 1) else {
            continue;
        };
        let (key, text) = match name.as_str() {
            "ArrowDown" => (Key::Named(NamedKey::ArrowDown), None),
            "ArrowUp" => (Key::Named(NamedKey::ArrowUp), None),
            "Enter" => (Key::Named(NamedKey::Enter), None),
            "Delete" => (Key::Named(NamedKey::Delete), None),
            "Escape" => (Key::Named(NamedKey::Escape), None),
            other => (Key::Character(other.into()), Some(other.to_owned())),
        };
        for pressed in [true, false] {
            ui.handle_event(WindowEvent::Key(KeyEvent {
                key: key.clone(),
                text: text.clone().filter(|_| pressed),
                modifiers: m,
                pressed,
                repeat: false,
            }));
        }
        step(&mut ui, &mut now, 4);
    }
    if let Some(p) = pointer {
        ui.handle_event(WindowEvent::PointerMoved {
            position: to_physical(p),
            modifiers: m,
        });
    }
    let _ = ui.wait_for_images(Duration::from_secs(5));
    step(&mut ui, &mut now, ms / 16);
    for i in 0..=frames {
        if i > 0 {
            step(&mut ui, &mut now, (every / 16).max(1));
        }
        ui.update();
        let scene = ui.paint(now);
        let pixels = gpu.render_to_rgba(&mut renderer, &scene);
        let path = if frames == 0 {
            out.clone()
        } else {
            out.replace(".png", &format!("-{i}.png"))
        };
        write_png(&path, w, h, &pixels)?;
    }
    // Whether anything still asks for frames at the end: it should not once all has settled.
    if ui.animating() {
        eprintln!("still asking for frames");
    }
    Ok(())
}
