//! The canvas painter for flat frames (port of `dom/paint.ts`, `dom/eye.ts` and the flat path
//! of `dom/materials/marks.ts`), on tiny-skia.
//!
//! - [`paint`] / [`paint_into`]: one frame into a pixmap, with the root motion baked in
//!   (`paintCanvas`), at a scale and offset you choose.
//! - [`CanvasSurface`]: the browser canvas's bookkeeping. The bitmap covers what frames actually
//!   paint plus slack (`fitSquare`, capped by `avatarExtent`), aligned on whole device pixels,
//!   and only grows while size and pixel ratio stay the same. [`CanvasSurface::paint_live`]
//!   is `paintLiveCanvas`: without ribbons or marks it keeps the bitmap in the body's own frame
//!   and returns the root motion for the compositor to apply, reusing the bitmap while the
//!   view and eyes move less than a fraction of a pixel.
//!
//! Every shape is filled with antialiasing, as a 2D canvas does: slices in the body colour with
//! their fill rule (evenodd for contour slices, nonzero for the core and marks), eyes in the ink
//! (pill: `roundRect`; arc: a stroke of its width with round caps; path: a nonzero fill), ribbon
//! pieces with their linear gradient (three stops at 0, ½, 1, or evenly spaced), in the frame's
//! back-to-front order. A mark's part is a plain fill in the flat material. The paths are the ones
//! a browser parses from the frame's `d` text (numbers rounded as written), except ribbon pieces,
//! which the TS painter also draws from their numeric points.

use tiny_skia::{
    Color, FillRule, GradientStop, LineCap, LinearGradient, Paint, PathBuilder, Pixmap,
    Point as SkPoint, Shader, SpreadMode, Stroke, Transform,
};

use crate::color::parse_hex;
use crate::geom::{round_rect_path, to_skia_path, BoxF, Matrix, IDENTITY};
use crate::jsmath::{self, hypot};
use crate::render::{
    AvatarFrame, EyeMark, FrameShape, LiveTransform, PreparedAvatar, Square, SurfaceEye, ViewBasis,
};

/// An axis-aligned box in viewBox units; empty when x0 > x1 (paint.ts `Bounds`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintBounds {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// Slack around a frame's bounds when the bitmap is laid out, viewBox units (`FIT_SLACK`).
pub const FIT_SLACK: f64 = 8.0;
/// Margin a frame keeps from the bitmap's edge for antialiasing, viewBox units (`FIT_PAD`).
pub const FIT_PAD: f64 = 1.0;

/// The square to lay the bitmap out over (`fitSquare`).
pub fn fit_square(prev: Option<Square>, b: PaintBounds, cap: Square) -> Square {
    if !(b.x0 <= b.x1 && b.y0 <= b.y1) {
        return prev.unwrap_or(cap);
    }
    if let Some(p) = prev {
        if b.x0 - FIT_PAD >= p.x
            && b.y0 - FIT_PAD >= p.y
            && b.x1 + FIT_PAD <= p.x + p.size
            && b.y1 + FIT_PAD <= p.y + p.size
        {
            return p;
        }
    }
    let (mut x0, mut y0, mut x1, mut y1) = (
        b.x0 - FIT_SLACK,
        b.y0 - FIT_SLACK,
        b.x1 + FIT_SLACK,
        b.y1 + FIT_SLACK,
    );
    if let Some(p) = prev {
        x0 = jsmath::min(x0, p.x);
        y0 = jsmath::min(y0, p.y);
        x1 = jsmath::max(x1, p.x + p.size);
        y1 = jsmath::max(y1, p.y + p.size);
    }
    let size = jsmath::max(x1 - x0, y1 - y0);
    let in_cap =
        b.x0 >= cap.x && b.y0 >= cap.y && b.x1 <= cap.x + cap.size && b.y1 <= cap.y + cap.size;
    if size >= cap.size && in_cap {
        return cap;
    }
    Square {
        x: (x0 + x1 - size) / 2.0,
        y: (y0 + y1 - size) / 2.0,
        size,
    }
}

