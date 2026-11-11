//! The agents panel: what Omarchy's quickshell shows for AI coding subscriptions, in shoji.
//!
//! Omarchy's `omarchy-agent-usage-update` writes one JSON record per agent to
//! `~/.local/state/omarchy/agents/usage/`; this only reads those and draws them: the plan, and one
//! meter per limit window with the time until it resets. Opened from the tray's grid (or with
//! `shoji-bar-4 msg toggle agents`).

use anti_webui::prelude::*;
use anti_webui_md3e::icons::{self, Symbol};
use serde_json::Value;
use shoji_core::theme::{TextStyle, colors, elevation};
use shoji_core::ui::{pressable, sym, txt};
use shoji_default::panels::{PanelDef, close, from_above};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

pub const PANEL: &str = "agents";
const WIDTH: f32 = 440.0;
const METER: f32 = 200.0;

pub fn panel() -> PanelDef {
    PanelDef::new(PANEL, |_output| content())
        .place(|root| root.items_end().justify_start().pt(46.0_f32).pr(14.0_f32))
        .enter(from_above)
}

/// What one agent's record says that is worth a section.
struct Agent {
    name: String,
    plan: String,
    status: String,
    limits: Vec<Limit>,
    today_tokens: u64,
    today_prompts: u64,
}

struct Limit {
    label: String,
    /// 0.0 to 1.0.
    used: f32,
    /// Seconds since the epoch.
    resets_at: Option<i64>,
}

fn read_agents() -> Vec<Agent> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")));
    let Some(dir) = state.map(|s| s.join("omarchy/agents/usage")) else {
        return Vec::new();
    };
    let mut agents: Vec<Agent> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .filter_map(|text| serde_json::from_str::<Value>(&text).ok())
        .map(|v| {
            let text = |k: &str| v[k].as_str().unwrap_or("").to_owned();
            Agent {
                name: text("name"),
                plan: text("tierLabel"),
                status: text("usageStatusText"),
                limits: v["limits"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|l| Limit {
                        label: l["label"].as_str().unwrap_or("").to_owned(),
                        used: l["percent"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0) as f32,
                        resets_at: l["resetsAt"].as_str().and_then(parse_time),
                    })
                    .collect(),
                today_tokens: v["todayTotalTokens"].as_u64().unwrap_or(0),
                today_prompts: v["todayPrompts"].as_u64().unwrap_or(0),
            }
        })
        .collect();
    agents.sort_by(|a, b| a.name.cmp(&b.name));
    agents
}

/// `2026-11-11T01:30:00.068394+00:00` as seconds since the epoch.
fn parse_time(s: &str) -> Option<i64> {
    let n = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, m, d, h, mi, sec) = (n(0, 4)?, n(5, 7)?, n(8, 10)?, n(11, 13)?, n(14, 16)?, n(17, 19)?);
    // Days since 1970-01-01 (civil-from-days, inverted).
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let mut t = days * 86400 + h * 3600 + mi * 60 + sec;
    // The zone, `+HH:MM` or `-HH:MM`, at the end.
    if let Some(z) = s.get(s.len().saturating_sub(6)..).filter(|z| z.as_bytes().get(3) == Some(&b':')) {
        let sign = if z.starts_with('-') { -1 } else { 1 };
        if let (Ok(zh), Ok(zm)) = (z[1..3].parse::<i64>(), z[4..6].parse::<i64>()) {
            t -= sign * (zh * 3600 + zm * 60);
        }
    }
    Some(t)
}

/// `1h 26m`, `6d 2h`, `12m`.
fn until(at: Option<i64>) -> String {
    let Some(at) = at else { return String::new() };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let s = (at - now).max(0);
    let (d, h, m) = (s / 86400, s % 86400 / 3600, s % 3600 / 60);
    match (d, h) {
        (0, 0) => format!("{m}m"),
        (0, _) => format!("{h}h {m}m"),
        _ => format!("{d}d {h}h"),
    }
}

