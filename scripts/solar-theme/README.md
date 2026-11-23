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
