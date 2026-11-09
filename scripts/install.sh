#!/bin/sh
# Comindly installer. Downloads the `comindly` command line from GitHub Releases; it installs the
# local engine itself the first time it needs one.
#
#   curl -LsSf https://raw.githubusercontent.com/sayef/comindly/main/scripts/install.sh | sh
#
# Env:
#   COMINDLY_VERSION  tag to install (default: latest)
#   COMINDLY_ENGINE   1 to install the engine now as well (offline machines, CI)
#   COMINDLY_BIN_DIR  install dir (default: $HOME/.local/bin)
#   GITHUB_TOKEN      optional (higher GitHub API rate limits, or installing from a private fork)
set -eu

REPO="sayef/comindly"
VERSION="${COMINDLY_VERSION:-latest}"
BIN_DIR="${COMINDLY_BIN_DIR:-$HOME/.local/bin}"

# --- detect target triple ---------------------------------------------------
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Linux)  os_part="unknown-linux-gnu" ;;
  Darwin) os_part="apple-darwin" ;;
  *) echo "comindly: unsupported OS: $os" >&2; exit 1 ;;
esac
case "$arch" in
  x86_64|amd64) arch_part="x86_64" ;;
  arm64|aarch64) arch_part="aarch64" ;;
  *) echo "comindly: unsupported arch: $arch" >&2; exit 1 ;;
esac
target="${arch_part}-${os_part}"

# --- resolve download URL ---------------------------------------------------
auth=""
[ -n "${GITHUB_TOKEN:-}" ] && auth="-H \"Authorization: Bearer $GITHUB_TOKEN\""
if [ "$VERSION" = "latest" ]; then
  base="https://github.com/$REPO/releases/latest/download"
else
  base="https://github.com/$REPO/releases/download/$VERSION"
fi
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$BIN_DIR"

# Download, check and install one binary: comindly or comindly-engine.
fetch() {
  asset="$1-${target}.tar.gz"
  url="$base/$asset"
  echo "comindly: installing $1 for $target from $url"
  # shellcheck disable=SC2086
  eval curl -LsSf $auth "$url" -o "$tmp/$asset"
  # shellcheck disable=SC2086
  eval curl -LsSf $auth "$url.sha256" -o "$tmp/$asset.sha256"
  want="$(cut -d' ' -f1 "$tmp/$asset.sha256")"
  if command -v sha256sum >/dev/null 2>&1; then
    got="$(sha256sum "$tmp/$asset" | cut -d' ' -f1)"
  else
    got="$(shasum -a 256 "$tmp/$asset" | cut -d' ' -f1)"
  fi
  if [ "$want" != "$got" ]; then
    echo "comindly: checksum mismatch for $asset" >&2
    exit 1
  fi
  tar -C "$tmp" -xzf "$tmp/$asset"
  # A new file renamed into place: macOS kills a binary overwritten where it lies.
  mv "$tmp/$1" "$BIN_DIR/$1.new"
  chmod +x "$BIN_DIR/$1.new"
  mv "$BIN_DIR/$1.new" "$BIN_DIR/$1"
}

fetch comindly
[ "${COMINDLY_ENGINE:-}" = 1 ] && fetch comindly-engine

echo "comindly: installed to $BIN_DIR/comindly"
case ":$PATH:" in
  *":$BIN_DIR:"*) : ;;
  *) echo "comindly: add $BIN_DIR to your PATH (e.g. echo 'export PATH=\"$BIN_DIR:\$PATH\"' >> ~/.profile)" ;;
esac
"$BIN_DIR/comindly" --version
echo "comindly: next → comindly link <your repos> on this machine, or comindly use <server url> <name> for a team's"
