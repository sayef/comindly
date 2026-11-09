#!/usr/bin/env bash
# Build the Linux engine on this machine (all cores, incremental) and package it as comindly:local.
set -euo pipefail
cd "$(dirname "$0")/.."

arch=$(uname -m)
case "$arch" in
  arm64 | aarch64) target=aarch64-unknown-linux-gnu docker_arch=arm64 ;;
  x86_64) target=x86_64-unknown-linux-gnu docker_arch=amd64 ;;
  *) echo "unsupported architecture: $arch" >&2; exit 1 ;;
esac

(cd ui && npm ci --silent && npm run build --silent)
CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
  cargo zigbuild --release --locked --target "$target.2.36" -p comindly-engine -j "${JOBS:-6}"
mkdir -p "dist/linux/$docker_arch"
cp "target/$target/release/comindly-engine" "dist/linux/$docker_arch/comindly-engine"
docker build --target local -t comindly:local .

# Random secrets for the local stack, made once and never committed.
mkdir -p .secrets
for f in admin_token secret_key; do
  [ -s ".secrets/$f" ] || head -c 32 /dev/urandom | base64 | tr -d '/+=\n' > ".secrets/$f"
done
chmod 644 .secrets/*  # readable by the container user
