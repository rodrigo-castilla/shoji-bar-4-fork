#!/usr/bin/env bash
# Keeps the machine from slowing down while the shell animates, at the cost of power.
#
#   scripts/performance.sh on     # performance power profile; iGPU clock held at its maximum (sudo)
#   scripts/performance.sh off    # back to balanced; iGPU clock floor back to 300 MHz
#
# The shell, the compositor and a video wallpaper all draw on the same integrated GPU, and the
# power profile lets it idle at 300 MHz: the first frames of an animation then run on a slow clock,
# which is what shows up as a stutter. The CPU does the same with its energy preference.

set -euo pipefail
card=$(ls -d /sys/class/drm/card*/gt_min_freq_mhz 2>/dev/null | head -n 1 || true)

case ${1:-} in
  on)
    powerprofilesctl set performance
    if [[ -n $card ]]; then
      max=$(cat "${card%/*}/gt_RP0_freq_mhz")
      echo "$max" | sudo tee "$card" >/dev/null
    fi
    ;;
  off)
    powerprofilesctl set balanced
    [[ -n $card ]] && echo 300 | sudo tee "$card" >/dev/null
    ;;
  *) echo "usage: $0 on|off" >&2; exit 1 ;;
esac
echo "profile: $(powerprofilesctl get); iGPU floor: $(cat "${card:-/dev/null}" 2>/dev/null) MHz"
