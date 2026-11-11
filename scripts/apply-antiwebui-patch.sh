#!/bin/bash
# Makes shoji-bar-4 build against an AntiWebUI with this fork's patches (UI zoom, which gives the
# `ui_scale` setting its effect, and the notification hints).
#
# It copies the AntiWebUI revision named in patches/antiwebui.rev out of cargo's git checkouts,
# applies every patches/*.patch to the copy, and writes .cargo/config.toml (git ignores it) to make
# cargo use the copy. Cargo.lock is committed as that build resolves it (the patched crates are
# path packages, with no machine-specific path in the lock), so after this a plain
# `cargo build --release` leaves it untouched. Change the revision by editing patches/antiwebui.rev,
# running this script again and committing the Cargo.lock that comes out.
#
#   scripts/apply-antiwebui-patch.sh            # once per clone; then: cargo build --release
#   SHOJI_ANTIWEBUI_DIR=/some/dir scripts/apply-antiwebui-patch.sh

set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
patch_files=("$root"/patches/*.patch)
dest=${SHOJI_ANTIWEBUI_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/shoji-bar-4/AntiWebUI}

rev=$(tr -d '[:space:]' <"$root/patches/antiwebui.rev")
[[ -n $rev ]] || { echo "no AntiWebUI revision in patches/antiwebui.rev" >&2; exit 1; }

checkout=$(ls -d "$HOME"/.cargo/git/checkouts/antiwebui-*/"${rev:0:7}" 2>/dev/null | head -n 1 || true)
if [[ -z $checkout ]]; then
  echo "AntiWebUI ${rev:0:7} is not in cargo's git checkouts yet: run 'cargo fetch' first" >&2
  exit 1
fi

rm -rf "$dest"
mkdir -p "$(dirname "$dest")"
cp -r "$checkout" "$dest"
chmod -R u+w "$dest"
for p in "${patch_files[@]}"; do patch -d "$dest" -p1 --quiet <"$p"; done
echo "patched AntiWebUI ${rev:0:7} at $dest"

# Every crate of the AntiWebUI workspace has to come from the copy, or cargo would mix two of them.
mkdir -p "$root/.cargo"
config=$root/.cargo/config.toml
[[ -f $config ]] && cp "$config" "$config.bak"
{
  echo "# Written by scripts/apply-antiwebui-patch.sh: AntiWebUI with the UI zoom patch."
  echo '[patch."https://github.com/bea4dev/AntiWebUI.git"]'
  for manifest in "$dest"/src/*/Cargo.toml; do
    name=$(grep -m1 '^name' "$manifest" | cut -d'"' -f2)
    [[ -n $name ]] && echo "$name = { path = \"$(dirname "$manifest")\" }"
  done
} >"$config"
echo "wrote $config"
