//! A frame meter, for finding out where the shell drops frames. It is off unless asked for, because
//! measuring means asking for a frame all the time (which keeps the shell drawing at full speed).
//!
//! Driven by the command file (see `colour.rs`): `meter start`, `meter mark <text>`, `meter stop`.
//! `scripts/fps-test.sh` does that while it changes palettes and switches the bar. On `stop` the
//! frames are written to `~/.local/state/shoji-bar-4/fps.csv` (`t_ms,dt_ms,mark`: the time since
//! `start`, the time since the frame before and the marker set since then) and a summary to
//! `fps.txt`. A frame is dropped when it takes more than 1.5 refresh intervals (the interval is
//! taken from `meter start <hz>`, 165 by default).

use anti_webui::prelude::*;
use std::cell::RefCell;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Default)]
struct Run {
    on: bool,
    started: Option<Instant>,
    last: Option<Instant>,
    hz: f64,
    pending_mark: String,
    /// (t_ms, dt_ms, cpu_ms, wait_ms, mark): `cpu_ms` is what the UI thread was on a CPU for
    /// since the frame before, `wait_ms` how long it was ready to run and not given one.
    frames: Vec<(f64, f64, f64, f64, String)>,
    last_sched: Option<(u64, u64)>,
}

thread_local! {
    static RUN: RefCell<Run> = RefCell::new(Run::default());
}

fn dir() -> Option<PathBuf> {
    Some(super::colour::state_file()?.with_file_name(""))
}

/// `meter <what> [rest]` from the command file.
pub fn command(what: &str, rest: &str) {
    match what {
        "start" => start(rest.trim().parse().unwrap_or(165.0)),
        "mark" => RUN.with(|r| {
            let mut r = r.borrow_mut();
            if r.on {
                r.pending_mark = rest.trim().to_owned();
            }
        }),
        "stop" => stop(),
        _ => {}
    }
}

fn start(hz: f64) {
    let already = RUN.with(|r| {
        let mut r = r.borrow_mut();
        let was = r.on;
        if !was {
            *r = Run { on: true, hz, ..Run::default() };
        }
        was
    });
    if !already {
        request_frame(tick);
    }
}

/// (on-CPU ns, run-queue wait ns) of this thread, from the kernel's scheduler statistics.
fn schedstat() -> Option<(u64, u64)> {
    let text = fs::read_to_string("/proc/thread-self/schedstat").ok()?;
    let mut w = text.split_whitespace();
    Some((w.next()?.parse().ok()?, w.next()?.parse().ok()?))
}

fn tick(now: Instant) {
    let go = RUN.with(|r| {
        let mut r = r.borrow_mut();
        if !r.on {
            return false;
        }
        let start = *r.started.get_or_insert(now);
        if let Some(last) = r.last {
            let dt = now.duration_since(last).as_secs_f64() * 1000.0;
            let t = now.duration_since(start).as_secs_f64() * 1000.0;
            let mark = std::mem::take(&mut r.pending_mark);
            let now_sched = schedstat();
            let (cpu, wait) = match (r.last_sched, now_sched) {
                (Some(a), Some(b)) => ((b.0 - a.0) as f64 / 1e6, (b.1 - a.1) as f64 / 1e6),
                _ => (0.0, 0.0),
            };
            r.last_sched = now_sched;
            r.frames.push((t, dt, cpu, wait, mark));
        } else {
            r.last_sched = schedstat();
        }
        r.last = Some(now);
        true
    });
    if go {
        request_frame(tick);
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

fn stop() {
    let run = RUN.with(|r| std::mem::take(&mut *r.borrow_mut()));
    if !run.on {
        return;
    }
    let Some(dir) = dir() else { return };
    let _ = fs::create_dir_all(&dir);
    let mut csv = String::from("t_ms,dt_ms,cpu_ms,wait_ms,mark\n");
    for (t, dt, cpu, wait, m) in &run.frames {
        let _ = writeln!(csv, "{t:.2},{dt:.3},{cpu:.3},{wait:.3},{m}");
    }
    let _ = fs::write(dir.join("fps.csv"), csv);

    let interval = 1000.0 / run.hz;
    let mut dts: Vec<f64> = run.frames.iter().map(|f| f.1).collect();
    dts.sort_by(|a, b| a.total_cmp(b));
    let total: f64 = dts.iter().sum();
    let dropped = dts.iter().filter(|d| **d > interval * 1.5).count();
    let mut text = String::new();
    let _ = writeln!(text, "refresh {} Hz, interval {interval:.2} ms", run.hz);
    let _ = writeln!(
        text,
        "frames {}, average {:.1} fps, dt p50 {:.2} p95 {:.2} p99 {:.2} worst {:.2} ms, dropped {} ({:.1}%)",
        dts.len(),
        if total > 0.0 { dts.len() as f64 * 1000.0 / total } else { 0.0 },
        percentile(&dts, 0.5),
        percentile(&dts, 0.95),
        percentile(&dts, 0.99),
        dts.last().copied().unwrap_or(0.0),
        dropped,
        100.0 * dropped as f64 / dts.len().max(1) as f64,
    );
    // By stretch: from each mark to the next.
    let mut stretches: Vec<(String, Vec<f64>)> = Vec::new();
    for (_, dt, _, _, mark) in &run.frames {
        if !mark.is_empty() || stretches.is_empty() {
            stretches.push((mark.clone(), Vec::new()));
        }
        stretches.last_mut().unwrap().1.push(*dt);
    }
    for (mark, mut v) in stretches {
        v.sort_by(|a, b| a.total_cmp(b));
        let drops = v.iter().filter(|d| **d > interval * 1.5).count();
        let _ = writeln!(
            text,
            "  {:<28} frames {:>5}  p95 {:>6.2}  worst {:>7.2} ms  dropped {:>4}",
            if mark.is_empty() { "(start)" } else { &mark },
            v.len(),
            percentile(&v, 0.95),
            v.last().copied().unwrap_or(0.0),
            drops,
        );
    }
    let _ = fs::write(dir.join("fps.txt"), text);
}
