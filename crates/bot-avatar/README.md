# bot-avatar

A pure-Rust port of the [open-bot-avatar](https://github.com/DominikScholz/open-bot-avatar)
rendering engine by Dominik Scholz (MIT, commit `e10afe4`, package 0.5.2), with a tiny-skia
painter for the flat material. No gpui dependency: `serde`, `serde_json`, `tiny-skia 0.11.4`.

Frames equal the TypeScript engine's: every number within 1e-6 and every SVG path text byte for
byte, checked by golden tests generated from the upstream source itself (see *Golden tests*).

## Layout

```text
src/
  lib.rs        re-exports (the public API below)
  jsmath.rs     JS number semantics: fdlibm sin/cos/atan2/exp/cbrt/acos as V8 ships them,
                V8's Math.hypot, Math.round, Number#toString, the renderer's f()
  path.rs       path.ts: parser, serializer, flattener, winding, EdgeBands
  contour.rs    contour.ts: marching squares, RDP, loopsToPath (exact `d` + numeric commands)
  distance.rs   distance.ts: EdgeIndex
  volume.rs     volume.ts: buildVolume, faceRoom, boostedEyeScale
  validate.rs   types.ts + validate.ts: AvatarDefinition, validate_avatar, analyze_body, gaze_range
  seams.rs      seams.ts (strict-only check)
  color.rs      color.ts
  eyes.rs       eyes.ts
  marks.rs      marks.ts (AvatarMark trait, the dots and the exclamation mark)
  effects.rs    effects.ts: ribbons, rims, mark placement, body scale
  motion.rs     motion.ts: poses, the six states, moods, blending, custom states, working-compact
  animator.rs   mood.ts + follow.ts + animator.ts
  render.rs     render.ts: PreparedAvatar, avatarFrame / liveFrame / liveTransform, extent, palette
  paint.rs      dom/paint.ts + dom/eye.ts (flat): paint, CanvasSurface (fitSquare, live reuse)
  geom.rs       matrices, path commands, Outline (d text + numbers + cached tiny-skia path)
  examples.rs   examples.ts (7 starters)
  gallery.rs    gallery/*.json (16 avatars + colors.json), solar/*.json (10 Solar avatars)
gallery/        upstream gallery definitions (MIT)
solar/          the Solar avatars and their MIT notice
upstream/       verbatim upstream engine source (packages/core/src minus dom/), LICENSE, README
tests/golden.rs        golden tests (frames, volumes, validation, strict checks, animator)
tests/raster.rs        painter sanity tests, PNGs in target/tmp/bot-avatar/
tests/perf.rs          timing (ignored)
tests/canvas_compare.rs painter vs browser canvas (ignored, see tests/golden/canvas/README.md)
tests/golden/gen.mjs   fixture generator (runs the upstream engine with Node)
tests/golden/fixtures/ the fixtures
```

## Public API

```rust
use bot_avatar::*;

// Definitions
let def: AvatarDefinition = solar_avatar("frog").unwrap().clone(); // or example("mug"), gallery_avatar("cat")
let normalized = validate_avatar(&serde_json::json!({ /* untrusted JSON */ }));   // Result<AvatarDefinition, Vec<String>>
let normalized = validate_definition(&def);

// Per (definition, depth): validate, build the volume, keep it.
let avatar = PreparedAvatar::new(&def, Some(6.0))?;   // depth 0..14, default 6

// Per frame.
let mut options = RenderOptions {
    color: Some("#1fbf68".into()),
    state: Some(AvatarState::Working.into()),        // or working_compact(), or State::Custom(..)
    mood: Some(AvatarMood::Neutral),
    time: Some(seconds),
    size: Some(28.0),                                 // CSS px: live frames skip sub-pixel slices
    ..RenderOptions::default()                        // tuning: Tuning::Solar by default
};
let step = animator.step(&AnimatorInput { now: ms, state, mood, target, hold, reduced_motion, ..Default::default() });
step.apply(&mut options);                             // gaze, motion_blend, state_age
let frame: AvatarFrame = avatar.frame(&options);      // liveFrame
let frame: AvatarFrame = avatar.avatar_frame(&options); // avatarFrame (every slice)
let t = avatar.live_transform(&options);              // liveTransform; t.build_frame()

// Painting (premultiplied RGBA tiny_skia::Pixmap).
let pixmap = paint(&frame, 112);                      // viewBox 0..100 onto 112 px, root baked in
paint_into(&mut pixmap, &frame, &PaintParams { scale, left, top, fill, ink, delegate_root });
let mut surface = CanvasSurface::new(&avatar);        // the browser canvas's layout and reuse
let out = surface.paint_live(&t, 28.0, 2.0, None, None); // out.reused, out.root (CSS-px matrix)
let pixmap = surface.pixmap();
```

Main types:

- `AvatarDefinition { version, id, name, body: AvatarBody { path }, eyes: AvatarEyes { x, y, spacing, scale } }`
- `RenderOptions { color, eye_color, size, depth, view: Option<View>, time, gaze: Option<Gaze>, state: Option<State>, mood, motion, state_age, motion_blend: Option<MotionBlend>, reduced_motion, tuning }`
- `AvatarFrame { name, color, ink, paths: Vec<FramePath>, root: Matrix, shapes: Vec<FrameShape>, surface }`,
  shapes back to front:
  `FrameShape::Slice { key, path /* index into paths */, matrix }`,
  `FrameShape::Eye { key, matrix, mark: EyeMark::{Pill, Arc, Path} }`,
  `FrameShape::Ribbon(RibbonShape { points, bbox, matrix, front, fill, gradient })`.
  Geometry is numeric (`PathCmd` lists, point lists); `FramePath::d()`, `EyeMark::d()`,
  `RibbonShape::d()` and `geom::path_d` give the exact TS text. `Matrix` is `[a, b, c, d, e, f]`.
- `Animator` (`createAnimator`): `step(&AnimatorInput) -> AnimatorStep { gaze, motion_blend, state_age, moving }`, `reset()`.
- Free functions as in render.ts, with a per-thread cache of 64 prepared avatars:
  `avatar_frame(&def, &opts)`, `live_frame`, `avatar_extent`, `prepare`.
- Lower level: `compute_pose`, `blend_poses`, `pointer_gaze`, `follow_step`, `settle_mood`,
  `volume::build_volume`, `validate::analyze_body`, `seams::silhouette_seams`, `volume::face_room`.
- Extension points (experimental upstream): `StateDefinition` (custom states, `PosePatch`) and
  `AvatarMark` (custom marks).

## Solar tuning and working-compact

`RenderOptions::tuning` defaults to `Tuning::Solar`, the art direction's ribbon colours
(04-maquettes/explorations/open-bot-avatar/direction-animation/DIRECTION.md). It changes
exactly three things in `ribbonPalette`:

```
NEUTRAL_TINT = 0.025                                        → 0
h   = turnToward(base, step > 0 ? WARM : WARM + 180, …)     → step > 0 ? turnToward(base, WARM, …) : base
hue = turnToward(h, away > 0 ? WARM : WARM + 180, …)        → away > 0 ? turnToward(h, WARM, …) : h
```

`Tuning::Upstream` is the published engine; the golden tests use it. The eye ink defaults to
`#151515` (`DEFAULT_EYE_COLOR`) whatever the body; `color::auto_eye_color` is ported but never
used by the renderer. `working_compact()` is the `working` pose with `ribbons: 0`, a custom
state with id `working-compact`, for avatars under 28 pt.

## Golden tests

```sh
node crates/bot-avatar/tests/golden/gen.mjs        # regenerate tests/golden/fixtures (Node 22.18+)
cd crates/solar && cargo test -p bot-avatar         # LIBRARY_PATH/LD_LIBRARY_PATH as for the app
```

The fixtures hold, for 7 examples, 16 gallery and 10 Solar avatars: validation and
normalization, body analysis, faceRoom and seams, the volume at depths 0, 2.3, 6 and 14 (every
slice's level, inset, box and path text), and 1,431 frames (avatarFrame and liveFrame) over
states × moods × times, done and working ages, gazes, depths, views, reduced motion, motion
blends, colours and live sizes, plus the pose of each; and four `createAnimator` sequences with
their frames. Path texts are stored as a 64-bit hash plus length (the first frame's in full).
Within a run of parallel slices, the fixture keeps every slice's path, part and index, both
ends' translation and checksums of all translations.

Ignored tests: `perf` (timing, `--release -- --ignored --nocapture`) and `canvas_compare`
(needs `OBA_CANVAS_DIR` and a browser, see `tests/golden/canvas/README.md`).

## Deviations from the TypeScript

- The browser canvas antialiases with Skia; tiny-skia's coverage differs by a few levels along
  edges (measured: mean 0.6–2.2 / 255 over painted pixels, no pixel over 96).
- Arcs (marks, arc segments of body paths) are drawn as cubic Béziers where Skia uses conics.
- `pointer_gaze` uses the platform `tanh` (V8 uses fdlibm; they differ by one ulp on about one
  argument in ten thousand). `Math.sin`/`cos` beyond |x| ≈ 8.2e5 rad fall back to the platform
  libm (fdlibm's large-argument reduction is not ported); clocks stay far below that.
- Unknown-field errors of `validate_avatar` are listed in `serde_json`'s key order (sorted
  unless `preserve_order` is on), where the TS lists them in source order.
- Custom marks give numeric outlines; their `d` is written with `f()` rather than echoed.
- Not ported: the fluffy and glass materials, `renderAvatarSvg`, `renderAvatarScene`, `sceneFrame`.
