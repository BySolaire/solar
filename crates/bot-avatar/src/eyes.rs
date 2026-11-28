//! The outline of one eye for any look (port of `eyes.ts`). The pill, the happy arc and the slit
//! are each walked as a closed outline from the top centre, clockwise, at even steps; mixing them
//! point by point gives every shape in between.

use crate::geom::PathCmd;
use crate::jsmath::{self, atan2, clamp, cos, hypot, sin};
use crate::motion::EyeLook;
use crate::path::Point;
use crate::validate::happy_arc;

const PI: f64 = std::f64::consts::PI;

/// Where one eye is drawn this frame, in its face plane (`EyeSpot`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EyeSpot {
    /// Centre, gaze included.
    pub x: f64,
    pub y: f64,
    /// Neutral size.
    pub w: f64,
    pub h: f64,
    /// Height of the pill after blink and squint, and how far that moves it down.
    pub open: f64,
    pub shift: f64,
}

/// Points per outline.
const STEPS: usize = 40;
const SLIT_LENGTH: f64 = 1.9;
const SLIT_THICK: f64 = 0.5;
const SLIT_TIP: f64 = 0.2;

/// `count` points at even steps along a closed polyline (`resample`).
fn resample(pts: &[Point], count: usize) -> Vec<Point> {
    let n = pts.len();
    let mut at = Vec::with_capacity(n + 1);
    at.push(0.0f64);
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        at.push(at[i] + hypot(b[0] - a[0], b[1] - a[1]));
    }
    let total = at[n];
    let mut out = Vec::with_capacity(count);
    let mut i = 0;
    for k in 0..count {
        let s = (k as f64 / count as f64) * total;
        while i < n - 1 && at[i + 1] < s {
            i += 1;
        }
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let u = if at[i + 1] > at[i] {
            (s - at[i]) / (at[i + 1] - at[i])
        } else {
            0.0
        };
        out.push([a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u]);
    }
    out
}

/// An arc of a circle as points, ends included (`sweep`).
fn sweep(cx: f64, cy: f64, r: f64, from: f64, to: f64, steps: usize, out: &mut Vec<Point>) {
    for i in 0..=steps {
        let a = from + ((to - from) * i as f64) / steps as f64;
        out.push([cx + cos(a) * r, cy + sin(a) * r]);
    }
}

/// The pill: a stadium about (cx, cy), from the top centre, clockwise (`pill`).
fn pill(cx: f64, cy: f64, w: f64, h: f64) -> Vec<Point> {
    let r = jsmath::min(w, h) / 2.0;
    let fx = w / 2.0 - r;
    let fy = h / 2.0 - r;
    let q = PI / 2.0;
    let mut pts = vec![[cx, cy - h / 2.0]];
    sweep(cx + fx, cy - fy, r, -q, 0.0, 6, &mut pts);
    sweep(cx + fx, cy + fy, r, 0.0, q, 6, &mut pts);
    sweep(cx - fx, cy + fy, r, q, 2.0 * q, 6, &mut pts);
    sweep(cx - fx, cy - fy, r, 2.0 * q, 3.0 * q, 6, &mut pts);
    pts
}

/// The happy arc as an outline (`arc`).
fn arc(e: &EyeSpot) -> Vec<Point> {
    let hw = e.w * happy_arc::HALF_WIDTH;
    let base = e.y + e.h * happy_arc::BASE;
    let peak = e.y - e.h * happy_arc::PEAK;
    let r = (e.w * happy_arc::STROKE) / 2.0;
    let at = |s: f64| -> (Point, Point) {
        let m = 1.0 - s;
        let x = e.x + hw * (2.0 * s - 1.0);
        let y = (m * m + s * s) * base + 2.0 * m * s * peak;
        let tx = 2.0 * hw;
        let ty = 2.0 * (2.0 * s - 1.0) * (base - peak);
        let len = hypot(tx, ty);
        ([x, y], [ty / len, -tx / len])
    };
    let edge = |from: f64, to: f64, side: f64, out: &mut Vec<Point>| {
        for i in 0..=8 {
            let (p, n) = at(from + ((to - from) * i as f64) / 8.0);
            out.push([p[0] + n[0] * r * side, p[1] + n[1] * r * side]);
        }
    };
    let end = |s: f64, out: &mut Vec<Point>| {
        let (p, n) = at(s);
        let up = atan2(n[1], n[0]);
        if s == 1.0 {
            sweep(p[0], p[1], r, up, up + PI, 8, out);
        } else {
            sweep(p[0], p[1], r, up + PI, up + 2.0 * PI, 8, out);
        }
    };
    let mut pts = Vec::with_capacity(80);
    edge(0.5, 1.0, 1.0, &mut pts);
    end(1.0, &mut pts);
    edge(1.0, 0.0, -1.0, &mut pts);
    end(0.0, &mut pts);
    edge(0.0, 0.5, 1.0, &mut pts);
    pts
}

