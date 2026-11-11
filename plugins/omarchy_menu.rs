//! Omarchy's menu (`Super+Space` and its routes: system, capture, toggle, hardware, theme...), drawn
//! by this shell. Omarchy opens it with `omarchy-menu toggle <route>`, which is
//! `omarchy-shell shell toggle omarchy.menu '{"menu":"<route>"}'`; `shoji_default::omarchy` gets that
//! call and hands it to the hook this file registers.
//!
//! What is in the menu is Omarchy's: `default/omarchy/omarchy-menu.jsonc` with the user's
//! `~/.config/omarchy/extensions/omarchy-menu.jsonc` on top (see the comments in the first for the
//! fields). A row is an action (a shell command), a submenu (dotted ids make the tree) or a link; `when`,
//! `checked` and `disabled` are shell conditions; `provider` fills a submenu from a command (fonts,
//! kernels, languages, power profiles).
//!
//! The same panel is Omarchy's `select` and `input` prompts (`omarchy-menu-select`,
//! `omarchy-menu-input`): they send `{"mode":"select",...,"selectionFile":..,"doneFile":..}` and wait
//! for the answer to be written to those files.
//!
//! Keys: type to search, arrows to move, Enter to choose, Escape or Backspace on an empty search to
//! go back and then to close.

use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui_md3e::icons;
use shoji_core::theme::{TextStyle, colors, elevation};
use shoji_core::ui::{icon_btn, pressable, static_txt, sym, txt};
use shoji_default::Ui;
use shoji_default::panels::{PanelDef, close, is_open, open, rise_in};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;

const PANEL: &str = "omarchy-menu";
const WIDTH: f32 = 340.0;
const ROW_H: f32 = 40.0;
/// Rows visible before the list scrolls.
const VISIBLE: f32 = 10.0;

// ----------------------------------------------------------------------------------- the items

#[derive(Clone, Debug, Default)]
struct Item {
    id: String,
    parent: String,
    icon: String,
    label: String,
    title: String,
    target: String,
    description: String,
    action: String,
    provider: String,
    aliases: Vec<String>,
    when: String,
    checked: String,
    disabled: String,
}

impl Item {
    fn is_action(&self) -> bool {
        !self.action.is_empty()
    }
    fn is_link(&self) -> bool {
        self.action.is_empty() && !self.target.is_empty()
    }
    fn is_menu(&self) -> bool {
        !self.is_action()
    }
}

