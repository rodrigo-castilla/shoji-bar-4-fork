//! The tray as a drop-down: the bar shows one `⌄` island instead of every icon, and a click on it
//! opens a scrollable 3-column grid of the same tray items, sliding down from under the bar.
//!
//! Left click on an item activates it, right click opens its menu — as in the stock tray.

use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui::shell::{MenuToggle, TrayIcon, TrayMenuItem};
use anti_webui_md3e::components::{MenuItem, MenuStyle, menu_item, styled_menu};
use shoji_core::theme::{TextStyle, colors, elevation};
use shoji_core::ui::{island, overshoot, pressable, txt};
use shoji_default::panels::{PanelDef, close, from_above, is_open, open, toggle};
use anti_webui_md3e::icons;
use shoji_core::ui::sym;
use super::agents;
use anti_webui::core::images::Bitmap;
use std::sync::Arc;
use shoji_default::{Island, Ui};

const PANEL: &str = "tray";
const COLUMNS: usize = 3;
const CELL: f32 = 38.0;
const GAP: f32 = 2.0;
const ICON: f32 = 24.0;
/// Rows visible before the grid scrolls.
const VISIBLE_ROWS: f32 = 3.0;

/// Swaps the bar's tray island for the `⌄` one and registers the grid panel.
pub fn install(ui: &mut Ui) {
    if let Some(bar) = ui.bar.as_mut() {
        bar.replace("tray", Island::new("tray", |output| chevron(output)));
    }
    ui.replace_panel(agents::panel());
    ui.replace_panel(
        PanelDef::new(PANEL, |output| grid(output))
            // Under the bar, at the right edge, where the island is.
            .place(|root| root.items_end().justify_start().pt(46.0_f32).pr(14.0_f32))
            .enter(from_above),
    );
}

/// The `⌄` island: it turns into `⌃` while the grid is open.
fn chevron(output: Output) -> Show {
    let open = memo({
        let output = output.clone();
        move || is_open(&output, PANEL)
    });
    show(
        || true,
        move || {
            let output = output.clone();
            pressable(
                island()
                    .px(12.0_f32)
                    .on_click(move |_| toggle(&output, PANEL))
                    .dyn_style(move |s| {
                        let c = colors();
                        if open.get() {
                            s.bg(c.primary_container).text_color(c.on_primary_container)
                        } else {
                            s.text_color(c.on_surface)
                        }
                    })
                    .child(
                        div()
                            .relative()
                            .w(16.0_f32)
                            .h(20.0_f32)
                            .child(glyph("⌄", open, false))
                            .child(glyph("⌃", open, true)),
                    ),
                |c| c.on_surface,
            )
        },
    )
    .enter(|s| s.opacity(0.0_f32).scale(0.8_f32))
    .exit(|s| s.opacity(0.0_f32).scale(0.8_f32))
    .transition(overshoot(300))
}

/// One of the two arrows. They sit on top of each other and swap as the panel opens or closes:
/// the one that is leaving fades and shrinks while the other grows in.
fn glyph(arrow: &'static str, open: Memo<bool>, shown_when_open: bool) -> Div {
    txt(TextStyle::LABEL_LARGE, move || arrow.to_owned())
        .absolute()
        .inset(0.0)
        .flex()
        .items_center()
        .justify_center()
        .transition(overshoot(350))
        .dyn_style(move |s| {
            if open.get() == shown_when_open {
                s.opacity(1.0_f32).scale(1.0_f32)
            } else {
                s.opacity(0.0_f32).scale(0.4_f32)
            }
        })
}

/// The panel: the tray items, three to a row, scrolling when there are more than three rows.
fn grid(output: Output) -> Div {
    let width = COLUMNS as f32 * CELL + (COLUMNS as f32 - 1.0) * GAP;
    let height = VISIBLE_ROWS * CELL + (VISIBLE_ROWS - 1.0) * GAP;
    div()
        .p(6.0_f32)
        .rounded(18.0_f32)
        .shadows(elevation(3))
        .dyn_style(|s| s.bg(colors().surface_container_high).text_color(colors().on_surface))
        .child(
            div()
                .w(width)
                .max_h(height)
                .overflow_y_scroll()
                .child(dyn_view(move || {
                    // The agents' button first, then what the tray has.
                    let mut items: Vec<Option<TrayItem>> = vec![None];
                    items.extend(tray().items().into_iter().map(Some));
                    let mut rows = div().flex().flex_col().gap(GAP);
                    for chunk in items.chunks(COLUMNS) {
                        let mut row = div().flex().flex_row().gap(GAP);
                        for item in chunk {
                            row = row.child(match item {
                                Some(item) => cell(item.clone()),
                                None => agents_cell(output.clone()),
                            });
                        }
                        rows = rows.child(row);
                    }
                    rows.into_view()
                })),
        )
}

thread_local! {
    /// The pixels of the tray's pictures, by the id of the bitmap the shell made of them.
    static PIXELS: std::cell::RefCell<std::collections::HashMap<u64, (u32, u32, Vec<u8>, bool)>> =
        Default::default();
}

