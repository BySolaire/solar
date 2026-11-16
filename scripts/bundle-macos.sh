#!/bin/sh
# Construit SOLAR.app (Apple Silicon) dans target/release, signé « ad hoc » :
# aucun compte Apple, aucun certificat. Avec --install, le copie dans
# /Applications (en remplaçant l'ancienne version, si SOLAR n'est pas ouvert).
#   scripts/bundle-macos.sh [--install]
set -eu
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
app=target/release/SOLAR.app

cargo build --release -p solar

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/solar "$app/Contents/MacOS/solar"
cp assets/logo/identity-kit/macOS/Solar.icns "$app/Contents/Resources/Solar.icns"
cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>com.bysolaire.solar</string>
  <key>CFBundleName</key><string>SOLAR</string>
  <key>CFBundleDisplayName</key><string>SOLAR</string>
  <key>CFBundleExecutable</key><string>solar</string>
  <key>CFBundleIconFile</key><string>Solar</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>SOLAR</string>
</dict>
</plist>
EOF

codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"
echo "Construit : $app (version $version, signature ad hoc)"

if [ "${1:-}" = "--install" ]; then
  if pgrep -f "/Applications/SOLAR.app/Contents/MacOS/solar" >/dev/null; then
    echo "SOLAR est ouvert : quitte-le, puis relance avec --install." >&2
    exit 1
  fi
  rm -rf /Applications/SOLAR.app
  ditto "$app" /Applications/SOLAR.app
  echo "Installé : /Applications/SOLAR.app"
fi
