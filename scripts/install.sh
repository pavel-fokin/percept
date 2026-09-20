#!/bin/sh
# Builds percept in release mode and installs the binary under
# PERCEPT_HOME (default ~/.percept), where the event log lives too. See
# lib-install.sh for how the binary is placed and kept on PATH. Rerun
# after every build.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
. "$root/scripts/lib-install.sh"
built="$root/target/release/percept"

# build.rs embeds web/dist/index.html, which the checkout does not
# carry: without this the binary serves the stub page instead.
if ! command -v npm >/dev/null 2>&1; then
  echo "npm is needed to build the page percept web serves" >&2
  exit 1
fi
(cd "$root/web" && npm install --silent && npm run build)

cargo build --release --manifest-path "$root/Cargo.toml"
if [ ! -f "$built" ]; then
  echo "no binary at $built - is CARGO_TARGET_DIR set?" >&2
  exit 1
fi

install_binary "$built"
