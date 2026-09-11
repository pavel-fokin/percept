#!/bin/sh
# Builds percept in release mode and installs the binary under
# PERCEPT_HOME (default ~/.percept), where the event log lives too. See
# lib-install.sh for how the binary is placed and kept on PATH. Rerun
# after every build; `make install` does that in one step.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
. "$root/scripts/lib-install.sh"
built="$root/target/release/percept"

cargo build --release --manifest-path "$root/Cargo.toml"
if [ ! -f "$built" ]; then
  echo "no binary at $built - is CARGO_TARGET_DIR set?" >&2
  exit 1
fi

install_binary "$built"