/// The numbers of a path's text, as `eyeBox` reads them (`/-?\d+(?:\.\d+)?/g`).
fn numbers_of(d: &str) -> Vec<f64> {
    let b = d.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        let mut j = i;
        if b[j] == b'-' {
            j += 1;
        }
        let digits = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j == digits {
            i = start + 1;
            continue;
        }
        if j + 1 < b.len() && b[j] == b'.' && b[j + 1].is_ascii_digit() {
            j += 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
        }
        out.push(d[start..j].parse().unwrap_or(0.0));
        i = j;
    }
    out
}

/// An eye mark's bounds (`eyeBox`).
pub fn eye_box(mark: &EyeMark) -> BoxF {
    match mark {
        EyeMark::Pill {
            x,
            y,
            width,
            height,
            ..
        } => BoxF {
            x: *x,
            y: *y,
            w: *width,
            h: *height,
        },
        _ => {
            let reach = if let EyeMark::Arc { stroke_width, .. } = mark {
                *stroke_width
            } else {
                0.0
            };
            let n = numbers_of(&mark.d().unwrap_or_default());
            let xs = n.iter().step_by(2).copied();
            let ys = n.iter().skip(1).step_by(2).copied();
            let x = jsmath::min_of(xs.clone()) - reach;
            let y = jsmath::min_of(ys.clone()) - reach;
            BoxF {
                x,
                y,
                w: jsmath::max_of(xs) + reach - x,
                h: jsmath::max_of(ys) + reach - y,
            }
        }
    }
}

/// Each shape's matrix with `root` applied, and the bounds of everything painted (`placementWith`).
fn placement_with(frame: &AvatarFrame, root: &Matrix) -> (Vec<Matrix>, PaintBounds) {
    let [ra, rb, rc, rd, re, rf] = *root;
    let mut bounds = PaintBounds {
        x0: f64::INFINITY,
        y0: f64::INFINITY,
        x1: f64::NEG_INFINITY,
        y1: f64::NEG_INFINITY,
    };
    let mut add = |m: &Matrix, x: f64, y: f64| {
        let xx = m[0] * x + m[2] * y + m[4];
        let yy = m[1] * x + m[3] * y + m[5];
        if xx < bounds.x0 {
            bounds.x0 = xx;
        }
        if xx > bounds.x1 {
            bounds.x1 = xx;
        }
        if yy < bounds.y0 {
            bounds.y0 = yy;
        }
        if yy > bounds.y1 {
            bounds.y1 = yy;
        }
    };
    let matrices = frame
        .shapes
        .iter()
        .map(|shape| {
            let [a, b, c, d, e, f] = *shape.matrix();
            let m: Matrix = [
                ra * a + rc * b,
                rb * a + rd * b,
                ra * c + rc * d,
                rb * c + rd * d,
                ra * e + rc * f + re,
                rb * e + rd * f + rf,
            ];
            let bx = match shape {
                FrameShape::Slice { .. } => frame.path_of(shape).map(|p| p.bbox),
                FrameShape::Eye { mark, .. } => Some(eye_box(mark)),
                FrameShape::Ribbon(r) => Some(r.bbox),
            };
            if let Some(bx) = bx {
                add(&m, bx.x, bx.y);
                add(&m, bx.x + bx.w, bx.y);
                add(&m, bx.x, bx.y + bx.h);
                add(&m, bx.x + bx.w, bx.y + bx.h);
            }
            m
        })
        .collect();
    (matrices, bounds)
}

/// Each shape's matrix with the frame's root motion applied, and the painted bounds
/// (`framePlacement`).
pub fn frame_placement(frame: &AvatarFrame) -> (Vec<Matrix>, PaintBounds) {
    placement_with(frame, &frame.root)
}

