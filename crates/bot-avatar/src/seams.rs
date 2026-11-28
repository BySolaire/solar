//! Silhouette seams: small accidental steps where parts meet (port of `seams.ts`, the strict-only
//! check of `validate.sh --strict`).

use crate::jsmath;
use crate::path::{bounds, Point};

/// Which side of the body a seam is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeamSide {
    Left,
    Right,
    Top,
    Bottom,
}

/// One seam (`Seam`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seam {
    pub side: SeamSide,
    /// Where along the side the step sits (y for left/right, x for top/bottom).
    pub at: f64,
    /// Step height: positive outward, negative inward.
    pub size: f64,
}

/// Steps below this are invisible (`SEAM_MIN`).
pub const SEAM_MIN: f64 = 0.3;
/// Steps at least this tall read as deliberate (`SEAM_MAX`).
pub const SEAM_MAX: f64 = 2.5;
const STEP: f64 = 0.1;
const SUDDEN: f64 = 2.5;
const CONTINUES: f64 = 0.6;
const END_SKIP: f64 = 1.0;

/// Filled intervals of the line y = s under the nonzero rule (`spans`).
fn spans(polys: &[Vec<Point>], s: f64) -> Vec<[f64; 2]> {
    let mut xs: Vec<(f64, i32)> = Vec::new();
    for poly in polys {
        let n = poly.len();
        let mut j = n.wrapping_sub(1);
        for i in 0..n {
            let [ax, ay] = poly[j];
            let [bx, by] = poly[i];
            if (ay <= s && by > s) || (by <= s && ay > s) {
                xs.push((
                    ax + ((s - ay) * (bx - ax)) / (by - ay),
                    if by > ay { 1 } else { -1 },
                ));
            }
            j = i;
        }
    }
    xs.sort_by(|m, n| m.0.partial_cmp(&n.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut out = Vec::new();
    let (mut w, mut start) = (0, 0.0);
    for (x, d) in xs {
        let was = w;
        w += d;
        if was == 0 && w != 0 {
            start = x;
        } else if was != 0 && w == 0 {
            out.push([start, x]);
        }
    }
    out
}

fn profile(polys: &[Vec<Point>], lo: f64, hi: f64, low: bool) -> (Vec<f64>, Vec<f64>) {
    let (mut at, mut e) = (Vec::new(), Vec::new());
    let mut s = lo + STEP / 2.0;
    while s < hi {
        let sp = spans(polys, s);
        at.push(s);
        e.push(match (sp.first(), sp.last()) {
            (Some(first), Some(last)) => {
                if low {
                    -first[0]
                } else {
                    last[1]
                }
            }
            _ => f64::NAN,
        });
        s += STEP;
    }
    (at, e)
}

fn steps_in(at: &[f64], e: &[f64]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut k = 0;
    while k < e.len() {
        if e[k].is_nan() {
            k += 1;
            continue;
        }
        let a = k;
        while k < e.len() && !e[k].is_nan() {
            k += 1;
        }
        let b = k - 1;
        let d = |i: usize| e[i + 1] - e[i];
        let mut i = a;
        while i < b {
            if d(i).abs() <= SUDDEN * STEP {
                i += 1;
                continue;
            }
            let sign = jsmath::sign(d(i));
            let (mut lo, mut hi) = (i, i);
            while lo > a && jsmath::sign(d(lo - 1)) == sign && d(lo - 1).abs() > CONTINUES * STEP {
                lo -= 1;
            }
            while hi + 1 < b
                && jsmath::sign(d(hi + 1)) == sign
                && d(hi + 1).abs() > CONTINUES * STEP
            {
                hi += 1;
            }
            let size = e[hi + 1] - e[lo];
            let touches_end =
                ((lo - a) as f64) * STEP < END_SKIP || ((b - hi - 1) as f64) * STEP < END_SKIP;
            if !touches_end && size.abs() >= SEAM_MIN && size.abs() < SEAM_MAX {
                out.push((at[i], jsmath::round(size * 100.0) / 100.0));
            }
            i = hi + 1;
        }
    }
    out
}

/// Accidental steps in the body's outer silhouette (`silhouetteSeams`).
pub fn silhouette_seams(polys: &[Vec<Point>]) -> Vec<Seam> {
    let b = bounds(polys);
    let cols: Vec<Vec<Point>> = polys
        .iter()
        .map(|p| p.iter().map(|&[x, y]| [y, x]).collect())
        .collect();
    let mut seams = Vec::new();
    let mut add = |side: SeamSide, (at, e): (Vec<f64>, Vec<f64>)| {
        for (at, size) in steps_in(&at, &e) {
            seams.push(Seam { side, at, size });
        }
    };
    add(SeamSide::Left, profile(polys, b.min_y, b.max_y, true));
    add(SeamSide::Right, profile(polys, b.min_y, b.max_y, false));
    add(SeamSide::Top, profile(&cols, b.min_x, b.max_x, true));
    add(SeamSide::Bottom, profile(&cols, b.min_x, b.max_x, false));
    seams
}
