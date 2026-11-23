#!/bin/sh
# Gera o OpenPod Tune para Linux (rode NUM LINUX x86_64): dist/openpod-tune-linux-x86_64.tar.gz
set -eu
cd "$(dirname "$0")"
. "$HOME/.cargo/env"
cargo build --release
d=dist/openpod-tune-linux
rm -rf "$d" && mkdir -p "$d"
cp target/release/openpod-tune "$d/openpod-tune"
cp packaging/linux/70-openpod.rules packaging/linux/instalar-regra.sh packaging/linux/LEIA-ME.txt "$d/"
chmod +x "$d/openpod-tune" "$d/instalar-regra.sh"
tar -czf dist/openpod-tune-linux-x86_64.tar.gz -C dist openpod-tune-linux
ls -lh dist/openpod-tune-linux-x86_64.tar.gz
