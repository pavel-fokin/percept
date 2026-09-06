#!/bin/sh
# Builds percept in release mode and links the binary under PERCEPT_HOME
# (default ~/.percept), where the event log lives too. The link points
# at target/release/percept, so a later `cargo build --release` or
# `make install` updates the installed version in place. When
# ~/.percept/bin is not on PATH, also links into ~/.local/bin or ~/bin
# if one of those is, so `percept` resolves without editing PATH.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
home=${PERCEPT_HOME:-"$HOME/.percept"}
bin="$home/bin"
target="$root/target/release/percept"

cargo build --release --manifest-path "$root/Cargo.toml"

mkdir -p "$bin"
ln -sf "$target" "$bin/percept"
echo "linked $bin/percept -> $target"

on_path() {
  case ":$PATH:" in *":$1:"*) return 0 ;; *) return 1 ;; esac
}

if ! on_path "$bin"; then
  linked=""
  for dir in "$HOME/.local/bin" "$HOME/bin"; do
    if [ -d "$dir" ] && on_path "$dir"; then
      ln -sf "$target" "$dir/percept"
      echo "linked $dir/percept -> $target"
      linked=1
      break
    fi
  done
  if [ -z "$linked" ]; then
    echo "add it to your PATH:"
    echo "  export PATH=\"$bin:\$PATH\""
  fi
fi
