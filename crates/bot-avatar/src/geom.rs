//! Plain geometry shared by the frame builder and the painter: boxes, 2D affine matrices (SVG
//! order `a b c d e f`, as render.ts's `Matrix`), numeric path commands and their SVG text.

use crate::jsmath;
pub use crate::path::Segment as PathCmd;

/// An axis-aligned box `{ x, y, w, h }`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoxF {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// 2D affine matrix in SVG order (a b c d e f): x' = a x + c y + e, y' = b x + d y + f.
pub type Matrix = [f64; 6];

/// The identity matrix.
pub const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `mul(m, n)`: m applied after n.
#[inline]
pub fn mul(m: &Matrix, n: &Matrix) -> Matrix {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}

/// `translate(x, y)`.
#[inline]
pub fn translate(x: f64, y: f64) -> Matrix {
    [1.0, 0.0, 0.0, 1.0, x, y]
}

/// `about(x, y, m)`: m about the point (x, y).
#[inline]
pub fn about(x: f64, y: f64, m: &Matrix) -> Matrix {
    mul(&translate(x, y), &mul(m, &translate(-x, -y)))
}

/// `invert`.
#[inline]
pub fn invert(m: &Matrix) -> Matrix {
    let [a, b, c, d, e, g] = *m;
    let det = a * d - b * c;
    [
        d / det,
        -b / det,
        -c / det,
        a / det,
        (c * g - d * e) / det,
        (b * e - a * g) / det,
    ]
}

/// `serializePath`-style SVG text of absolute commands: `M L C Q A Z`, every number through the
/// renderer's `f()` (2 decimals, -0 as 0). This is the format of the body's core slices, the
/// marks' outlines, the eye paths and arcs and the ribbon pieces in TS frames.
pub fn path_d(cmds: &[PathCmd]) -> String {
    crate::path::serialize_path(cmds)
}

/// Builds a tiny-skia path from absolute commands. With `rounded`, every number is first rounded
/// as `f()` writes it, so the path is the one a browser parses from the TS frame's `d` text.
/// Arcs become cubic Béziers (at most a quarter turn each), as SVG's endpoint parameterization.
pub fn to_skia_path(cmds: &[PathCmd], rounded: bool) -> Option<tiny_skia::Path> {
    let r = |v: f64| if rounded { jsmath::round2(v) } else { v };
    let mut pb = tiny_skia::PathBuilder::new();
    let (mut cx, mut cy) = (0.0f64, 0.0f64);
    let (mut sx, mut sy) = (0.0f64, 0.0f64);
    let mut open = false;
    for c in cmds {
        match *c {
            PathCmd::M { x, y } => {
                let (x, y) = (r(x), r(y));
                pb.move_to(x as f32, y as f32);
                cx = x;
                cy = y;
                sx = x;
                sy = y;
                open = true;
            }
            PathCmd::L { x, y } => {
                let (x, y) = (r(x), r(y));
                ensure_open(&mut pb, &mut open, cx, cy);
                pb.line_to(x as f32, y as f32);
                cx = x;
                cy = y;
            }
            PathCmd::Q { x1, y1, x, y } => {
                let (x1, y1, x, y) = (r(x1), r(y1), r(x), r(y));
                ensure_open(&mut pb, &mut open, cx, cy);
                pb.quad_to(x1 as f32, y1 as f32, x as f32, y as f32);
                cx = x;
                cy = y;
            }
            PathCmd::C {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (x1, y1, x2, y2, x, y) = (r(x1), r(y1), r(x2), r(y2), r(x), r(y));
                ensure_open(&mut pb, &mut open, cx, cy);
                pb.cubic_to(
                    x1 as f32, y1 as f32, x2 as f32, y2 as f32, x as f32, y as f32,
                );
                cx = x;
                cy = y;
            }
            PathCmd::A {
                rx,
                ry,
                rot,
                large,
                sweep,
                x,
                y,
            } => {
                let (rx, ry, rot, x, y) = (r(rx), r(ry), r(rot), r(x), r(y));
                ensure_open(&mut pb, &mut open, cx, cy);
                arc_to(&mut pb, cx, cy, rx, ry, rot, large, sweep, x, y);
                cx = x;
                cy = y;
            }
            PathCmd::Z => {
                if open {
                    pb.close();
                }
                open = false;
                cx = sx;
                cy = sy;
            }
        }
    }
    pb.finish()
}

fn ensure_open(pb: &mut tiny_skia::PathBuilder, open: &mut bool, x: f64, y: f64) {
    if !*open {
        pb.move_to(x as f32, y as f32);
        *open = true;
    }
}

