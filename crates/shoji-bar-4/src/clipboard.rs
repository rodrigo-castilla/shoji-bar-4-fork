//! The clipboard's history, as cliphist keeps it: a search field over what was copied, newest
//! first, the pictures as small copies of themselves. Arrows move the choice, Enter or a click puts
//! it back on the clipboard, Delete forgets it (Shift+Delete while searching), Escape closes.

use crate::panels::close;
use crate::theme::{TextStyle, colors, elevation};
use crate::ui::*;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui_md3e::components::sized_icon;
use anti_webui_md3e::icons::{self, Symbol};
use anti_webui_md3e::motion::{default_spatial, fast_spatial};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

const WIDTH: f32 = 560.0;
/// How tall the list is: about seven lines of text.
const LIST_H: f32 = 452.0;
const TEXT_H: f32 = 60.0;
const PICTURE_H: f32 = 96.0;
/// Between two entries, as a part of the one above, so that it goes when that one does.
const GAP: f32 = 4.0;
/// How far the list keeps the choice from its ends when it scrolls to it.
const MARGIN: f32 = 8.0;
/// How many of the matches are listed.
const SHOWN: usize = 100;
/// How many entries come in one after another when it opens; the rest come with the last.
const STAGGERED: usize = 9;

/// Something cliphist kept.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    /// The line cliphist listed it as, which is how it is asked for again.
    line: String,
    id: u64,
    /// The start of the text, or what cliphist says of a picture.
    preview: String,
    picture: Option<Picture>,
}

#[derive(Clone, Debug, PartialEq)]
struct Picture {
    format: String,
    width: u32,
    height: u32,
    size: String,
}

impl Entry {
    fn parse(line: &str) -> Option<Self> {
        let (id, preview) = line.split_once('\t')?;
        Some(Self {
            line: line.to_owned(),
            id: id.parse().ok()?,
            preview: preview.trim().to_owned(),
            picture: Picture::parse(preview),
        })
    }

    fn height(&self) -> f32 {
        if self.picture.is_some() {
            PICTURE_H
        } else {
            TEXT_H
        }
    }

    /// Whether a search finds it: in its text, or a picture by its format.
    fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || match &self.picture {
                Some(p) => p.format.contains(&query) || "image".contains(&query),
                None => self.preview.to_lowercase().contains(&query),
            }
    }

    /// Where the small copy of its picture goes.
    fn picture_path(&self) -> Option<PathBuf> {
        let p = self.picture.as_ref()?;
        Some(pictures_dir().join(format!("{}.{}", self.id, p.format)))
    }
}

impl Picture {
    /// What cliphist lists a picture as: `[[ binary data 48 KiB png 316x192 ]]`.
    fn parse(preview: &str) -> Option<Self> {
        let rest = preview
            .trim()
            .strip_prefix("[[ binary data ")?
            .strip_suffix("]]")?;
        let mut words = rest.split_whitespace();
        let size = format!("{} {}", words.next()?, words.next()?);
        let format = words.next()?.to_lowercase();
        if !matches!(
            format.as_str(),
            "png" | "jpeg" | "jpg" | "webp" | "gif" | "bmp"
        ) {
            return None;
        }
        let (width, height) = words.next()?.split_once('x')?;
        Some(Self {
            format,
            width: width.parse().ok()?,
            height: height.parse().ok()?,
            size,
        })
    }

    fn describe(&self) -> String {
        format!(
            "{} · {} × {} · {}",
            self.format.to_uppercase(),
            self.width,
            self.height,
            self.size
        )
    }
}

thread_local! {
    /// What was listed the last time, shown at once the next time while cliphist is asked again.
    static LAST: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
}

/// Where pictures are written out for their small copies; they last until the session ends.
fn pictures_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("shoji-bar-4-clipboard")
}

/// Asks cliphist for what it has, off this thread. `None` when it couldn't.
fn load(done: impl Fn(Option<Vec<Entry>>) + 'static) {
    let to_ui = ui_channel(done);
    let _ = std::thread::Builder::new()
        .name("shoji-bar-4 cliphist".into())
        .spawn(move || {
            let listed = Command::new("cliphist")
                .args(["-preview-width", "300", "list"])
                .stderr(Stdio::null())
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| {
                    String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .filter_map(Entry::parse)
                        .collect()
                });
            to_ui.send(listed);
        });
}