/// `1.2M`, `340k`.
fn short(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{}k", n / 1_000),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}

fn spawn(program: &str, args: &[&str]) {
    let _ = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn content() -> Div {
    // Read as the panel opens, so it is as fresh as the records are.
    let agents: Vec<Agent> = read_agents()
        .into_iter()
        .filter(|a| !a.limits.is_empty() || !a.plan.is_empty())
        .collect();
    let tokens: u64 = agents.iter().map(|a| a.today_tokens).sum();
    let prompts: u64 = agents.iter().map(|a| a.today_prompts).sum();

    let mut col = div().flex().flex_col().gap(14.0_f32).w(WIDTH).p(20.0_f32);
    col = col.child(hero(format!(
        "{} TOKENS · {} PROMPTS TODAY",
        short(tokens),
        prompts
    )));
    for agent in &agents {
        col = col.child(divider()).child(section(agent));
    }
    col = col.child(divider()).child(starters());

    div()
        .rounded(12.0_f32)
        .shadows(elevation(3))
        .dyn_style(|s| s.bg(colors().surface_container_high).text_color(colors().on_surface))
        .child(col)
}

fn divider() -> Div {
    div().h(1.0_f32).dyn_style(|s| s.bg(colors().outline_variant))
}

fn muted(text: String, style: TextStyle) -> Div {
    txt(style, move || text.clone()).dyn_style(|s| s.text_color(colors().on_surface_variant))
}

fn hero(subtitle: String) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(14.0_f32)
        .child(sym(icons::ROBOT, 36.0, |c| c.on_surface))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .child(txt(TextStyle::TITLE_MEDIUM, || "Agents".to_owned()))
                .child(muted(subtitle, TextStyle::LABEL_SMALL)),
        )
        .child(button(icons::TERMINAL, || {
            close();
            spawn("omarchy-agent", &[]);
        }))
}

fn button(symbol: &'static Symbol, on_click: impl Fn() + 'static) -> Div {
    pressable(
        div()
            .size(40.0_f32)
            .rounded(8.0_f32)
            .flex()
            .items_center()
            .justify_center()
            .dyn_style(|s| s.bg(colors().surface_container_highest))
            .on_click(move |_| on_click())
            .child(sym(symbol, 20.0, |c| c.on_surface)),
        |c| c.on_surface,
    )
}

fn section(agent: &Agent) -> Div {
    let mut col = div().flex().flex_col().gap(8.0_f32).child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .child(txt(TextStyle::TITLE_MEDIUM, {
                        let name = agent.name.clone();
                        move || name.clone()
                    })),
            )
            .child(muted(agent.plan.clone(), TextStyle::LABEL_MEDIUM)),
    );
    if agent.limits.is_empty() && !agent.status.is_empty() {
        col = col.child(muted(agent.status.clone(), TextStyle::BODY_SMALL));
    }
    for limit in &agent.limits {
        col = col.child(limit_row(limit));
    }
    col
}

fn limit_row(limit: &Limit) -> Div {
    // "Session (5-hour)" -> "Session".
    let label = limit.label.split(" (").next().unwrap_or("").to_owned();
    let fill = (METER * limit.used).max(3.0);
    div()
        .flex()
        .flex_row()
        .items_center()
        .child(div().w(90.0_f32).child(muted(label, TextStyle::BODY_MEDIUM)))
        .child(
            div()
                .w(METER)
                .h(6.0_f32)
                .rounded(3.0_f32)
                .flex_none()
                .dyn_style(|s| s.bg(colors().surface_container_highest))
                .child(
                    div()
                        .w(fill)
                        .h(6.0_f32)
                        .rounded(3.0_f32)
                        .dyn_style(|s| s.bg(colors().primary)),
                ),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .justify_end()
                .child(muted(until(limit.resets_at), TextStyle::BODY_MEDIUM)),
        )
}

/// Three tiles that start the default agent on something to make.
fn starters() -> Div {
    let tile = |symbol: &'static Symbol, label: &'static str, prompt: &'static str| {
        pressable(
            div()
                .flex_1()
                .h(48.0_f32)
                .rounded(8.0_f32)
                .flex()
                .flex_row()
                .items_center()
                .justify_center()
                .gap(8.0_f32)
                .dyn_style(|s| s.bg(colors().surface_container_highest))
                .on_click(move |_| {
                    close();
                    spawn("omarchy-agent-prompt", &[prompt]);
                })
                .child(sym(symbol, 18.0, |c| c.primary))
                .child(txt(TextStyle::BODY_MEDIUM, move || label.to_owned())),
            |c| c.on_surface,
        )
    };
    div()
        .flex()
        .flex_col()
        .gap(10.0_f32)
        .child(muted("MAKE SOMETHING COOL".to_owned(), TextStyle::LABEL_SMALL))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(10.0_f32)
                .child(tile(
                    icons::PALETTE,
                    "Theme",
                    "Make me a new Omarchy theme. Ask me what look or inspiration I have in mind, then build it following the Omarchy skill's theming guide and switch to it.",
                ))
                .child(tile(
                    icons::EXTENSION,
                    "Plugin",
                    "Make me a new Omarchy shell plugin. Ask me what I'd like it to do, then build it following the Omarchy skill's plugin guide and enable it.",
                ))
                .child(tile(
                    icons::WEB_ASSET,
                    "App",
                    "Make me a new app for my Omarchy desktop. Ask me what it should do, then build it following the omarchy-app skill and install it so it shows up in the app launcher.",
                )),
        )
}
