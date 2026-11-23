#!/bin/sh
# Gera o .app e o .dmg do Instalador OpenPod no macOS. Assinatura ad hoc
# (sem conta Apple) é suficiente: quem baixa recebe "Abrir assim mesmo" em
# Privacidade e Segurança, não "danificado".
set -eu
cd "$(dirname "$0")"
. "$HOME/.cargo/env"

cargo build --release

app="dist/OpenPod Instalador.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/openpod-instalador "$app/Contents/MacOS/openpod-instalador"
cp assets/OpenLogo.png "$app/Contents/Resources/"
cp assets/Inter-OFL.txt "$app/Contents/Resources/"

cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Instalador OpenPod</string>
    <key>CFBundleDisplayName</key><string>Instalador OpenPod</string>
    <key>CFBundleIdentifier</key><string>io.github.openpod.instalador</string>
    <key>CFBundleExecutable</key><string>openpod-instalador</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0.1.0</string>
    <key>CFBundleVersion</key><string>0.1.0</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$app"

rm -f "dist/openpod-instalador-macos.dmg"
hdiutil create -volname "Instalador OpenPod" -srcfolder "$app" -ov -format UDZO \
    "dist/openpod-instalador-macos.dmg" >/dev/null

echo "pronto:"
ls -lh "$app/Contents/MacOS/openpod-instalador" "dist/openpod-instalador-macos.dmg"
