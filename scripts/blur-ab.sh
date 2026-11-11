#!/usr/bin/env bash
# iGPU busy % of translucent under different Hyprland blur settings, same animation load.
state=${XDG_STATE_HOME:-$HOME/.local/state}/shoji-bar-4; gt=$(ls -d /sys/class/drm/card*/gt/gt0 | head -n1)
send() { echo "$*" >"$state/command"; sleep 0.4; }
run() { # label passes size [extra]
  hyprctl eval "hl.config({ decoration = { blur = { passes = $2, size = $3, ${4:-} } } })" >/dev/null
  sleep 1; a=$(cat $gt/rc6_residency_ms); t0=$(date +%s%N)
  for _ in 1 2 3; do send "bar off"; sleep 1.8; send "bar on"; sleep 1.8; done
  b=$(cat $gt/rc6_residency_ms); t1=$(date +%s%N)
  awk -v l="$1" -v r=$((b-a)) -v d=$(((t1-t0)/1000000)) 'BEGIN{printf "%-28s igpu busy %.1f%%\n", l, 100-100*r/d}'
}
send "palette translucent"; sleep 2
for i in 1 2; do
run "current p3 s4" 3 4; run "p2 s8" 2 8; run "p1 s16" 1 16; run "p1 s8" 1 8; done
hyprctl eval "hl.config({ decoration = { blur = { passes = 3, size = 4 } } })" >/dev/null
