#!/usr/bin/env python3
"""Statistics of scripts/perf-ab.sh: per palette and scenario step, frame-time percentiles, 1% /
0.1% lows, jitter, dropped and stutter frames, CPU time per frame, plus system columns, with a
Mann-Whitney U test (and the rank-biserial effect size) between the first palette and the others.
usage: perf-report.py logs/ab-<date> [baseline-palette]"""
import csv, glob, os, re, statistics as st, sys
from collections import defaultdict
from scipy.stats import mannwhitneyu

d = sys.argv[1]
base = sys.argv[2] if len(sys.argv) > 2 else "dark"

def pct(v, p):
    v = sorted(v); return v[min(len(v) - 1, round((len(v) - 1) * p))] if v else float("nan")

system = defaultdict(lambda: defaultdict(list))
for f in sorted(glob.glob(f"{d}/run-*.system.csv")):
    m = re.match(r"run-(.+)-(\d+)\.system\.csv", os.path.basename(f)); pal = m[1]
    for r in csv.DictReader(open(f)):
        for k, v in r.items():
            if k != "t": system[pal][k].append(float(v))

# the meter attaches the mark to the first frame after it; propagate the step to later frames
def steps_of(pal):
    out = defaultdict(list); cur = None
    for f in sorted(glob.glob(f"{d}/run-{pal}-*.csv")):
        if f.endswith(".system.csv"): continue
        rep = int(re.search(r"-(\d+)\.csv", f)[1])
        for r in csv.DictReader(open(f)):
            if r["mark"]: cur = r["mark"]
            out[cur].append((rep, float(r["dt_ms"]), float(r["cpu_ms"]), float(r["wait_ms"])))
    return out

pals = sorted({re.match(r"run-(.+)-\d+\.csv", os.path.basename(f))[1] for f in glob.glob(f"{d}/run-*.csv") if not f.endswith("system.csv")}, key=lambda p: (p != base, p))
data = {p: steps_of(p) for p in pals}
allsteps = [s for s in data[pals[0]].keys() if s]
hz_dt = st.median([x[1] for x in data[pals[0]]["idle"]]) if "idle" in data[pals[0]] else 6.06

def row(vals):
    dt = [v[1] for v in vals]; cpu = [v[2] for v in vals]; wait = [v[3] for v in vals]
    n = len(dt); worst = sorted(dt, reverse=True)
    low = lambda frac: 1000 / st.mean(worst[:max(1, int(n * frac))])
    drop = sum(x > 1.5 * hz_dt for x in dt) / n * 100
    stut = sum(x > 2.5 * hz_dt for x in dt) / n * 100
    return dict(n=n, fps=1000 / st.mean(dt), p50=pct(dt, .5), p95=pct(dt, .95), p99=pct(dt, .99), p999=pct(dt, .999),
                low1=low(.01), low01=low(.001), jit=st.pstdev(dt), drop=drop, stut=stut, mx=max(dt),
                cpu=st.mean(cpu), cpu99=pct(cpu, .99), wait=st.mean(wait))

out = []
w = out.append
w(f"# Performance A/B — {os.path.basename(d.rstrip('/'))}\n")
w("```\n" + open(f"{d}/env.txt").read() + "```\n")
w(f"Frame interval reference (idle median): {hz_dt:.2f} ms. dropped = dt > 1.5x, stutter = dt > 2.5x.\n")
for step in allsteps:
    w(f"\n## {step}\n")
    w("| palette | frames | fps | p50 ms | p95 | p99 | p99.9 | max | 1% low fps | 0.1% low fps | jitter σ ms | dropped % | stutter % | CPU/frame ms | CPU p99 ms | run-queue wait ms |")
    w("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for p in pals:
        r = row(data[p][step])
        w(f"| {p} | {r['n']} | {r['fps']:.1f} | {r['p50']:.2f} | {r['p95']:.2f} | {r['p99']:.2f} | {r['p999']:.2f} | {r['mx']:.1f} | {r['low1']:.1f} | {r['low01']:.1f} | {r['jit']:.2f} | {r['drop']:.2f} | {r['stut']:.2f} | {r['cpu']:.3f} | {r['cpu99']:.3f} | {r['wait']:.3f} |")
    for p in pals[1:]:
        a = [v[1] for v in data[base][step]]; b = [v[1] for v in data[p][step]]
        u, pv = mannwhitneyu(a, b, alternative="two-sided")
        rb = 1 - 2 * u / (len(a) * len(b))   # rank-biserial: >0 means base has larger dt
        # per-repetition medians of the 1% low, to show run-to-run spread
        def lows(pp):
            res = []
            for rep in sorted({v[0] for v in data[pp][step]}):
                x = sorted((v[1] for v in data[pp][step] if v[0] == rep), reverse=True)
                res.append(1000 / st.mean(x[:max(1, len(x) // 100)]))
            return res
        la, lb = lows(base), lows(p)
        w(f"\n{base} vs {p}: Mann-Whitney U p={pv:.3g}, effect (rank-biserial) {rb:+.2f}; 1% low per run: {base} {st.mean(la):.1f}±{st.pstdev(la):.1f}, {p} {st.mean(lb):.1f}±{st.pstdev(lb):.1f} fps")
w("\n## System (mean over all runs)\n")
cols = ["cpu_busy_pct", "cpu_mhz", "igpu_mhz", "igpu_busy_pct", "temp_c", "shoji_cpu_pct", "shoji_rss_kb", "hypr_cpu_pct", "hypr_rss_kb"]
w("| palette | " + " | ".join(cols) + " |"); w("|---|" + "---|" * len(cols))
for p in pals:
    w(f"| {p} | " + " | ".join(f"{st.mean(system[p][c]):.1f}" for c in cols) + " |")
text = "\n".join(out)
open(f"{d}/report.md", "w").write(text); print(text)