/// Omarchy's JSONC: whole-line `//` comments and trailing commas, which is all it uses.
fn strip_jsonc(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().filter(|l| !l.trim_start().starts_with("//")).collect();
    let text = lines.join("\n");
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let (mut in_string, mut escaped) = (false, false);
    for (i, &c) in chars.iter().enumerate() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            ',' => {
                let next = chars[i + 1..].iter().find(|c| !c.is_whitespace());
                if !matches!(next, Some('}') | Some(']')) {
                    out.push(c);
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// The keys of the top-level object, in the order they are written (the parser sorts them).
fn top_level_keys(text: &str) -> Vec<String> {
    let (mut depth, mut in_string, mut escaped) = (0i32, false, false);
    let mut current = String::new();
    let mut keys = Vec::new();
    let mut expecting_key = false;
    let mut key_depth1 = false;
    for c in text.chars() {
        if in_string {
            if escaped {
                escaped = false;
                current.push(c);
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
                if key_depth1 {
                    keys.push(std::mem::take(&mut current));
                    key_depth1 = false;
                }
            } else {
                current.push(c);
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                current.clear();
                key_depth1 = depth == 1 && expecting_key;
                expecting_key = false;
            }
            '{' | '[' => {
                depth += 1;
                if depth == 1 {
                    expecting_key = true;
                }
            }
            '}' | ']' => depth -= 1,
            ',' if depth == 1 => expecting_key = true,
            _ => {}
        }
    }
    keys
}

fn read_items(path: &str) -> Vec<Item> {
    let Ok(raw) = fs::read_to_string(path) else { return Vec::new() };
    let text = strip_jsonc(&raw);
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let map = match map.get("items") {
        Some(serde_json::Value::Object(inner)) => inner.clone(),
        _ => map,
    };
    let order = top_level_keys(&text);
    let mut out = Vec::new();
    for id in order {
        let Some(serde_json::Value::Object(v)) = map.get(&id) else { continue };
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_owned();
        let aliases = match v.get("aliases") {
            Some(serde_json::Value::Array(a)) => {
                a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()
            }
            Some(serde_json::Value::String(a)) => vec![a.clone()],
            _ => Vec::new(),
        };
        let parent = match v.get("parent").and_then(|x| x.as_str()) {
            Some(p) => p.to_owned(),
            None if id == "root" => String::new(),
            None => id.rsplit_once('.').map_or("root".to_owned(), |(p, _)| p.to_owned()),
        };
        out.push(Item {
            parent,
            icon: s("icon"),
            label: if s("label").is_empty() { id.clone() } else { s("label") },
            title: s("title"),
            target: s("target"),
            description: s("description"),
            action: s("action"),
            provider: s("provider"),
            aliases,
            when: s("when"),
            checked: s("checked"),
            disabled: s("disabled"),
            id,
        });
    }
    out
}

/// The default menu with the user's on top: a row with a known id keeps the fields the user's does
/// not set.
fn load() -> Vec<Item> {
    let omarchy = std::env::var("OMARCHY_PATH").unwrap_or_else(|_| "/usr/share/omarchy".into());
    let home = std::env::var("HOME").unwrap_or_default();
    let mut items = read_items(&format!("{omarchy}/default/omarchy/omarchy-menu.jsonc"));
    for user in read_items(&format!("{home}/.config/omarchy/extensions/omarchy-menu.jsonc")) {
        match items.iter_mut().find(|i| i.id == user.id) {
            Some(mine) => {
                // Only what the user's file says: the parser fills the rest with defaults.
                *mine = Item {
                    label: if user.label == user.id { mine.label.clone() } else { user.label },
                    icon: if user.icon.is_empty() { mine.icon.clone() } else { user.icon },
                    title: pick(user.title, &mine.title),
                    target: pick(user.target, &mine.target),
                    description: pick(user.description, &mine.description),
                    action: pick(user.action, &mine.action),
                    provider: pick(user.provider, &mine.provider),
                    when: pick(user.when, &mine.when),
                    checked: pick(user.checked, &mine.checked),
                    disabled: pick(user.disabled, &mine.disabled),
                    aliases: if user.aliases.is_empty() { mine.aliases.clone() } else { user.aliases },
                    id: mine.id.clone(),
                    parent: mine.parent.clone(),
                };
            }
            None => items.push(user),
        }
    }
    items
}

fn pick(new: String, old: &str) -> String {
    if new.is_empty() { old.to_owned() } else { new }
}

/// The id a route names: an id, an alias of one, or the route itself.
fn resolve(items: &[Item], route: &str) -> String {
    let raw = route.to_lowercase().replace('_', "-");
    if raw.is_empty() || raw == "go" || raw == "menu" {
        return "root".into();
    }
    if items.iter().any(|i| i.id == raw) {
        return raw;
    }
    items
        .iter()
        .find(|i| i.aliases.iter().any(|a| a.to_lowercase().replace('_', "-") == raw))
        .map_or(raw, |i| i.id.clone())
}

/// What a provider's submenu is filled with: `label<TAB>value<TAB>current`, one per line.
fn provider_script(name: &str) -> Option<(&'static str, &'static str, &'static str)> {
    // (script, icon, command that sets a value, with `{}` for it)
    Some(match name {
        "fonts" => (
            "current=$(omarchy-font-current 2>/dev/null); omarchy-font-list 2>/dev/null | while read -r f; do [[ -z $f ]] && continue; printf '%s\\t%s\\t%s\\n' \"$f\" \"$f\" \"$current\"; done",
            "\u{f031}",
            "omarchy-font-set",
        ),
        "power-profiles" => (
            "current=$(powerprofilesctl get 2>/dev/null); omarchy-powerprofiles-list 2>/dev/null | while read -r p; do [[ -z $p ]] && continue; printf '%s\\t%s\\t%s\\n' \"$p\" \"$p\" \"$current\"; done",
            "\u{f0b0b}",
            "omarchy-powerprofiles-set autodetect",
        ),
        "kernels" => (
            "current=$(omarchy-kernel get 2>/dev/null); omarchy-kernel list 2>/dev/null | while read -r k; do [[ -z $k ]] && continue; printf '%s\\t%s\\t%s\\n' \"$k\" \"$k\" \"$current\"; done",
            "\u{f17c}",
            "omarchy-kernel set",
        ),
        "languages" => (
            "current=$(omarchy-language 2>/dev/null); omarchy-language list 2>/dev/null | while IFS=$'\\t' read -r name code english; do [[ -z $code ]] && continue; printf '%s\\t%s\\t%s\\t%s\\n' \"$name\" \"$code\" \"$current\" \"$english\"; done",
            "\u{f1ab}",
            "omarchy-language",
        ),
        _ => return None,
    })
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn provider_rows(menu: &str, provider: &str) -> Vec<Item> {
    let Some((script, icon, set)) = provider_script(provider) else { return Vec::new() };
    let Ok(out) = Command::new("bash").args(["-lc", script]).stderr(Stdio::null()).output() else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for (i, line) in String::from_utf8_lossy(&out.stdout).lines().enumerate() {
        let parts: Vec<&str> = line.split('\t').collect();
        let label = parts.first().copied().unwrap_or("");
        if label.is_empty() {
            continue;
        }
        let value = parts.get(1).copied().unwrap_or(label);
        let current = parts.get(2).copied().unwrap_or("");
        rows.push(Item {
            id: format!("{menu}.{i}"),
            parent: menu.to_owned(),
            icon: if value == current { "✓".into() } else { icon.into() },
            label: label.to_owned(),
            action: format!("{set} {}", quote(value)),
            aliases: parts.get(3).map(|a| vec![(*a).to_owned()]).unwrap_or_default(),
            ..Item::default()
        });
    }
    rows
}

/// Runs the shell conditions of the menu, all at once, and says which hold:
/// `w:<id>` for `when`, `c:` for `checked`, `d:` for `disabled`.
fn evaluate(items: &[Item]) -> HashSet<String> {
    let mut script = String::new();
    for i in items {
        for (tag, cond) in [("w", &i.when), ("c", &i.checked), ("d", &i.disabled)] {
            if !cond.is_empty() {
                script.push_str(&format!(
                    "( {cond} ) >/dev/null 2>&1 && echo {} &\n",
                    quote(&format!("{tag}:{}", i.id))
                ));
            }
        }
    }
    script.push_str("wait\n");
    Command::new("bash")
        .args(["-lc", &script])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

fn run_action(command: &str) {
    let command = command.to_owned();
    let _ = Command::new("setsid")
        .args(["-f", "bash", "-lc", &command])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|mut c| std::thread::spawn(move || c.wait()));
}

// ----------------------------------------------------------------------------- the requests

/// What the panel is asked to show.
#[derive(Clone)]
enum Request {
    Route(String),
    Select(Dmenu),
    Input(Dmenu),
}

#[derive(Clone, Default)]
struct Dmenu {
    prompt: String,
    /// `<icon>\t<label>[\t<subtext>]`, or just the label.
    options: Vec<String>,
    multiple: bool,
    selected: Vec<String>,
    selection_file: String,
    done_file: String,
}

thread_local! {
    /// When a key last moved the choice: a pointer resting over the list must not take it back.
    static KEYED: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
    /// What Escape does before it closes the panel: back out of a submenu.
    static ESCAPE: RefCell<Option<Rc<dyn Fn() -> bool>>> = const { RefCell::new(None) };
    static REQUEST: RefCell<Option<Request>> = const { RefCell::new(None) };
    static ITEMS: RefCell<Vec<Item>> = const { RefCell::new(Vec::new()) };
}

/// Where a dmenu's answer goes: the selection (if there is one), then the file that says it is
/// done, which is what the caller waits for.
fn answer(menu: &Dmenu, selection: Option<&str>) {
    if menu.done_file.is_empty() {
        return;
    }
    if let Some(text) = selection {
        let _ = fs::write(PathBuf::from(&menu.selection_file), format!("{text}\n"));
    }
    let _ = fs::write(PathBuf::from(&menu.done_file), "");
}

fn parse_request(payload: &str) -> Request {
    let json: serde_json::Value = serde_json::from_str(payload).unwrap_or_default();
    let s = |k: &str| json.get(k).and_then(|v| v.as_str()).unwrap_or("").to_owned();
    let list = |k: &str| -> Vec<String> {
        json.get(k)
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
            .unwrap_or_default()
    };
    let dmenu = || Dmenu {
        prompt: s("prompt"),
        options: list("options"),
        multiple: json.get("multiple").and_then(|v| v.as_bool()).unwrap_or(false),
        selected: list("selected"),
        selection_file: s("selectionFile"),
        done_file: s("doneFile"),
    };
    match json.get("mode").and_then(|v| v.as_str()) {
        Some("select") => Request::Select(dmenu()),
        Some("input") => Request::Input(dmenu()),
        _ => Request::Route(
            [s("initialMenu"), s("menu")].into_iter().find(|r| !r.is_empty()).unwrap_or("root".into()),
        ),
    }
}

/// Opens the panel for `request`, in place of a panel that is open.
fn present(request: Request) {
    let Some(output) = shoji_default::ipc::focused_output() else { return };
    ITEMS.with(|i| *i.borrow_mut() = load());
    // A route that names an action just runs it.
    if let Request::Route(route) = &request {
        let found = ITEMS.with(|i| {
            let i = i.borrow();
            let id = resolve(&i, route);
            i.iter().find(|x| x.id == id).cloned()
        });
        if let Some(item) = found.filter(Item::is_action) {
            close();
            run_action(&item.action);
            return;
        }
    }
    REQUEST.with(|r| *r.borrow_mut() = Some(request));
    if is_open(&Output { name: output.clone(), ..Default::default() }, PANEL) {
        close();
        request_frame(move |_| open(&output, PANEL));
    } else {
        open(&output, PANEL);
    }
}

/// The hook for `omarchy-shell shell toggle|summon|hide|call omarchy.menu <payload>`.
fn shell_call(verb: &str, id: &str, payload: &str) -> bool {
    if id != "omarchy.menu" {
        return false;
    }
    match verb {
        "toggle" => {
            let output = shoji_default::ipc::focused_output().unwrap_or_default();
            let open_now = shoji_default::panels::current().is_some_and(|(o, p)| o == output && p.0 == PANEL);
            if open_now {
                close();
            } else {
                present(parse_request(payload));
            }
        }
        "summon" | "show" | "open" => present(parse_request(payload)),
        "hide" | "close" => close(),
        // `refresh` and `ping`: the files are read at each opening.
        _ => {}
    }
    true
}

pub fn install(_ui: &mut Ui) {
    shoji_default::omarchy::on_shell_call(shell_call);
    shoji_core::panels::register(
        PanelDef::new(PANEL, content)
            .with_field()
            .veiled()
            .on_escape(|| ESCAPE.with(|e| e.borrow().clone()).is_some_and(|f| f()))
            .enter(rise_in),
    );
}

// ------------------------------------------------------------------------------------- the panel

/// Notes that a key (or the opening) has just decided what is chosen.
fn keyed() {
    KEYED.with(|k| k.set(Some(std::time::Instant::now())));
}

/// A row as it is drawn.
#[derive(Clone, PartialEq)]
struct Row {
    /// An item id, or the index of a dmenu option.
    key: String,
    icon: String,
    label: String,
    detail: String,
    submenu: bool,
    checked: bool,
    disabled: bool,
}

fn content(_output: Output) -> Div {
    let request = REQUEST.with(|r| r.borrow_mut().take()).unwrap_or(Request::Route("root".into()));
    match request {
        Request::Route(route) => menu_card(route),
        Request::Select(d) => dmenu_card(d, false),
        Request::Input(d) => dmenu_card(d, true),
    }
}

fn card() -> Div {
    div()
        .w(WIDTH)
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
}

fn search_field(query: Signal<String>, placeholder: &'static str, input: NodeRef) -> TextInput {
    TextStyle::BODY_MEDIUM
        .apply(text_input())
        .value(query)
        .placeholder(placeholder)
        .select_text()
        .node_ref(input)
        .flex_1()
}

/// The list of rows, with the chosen one on a highlight; `choose` takes a row's key.
fn list(
    rows: Memo<Vec<Row>>,
    picked: Signal<usize>,
    choose: Rc<dyn Fn(&Row)>,
) -> Div {
    div()
        .relative()
        .max_h(ROW_H * VISIBLE)
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap(2.0_f32)
        .child(
            for_each(
                move || rows.get(),
                |r| r.key.clone(),
                move |r| {
                    let key = r.key.clone();
                    let chosen = memo(move || {
                        rows.with(|all| all.iter().position(|x| x.key == key)) == Some(picked.get())
                    });
                    row_view(r, chosen, picked, rows, choose.clone())
                },
            ),
        )
        .child(show(
            move || rows.with(Vec::is_empty),
            move || {
                div()
                    .h(ROW_H)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        static_txt(TextStyle::BODY_MEDIUM, "Nothing here")
                            .dyn_style(|s| s.text_color(colors().on_surface_variant)),
                    )
            },
        ))
}

fn row_view(
    r: Row,
    chosen: Memo<bool>,
    picked: Signal<usize>,
    rows: Memo<Vec<Row>>,
    choose: Rc<dyn Fn(&Row)>,
) -> Div {
    let key = r.key.clone();
    let for_click = r.clone();
    let dim = r.disabled;
    pressable(
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(12.0_f32)
            .min_h(ROW_H)
            .px(10.0_f32)
            .rounded(20.0_f32)
            .on_hover(move |h| {
                let keyed = KEYED.with(|k| k.get()).is_some_and(|t| t.elapsed().as_millis() < 700);
                if h && !keyed && let Some(i) = rows.with(|all| all.iter().position(|x| x.key == key)) {
                    picked.set(i);
                }
            })
            .on_click(move |_| {
                if !dim {
                    choose(&for_click)
                }
            })
            .dyn_style(move |s| {
                let c = colors();
                let s = s.bg(if chosen.get() { c.secondary_container } else { c.secondary_container.with_alpha(0.0) });
                s.text_color(if chosen.get() { c.on_secondary_container } else { c.on_surface })
                    .opacity(if dim { 0.45 } else { 1.0 })
            })
            .child(
                div()
                    .w(26.0)
                    .flex_none()
                    .flex()
                    .justify_center()
                    .child(static_txt(TextStyle::TITLE_MEDIUM, r.icon.clone())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(0.0)
                    .flex()
                    .flex_col()
                    .child(static_txt(TextStyle::BODY_LARGE, r.label.clone()).overflow_hidden())
                    .child(show(
                        {
                            let has = !r.detail.is_empty();
                            move || has
                        },
                        {
                            let detail = r.detail.clone();
                            move || {
                                static_txt(TextStyle::LABEL_SMALL, detail.clone())
                                    .overflow_hidden()
                                    .dyn_style(|s| s.text_color(colors().on_surface_variant))
                            }
                        },
                    )),
            )
            .child(show(
                {
                    let (checked, submenu) = (r.checked, r.submenu);
                    move || checked || submenu
                },
                {
                    let (checked, _submenu) = (r.checked, r.submenu);
                    move || {
                        if checked {
                            sym(icons::CHECK, 18.0, |c| c.primary)
                        } else {
                            sym(icons::CHEVRON_RIGHT, 18.0, |c| c.on_surface_variant)
                        }
                    }
                },
            )),
        |c| c.on_secondary_container,
    )
}

/// The menu of Omarchy, from the route it was asked for.
fn menu_card(route: String) -> Div {
    keyed();
    let items: Rc<Vec<Item>> = Rc::new(ITEMS.with(|i| i.borrow().clone()));
    let start = {
        let id = resolve(&items, &route);
        match items.iter().find(|i| i.id == id) {
            Some(i) if i.is_link() => i.target.clone(),
            _ => id,
        }
    };
    let stack = signal(vec![start]);
    let query = signal(String::new());
    let picked = signal(0usize);
    // Which shell conditions hold, once they have been run (the rows that wait on them are not
    // shown until then), and what the providers gave.
    let holds = signal(HashSet::<String>::new());
    let ready = signal(false);
    let provided = signal(HashMap::<String, Vec<Item>>::new());
    {
        let all: Vec<Item> = items.as_ref().clone();
        task(move || evaluate(&all)).then(move |set| {
            holds.set(set);
            ready.set(true);
        });
    }
    let current = memo(move || stack.with(|s| s.last().cloned().unwrap_or_else(|| "root".into())));
    // A provider's rows are asked for each time its submenu is entered.
    {
        let items = items.clone();
        effect(move || {
            let id = current.get();
            let Some(item) = items.iter().find(|i| i.id == id && !i.provider.is_empty()).cloned() else { return };
            let id2 = id.clone();
            task(move || provider_rows(&id2, &item.provider))
                .then(move |rows| provided.update(|p| { p.insert(id, rows); }));
        });
    }
    effect(move || {
        query.with(|_| ());
        current.with(|_| ());
        picked.set(0);
    });

    let children_of = {
        let items = items.clone();
        move |menu: &str| -> Vec<Item> {
            let mut v: Vec<Item> = items.iter().filter(|i| i.parent == menu).cloned().collect();
            v.extend(provided.with(|p| p.get(menu).cloned().unwrap_or_default()));
            v
        }
    };
    let visible = {
        let items = items.clone();
        move |i: &Item| -> bool {
            if !i.when.is_empty() && !holds.with(|h| h.contains(&format!("w:{}", i.id))) {
                return false;
            }
            // A menu with nothing to show is not shown, nor is one whose rows are all hidden.
            if i.is_menu() && i.provider.is_empty() && !items.iter().any(|c| c.parent == i.id) {
                return false;
            }
            true
        }
    };
    let rows = memo({
        let items = items.clone();
        let children_of = children_of.clone();
        let visible = visible.clone();
        move || {
            holds.with(|_| ());
            provided.with(|_| ());
            let q = query.get().trim().to_lowercase();
            let make = |i: &Item, detail: String| Row {
                key: i.id.clone(),
                icon: i.icon.clone(),
                label: i.label.clone(),
                detail,
                submenu: i.is_menu() || i.is_link(),
                checked: holds.with(|h| h.contains(&format!("c:{}", i.id)))
                    || holds.with(|h| h.contains(&format!("d:{}", i.id))),
                disabled: holds.with(|h| h.contains(&format!("d:{}", i.id))),
            };
            if q.is_empty() {
                return children_of(&current.get()).iter().filter(|i| visible(i)).map(|i| make(i, i.description.clone())).collect();
            }
            // A search covers the whole menu: what can be done, with where it is.
            let mut found = Vec::new();
            let mut every: Vec<Item> = items.iter().cloned().collect();
            every.extend(provided.with(|p| p.values().flatten().cloned().collect::<Vec<_>>()));
            for i in every.iter().filter(|i| i.is_action() && visible(i) && i.id != "apps") {
                let hay = format!("{} {} {}", i.label, i.aliases.join(" "), i.description).to_lowercase();
                if hay.contains(&q) {
                    found.push((i.label.to_lowercase().starts_with(&q), make(i, path_of(&items, i))));
                }
            }
            found.sort_by_key(|(starts, _)| !*starts);
            found.into_iter().map(|(_, r)| r).collect()
        }
    });
    let _ = ready;

    let title = memo({
        let items = items.clone();
        move || {
            items.iter().find(|i| i.id == current.get()).map_or("Go".to_owned(), |i| {
                if i.title.is_empty() { i.label.clone() } else { i.title.clone() }
            })
        }
    });
    let depth = memo(move || stack.with(Vec::len));
    let back = move || {
        if stack.with(Vec::len) > 1 {
            keyed();
            stack.update(|s| { s.pop(); });
            query.set(String::new());
        } else {
            close();
        }
    };
    ESCAPE.with(|e| {
        *e.borrow_mut() = Some(Rc::new(move || {
            if stack.with_untracked(Vec::len) > 1 {
                keyed();
                stack.update(|s| { s.pop(); });
                query.set(String::new());
                true
            } else {
                false
            }
        }));
    });
    on_cleanup(|| ESCAPE.with(|e| *e.borrow_mut() = None));
    let choose = {
        let items = items.clone();
        let children_of = children_of.clone();
        move |row: &Row| {
            keyed();
            let find = || {
                items
                    .iter()
                    .find(|i| i.id == row.key)
                    .cloned()
                    .or_else(|| provided.with(|p| p.values().flatten().find(|i| i.id == row.key).cloned()))
            };
            let Some(item) = find() else { return };
            if item.is_action() {
                close();
                run_action(&item.action);
            } else {
                let next = if item.is_link() { item.target.clone() } else { item.id.clone() };
                let _ = &children_of;
                stack.update(|s| s.push(next));
                query.set(String::new());
            }
        }
    };
    let choose = Rc::new(choose);
    let input = node_ref();
    request_frame(move |_| input.focus());
    // Moving between menus must not lose the keyboard.
    effect(move || {
        current.with(|_| ());
        request_frame(move |_| input.focus());
    });
    let pick = {
        let choose = choose.clone();
        move || {
            if let Some(row) = rows.with(|r: &Vec<Row>| r.get(untrack(|| picked.get())).cloned()) {
                if !row.disabled {
                    choose(&row);
                }
            }
        }
    };
    let step = move |by: isize| {
        keyed();
        let n = rows.with(Vec::len) as isize;
        if n > 0 {
            picked.update(|p| *p = (*p as isize + by).rem_euclid(n) as usize);
        }
    };
    let choose_for_list: Rc<dyn Fn(&Row)> = choose.clone();
    card()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(8.0_f32)
                .px(4.0_f32)
                .child(show(
                    move || depth.get() > 1,
                    move || icon_btn(icons::ARROW_BACK, 32.0, |c| c.on_surface_variant).on_click(move |_| back()),
                ))
                .child(txt(TextStyle::TITLE_MEDIUM, move || title.get()).flex_1()),
        )
        .child(
            div()
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
                    search_field(query, "Search", input)
                        .on_submit(move |_| pick())
                        .on_key_down(move |e| match e.key {
                            Key::Named(NamedKey::Backspace) if query.with_untracked(String::is_empty) => back(),
                            Key::Named(NamedKey::ArrowDown) => step(1),
                            Key::Named(NamedKey::ArrowUp) => step(-1),
                            Key::Named(NamedKey::PageDown) => step(6),
                            Key::Named(NamedKey::PageUp) => step(-6),
                            _ => {}
                        }),
                ),
        )
        .child(list(rows, picked, choose_for_list))
}

/// Where an item is in the menu, for the result of a search: `System › Lock`.
fn path_of(items: &[Item], item: &Item) -> String {
    let mut parts = Vec::new();
    let mut parent = item.parent.clone();
    while parent != "root" && !parent.is_empty() {
        match items.iter().find(|i| i.id == parent) {
            Some(p) => {
                parts.push(p.label.clone());
                parent = p.parent.clone();
            }
            None => break,
        }
    }
    parts.reverse();
    parts.join(" › ")
}

/// `omarchy-menu-select` and `omarchy-menu-input`.
fn dmenu_card(menu: Dmenu, input_mode: bool) -> Div {
    keyed();
    let menu = Rc::new(menu);
    let answered = Rc::new(std::cell::Cell::new(false));
    // Closed without an answer (Escape, a click outside): the caller must still be let go.
    {
        let (menu, answered) = (menu.clone(), answered.clone());
        on_cleanup(move || {
            if !answered.get() {
                answer(&menu, None);
            }
        });
    }
    let query = signal(String::new());
    let picked = signal(0usize);
    let ticked = signal(menu.selected.clone());
    // (icon, label, subtext, returned value)
    let options: Rc<Vec<(String, String, String, String)>> = Rc::new(
        menu.options
            .iter()
            .map(|o| {
                let p: Vec<&str> = o.split('\t').collect();
                match p.as_slice() {
                    [label] => (String::new(), (*label).to_owned(), String::new(), (*label).to_owned()),
                    [icon, label] => ((*icon).to_owned(), (*label).to_owned(), String::new(), (*label).to_owned()),
                    [icon, label, sub, ..] => {
                        ((*icon).to_owned(), (*label).to_owned(), (*sub).to_owned(), format!("{label}\t{sub}"))
                    }
                    [] => Default::default(),
                }
            })
            .collect(),
    );
    let rows = memo({
        let options = options.clone();
        let multiple = menu.multiple;
        move || {
            let q = query.get().to_lowercase();
            let mut out: Vec<Row> = Vec::new();
            if multiple {
                out.push(Row {
                    key: "apply".into(),
                    icon: "".into(),
                    label: format!("Apply ({})", ticked.with(Vec::len)),
                    detail: String::new(),
                    submenu: false,
                    checked: false,
                    disabled: false,
                });
            }
            for (i, (icon, label, sub, value)) in options.iter().enumerate() {
                if !q.is_empty() && !format!("{label} {sub}").to_lowercase().contains(&q) {
                    continue;
                }
                let on = ticked.with(|t| t.contains(value));
                out.push(Row {
                    key: i.to_string(),
                    icon: if multiple { if on { "✓".into() } else { "○".into() } } else { icon.clone() },
                    label: label.clone(),
                    detail: sub.clone(),
                    submenu: false,
                    checked: false,
                    disabled: false,
                });
            }
            out
        }
    });
    effect(move || {
        query.with(|_| ());
        picked.set(0);
    });
    let finish = {
        let (menu, answered) = (menu.clone(), answered.clone());
        move |text: &str| {
            answered.set(true);
            answer(&menu, Some(text));
            close();
        }
    };
    let finish = Rc::new(finish);
    let choose = {
        let (options, finish, menu) = (options.clone(), finish.clone(), menu.clone());
        move |row: &Row| {
            if row.key == "apply" {
                let json = serde_json::to_string(&ticked.get_untracked()).unwrap_or("[]".into());
                finish(&json);
                return;
            }
            let Some((_, _, _, value)) = row.key.parse::<usize>().ok().and_then(|i| options.get(i)) else { return };
            if menu.multiple {
                ticked.update(|t| {
                    if let Some(at) = t.iter().position(|x| x == value) { t.remove(at); } else { t.push(value.clone()); }
                });
            } else {
                finish(value);
            }
        }
    };
    let choose = Rc::new(choose);
    let input = node_ref();
    request_frame(move |_| input.focus());
    let step = move |by: isize| {
        keyed();
        let n = rows.with(Vec::len) as isize;
        if n > 0 {
            picked.update(|p| *p = (*p as isize + by).rem_euclid(n) as usize);
        }
    };
    let prompt = menu.prompt.clone();
    let choose_pick = choose.clone();
    let finish_input = finish.clone();
    let choose_list: Rc<dyn Fn(&Row)> = choose.clone();
    let field = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(10.0_f32)
        .h(42.0)
        .px(14.0_f32)
        .rounded(21.0_f32)
        .dyn_style(|s| s.bg(colors().surface_container_highest))
        .child(sym(if input_mode { icons::EDIT } else { icons::SEARCH }, 18.0, |c| c.on_surface_variant))
        .child(
            search_field(query, if input_mode { "Type and press Enter" } else { "Search" }, input)
                .on_submit(move |text| {
                    if input_mode {
                        finish_input(text);
                    } else if let Some(row) = rows.with(|r: &Vec<Row>| r.get(untrack(|| picked.get())).cloned()) {
                        choose_pick(&row);
                    }
                })
                .on_key_down(move |e| match e.key {
                    Key::Named(NamedKey::Escape) => close(),
                    Key::Named(NamedKey::ArrowDown) => step(1),
                    Key::Named(NamedKey::ArrowUp) => step(-1),
                    _ => {}
                }),
        );
    let head = div().px(4.0_f32).child(static_txt(TextStyle::TITLE_MEDIUM, prompt));
    if input_mode {
        card().child(head).child(field)
    } else {
        card().child(head).child(field).child(list(rows, picked, choose_list))
    }
}
