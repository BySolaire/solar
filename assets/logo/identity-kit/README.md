# SOLAR — Official Identity Asset Kit, approved direction

## Source of truth

`assets/svg/solar-logo-master.svg` is the **vector master**, rebuilt from the approved final raster reference (`reference/approved-concept.png`). The final choice has:

- An aerospace horizon arc, warm-white luminous center, sharp fading ends.
- Custom letter silhouettes (no font dependency).
- **S** with deliberate parallel 45° cuts at upper-right and lower-left.
- **L** with matching diagonal cut at the lower-right.
- **A** without crossbar; its terminal angle serves as the family reference.
- **R** semi-open, with straight inner lower corner and precise diagonal leg.
- Monochrome identity. Space Grotesk is the recommended font **for the app UI**, not the logo drawing.

These outlines and the arc geometry are stored as SVG paths, not as raster images or font characters. All exported PNG assets are built from the same paths.

## How to use

| File | Usage |
| --- | --- |
| `assets/svg/solar-logo-master.svg` | Primary transparent logo for dark UI surfaces |
| `assets/svg/solar-logo-black.svg` | Branded splash/loading screen on black |
| `assets/svg/solar-logo-flat.svg` | Clean, no-glow white logo on dark backgrounds |
| `assets/svg/solar-logo-white.svg` | Dark flat logo on white background |
| `assets/svg/solar-wordmark.svg` | Wordmark alone in sidebar, header, docs |
| `assets/svg/solar-symbol.svg` | Approved arc alone |
| `assets/svg/solar-macos-icon.svg` | Mac icon source (same arc, optically resized within rounded tile) |
| `macOS/Solar.icns` | macOS Dock/application icon |
| `macOS/Solar.iconset/` | All standard macOS PNG icon sizes |
| `assets/png/favicon-16.png` etc. | Web favicons & small assets |
| `comparison/solar-original-vs-vector.png` | Direct visual comparison with approved raster |

SVG wordmark is custom vector lettering with **no dependency on downloading or bundling a font**.

## Implementing in SOLAR

Copy SVGs into your app's assets directory, e.g. `public/brand/` or `src/assets/brand/` depending on the app framework. Use `solar-wordmark.svg` inside the product UI and `solar-logo-master.svg` for larger marketing and splash contexts.

macOS `.icns` can be used directly in the native app target. If your tooling requires a `.iconset`, it is included.

## Design integrity

The selected reference was an AI-generated raster image, not an editable design source. This is a vector **reconstruction**, not recovery of original control points. Fine glow, texture and antialiasing will vary across renderers and pixel densities; refer to `comparison/solar-original-vs-vector.png` for validation.

The small-size icon is an optical adaptation of the master arc placed on a rounded black tile. The wordmark silhouette is **not altered across exports**. Do not manually regenerate the lettering separately for app and website.

Use a flat version in contexts where the luminous halo loses definition or reduces contrast. For tiny favicons, the arc may not be distinguishable enough; verify the chosen icon on its real background before shipping.
