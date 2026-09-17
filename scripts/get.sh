#!/bin/sh
# Downloads the prebuilt percept binary from the newest release and
# installs it, for a stranger without a Rust toolchain.
# `scripts/install.sh` (via `make install`) builds from source
# instead. See lib-install.sh for how the binary is placed and kept on
# PATH.
set -eu

repo="pavel-fokin/percept"

# Resolve the newest release to its own tag before downloading
# anything: /releases/latest/download moves the moment a merge
# publishes, and a tarball and a SHA256SUMS from either side of that
# moment would fail the checksum as if the download had been tampered
# with. A repo with no release redirects to the releases page and
# answers 200 doing it, so where we landed is what says whether there
# is anything to install.
newest=$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
  "https://github.com/$repo/releases/latest")
case "$newest" in
  */releases/tag/*) tag=${newest##*/} ;;
  *)
    echo "no published release for $repo - build from source instead: https://github.com/$repo#install" >&2
    exit 1
    ;;
esac

release_url="https://github.com/$repo/releases/download/$tag"

os=$(uname -s)
arch=$(uname -m)

case "$os" in
  Darwin) plat=apple-darwin ;;
  Linux) plat=unknown-linux-gnu ;;
  *)
    echo "unsupported OS: $os - build from source instead: https://github.com/$repo#install" >&2
    exit 1
    ;;
esac

case "$arch" in
  arm64 | aarch64) arch=aarch64 ;;
  x86_64 | amd64) arch=x86_64 ;;
  *)
    echo "unsupported architecture: $arch - build from source instead: https://github.com/$repo#install" >&2
    exit 1
    ;;
esac

target="$arch-$plat"
asset="percept-$target.tar.gz"

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    echo "need sha256sum or shasum to verify the download" >&2
    exit 1
  fi
}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

echo "downloading $asset from $tag..."
curl -fsSL "$release_url/$asset" -o "$work/$asset"
curl -fsSL "$release_url/SHA256SUMS" -o "$work/SHA256SUMS"

got=$(cd "$work" && sha256 "$asset")
want=$(grep " $asset\$" "$work/SHA256SUMS" | awk '{print $1}')
if [ "$got" != "$want" ]; then
  echo "checksum mismatch for $asset" >&2
  exit 1
fi

tar xzf "$work/$asset" -C "$work"

# lib-install.sh travels inside the checksummed tarball, not a
# separate curl from main: the binary and the helper that places it
# come from the same verified release, never a newer or older commit.
. "$work/percept-$target/lib-install.sh"

install_binary "$work/percept-$target/percept"

echo
echo "next: cd into a project and run 'percept init claude-code' or 'percept init codex'"