/// How a frame is laid onto a pixmap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintParams {
    /// Device pixels per viewBox unit: `size / 100 * dpr`.
    pub scale: f64,
    /// Device pixel of the viewBox origin's left/top edge, subtracted from every x/y
    /// (`surface.left`, `surface.top`).
    pub left: f64,
    pub top: f64,
    /// Body fill, overriding `frame.color` (any colour, as RGBA 0..1).
    pub fill: Option<Color>,
    /// Eye ink, overriding `frame.ink`.
    pub ink: Option<Color>,
    /// Leave the root motion out (the compositor applies it, as paintLiveCanvas does).
    pub delegate_root: bool,
}

impl PaintParams {
    /// The viewBox 0..100 onto `size_px` pixels, origin at the pixmap's corner.
    pub fn square(size_px: f64) -> PaintParams {
        PaintParams {
            scale: size_px / 100.0,
            left: 0.0,
            top: 0.0,
            fill: None,
            ink: None,
            delegate_root: false,
        }
    }
}

fn hex_color(s: &str) -> Color {
    let [r, g, b] = parse_hex(s).unwrap_or([0.0, 0.0, 0.0]);
    Color::from_rgba8(r as u8, g as u8, b as u8, 255)
}

fn solid(c: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c);
    p.anti_alias = true;
    p
}

/// Paints `frame` into `pixmap` (the flat branch of `paintFrame`, without the bitmap layout:
/// see [`CanvasSurface`] for that). Returns the matrices used, root included.
pub fn paint_into(pixmap: &mut Pixmap, frame: &AvatarFrame, params: &PaintParams) {
    let scale = params.scale;
    if scale <= 0.0 || !scale.is_finite() {
        return;
    }
    let fill = params.fill.unwrap_or_else(|| hex_color(&frame.color));
    let ink = params.ink.unwrap_or_else(|| hex_color(&frame.ink));
    let paint_root = if params.delegate_root {
        IDENTITY
    } else {
        frame.root
    };
    let (matrices, _) = placement_with(frame, &paint_root);
    let fill_paint = solid(fill);
    let ink_paint = solid(ink);
    for (shape, m) in frame.shapes.iter().zip(&matrices) {
        let ts = Transform::from_row(
            (scale * m[0]) as f32,
            (scale * m[1]) as f32,
            (scale * m[2]) as f32,
            (scale * m[3]) as f32,
            (scale * m[4] - params.left) as f32,
            (scale * m[5] - params.top) as f32,
        );
        match shape {
            FrameShape::Slice { .. } => {
                // Body slices and (flat) mark parts alike: `ctx.fill(section.path, section.rule)`.
                let Some(section) = frame.path_of(shape) else {
                    continue;
                };
                if let Some(path) = section.outline.skia_path() {
                    let rule = if section.even_odd {
                        FillRule::EvenOdd
                    } else {
                        FillRule::Winding
                    };
                    pixmap.fill_path(path, &fill_paint, rule, ts, None);
                }
            }
            FrameShape::Eye { mark, .. } => paint_eye(pixmap, mark, &ink_paint, ts),
            FrameShape::Ribbon(r) => {
                let g = &r.gradient;
                let n = g.stops.len();
                let stops: Vec<GradientStop> = if n == 3 {
                    vec![
                        GradientStop::new(0.0, hex_color(&g.stops[0])),
                        GradientStop::new(0.5, hex_color(&g.stops[1])),
                        GradientStop::new(1.0, hex_color(&g.stops[2])),
                    ]
                } else {
                    g.stops
                        .iter()
                        .enumerate()
                        .map(|(i, c)| {
                            GradientStop::new(
                                if n > 1 {
                                    i as f32 / (n - 1) as f32
                                } else {
                                    0.0
                                },
                                hex_color(c),
                            )
                        })
                        .collect()
                };
                let shader = LinearGradient::new(
                    SkPoint::from_xy(g.x1 as f32, g.y1 as f32),
                    SkPoint::from_xy(g.x2 as f32, g.y2 as f32),
                    stops,
                    SpreadMode::Pad,
                    Transform::identity(),
                )
                .unwrap_or(Shader::SolidColor(hex_color(&r.fill)));
                let paint = Paint {
                    shader,
                    anti_alias: true,
                    ..Paint::default()
                };
                // ribbonPointsOf: moveTo, lineTo…, closePath, fill() (nonzero).
                let mut pb = PathBuilder::new();
                if let Some(first) = r.points.first() {
                    pb.move_to(first[0] as f32, first[1] as f32);
                    for p in &r.points[1..] {
                        pb.line_to(p[0] as f32, p[1] as f32);
                    }
                    pb.close();
                }
                if let Some(path) = pb.finish() {
                    pixmap.fill_path(&path, &paint, FillRule::Winding, ts, None);
                }
            }
        }
    }
}

