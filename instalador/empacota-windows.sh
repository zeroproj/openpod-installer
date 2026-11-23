#!/bin/sh
# Gera o .exe do Instalador OpenPod para o Windows. Da para rodar em qualquer
# sistema com o Rust e o MinGW (x86_64-w64-mingw32) instalados; o proprio
# build.rs cuida do icone, do nome, da versao e do manifesto do .exe.
#
# O cliente NAO instala driver nenhum: o instalador fala com o player pelo
# driver de disco do proprio Windows (SCSI pass-through), sem WinUSB e sem
# Zadig.
set -eu
cd "$(dirname "$0")"
. "$HOME/.cargo/env"

alvo=x86_64-pc-windows-gnu
cargo build --release --target "$alvo"

mkdir -p dist
cp "target/$alvo/release/openpod-instalador.exe" "dist/openpod-instalador-windows.exe"

echo "pronto:"
ls -lh "dist/openpod-instalador-windows.exe"
