#!/bin/sh
# Builds percept in release mode and installs the binary under
# PERCEPT_HOME (default ~/.percept), where the event log lives too. The
# binary is copied, not linked: it must sit outside target/ so the
# running percept can tell an install from a `cargo` build and keep the
# two logs apart. When ~/.percept/bin is not on PATH, also copies into
# ~/.local/bin or ~/bin if one of those is, so `percept` resolves.
# Rerun after every build - `make install` does that in one step.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
home=${PERCEPT_HOME:-"$HOME/.percept"}
bin="$home/bin"
built="$root/target/release/percept"

cargo build --release --manifest-path "$root/Cargo.toml"

mkdir -p "$bin"
rm -f "$bin/percept"
install -m 755 "$built" "$bin/percept"
echo "installed $bin/percept"

on_path() {
  case ":$PATH:" in *":$1:"*) return 0 ;; *) return 1 ;; esac
}

if ! on_path "$bin"; then
  linked=""
  for dir in "$HOME/.local/bin" "$HOME/bin"; do
    if [ -d "$dir" ] && on_path "$dir"; then
      rm -f "$dir/percept"
      install -m 755 "$built" "$dir/percept"
      echo "installed $dir/percept"
      linked=1
      break
    fi
  done
  if [ -z "$linked" ]; then
    echo "add it to your PATH:"
    echo "  export PATH=\"$bin:\$PATH\""
  fi
fi