/// Fill or stroke an eye mark in the ink (`paintEye`).
fn paint_eye(pixmap: &mut Pixmap, mark: &EyeMark, ink: &Paint, ts: Transform) {
    match mark {
        EyeMark::Path { cmds } => {
            if let Some(path) = to_skia_path(cmds, true) {
                pixmap.fill_path(&path, ink, FillRule::Winding, ts, None);
            }
        }
        EyeMark::Arc { cmds, stroke_width } => {
            if let Some(path) = to_skia_path(cmds, true) {
                let stroke = Stroke {
                    width: *stroke_width as f32,
                    line_cap: LineCap::Round,
                    ..Stroke::default()
                };
                pixmap.stroke_path(&path, ink, &stroke, ts, None);
            }
        }
        EyeMark::Pill {
            x,
            y,
            width,
            height,
            radius,
        } => {
            if let Some(path) = round_rect_path(*x, *y, *width, *height, *radius) {
                pixmap.fill_path(&path, ink, FillRule::Winding, ts, None);
            }
        }
    }
}

/// One frame onto a new `size_px` × `size_px` pixmap covering the viewBox 0..100, root motion
/// baked in. Shapes reaching outside the box are clipped; use [`CanvasSurface`] for the
/// browser's bitmap that grows to hold them.
pub fn paint(frame: &AvatarFrame, size_px: u32) -> Pixmap {
    let size = size_px.clamp(1, 16384);
    let mut pixmap =
        Pixmap::new(size, size).unwrap_or_else(|| Pixmap::new(1, 1).expect("1×1 pixmap"));
    paint_into(&mut pixmap, frame, &PaintParams::square(size as f64));
    pixmap
}

/// Max CSS-pixel surface shift below which a live bitmap is reused (`VIEW_EPS_CSS_PX`).
const VIEW_EPS_CSS_PX: f64 = 0.6;

#[derive(Clone, Debug)]
struct PaintedState {
    fill: Color,
    ink: Color,
    layout: (u64, u64),
    body_scale: f64,
    eyes_key: String,
    view: ViewBasis,
    rx: f64,
    ry: f64,
    rz: f64,
}

/// Exact upper bound on the screen shift of a box between two views (`viewShift`).
fn view_shift(a: &ViewBasis, b: &ViewBasis, rx: f64, ry: f64, rz: f64) -> f64 {
    let dx = rx * (a.c_y - b.c_y).abs() + rz * (a.s_y - b.s_y).abs();
    let dy = ry * (a.c_p - b.c_p).abs()
        + rx * (a.s_y * a.s_p - b.s_y * b.s_p).abs()
        + rz * (a.c_y * a.s_p - b.c_y * b.s_p).abs();
    hypot(dx, dy)
}

fn eye_mark_key(m: &EyeMark) -> String {
    let r = |v: f64| jsmath::number_to_string(jsmath::round(v * 20.0));
    match m {
        EyeMark::Pill {
            width,
            height,
            radius,
            ..
        } => format!("p:{},{},{}", r(*width), r(*height), r(*radius)),
        EyeMark::Arc { stroke_width, .. } => {
            format!("a:{}:{}", r(*stroke_width), m.d().unwrap_or_default())
        }
        EyeMark::Path { .. } => format!("d:{}", m.d().unwrap_or_default()),
    }
}

