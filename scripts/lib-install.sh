# Shared by scripts/install.sh and scripts/get.sh: copies a built
# percept binary into PERCEPT_HOME/bin and keeps ~/.local/bin or ~/bin
# in sync, so a source build and a curl install land the same way. The
# binary is copied, not linked: it must sit outside target/ so the
# running percept can tell an install from a `cargo` build and keep the
# two logs apart.
home=${PERCEPT_HOME:-"$HOME/.percept"}
bin="$home/bin"

on_path() {
  case ":$PATH:" in *":$1:"*) return 0 ;; *) return 1 ;; esac
}

put() {
  mkdir -p "$1"
  rm -f "$1/percept"
  install -m 755 "$2" "$1/percept"
  echo "installed $1/percept"
}

install_binary() {
  built=$1
  put "$bin" "$built"

  # Refresh any PATH copy an earlier run left, so it never lags $bin.
  on_path_copy=""
  for dir in "$HOME/.local/bin" "$HOME/bin"; do
    if [ -e "$dir/percept" ] && [ "$dir" != "$bin" ]; then
      put "$dir" "$built"
      on_path "$dir" && on_path_copy=1
    fi
  done

  # Still not on PATH and no copy there: make one in the first PATH dir.
  if ! on_path "$bin" && [ -z "$on_path_copy" ]; then
    for dir in "$HOME/.local/bin" "$HOME/bin"; do
      if [ -d "$dir" ] && on_path "$dir"; then
        put "$dir" "$built"
        on_path_copy=1
        break
      fi
    done
  fi

  if ! on_path "$bin" && [ -z "$on_path_copy" ]; then
    echo "add it to your PATH:"
    echo "  export PATH=\"$bin:\$PATH\""
  fi
}
