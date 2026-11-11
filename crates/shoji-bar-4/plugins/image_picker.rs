//! Omarchy's image picker: the grid the Theme and Background rows of the menu open.
//!
//! Two ways in, both answered here instead of by Quickshell:
//! - `omarchy-shell image-selector open "" <rows base64> <selected> <selection file> <done file>
//!   <labels> <filterable>` (what `omarchy-menu-images` calls, and so `omarchy-theme-bg-switcher`):
//!   the choice is written to the selection file, then the done file is created, which is what the
//!   caller waits for.
//! - `omarchy-shell shell summon omarchy.image-picker '{"source":"themes"}'`: the themes, from
//!   `omarchy-theme-switcher --print-rows`; the choice is applied with `omarchy-theme-set`.
//!
//! Arrows move, Enter chooses, Escape closes, typing filters, a click chooses.

use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui_md3e::icons;
use shoji_core::theme::{TextStyle, colors, elevation};
use shoji_core::ui::{pressable, static_txt, sym};
use shoji_default::Ui;
use shoji_default::panels::{PanelDef, close, open, rise_in};
use std::cell::RefCell;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;

const PANEL: &str = "omarchy-image-picker";
const COLUMNS: usize = 3;
const CELL_W: f32 = 232.0;
const CELL_H: f32 = 130.0;

#[derive(Clone, Default)]
struct Picker {
    /// (image, thumbnail)
    rows: Vec<(String, String)>,
    selected: String,
    selection_file: String,
    done_file: String,
    themes: bool,
    labels: bool,
}

thread_local! {
    static REQUEST: RefCell<Option<Picker>> = const { RefCell::new(None) };
}

fn stem(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(s, _)| s).to_owned()
}

fn label(path: &str) -> String {
    let mut up = true;
    stem(path)
        .chars()
        .map(|c| {
            let c = if matches!(c, '-' | '_') { ' ' } else { c };
            let out = if up { c.to_ascii_uppercase() } else { c };
            up = c == ' ';
            out
        })
        .collect()
}

fn parse_rows(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let (image, thumb) = l.split_once('\t')?;
            Some((image.to_owned(), thumb.to_owned()))
        })
        .collect()
}

fn base64_decode(text: &str) -> String {
    let Ok(mut child) = Command::new("base64")
        .arg("-d")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return String::new();
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }
    child.wait_with_output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

fn present(picker: Picker) {
    let Some(output) = shoji_default::ipc::focused_output() else {
        release(&picker, None);
        return;
    };
    // A picker already open is let go first (its caller is waiting).
    REQUEST.with(|r| *r.borrow_mut() = Some(picker));
    close();
    request_frame(move |_| open(&output, PANEL));
}

/// Lets the caller go: the choice (if there is one), then the file that says it is done.
fn release(p: &Picker, choice: Option<&str>) {
    if p.done_file.is_empty() {
        return;
    }
    if let Some(text) = choice {
        let _ = fs::write(PathBuf::from(&p.selection_file), text);
    }
    let _ = fs::write(PathBuf::from(&p.done_file), "");
}