/// The key that tells whether the eyes changed (`eyesCacheKey`).
pub fn eyes_cache_key(eyes: &[SurfaceEye]) -> String {
    let r = |v: f64| jsmath::number_to_string(jsmath::round(v * 20.0));
    let mut key = String::new();
    for (i, e) in eyes.iter().enumerate() {
        if i > 0 {
            key.push('|');
        }
        key.push_str(&format!(
            "{},{},{}",
            r(e.dx),
            r(e.dy),
            eye_mark_key(&e.mark)
        ));
    }
    key
}

/// The root motion as the compositor applies it, in CSS pixels (`rootCss`): None for identity,
/// else `[a, b, c, d, e, f]` with the TS rounding (4 decimals, translation 2 decimals).
pub fn root_css(root: &Matrix, size: f64) -> Option<Matrix> {
    let [a, b, c, d, e, f] = *root;
    if a == 1.0 && b == 0.0 && c == 0.0 && d == 1.0 && e == 0.0 && f == 0.0 {
        return None;
    }
    let k = size / 100.0;
    let r4 = |n: f64| jsmath::round(n * 10000.0) / 10000.0;
    let r2 = |n: f64| jsmath::round(n * 100.0) / 100.0;
    Some([r4(a), r4(b), r4(c), r4(d), r2(e * k), r2(f * k)])
}

/// What a live paint did.
#[derive(Clone, Debug, PartialEq)]
pub struct LivePaint {
    /// The bitmap was kept as it was (only the root motion changed).
    pub reused: bool,
    /// The root motion the compositor applies to the bitmap, CSS pixels about the viewBox origin
    /// (None: none, or already baked into the bitmap).
    pub root: Option<Matrix>,
    /// `eyesCacheKey` of this frame.
    pub eyes_key: String,
}

/// A canvas and the square and size its bitmap was last laid out at (`CanvasSurface`).
pub struct CanvasSurface {
    cap: Square,
    fit: Option<Square>,
    layout: Option<(u64, u64)>,
    /// Device pixel of the viewBox origin's left and top edge.
    pub left: f64,
    pub top: f64,
    pixmap: Option<Pixmap>,
    painted: Option<PaintedState>,
    half_extent: (f64, f64, f64),
}

impl CanvasSurface {
    /// `mountCanvas`: a surface capped by the avatar's extent.
    pub fn new(avatar: &PreparedAvatar) -> CanvasSurface {
        let vol = avatar.volume();
        let rz = vol.parts.iter().fold(
            0.0f64,
            |m, p| if p.depth / 2.0 > m { p.depth / 2.0 } else { m },
        );
        CanvasSurface {
            cap: avatar.extent(),
            fit: None,
            layout: None,
            left: 0.0,
            top: 0.0,
            pixmap: None,
            painted: None,
            half_extent: (
                (vol.bounds.max_x - vol.bounds.min_x) / 2.0,
                (vol.bounds.max_y - vol.bounds.min_y) / 2.0,
                rz,
            ),
        }
    }

    /// The bitmap last painted.
    pub fn pixmap(&self) -> Option<&Pixmap> {
        self.pixmap.as_ref()
    }

    /// The square of viewBox units the bitmap is laid out over.
    pub fn fit(&self) -> Option<Square> {
        self.fit
    }

