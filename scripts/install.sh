#!/bin/sh
# Builds percept in release mode and links the binary under PERCEPT_HOME
# (default ~/.percept), where the event log lives too. The link points
# at target/release/percept, so a later `cargo build --release` or
# `make install` updates the installed version in place.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
home=${PERCEPT_HOME:-"$HOME/.percept"}
bin="$home/bin"

cargo build --release --manifest-path "$root/Cargo.toml"

mkdir -p "$bin"
ln -sf "$root/target/release/percept" "$bin/percept"

echo "linked $bin/percept -> $root/target/release/percept"
case ":$PATH:" in
  *":$bin:"*) ;;
  *) echo "add it to your PATH:"; echo "  export PATH=\"$bin:\$PATH\"" ;;
esac
