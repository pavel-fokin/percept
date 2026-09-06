#!/bin/sh
# Builds percept in release mode and installs the binary under
# PERCEPT_HOME (default ~/.percept), where the event log lives too. The
# binary is copied, not linked: it must sit outside target/ so the
# running percept can tell an install from a `cargo` build and keep the
# two logs apart. A copy is also kept in ~/.local/bin or ~/bin - created
# when ~/.percept/bin is not on PATH so `percept` resolves, and
# refreshed on every run once it exists, so PATH never finds a stale
# build. Rerun after every build; `make install` does that in one step.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
home=${PERCEPT_HOME:-"$HOME/.percept"}
bin="$home/bin"
built="$root/target/release/percept"

cargo build --release --manifest-path "$root/Cargo.toml"
if [ ! -f "$built" ]; then
  echo "no binary at $built - is CARGO_TARGET_DIR set?" >&2
  exit 1
fi

on_path() {
  case ":$PATH:" in *":$1:"*) return 0 ;; *) return 1 ;; esac
}

put() {
  mkdir -p "$1"
  rm -f "$1/percept"
  install -m 755 "$built" "$1/percept"
  echo "installed $1/percept"
}

put "$bin"

# Refresh any PATH copy an earlier run left, so it never lags $bin.
on_path_copy=""
for dir in "$HOME/.local/bin" "$HOME/bin"; do
  if [ -e "$dir/percept" ] && [ "$dir" != "$bin" ]; then
    put "$dir"
    on_path "$dir" && on_path_copy=1
  fi
done

# Still not on PATH and no copy there: make one in the first PATH dir.
if ! on_path "$bin" && [ -z "$on_path_copy" ]; then
  for dir in "$HOME/.local/bin" "$HOME/bin"; do
    if [ -d "$dir" ] && on_path "$dir"; then
      put "$dir"
      on_path_copy=1
      break
    fi
  done
fi

if ! on_path "$bin" && [ -z "$on_path_copy" ]; then
  echo "add it to your PATH:"
  echo "  export PATH=\"$bin:\$PATH\""
fi