/// Writes pictures out, one after another off this thread, and says which are there. It stops
/// when what it returns goes.
fn picture_writer(written: Signal<HashSet<u64>>) -> mpsc::Sender<Entry> {
    let (tx, rx) = mpsc::channel::<Entry>();
    let to_ui = ui_channel(move |id: u64| written.update(|w| _ = w.insert(id)));
    let _ = std::thread::Builder::new()
        .name("shoji-bar-4 pictures".into())
        .spawn(move || {
            let _ = std::fs::create_dir_all(pictures_dir());
            for entry in rx {
                let Some(path) = entry.picture_path() else {
                    continue;
                };
                if path.exists() || decode(&entry.line, &path).is_ok() {
                    to_ui.send(entry.id);
                }
            }
        });
    tx
}

/// `cliphist decode` into a file, which is only there once it is whole.
fn decode(line: &str, to: &Path) -> std::io::Result<()> {
    let part = to.with_extension("part");
    let mut child = Command::new("cliphist")
        .arg("decode")
        .stdin(Stdio::piped())
        .stdout(std::fs::File::create(&part)?)
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        writeln!(stdin, "{line}")?;
    }
    if child.wait()?.success() {
        std::fs::rename(&part, to)
    } else {
        let _ = std::fs::remove_file(&part);
        Err(std::io::Error::other("cliphist decode failed"))
    }
}

/// Runs a shell script with `line` as `$1`, off this thread.
fn run(script: &'static str, line: String) {
    let _ = std::thread::Builder::new()
        .name("shoji-bar-4 cliphist".into())
        .spawn(move || {
            let _ = Command::new("sh")
                .args(["-c", script, "sh", &line])
                .stdin(Stdio::null())
                .status();
        });
}

/// Puts it on the clipboard again; cliphist then lists it first.
fn copy(entry: &Entry) {
    run(
        r#"printf '%s\n' "$1" | cliphist decode | wl-copy"#,
        entry.line.clone(),
    );
}

/// Has cliphist forget it.
fn forget(entry: &Entry) {
    run(
        r#"printf '%s\n' "$1" | cliphist delete"#,
        entry.line.clone(),
    );
    if let Some(path) = entry.picture_path() {
        let _ = std::fs::remove_file(path);
    }
}

/// What `f` says of the list, without depending on it.
fn untrack_with<R>(list: Memo<Vec<Entry>>, f: impl FnOnce(&Vec<Entry>) -> R) -> R {
    untrack(|| list.with(f))
}

/// Where an entry of the list is in it, and how tall: the place of the highlight.
fn spot(list: &[Entry], index: usize) -> Option<(f32, f32)> {
    let entry = list.get(index)?;
    let top = list[..index].iter().map(|e| e.height() + GAP).sum();
    Some((top, entry.height()))
}

