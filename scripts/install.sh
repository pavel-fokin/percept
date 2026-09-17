#!/bin/sh
# Builds the web page and then percept in release mode, and installs
# the binary under PERCEPT_HOME (default ~/.percept), where the event
# log lives too. Needs Rust and Node: build.rs compiles
# web/dist/index.html into the binary, and that file is not in the
# checkout, so a build without the page step ships a stub `percept
# web` serves instead. See lib-install.sh for how the binary is placed
# and kept on PATH. Rerun after every build; `make install` does that
# in one step.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
. "$root/scripts/lib-install.sh"
built="$root/target/release/percept"

if ! command -v npm >/dev/null 2>&1; then
  echo "percept needs Node to build its web page - install it, or use scripts/get.sh for a prebuilt binary" >&2
  exit 1
fi

(cd "$root/web" && npm ci && npm run build)

cargo build --release --manifest-path "$root/Cargo.toml"
if [ ! -f "$built" ]; then
  echo "no binary at $built - is CARGO_TARGET_DIR set?" >&2
  exit 1
fi

install_binary "$built"
