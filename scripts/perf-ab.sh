#!/usr/bin/env bash
# A/B performance harness: the same scenarios under two or more palettes, repeated and interleaved
# (A B A B ...) so thermal drift and background noise hit every palette alike. Needs the shell
# running with plugins/meter.rs.
#
#   scripts/perf-ab.sh [REPS=5] [PALETTES="dark translucent"]
#
# Per run (palette x repetition): settle, warm-up (discarded), then the scenario with a marker per
# step: idle, bar pane off/on x3, quick-settings x3, launcher x3, calendar x3. Alongside the frames
# it samples, every 250 ms: CPU busy %, CPU MHz, iGPU MHz, iGPU busy % (from RC6 residency), the
# CPU package temperature and CPU % + RSS of the shell and of Hyprland.
# Result in logs/ab-<date>/: run-<palette>-<rep>.{csv,system.csv}; then scripts/perf-report.py.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
state=${XDG_STATE_HOME:-$HOME/.local/state}/shoji-bar-4
cmd=$state/command
reps=${1:-5}
palettes=${2:-"dark translucent"}
bin=$(pgrep -x shoji-bar-4 | head -n1)
[ -n "$bin" ] || { echo "the shell is not running"; exit 1; }
hz=$(hyprctl monitors 2>/dev/null | grep -oE '@[0-9.]+' | head -n1 | tr -d @ | cut -d. -f1); hz=${hz:-165}
dest="$here/../logs/ab-$(date +%Y%m%d-%H%M%S)"; mkdir -p "$dest"
send() { echo "$*" >"$cmd"; sleep 0.4; }
mark() { send "meter mark $*"; }
msg() { shoji-bar-4 msg "$@" >/dev/null 2>&1 || "$here/../target/fast/shoji-bar-4" msg "$@" >/dev/null 2>&1; }
first=$(cat "$state/palette" 2>/dev/null || echo dark)
bar=$(cat "$state/bar-pane" 2>/dev/null || echo on)

gt=$(ls -d /sys/class/drm/card*/gt/gt0 2>/dev/null | head -n1)
temp() { for z in /sys/class/thermal/thermal_zone*; do [ "$(cat $z/type)" = x86_pkg_temp ] && { echo $(( $(cat $z/temp) / 1000 )); return; }; done; echo 0; }
proc() { # cpu-ticks rss-kB of a pid
  awk '{print $14+$15}' /proc/$1/stat 2>/dev/null; awk '/VmRSS/{print $2}' /proc/$1/status 2>/dev/null; }
sample() { # $1 output file
  local out=$1 hypr; hypr=$(pgrep -x Hyprland | head -n1)
  echo "t,cpu_busy_pct,cpu_mhz,igpu_mhz,igpu_busy_pct,temp_c,shoji_cpu_pct,shoji_rss_kb,hypr_cpu_pct,hypr_rss_kb" >"$out"
  local t0 p_t p_idle p_rc6 p_s p_h clk=$(getconf CLK_TCK)
  t0=$(date +%s.%N)
  read -r _ a b c idle rest </proc/stat; p_t=$((a+b+c+idle)); p_idle=$idle
  p_rc6=$(cat "$gt/rc6_residency_ms"); read -r p_s _ < <(proc "$bin" | tr '\n' ' '); read -r p_h _ < <(proc "$hypr" | tr '\n' ' ')
  local p_now=$t0
  while :; do
    sleep 0.25
    local now; now=$(date +%s.%N)
    read -r _ a b c idle rest </proc/stat; local tot=$((a+b+c+idle)) d di; d=$((tot-p_t)); di=$((idle-p_idle))
    local busy=$(( d>0 ? 100*(d-di)/d : 0 )); p_t=$tot; p_idle=$idle
    local rc6; rc6=$(cat "$gt/rc6_residency_ms"); local dt_ms; dt_ms=$(awk -v a="$now" -v b="$p_now" 'BEGIN{printf "%d",(a-b)*1000}')
    local gbusy; gbusy=$(awk -v r=$((rc6-p_rc6)) -v d="$dt_ms" 'BEGIN{v=100-100*r/d; if(v<0)v=0; printf "%.1f",v}'); p_rc6=$rc6
    local s h s_rss h_rss
    read -r s s_rss < <(proc "$bin" | tr '\n' ' '); read -r h h_rss < <(proc "$hypr" | tr '\n' ' ')
    local mhz; mhz=$(awk '{s+=$1;n++} END{printf "%d",s/n/1000}' /sys/devices/system/cpu/cpu*/cpufreq/scaling_cur_freq)
    printf '%.2f,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' "$(awk -v a="$now" -v b="$t0" 'BEGIN{print a-b}')" "$busy" "$mhz" \
      "$(cat "$gt/rps_act_freq_mhz")" "$gbusy" "$(temp)" \
      "$(awk -v a=$((s-p_s)) -v c=$clk -v d="$dt_ms" 'BEGIN{printf "%.1f",100*a/c/(d/1000)}')" "$s_rss" \
      "$(awk -v a=$((h-p_h)) -v c=$clk -v d="$dt_ms" 'BEGIN{printf "%.1f",100*a/c/(d/1000)}')" "$h_rss" >>"$out"
    p_s=$s; p_h=$h; p_now=$now
  done
}

scenario() {
  local p
  mark "idle"; sleep 3
  mark "bar"; for _ in 1 2 3; do send "bar off"; sleep 1.8; send "bar on"; sleep 1.8; done
  for p in quick-settings launcher calendar; do
    mark "$p"; for _ in 1 2 3; do msg toggle "$p"; sleep 1.6; msg toggle "$p"; sleep 1.6; done
  done
}

{ echo "date: $(date -Is)"; echo "kernel: $(uname -r)"; echo "cpu: $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2)"
  echo "profile: $(powerprofilesctl get)"; echo "monitor: $(hyprctl monitors | grep -m1 -E '@')"
  echo "shoji: $(shoji-bar-4 --version 2>/dev/null)"; echo "reps: $reps palettes: $palettes"; echo "blur: $(hyprctl getoption decoration:blur:passes 2>/dev/null | head -n1)"; } >"$dest/env.txt"

for rep in $(seq 1 "$reps"); do
  for p in $palettes; do
    echo "== rep $rep/$reps  palette $p"
    send "palette $p"; send "bar on"; send "opacity 1"
    sleep 2.5
    # warm-up, discarded
    send "meter start $hz"; sleep 4; send "meter stop"; sleep 1
    sample "$dest/run-$p-$rep.system.csv" & sp=$!
    send "meter start $hz"
    scenario
    send "meter stop"; sleep 1
    kill $sp 2>/dev/null; wait $sp 2>/dev/null
    cp "$state/fps.csv" "$dest/run-$p-$rep.csv"
    sleep 3   # cool-down between runs
  done
done
send "palette $first"; send "bar $bar"
echo "$dest"
