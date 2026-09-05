#!/bin/sh
# Builds percept in release mode and installs the binary under
# PERCEPT_HOME (default ~/.percept), where the event log lives too.
# Safe to rerun: each run replaces the binary in place.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
home=${PERCEPT_HOME:-"$HOME/.percept"}
bin="$home/bin"

cargo build --release --manifest-path "$root/Cargo.toml"

mkdir -p "$bin"
install -m 755 "$root/target/release/percept" "$bin/percept"

echo "installed $bin/percept"
case ":$PATH:" in
  *":$bin:"*) ;;
  *) echo "add it to your PATH:"; echo "  export PATH=\"$bin:\$PATH\"" ;;
esac