fn themes() -> Picker {
    let out = Command::new("omarchy-theme-switcher").arg("--print-rows").output();
    let rows = out.map(|o| parse_rows(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default();
    let current = fs::read_to_string(format!(
        "{}/.local/state/omarchy/current/theme.name",
        std::env::var("HOME").unwrap_or_default()
    ))
    .unwrap_or_default();
    let current = current.trim().to_lowercase().replace(' ', "-");
    let selected = rows.iter().find(|(i, _)| stem(i).to_lowercase() == current).map(|(i, _)| i.clone());
    Picker { rows, selected: selected.unwrap_or_default(), themes: true, labels: true, ..Default::default() }
}

fn shell_call(verb: &str, id: &str, payload: &str) -> bool {
    if id != "omarchy.image-picker" {
        return false;
    }
    match verb {
        "toggle" | "summon" | "show" | "open" => {
            let json: serde_json::Value = serde_json::from_str(payload).unwrap_or_default();
            if json.get("source").and_then(|v| v.as_str()) == Some("themes") {
                // The scan can take a moment on a cold cache: not on the UI thread.
                let tx = ui_channel::<Picker>(present);
                std::thread::spawn(move || tx.send(themes()));
            }
        }
        "hide" | "close" => close(),
        _ => {}
    }
    true
}

fn call(target: &str, method: &str, args: &[String]) -> Option<String> {
    if target != "image-selector" {
        return None;
    }
    match method {
        "open" => {
            let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
            let flag = |i: usize| arg(i) == "true";
            present(Picker {
                rows: parse_rows(&base64_decode(&arg(1))),
                selected: arg(2),
                selection_file: arg(3),
                done_file: arg(4),
                labels: flag(5),
                themes: false,
            });
            Some("ok".into())
        }
        "close" => {
            close();
            Some("ok".into())
        }
        // `preload`, `state`...: nothing to keep warm here.
        _ => Some("ok".into()),
    }
}

pub fn install(_ui: &mut Ui) {
    shoji_default::omarchy::on_shell_call(shell_call);
    shoji_default::omarchy::on_call(call);
    shoji_core::panels::register(
        PanelDef::new(PANEL, content).with_field().veiled().enter(rise_in),
    );
}

fn content(_output: Output) -> Div {
    let picker = REQUEST.with(|r| r.borrow_mut().take()).unwrap_or_default();
    grid(picker)
}

fn grid(picker: Picker) -> Div {
    let picker = Rc::new(picker);
    let answered = Rc::new(std::cell::Cell::new(false));
    {
        let (p, answered) = (picker.clone(), answered.clone());
        on_cleanup(move || {
            if !answered.get() {
                release(&p, None);
            }
        });
    }
    let query = signal(String::new());
    let shown = memo({
        let picker = picker.clone();
        move || {
            let q = query.get().to_lowercase();
            picker
                .rows
                .iter()
                .filter(|(i, _)| q.is_empty() || stem(i).to_lowercase().contains(&q) || label(i).to_lowercase().contains(&q))
                .cloned()
                .collect::<Vec<_>>()
        }
    });
    let start = untrack(|| shown.with(|s| s.iter().position(|(i, _)| *i == picker.selected))).unwrap_or(0);
    let picked = signal(start);
    effect(move || {
        query.with(|_| ());
        picked.set(0);
    });
    let choose = {
        let (picker, answered) = (picker.clone(), answered.clone());
        Rc::new(move |image: &str| {
            answered.set(true);
            if picker.themes {
                let name = stem(image);
                let _ = Command::new("setsid")
                    .args(["-f", "omarchy-theme-set", &name])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .map(|mut c| std::thread::spawn(move || c.wait()));
                release(&picker, None);
            } else {
                release(&picker, Some(image));
            }
            close();
        })
    };
    let step = move |by: isize| {
        let n = shown.with(Vec::len) as isize;
        if n > 0 {
            picked.update(|p| *p = (*p as isize + by).clamp(0, n - 1) as usize);
        }
    };
    let input = node_ref();
    request_frame(move |_| input.focus());
    let submit = choose.clone();
    let field = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(10.0_f32)
        .h(42.0)
        .px(14.0_f32)
        .rounded(21.0_f32)
        .dyn_style(|s| s.bg(colors().surface_container_highest))
        .child(sym(icons::SEARCH, 18.0, |c| c.on_surface_variant))
        .child(
            TextStyle::BODY_MEDIUM
                .apply(text_input())
                .value(query)
                .placeholder("Search")
                .select_text()
                .node_ref(input)
                .flex_1()
                .on_submit(move |_| {
                    if let Some((image, _)) = shown.with(|s| s.get(untrack(|| picked.get())).cloned()) {
                        submit(&image);
                    }
                })
                .on_key_down(move |e| match e.key {
                    Key::Named(NamedKey::Escape) => close(),
                    Key::Named(NamedKey::ArrowRight) => step(1),
                    Key::Named(NamedKey::ArrowLeft) => step(-1),
                    Key::Named(NamedKey::ArrowDown) => step(COLUMNS as isize),
                    Key::Named(NamedKey::ArrowUp) => step(-(COLUMNS as isize)),
                    _ => {}
                }),
        );
    let labels = picker.labels;
    let cells = for_each(
        move || shown.get(),
        |(i, _)| i.clone(),
        move |(image, thumb)| {
            let key = image.clone();
            let chosen = memo(move || shown.with(|s| s.iter().position(|(i, _)| *i == key)) == Some(picked.get()));
            let hover_key = image.clone();
            let click = choose.clone();
            let click_image = image.clone();
            let name = label(&image);
            pressable(
                div()
                    .w(CELL_W)
                    .flex()
                    .flex_col()
                    .gap(4.0_f32)
                    .p(6.0_f32)
                    .rounded(20.0_f32)
                    .on_hover(move |h| {
                        if h && let Some(i) = shown.with(|s| s.iter().position(|(x, _)| *x == hover_key)) {
                            picked.set(i);
                        }
                    })
                    .on_click(move |_| click(&click_image))
                    .dyn_style(move |s| {
                        let c = colors();
                        s.bg(if chosen.get() { c.secondary_container } else { c.secondary_container.with_alpha(0.0) })
                            .text_color(c.on_surface)
                    })
                    .child(
                        div()
                            .w(CELL_W - 12.0)
                            .h(CELL_H)
                            .rounded(14.0_f32)
                            .overflow_hidden()
                            .child(img(thumb).size_full().object_fit(ObjectFit::Cover)),
                    )
                    .child(show(move || labels, move || {
                        static_txt(TextStyle::LABEL_MEDIUM, name.clone()).overflow_hidden()
                    })),
                |c| c.on_secondary_container,
            )
        },
    );
    div()
        .w(COLUMNS as f32 * CELL_W + 36.0)
        .flex()
        .flex_col()
        .gap(8.0_f32)
        .p(12.0_f32)
        .rounded(28.0_f32)
        .shadows(elevation(4))
        .dyn_style(|s| {
            let c = colors();
            s.bg(c.surface_container).text_color(c.on_surface)
        })
        .child(field)
        .child(div().max_h(3.0 * (CELL_H + 40.0)).overflow_y_scroll().flex().flex_row().flex_wrap().child(cells))
}
