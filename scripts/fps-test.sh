#!/usr/bin/env bash
# Measures the frames of the shell while palettes change, the bar's pane switches and you open and
# close panels, and leaves a log to read afterwards.
#
#   scripts/fps-test.sh [MANUAL_SECONDS]      # 20 seconds of your own clicking by default
#
# It needs the shell running with plugins/meter.rs. Steps (each one is a marker in the log):
#   1. idle            nothing happens
#   2. bar off / on    the pane's animation, three times
#   3. palettes        each palette in turn, back to the first
#   4. opacity         the slider's range, swept
#   5. manual          you open and close panels, hover, click: whatever dropped frames for you
#
# Result, in ~/.local/state/shoji-bar-4/:  fps.txt (summary by step), fps.csv (every frame),
# system.csv (CPU and iGPU clocks and load every 250 ms). Copy them to the project's logs/ with the
# date: the script does that too.

set -uo pipefail
state=${XDG_STATE_HOME:-$HOME/.local/state}/shoji-bar-4
cmd=$state/command
manual=${1:-20}
hz=$(hyprctl monitors 2>/dev/null | grep -oE '@[0-9.]+' | head -n1 | tr -d @ | cut -d. -f1)
hz=${hz:-165}
mkdir -p "$state"

send() { echo "$*" >"$cmd"; sleep 0.4; }   # the shell reads the file every 150 ms
mark() { send "meter mark $*"; }
first=$(cat "$state/palette" 2>/dev/null || echo dark)
bar=$(cat "$state/bar-pane" 2>/dev/null || echo on)
opacity=$(cat "$state/opacity" 2>/dev/null || echo 1)
palettes=$(ls "$(dirname "$0")/../plugins/palettes" | sed 's/\.palette$//')

# System sampler
igpu=$(ls /sys/class/drm/card*/gt_cur_freq_mhz 2>/dev/null | head -n1)
echo "t,cpu_busy_pct,cpu_mhz_avg,igpu_mhz,profile" >"$state/system.csv"
(
  t0=$(date +%s.%N)
  read -r _ a b c idle rest </proc/stat
  prev_total=$((a + b + c + idle)); prev_idle=$idle
  while :; do
    sleep 0.25
    read -r _ a b c idle rest </proc/stat
    total=$((a + b + c + idle)); d=$((total - prev_total)); di=$((idle - prev_idle))
    busy=$(( d > 0 ? 100 * (d - di) / d : 0 )); prev_total=$total; prev_idle=$idle
    mhz=$(awk '{s+=$1; n++} END {printf "%d", s/n/1000}' /sys/devices/system/cpu/cpu*/cpufreq/scaling_cur_freq)
    g=$([ -n "$igpu" ] && cat "$igpu" || echo 0)
    printf '%.2f,%s,%s,%s,%s\n' "$(awk -v a="$(date +%s.%N)" -v b="$t0" 'BEGIN {print a - b}')" "$busy" "$mhz" "$g" "$(powerprofilesctl get)" >>"$state/system.csv"
  done
) &
sampler=$!
trap 'kill $sampler 2>/dev/null' EXIT

send "meter start $hz"
mark "1 idle"; sleep 3

mark "2 bar off/on x3"
for _ in 1 2 3; do send "bar off"; sleep 1.8; send "bar on"; sleep 1.8; done

mark "3 palettes"
for p in $palettes; do mark "3 palette $p"; send "palette $p"; sleep 1.5; done
mark "3 palettes with the bar off"
send "bar off"; sleep 1.2
for p in $palettes; do mark "3 off palette $p"; send "palette $p"; sleep 1.2; done
send "bar on"; sleep 1.2

mark "4 opacity sweep"
for v in 0.8 0.6 0.4 0.2 0.1 0.4 0.7 1.0; do send "opacity $v"; sleep 0.5; done

mark "5 manual: open/close panels, hover, click"
echo ">>> $manual s: open and close panels now"
sleep "$manual"

mark "6 idle after"; sleep 2
# back to how it was
send "palette $first"; send "bar $bar"; send "opacity $opacity"
send "meter stop"; sleep 1
kill $sampler 2>/dev/null

dest="$(dirname "$0")/../logs/$(date +%Y%m%d-%H%M%S)"
mkdir -p "$dest"
cp "$state"/fps.txt "$state"/fps.csv "$state"/system.csv "$dest"/ 2>/dev/null
cat "$state/fps.txt"
echo "log: $dest"