pub fn panel(output: Output) -> Div {
    let _ = output;
    let entries = signal(LAST.with(|l| l.borrow().clone()));
    let loaded = signal(false);
    let failed = signal(false);
    load(move |listed| {
        loaded.set(true);
        match listed {
            Some(list) => {
                LAST.with(|l| *l.borrow_mut() = list.clone());
                entries.set(list);
            }
            None => failed.set(true),
        }
    });
    let query = signal(String::new());
    let picked = signal(0usize);
    let results = memo(move || {
        let query = query.get();
        entries.with(|all| {
            all.iter()
                .filter(|e| e.matches(&query))
                .take(SHOWN)
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    let written = signal(HashSet::new());
    let writer = Rc::new(picture_writer(written));

    let list = node_ref();
    // The list's scrolling while the choice is followed by keys; the wheel scrolls it as it is.
    let scroll = animated(0.0f32);
    effect(move || {
        let y = scroll.get();
        list.scroll_to(0.0, y);
    });
    effect(move || {
        query.with(|_| ());
        picked.set(0);
        scroll.stop();
        scroll.set(0.0);
        list.scroll_to(0.0, 0.0);
    });
    // Scrolls so that the choice is in view, if it isn't.
    let reveal = move |index: usize| {
        let Some((top, height)) = untrack_with(results, |r| spot(r, index)) else {
            return;
        };
        let now = list.scroll_offset().y;
        let to = if top - MARGIN < now {
            top - MARGIN
        } else if top + height + MARGIN > now + LIST_H {
            top + height + MARGIN - LIST_H
        } else {
            return;
        };
        scroll.set(now);
        scroll.spring_to(to.max(0.0), default_spatial());
    };
    let step = move |by: isize| {
        let n = untrack_with(results, Vec::len) as isize;
        if n > 0 {
            let to = (picked.get_untracked() as isize + by).clamp(0, n - 1) as usize;
            picked.set(to);
            reveal(to);
        }
    };
    let choose = move |entry: &Entry| {
        copy(entry);
        close();
    };
    let drop_entry = move |entry: &Entry| {
        forget(entry);
        let id = entry.id;
        entries.update(|all| all.retain(|e| e.id != id));
        LAST.with(|l| l.borrow_mut().retain(|e| e.id != id));
        let n = untrack_with(results, Vec::len);
        picked.update(|p| *p = (*p).min(n.saturating_sub(1)));
    };
    let picked_entry = move || untrack_with(results, |r| r.get(picked.get_untracked()).cloned());

    let input = node_ref();
    request_frame(move |_| input.focus());

    // The highlight under the choice, which slides and stretches to the next one.
    let target = memo(move || results.with(|r| spot(r, picked.get())));
    let top = animated(0.0f32);
    let tall = animated(TEXT_H);
    let placed = Cell::new(false);
    effect(move || {
        if let Some((y, h)) = target.get() {
            if placed.replace(true) {
                top.spring_to(y, default_spatial());
                tall.spring_to(h, default_spatial());
            } else {
                top.set(y);
                tall.set(h);
            }
        }
    });
    let lit = follow(
        move || if target.get().is_some() { 1.0f32 } else { 0.0 },
        fast_spatial(),
    );

    let field = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(10.0_f32)
        .h(46.0)
        .px(16.0_f32)
        .rounded(23.0_f32)
        .dyn_style(|s| s.bg(colors().surface_container_highest))
        .child(sym(icons::SEARCH, 20.0, |c| c.on_surface_variant))
        .child(
            TextStyle::BODY_MEDIUM
                .apply(text_input())
                .value(query)
                .placeholder("Search the clipboard")
                .select_text()
                .node_ref(input)
                .flex_1()
                .on_submit(move |_| {
                    if let Some(entry) = picked_entry() {
                        choose(&entry);
                    }
                })
                .on_key_down(move |e| match e.key {
                    Key::Named(NamedKey::Escape) => close(),
                    Key::Named(NamedKey::ArrowDown) => step(1),
                    Key::Named(NamedKey::ArrowUp) => step(-1),
                    Key::Named(NamedKey::PageDown) => step(6),
                    Key::Named(NamedKey::PageUp) => step(-6),
                    Key::Named(NamedKey::Delete)
                        if e.modifiers.shift || query.with_untracked(String::is_empty) =>
                    {
                        if let Some(entry) = picked_entry() {
                            drop_entry(&entry);
                        }
                    }
                    _ => {}
                }),
        )
        .child(show(
            move || !query.with(String::is_empty),
            move || {
                icon_btn(icons::CLOSE, 30.0, |c| c.on_surface_variant)
                    .on_click(move |_| query.set(String::new()))
            },
        ));

    let highlight = div()
        .absolute()
        .left(0.0)
        .right(0.0)
        .z_index(-1)
        .pointer_events_none()
        .dyn_style(move |s| {
            let c = colors();
            let h = tall.get().max(0.0);
            s.top(top.get())
                .h(h)
                .rounded((h / 2.0).min(28.0))
                .bg(c.secondary_container)
                .opacity(lit.get().clamp(0.0, 1.0))
        });

    let rows = div()
        .relative()
        // Its own stacking context, so that the highlight is behind the entries but not behind
        // this, which would take their clicks.
        .z_index(0)
        .flex()
        .flex_col()
        .child(highlight)
        .child(
            for_each(
                move || results.get(),
                |e| e.id,
                move |entry| {
                    let id = entry.id;
                    let index = memo(move || results.with(|r| r.iter().position(|e| e.id == id)));
                    if entry.picture_path().is_some_and(|p| !p.exists()) {
                        let _ = writer.send(entry.clone());
                    }
                    row(entry, index, picked, written, choose, drop_entry)
                },
            )
            // Goes by closing up, those below it rising into its place.
            .exit(|s| s.h(0.0).opacity(0.0_f32).translate_x(-24.0_f32))
            .moves(emphasized(380))
            .transition(emphasized(320)),
        );

    div()
        .w(WIDTH)
        .flex()
        .flex_col()
        .gap(12.0_f32)
        .p(16.0_f32)
        .rounded(32.0_f32)
        .shadows(elevation(4))
        .dyn_style(|s| {
            let c = colors();
            s.bg(c.surface_container).text_color(c.on_surface)
        })
        .child(field)
        .child(
            div()
                .relative()
                .h(LIST_H)
                .child(
                    div()
                        .node_ref(list)
                        .size_full()
                        .overflow_y_scroll()
                        .child(rows),
                )
                .child(
                    show(
                        move || loaded.get() && results.with(Vec::is_empty),
                        move || nothing(query, failed),
                    )
                    .appear()
                    .enter(|s| s.opacity(0.0_f32).scale(0.9_f32))
                    .exit(|s| s.opacity(0.0_f32))
                    .transition(overshoot(360)),
                ),
        )
        .child(footer(results, entries, query))
}

/// What the list says when it has nothing to show.
fn nothing(query: Signal<String>, failed: Signal<bool>) -> Div {
    let (symbol, said): (&'static Symbol, String) = if failed.get_untracked() {
        (icons::CONTENT_PASTE_OFF, "cliphist isn't there".into())
    } else if query.with_untracked(|q| q.trim().is_empty()) {
        (icons::CONTENT_PASTE, "Nothing copied yet".into())
    } else {
        (icons::SEARCH_OFF, String::new())
    };
    div()
        .absolute()
        .inset(0.0)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(8.0_f32)
        .pointer_events_none()
        .child(sym(symbol, 48.0, |c| c.outline))
        .child(
            txt(TextStyle::TITLE_MEDIUM, move || {
                if said.is_empty() {
                    format!("Nothing like “{}”", query.get().trim())
                } else {
                    said.clone()
                }
            })
            .dyn_style(|s| s.text_color(colors().on_surface_variant)),
        )
}

/// The keys, and how many there are.
fn footer(results: Memo<Vec<Entry>>, entries: Signal<Vec<Entry>>, query: Signal<String>) -> Div {
    let key = |cap: &'static str, does: &'static str| {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(6.0_f32)
            .child(
                static_txt(TextStyle::LABEL_SMALL, cap)
                    .h(20.0)
                    .min_w(20.0)
                    .px(6.0_f32)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(6.0_f32)
                    .dyn_style(|s| {
                        let c = colors();
                        s.bg(c.surface_container_highest)
                            .text_color(c.on_surface_variant)
                    }),
            )
            .child(
                static_txt(TextStyle::LABEL_MEDIUM, does)
                    .dyn_style(|s| s.text_color(colors().on_surface_variant)),
            )
    };
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(14.0_f32)
        .px(6.0_f32)
        .child(key("↑↓", "Move"))
        .child(key("Enter", "Copy"))
        .child(key("Del", "Forget"))
        .child(key("Esc", "Close"))
        .child(div().flex_1())
        .child(
            txt(TextStyle::LABEL_MEDIUM, move || {
                let all = entries.with(Vec::len);
                if query.with(|q| q.trim().is_empty()) {
                    format!("{all} kept")
                } else {
                    let found = results.with(Vec::len);
                    let more = if found == SHOWN { "+" } else { "" };
                    format!("{found}{more} of {all}")
                }
            })
            .dyn_style(|s| s.text_color(colors().outline)),
        )
}

/// An entry: what it is, its text or its picture, and a button to forget it. It comes in a little
/// after the one above it the first time the list shows, and squashes when pressed.
fn row(
    entry: Entry,
    index: Memo<Option<usize>>,
    picked: Signal<usize>,
    written: Signal<HashSet<u64>>,
    choose: impl Fn(&Entry) + Copy + 'static,
    forget: impl Fn(&Entry) + Copy + 'static,
) -> Div {
    let chosen = memo(move || index.get() == Some(picked.get()));
    let hovered = signal(false);
    let pressed = signal(false);
    let squash = follow(
        move || if pressed.get() { 0.97 } else { 1.0 },
        fast_spatial(),
    );
    let near = follow(
        move || {
            if hovered.get() || chosen.get() {
                1.0f32
            } else {
                0.0
            }
        },
        fast_spatial(),
    );
    // Laid out first when the layer is; timers set before then could be over by then.
    let shown = animated(0.0f32);
    let started = Cell::new(false);
    let delay = untrack(|| index.get()).unwrap_or(0).min(STAGGERED) as u64 * 22;
    let height = entry.height();
    let id = entry.id;
    let leading = match entry.picture_path() {
        Some(path) => thumbnail(path, written, id),
        None => kind_badge(&entry.preview, chosen),
    };
    let (title, detail) = match &entry.picture {
        Some(p) => (
            static_txt(TextStyle::BODY_MEDIUM, "Image"),
            Some(p.describe()),
        ),
        None => (
            // Two lines at most.
            TextStyle::BODY_MEDIUM
                .apply(div())
                .max_h(40.0)
                .overflow_hidden()
                .child(entry.preview.clone()),
            None,
        ),
    };
    let chosen_text = move |s: Style| {
        let c = colors();
        s.text_color(if chosen.get() {
            c.on_secondary_container
        } else {
            c.on_surface
        })
    };
    let forget_entry = entry.clone();
    let chosen_entry = entry;
    let mut text = div().flex_1().min_w(0.0).flex().flex_col().gap(2.0_f32).child(
        title
            .transition_of([Prop::TextColor], Transition::ms(160))
            .dyn_style(chosen_text),
    );
    if let Some(d) = detail {
        text = text.child(
            static_txt(TextStyle::LABEL_MEDIUM, d)
                .dyn_style(|s| s.text_color(colors().on_surface_variant)),
        );
    }
    div()
        .h(height + GAP)
        .pb(GAP)
        .flex_none()
        .overflow_hidden()
        .on_resized(move |_, _| {
            if !started.replace(true) {
                set_timeout(Duration::from_millis(delay), move || {
                    shown.spring_to(1.0, fast_spatial())
                });
            }
        })
        .child(
            div()
                .size_full()
                .flex()
                .flex_row()
                .items_center()
                .gap(14.0_f32)
                .pl(10.0_f32)
                .pr(8.0_f32)
                .on_hover(move |h| {
                    hovered.set(h);
                    if h && let Some(i) = untrack(|| index.get()) {
                        picked.set(i);
                    }
                })
                .on_active(move |a| pressed.set(a))
                .on_click(move |_| choose(&chosen_entry))
                .dyn_style(move |s| {
                    let t = shown.get();
                    s.opacity(t.clamp(0.0, 1.0))
                        .scale((0.92 + 0.08 * t) * squash.get())
                        .translate_y(18.0 * (1.0 - t))
                })
                .child(leading)
                .child(text)
                .child(
                    icon_btn(icons::CLOSE, 32.0, |c| c.on_surface_variant)
                        .on_click(move |e| {
                            e.stop_propagation();
                            forget(&forget_entry);
                        })
                        .dyn_style(move |s| {
                            let n = near.get();
                            s.opacity(n.clamp(0.0, 1.0)).scale(0.6 + 0.4 * n)
                        }),
                ),
        )
}

/// A square that says what kind of text it is; the choice's is lit.
fn kind_badge(text: &str, chosen: Memo<bool>) -> Div {
    let t = text.trim_start();
    let symbol: &'static Symbol = if t.starts_with("http://") || t.starts_with("https://") {
        icons::LINK
    } else if t.starts_with('/') || t.starts_with("~/") || t.starts_with("$ ") {
        icons::TERMINAL
    } else {
        icons::NOTES
    };
    let round = follow(
        move || if chosen.get() { 1.0f32 } else { 0.0 },
        fast_spatial(),
    );
    div()
        .size(40.0)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .transition_of([Prop::Background], Transition::ms(200).ease_out())
        .dyn_style(move |s| {
            let c = colors();
            s.rounded(12.0 + 8.0 * round.get()).bg(if chosen.get() {
                c.primary
            } else {
                c.surface_container_highest
            })
        })
        .child(
            sized_icon(symbol, 20.0)
                .transition_of([Prop::TextColor], Transition::ms(200).ease_out())
                .dyn_style(move |s| {
                    let c = colors();
                    s.text_color(if chosen.get() {
                        c.on_primary
                    } else {
                        c.on_surface_variant
                    })
                }),
        )
}

/// The small copy of a picture, which fades in once it is written out.
fn thumbnail(path: PathBuf, written: Signal<HashSet<u64>>, id: u64) -> Div {
    let already = path.exists();
    div()
        .w(128.0)
        .h(PICTURE_H - 16.0)
        .flex_none()
        .relative()
        .rounded(14.0_f32)
        .overflow_hidden()
        .dyn_style(|s| s.bg(colors().surface_container_highest))
        .child(
            div()
                .absolute()
                .inset(0.0)
                .flex()
                .items_center()
                .justify_center()
                .child(sym(icons::IMAGE, 28.0, |c| c.outline)),
        )
        .child(
            show(
                move || already || written.with(|w| w.contains(&id)),
                move || {
                    img(path.clone())
                        .absolute()
                        .inset(0.0)
                        .rounded(14.0_f32)
                        .object_fit(ObjectFit::Cover)
                },
            )
            .enter(|s| s.opacity(0.0_f32).scale(1.08_f32))
            .transition(emphasized(400)),
        )
}
