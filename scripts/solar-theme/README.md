# Solar theme assets

Generated from the Studio Design handoff (validated home mockup, 2026-11-23).

| File | Source | How |
|---|---|---|
| `crates/solar/assets/fonts/Inter-*.ttf` | Inter 4.1, official release (github.com/rsms/inter, `extras/ttf/`) | copied as is; licence `Inter-OFL.txt` |
| `crates/solar/assets/fonts/SpaceGrotesk-Medium.ttf` | design-system `space-grotesk-latin.woff2` (variable) | `instance-space-grotesk.py <woff2> <ttf>`: static weight 500; licence `SpaceGrotesk-OFL.txt` |
| `crates/solar/assets/solar-horizon.png`, `solar-veil.png` | `horizon-accueil.jpg` and the mockup's `.main::before` / `.glass .window` CSS | `bake-backdrop.py <horizon.jpg> <out-dir>` |

`bake-backdrop.py` prints the bake geometry (`ref`, `rotated`, `crop`). If it
changes, update `BAKE_WIDTH`, `TURNED` and `BAND` in
`crates/solar/src/solar_backdrop.rs`.

Both scripts need Python 3 with `fonttools brotli pillow numpy`.

## Logo, option C (2026-11-23)

Source: Studio Design `02-identite/design-system/logo/option-c/` (« S » redessiné
par Identité, validé par le client).

| File | From | How |
|---|---|---|
| `src-tauri/macos/AppIcon.icon/Assets/icon.png` | `solar-icone-calque.svg` | `build-icons.py`; `icon.json` fill `#08080A`. Also the Monos menu-bar template image |
| `src-tauri/icons/*.png`, `icon.ico`, `public/solar.png` | `solar-icone.svg` | `build-icons.py` |
| `src-tauri/icons/icon.icns` | `solar-icone.svg` on Apple's 824/1024 grid | `build-icons.py` writes the iconset, then `iconutil -c icns` on a Mac |
| `src-tauri/macos/Assets.car` | `AppIcon.icon` | on a Mac: `xcrun actool AppIcon.icon --compile out --platform macosx --minimum-deployment-target 11.0 --app-icon AppIcon --include-all-app-icons --output-partial-info-plist out/partial.plist` |
| `crates/solar/assets/solar.png` | `solar-marque-app.png` | copied |