/// The picture of a tray icon, as a bitmap of its own that is made anew each time.
///
/// The shell's bitmaps let go of their pixels once the GPU has them, and a panel that closes
/// drops its textures: an icon drawn from the shell's bitmap shows the first time the panel opens
/// and is blank from then on (until the program sets its icon again, which makes a new bitmap).
/// So the pixels are copied out the first time they are seen, and every build of a cell gets a
/// fresh bitmap made from the copy.
fn picture(icon: &TrayIcon) -> Option<Arc<Bitmap>> {
    let source = icon.best(48)?;
    PIXELS.with(|pixels| {
        let mut pixels = pixels.borrow_mut();
        if !pixels.contains_key(&source.id()) {
            let mut level0 = None;
            source.with_levels(|i, _, data| {
                if i == 0 {
                    level0 = Some(data.to_vec());
                }
            });
            if let Some(data) = level0 {
                pixels.insert(
                    source.id(),
                    (source.width(), source.height(), data, source.is_opaque()),
                );
            }
        }
        let (mut w, mut h, mut data, opaque) = pixels.get(&source.id())?.clone();
        // The bitmap has no smaller levels, so shrink it here while it is much bigger than the
        // 28 px it is drawn at (2x2 average, alpha is premultiplied so it just averages).
        while w >= 56 && h >= 56 && w % 2 == 0 && h % 2 == 0 {
            let (nw, nh) = (w / 2, h / 2);
            let mut out = vec![0u8; (nw * nh * 4) as usize];
            for y in 0..nh {
                for x in 0..nw {
                    for c in 0..4 {
                        let at = |dx: u32, dy: u32| {
                            data[(((2 * y + dy) * w + 2 * x + dx) * 4 + c) as usize] as u32
                        };
                        out[((y * nw + x) * 4 + c) as usize] =
                            ((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1)) / 4) as u8;
                    }
                }
            }
            (w, h, data) = (nw, nh, out);
        }
        Some(Arc::new(Bitmap::from_premultiplied(w, h, data, opaque)))
    })
}

/// The button that opens the agents panel (see agents.rs).
fn agents_cell(output: Output) -> Div {
    pressable(
        div()
            .size(CELL)
            .rounded(12.0_f32)
            .flex()
            .items_center()
            .justify_center()
            .on_click(move |_| {
                close();
                open(&output.name, agents::PANEL);
            })
            .child(sym(icons::ROBOT, ICON, |c| c.on_surface)),
        |c| c.on_surface,
    )
}

/// The file of an icon an item names instead of drawing: in the item's own theme path, else in the
/// icon theme.
fn named_icon(icon: &TrayIcon) -> Option<std::path::PathBuf> {
    let name = icon.name.as_deref()?;
    if let Some(dir) = &icon.theme_path {
        for ext in ["png", "svg"] {
            let file = std::path::Path::new(dir).join(format!("{name}.{ext}"));
            if file.is_file() {
                return Some(file);
            }
        }
    }
    shoji_core::appinfo::icon_file(name, 64)
}

/// One item of the grid.
fn cell(item: TrayItem) -> Div {
    let open = signal(false);
    let key = item.key();
    let now = move || tray().items().into_iter().find(|i| i.key() == key);
    let shown = now.clone();
    let clicked = item.clone();
    pressable(
        div()
            .size(CELL)
            .rounded(12.0_f32)
            .flex()
            .items_center()
            .justify_center()
            .child(dyn_view(move || {
                let item = shown();
                match item.as_ref().and_then(|i| picture(&i.icon)) {
                    Some(picture) => bitmap_view(move || Some(picture.clone()))
                        .size(ICON)
                        .into_view(),
                    None => match item.as_ref().and_then(|i| named_icon(&i.icon)) {
                        Some(file) => img(file).size(ICON).into_view(),
                        None => txt(TextStyle::LABEL_LARGE, {
                        let shown = shown.clone();
                        move || {
                            shown()
                                .and_then(|i| i.title.chars().next())
                                .map(String::from)
                                .unwrap_or_default()
                        }
                    })
                        .into_view(),
                    },
                }
            }))
            .on_pointer_down(move |e| match e.button {
                Some(PointerButton::Secondary) if clicked.has_menu => open.set(true),
                Some(PointerButton::Primary) if clicked.item_is_menu => open.set(true),
                Some(PointerButton::Primary) => {
                    clicked.activate(0, 0);
                    close();
                }
                _ => {}
            })
            .child(menu(&item, open)),
        |c| c.on_surface,
    )
}

fn menu(item: &TrayItem, open: Signal<bool>) -> View {
    let item = item.clone();
    effect({
        let item = item.clone();
        move || {
            if open.get() {
                // Programs fill some menus only when told they are about to be shown.
                let _ = item.menu_about_to_show(0);
            }
        }
    });
    dyn_view(move || {
        styled_menu(
            move || open.get(),
            move || open.set(false),
            groups(&item, &item.menu()),
            MenuStyle::new().placement(Placement::BelowCenter),
        )
    })
}

fn groups(item: &TrayItem, list: &[TrayMenuItem]) -> Vec<Vec<MenuItem>> {
    let mut groups = vec![Vec::new()];
    for m in list.iter().filter(|m| m.visible) {
        if m.separator {
            groups.push(Vec::new());
            continue;
        }
        let mut row = menu_item(m.label.clone());
        if !m.children.is_empty() {
            row = row.submenu(groups_of(item, &m.children));
        } else {
            if let Some(MenuToggle::Check(on) | MenuToggle::Radio(on)) = m.toggle {
                row = row.selected_when(move || on);
            }
            let (item, id) = (item.clone(), m.id);
            row = row.on_click(move || {
                let _ = item.menu_clicked(id);
            });
        }
        if !m.enabled {
            row = row.disabled_when(|| true);
        }
        if let Some(keys) = &m.shortcut {
            row = row.trailing(keys.clone());
        }
        groups.last_mut().expect("never empty").push(row);
    }
    groups.retain(|g| !g.is_empty());
    groups
}

fn groups_of(item: &TrayItem, list: &[TrayMenuItem]) -> Vec<Vec<MenuItem>> {
    groups(item, list)
}