    /// `paintCanvas`: paint one frame `size` CSS pixels wide at `dpr`, root motion baked in.
    pub fn paint(
        &mut self,
        frame: &AvatarFrame,
        size: f64,
        dpr: f64,
        fill: Option<Color>,
        ink: Option<Color>,
    ) -> Option<&Pixmap> {
        self.paint_frame(frame, size, dpr, fill, ink, false, String::new());
        self.pixmap.as_ref()
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_frame(
        &mut self,
        frame: &AvatarFrame,
        size: f64,
        dpr: f64,
        fill: Option<Color>,
        ink: Option<Color>,
        delegate_root: bool,
        eyes_key: String,
    ) {
        if size <= 0.0 || !size.is_finite() {
            return;
        }
        let dpr = if dpr > 0.0 && dpr.is_finite() {
            dpr
        } else {
            1.0
        };
        let scale = (size / 100.0) * dpr;
        let layout = (size.to_bits(), dpr.to_bits());
        if !delegate_root {
            self.painted = None;
        }
        let paint_root = if delegate_root { IDENTITY } else { frame.root };
        let (_, bounds) = placement_with(frame, &paint_root);
        let prev = if self.layout == Some(layout) {
            self.fit
        } else {
            None
        };
        let fit = fit_square(prev, bounds, self.cap);
        if self.layout == Some(layout) && Some(fit) == self.fit && self.pixmap.is_some() {
            if let Some(p) = self.pixmap.as_mut() {
                p.fill(Color::TRANSPARENT);
            }
        } else {
            self.layout = Some(layout);
            self.fit = Some(fit);
            self.left = (fit.x * scale).floor();
            self.top = (fit.y * scale).floor();
            let bitmap = ((fit.size * scale).ceil() + 1.0).clamp(1.0, 16384.0) as u32;
            self.pixmap = Pixmap::new(bitmap, bitmap);
        }
        let fill_c = fill.unwrap_or_else(|| hex_color(&frame.color));
        let ink_c = ink.unwrap_or_else(|| hex_color(&frame.ink));
        if let Some(pixmap) = self.pixmap.as_mut() {
            let params = PaintParams {
                scale,
                left: self.left,
                top: self.top,
                fill: Some(fill_c),
                ink: Some(ink_c),
                delegate_root,
            };
            paint_into(pixmap, frame, &params);
        }
        if delegate_root && frame.surface.scale > 0.001 {
            let (rx, ry, rz) = self.half_extent;
            self.painted = Some(PaintedState {
                fill: fill_c,
                ink: ink_c,
                layout,
                body_scale: frame.surface.scale,
                eyes_key,
                view: frame.surface.view,
                rx,
                ry,
                rz,
            });
        }
    }

    /// `paintLiveCanvas` (flat): reuses the bitmap when nothing but the root motion changed,
    /// else builds the frame and paints it, in the body's own frame when it has no overlay.
    pub fn paint_live(
        &mut self,
        transform: &LiveTransform,
        size: f64,
        dpr: f64,
        fill: Option<Color>,
        ink: Option<Color>,
    ) -> LivePaint {
        let eyes_key = eyes_cache_key(&transform.eyes);
        if size <= 0.0 || !size.is_finite() {
            return LivePaint {
                reused: false,
                root: None,
                eyes_key,
            };
        }
        let dpr = if dpr > 0.0 && dpr.is_finite() {
            dpr
        } else {
            1.0
        };
        let layout = (size.to_bits(), dpr.to_bits());
        let fill_c = fill.unwrap_or_else(|| hex_color(&transform.color));
        let ink_c = ink.unwrap_or_else(|| hex_color(&transform.ink));
        if !transform.has_overlay {
            if let Some(prev) = &self.painted {
                if prev.fill == fill_c
                    && prev.ink == ink_c
                    && prev.layout == layout
                    && (prev.body_scale - 1.0).abs() < 1e-4
                    && prev.eyes_key == eyes_key
                {
                    let shift = view_shift(&transform.view, &prev.view, prev.rx, prev.ry, prev.rz)
                        * (size / 100.0);
                    if shift < VIEW_EPS_CSS_PX {
                        return LivePaint {
                            reused: true,
                            root: root_css(&transform.root, size),
                            eyes_key,
                        };
                    }
                }
            }
        }
        let frame = transform.build_frame();
        let delegate = !transform.has_overlay;
        self.paint_frame(
            &frame,
            size,
            dpr,
            Some(fill_c),
            Some(ink_c),
            delegate,
            eyes_key.clone(),
        );
        LivePaint {
            reused: false,
            root: if delegate {
                root_css(&frame.root, size)
            } else {
                None
            },
            eyes_key,
        }
    }
}