/// An SVG endpoint arc from (x0, y0) as cubic Béziers (SVG implementation notes F.6.5, F.6.6).
#[allow(clippy::too_many_arguments)]
fn arc_to(
    pb: &mut tiny_skia::PathBuilder,
    x0: f64,
    y0: f64,
    rx: f64,
    ry: f64,
    rot: f64,
    large: bool,
    sweep: bool,
    x: f64,
    y: f64,
) {
    if x0 == x && y0 == y {
        return;
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 {
        pb.line_to(x as f32, y as f32);
        return;
    }
    let phi = rot.to_radians();
    let (sin, cos) = phi.sin_cos();
    let dx = (x0 - x) / 2.0;
    let dy = (y0 - y) / 2.0;
    let x1p = cos * dx + sin * dy;
    let y1p = -sin * dx + cos * dy;
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let k = lambda.sqrt();
        rx *= k;
        ry *= k;
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut coef = (num / den).max(0.0).sqrt();
    if large == sweep {
        coef = -coef;
    }
    let cxp = coef * rx * y1p / ry;
    let cyp = -coef * ry * x1p / rx;
    let ccx = cos * cxp - sin * cyp + (x0 + x) / 2.0;
    let ccy = sin * cxp + cos * cyp + (y0 + y) / 2.0;
    let ang = |ux: f64, uy: f64, vx: f64, vy: f64| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let t1 = ang(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut dt = ang(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    let tau = std::f64::consts::TAU;
    if !sweep && dt > 0.0 {
        dt -= tau;
    }
    if sweep && dt < 0.0 {
        dt += tau;
    }
    let segments = (dt.abs() / (std::f64::consts::FRAC_PI_2 + 1e-9))
        .ceil()
        .max(1.0) as usize;
    let delta = dt / segments as f64;
    let k = 4.0 / 3.0 * (delta / 4.0).tan();
    let point = |t: f64| {
        let (s, c) = t.sin_cos();
        (rx * c, ry * s)
    };
    let to_world = |px: f64, py: f64| (cos * px - sin * py + ccx, sin * px + cos * py + ccy);
    let mut t = t1;
    for i in 0..segments {
        let t2 = t + delta;
        let (ax, ay) = point(t);
        let (bx, by) = point(t2);
        // Derivatives of the ellipse (before rotation) at both ends.
        let (dax, day) = (-rx * t.sin(), ry * t.cos());
        let (dbx, dby) = (-rx * t2.sin(), ry * t2.cos());
        let (c1x, c1y) = to_world(ax + k * dax, ay + k * day);
        let (c2x, c2y) = to_world(bx - k * dbx, by - k * dby);
        let (ex, ey) = if i + 1 == segments {
            (x, y)
        } else {
            to_world(bx, by)
        };
        pb.cubic_to(
            c1x as f32, c1y as f32, c2x as f32, c2y as f32, ex as f32, ey as f32,
        );
        t = t2;
    }
}

/// A rounded rectangle path as `CanvasRenderingContext2D.roundRect(x, y, w, h, r)` traces it:
/// from (x + r, y) clockwise, quarter circles as cubics.
pub fn round_rect_path(x: f64, y: f64, w: f64, h: f64, r: f64) -> Option<tiny_skia::Path> {
    // The canvas scales radii down to fit; the engine's pills always fit (r = min(w, h) / 2).
    let r = r.max(0.0).min(w.abs() / 2.0).min(h.abs() / 2.0);
    let k = 0.552_284_749_830_793_4 * r;
    let mut pb = tiny_skia::PathBuilder::new();
    let f = |v: f64| v as f32;
    pb.move_to(f(x + r), f(y));
    pb.line_to(f(x + w - r), f(y));
    pb.cubic_to(
        f(x + w - r + k),
        f(y),
        f(x + w),
        f(y + r - k),
        f(x + w),
        f(y + r),
    );
    pb.line_to(f(x + w), f(y + h - r));
    pb.cubic_to(
        f(x + w),
        f(y + h - r + k),
        f(x + w - r + k),
        f(y + h),
        f(x + w - r),
        f(y + h),
    );
    pb.line_to(f(x + r), f(y + h));
    pb.cubic_to(
        f(x + r - k),
        f(y + h),
        f(x),
        f(y + h - r + k),
        f(x),
        f(y + h - r),
    );
    pb.line_to(f(x), f(y + r));
    pb.cubic_to(f(x), f(y + r - k), f(x + r - k), f(y), f(x + r), f(y));
    pb.close();
    pb.finish()
}

/// One outline as a frame lists it: the SVG text exactly as the TS writes it, the same outline as
/// numeric commands, and (built on first use, then kept) the tiny-skia path the painter fills.
pub struct Outline {
    /// The SVG path text, byte for byte the TS `d`.
    pub d: String,
    /// The outline as absolute commands (the numbers `d` encodes, before `f()` rounding).
    pub cmds: Vec<PathCmd>,
    skia: std::sync::OnceLock<Option<tiny_skia::Path>>,
}

impl Outline {
    /// An outline from its commands, written with [`path_d`].
    pub fn from_cmds(cmds: Vec<PathCmd>) -> Outline {
        Outline {
            d: path_d(&cmds),
            cmds,
            skia: std::sync::OnceLock::new(),
        }
    }

    /// An outline whose text was written by another writer (the volume's contour paths).
    pub fn with_d(d: String, cmds: Vec<PathCmd>) -> Outline {
        Outline {
            d,
            cmds,
            skia: std::sync::OnceLock::new(),
        }
    }

    /// The path a browser parses from `d` (numbers rounded as written), built once.
    pub fn skia_path(&self) -> Option<&tiny_skia::Path> {
        self.skia
            .get_or_init(|| to_skia_path(&self.cmds, true))
            .as_ref()
    }
}

impl std::fmt::Debug for Outline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Outline").field("d", &self.d).finish()
    }
}

impl PartialEq for Outline {
    fn eq(&self, other: &Self) -> bool {
        self.d == other.d
    }
}