/// The slit: a thin capsule lying almost flat (`slit`).
fn slit(e: &EyeSpot) -> Vec<Point> {
    let length = e.w * SLIT_LENGTH;
    let thick = e.w * SLIT_THICK * clamp(e.open / e.h, 0.3, 1.0);
    let c = cos(SLIT_TIP);
    let s = sin(SLIT_TIP);
    pill(0.0, 0.0, length, thick)
        .into_iter()
        .map(|[x, y]| [e.x + x * c - y * s, e.y + x * s + y * c])
        .collect()
}

/// Which plain shape a look is exactly (`plainLook`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlainLook {
    Pill,
    Arc,
}

/// Whether a look is exactly the plain pill or exactly the happy arc (`plainLook`).
pub fn plain_look(look: EyeLook) -> Option<PlainLook> {
    if look.arc >= 1.0 {
        return Some(PlainLook::Arc);
    }
    if look.arc <= 0.0 && look.slit <= 0.0 {
        Some(PlainLook::Pill)
    } else {
        None
    }
}

/// The outline of one eye, closed, clockwise from the top (`eyeOutline` in eyes.ts).
pub fn eye_outline(look: EyeLook, e: &EyeSpot) -> Vec<Point> {
    let a = clamp(look.arc, 0.0, 1.0);
    let s = clamp(look.slit, 0.0, 1.0 - a);
    let mut out = vec![[0.0f64, 0.0f64]; STEPS];
    let weights = [1.0 - a - s, a, s];
    for (k, &weight) in weights.iter().enumerate() {
        if weight <= 0.0 {
            continue;
        }
        let shape = match k {
            0 => pill(e.x, e.y + e.shift, e.w, e.open),
            1 => arc(e),
            _ => slit(e),
        };
        for (i, [x, y]) in resample(&shape, STEPS).into_iter().enumerate() {
            out[i][0] += x * weight;
            out[i][1] += y * weight;
        }
    }
    out
}

/// A smooth closed path through an outline: a quadratic per point joined at the midpoints, as
/// absolute commands (`outlinePath`; its text is `geom::path_d` of these).
pub fn outline_cmds(pts: &[Point]) -> Vec<PathCmd> {
    let n = pts.len();
    let mid = |i: usize| -> Point {
        let a = pts[i % n];
        let b = pts[(i + 1) % n];
        [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
    };
    let start = mid(n - 1);
    let mut cmds = Vec::with_capacity(n + 2);
    cmds.push(PathCmd::M {
        x: start[0],
        y: start[1],
    });
    for (i, p) in pts.iter().enumerate() {
        let m = mid(i);
        cmds.push(PathCmd::Q {
            x1: p[0],
            y1: p[1],
            x: m[0],
            y: m[1],
        });
    }
    cmds.push(PathCmd::Z);
    cmds
}

/// The path of one eye for a look that is neither plain shape (`eyePath`), as commands.
pub fn eye_path_cmds(look: EyeLook, e: &EyeSpot) -> Vec<PathCmd> {
    outline_cmds(&eye_outline(look, e))
}

/// `eyePath`: the SVG text.
pub fn eye_path(look: EyeLook, e: &EyeSpot) -> String {
    crate::geom::path_d(&eye_path_cmds(look, e))
}
