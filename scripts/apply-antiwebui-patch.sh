#!/bin/bash
# Makes shoji-bar-4 build against an AntiWebUI with the UI zoom patch, which is what gives the
# `ui_scale` setting its effect.
#
# It copies the AntiWebUI revision that Cargo.lock pins out of cargo's git checkouts, applies
# patches/antiwebui-ui-zoom.patch to the copy, and writes .cargo/config.toml (which git ignores) to
# make cargo use the copy. Run it again after Cargo.lock moves to another AntiWebUI revision.
#
#   scripts/apply-antiwebui-patch.sh            # then: cargo build --release
#   SHOJI_ANTIWEBUI_DIR=/some/dir scripts/apply-antiwebui-patch.sh
#
# Cargo then rewrites Cargo.lock to point at the copy. That change is only good for this machine:
# do not commit it (`git checkout Cargo.lock` drops it).
#
# To go back to the unpatched AntiWebUI, delete .cargo/config.toml and run `git checkout Cargo.lock`.

set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
patch_files=("$root"/patches/*.patch)
dest=${SHOJI_ANTIWEBUI_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/shoji-bar-4/AntiWebUI}

# Once cargo builds against the copy it rewrites Cargo.lock for this machine, so read the revision
# from the lock as committed.
lock=$(git -C "$root" show HEAD:Cargo.lock 2>/dev/null || cat "$root/Cargo.lock")
rev=$(grep -m1 -oE 'AntiWebUI\.git\?branch=main#[0-9a-f]+' <<<"$lock" | sed 's/.*#//')
[[ -n $rev ]] || { echo "no AntiWebUI revision in Cargo.lock" >&2; exit 1; }

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
