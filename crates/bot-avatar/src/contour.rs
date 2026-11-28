//! Iso-contours of a sampled scalar field (marching squares), assembled into closed loops and
//! simplified (port of `contour.ts`). Used once per definition to turn a part's depth field into
//! plain cross-section paths.
//!
//! The TS keeps its scratch arrays in module globals; here they live in a [`Tracer`] the volume
//! builder owns. Every call leaves them as it found them, as in the TS, so results never depend
//! on earlier calls.

use crate::jsmath;
use crate::path::{Point, Segment};

/// A sampled field (`Field`): row-major `values`, `nx * ny`.
pub struct Field<'a> {
    pub values: &'a [f64],
    pub nx: usize,
    pub ny: usize,
    pub x0: f64,
    pub y0: f64,
    pub step: f64,
}

/// Cells per run of a row that a caller can rule out at once (`RUN`).
pub const RUN: usize = 8;

/// Runs in a row of cells of a field `nx` samples wide (`runsOf`).
pub fn runs_of(nx: usize) -> usize {
    (nx - 1).div_ceil(RUN)
}

/// Smallest and largest sample around each run of cells (`runRange`).
pub fn run_range(values: &[f64], nx: usize, ny: usize) -> (Vec<f64>, Vec<f64>) {
    let runs = runs_of(nx);
    let mut lo = vec![0.0f64; runs * ny];
    let mut hi = vec![0.0f64; runs * ny];
    for j in 0..ny {
        for r in 0..runs {
            let (mut a, mut b) = (f64::INFINITY, f64::NEG_INFINITY);
            let end = j * nx + (nx - 1).min((r + 1) * RUN);
            for &v in &values[j * nx + r * RUN..=end] {
                if v < a {
                    a = v;
                }
                if v > b {
                    b = v;
                }
            }
            lo[j * runs + r] = a;
            hi[j * runs + r] = b;
        }
    }
    let len = runs * ny.saturating_sub(1);
    let mut min = vec![0.0f64; len];
    let mut max = vec![0.0f64; len];
    for k in 0..len {
        min[k] = jsmath::min(lo[k], lo[k + runs]);
        max[k] = jsmath::max(hi[k], hi[k + runs]);
    }
    (min, max)
}

/// Scratch for tracing and simplifying, reused between calls.
#[derive(Default)]
pub struct Tracer {
    link_a: Vec<i32>,
    link_b: Vec<i32>,
    passed: Vec<u8>,
    order: Vec<i32>,
    traced: Vec<f64>,
    ends: Vec<usize>,
    kept: Vec<u8>,
    chains: Vec<usize>,
}

/// A comparison that plain products decide by more than this, relative, comes out the same
/// through `Math.hypot` or `Math.atan2` (`CLEAR`).
pub const CLEAR: f64 = 1e-9;

impl Tracer {
    pub fn new() -> Tracer {
        Tracer::default()
    }

    /// Traces the loops where the field crosses `level` into `traced` and `ends` (`trace`).
    fn trace(&mut self, field: &Field, level: f64, flat: Option<&[u8]>) -> usize {
        let Field {
            values,
            nx,
            ny,
            x0,
            y0,
            step,
        } = *field;
        if self.link_a.len() < 2 * nx * ny {
            self.link_a = vec![-1; 2 * nx * ny];
            self.link_b = vec![-1; 2 * nx * ny];
            self.passed = vec![0; 2 * nx * ny];
        }
        self.order.clear();
        let (a, b, order) = (&mut self.link_a, &mut self.link_b, &mut self.order);
        let mut join = |p: usize, q: usize| {
            if a[p] < 0 {
                order.push(p as i32);
                a[p] = q as i32;
            } else {
                b[p] = q as i32;
            }
        };
        let mut link = |p: usize, q: usize| {
            join(p, q);
            join(q, p);
        };

        let runs = runs_of(nx);
        for j in 0..ny - 1 {
            let row = j * nx;
            for r in 0..runs {
                if let Some(flat) = flat {
                    if flat[j * runs + r] != 0 {
                        continue;
                    }
                }
                let mut i = r * RUN;
                let end = (nx - 1).min(i + RUN);
                let mut tl = values[row + i] - level >= 0.0;
                let mut bl = values[row + nx + i] - level >= 0.0;
                while i < end {
                    let tr = values[row + i + 1] - level >= 0.0;
                    let br = values[row + nx + i + 1] - level >= 0.0;
                    if tl != tr || tr != br || br != bl {
                        let top = 2 * (row + i);
                        let bottom = 2 * (row + nx + i);
                        let left = top + 1;
                        let right = top + 3;
                        if tl == br && tr == bl {
                            let center = ((values[row + i] - level)
                                + (values[row + i + 1] - level)
                                + (values[row + nx + i + 1] - level)
                                + (values[row + nx + i] - level))
                                / 4.0
                                >= 0.0;
                            if tl == center {
                                link(top, right);
                                link(bottom, left);
                            } else {
                                link(left, top);
                                link(right, bottom);
                            }
                        } else if tl != tr {
                            link(
                                top,
                                if tr != br {
                                    right
                                } else if br != bl {
                                    bottom
                                } else {
                                    left
                                },
                            );
                        } else if tr != br {
                            link(right, if br != bl { bottom } else { left });
                        } else {
                            link(bottom, left);
                        }
                    }
                    tl = tr;
                    bl = br;
                    i += 1;
                }
            }
        }

        let count = self.order.len();
        self.traced.clear();
        self.ends.clear();
        let (a, b, passed) = (&self.link_a, &self.link_b, &mut self.passed);
        for k in 0..count {
            let first = self.order[k];
            if passed[first as usize] != 0 {
                continue;
            }
            let mut prev: i32 = -1;
            let mut cur = first;
            while passed[cur as usize] == 0 {
                passed[cur as usize] = 1;
                let idx = (cur >> 1) as usize;
                let i = idx % nx;
                let j = (idx - i) / nx;
                let v = values[idx] - level;
                if cur & 1 != 0 {
                    let t = v / (v - (values[idx + nx] - level));
                    self.traced.push(x0 + i as f64 * step);
                    self.traced.push(y0 + (j as f64 + t) * step);
                } else {
                    let t = v / (v - (values[idx + 1] - level));
                    self.traced.push(x0 + (i as f64 + t) * step);
                    self.traced.push(y0 + j as f64 * step);
                }
                let mut next = a[cur as usize];
                if next == prev || passed[next as usize] != 0 {
                    next = b[cur as usize];
                    if next < 0 || next == prev || passed[next as usize] != 0 {
                        break;
                    }
                }
                prev = cur;
                cur = next;
            }
            self.ends.push(self.traced.len());
        }
        for k in 0..count {
            let o = self.order[k] as usize;
            self.link_a[o] = -1;
            self.link_b[o] = -1;
            self.passed[o] = 0;
        }
        self.ends.len()
    }

    /// Closed loops where the field crosses `level` (`contourLoops`).
    pub fn contour_loops(
        &mut self,
        field: &Field,
        level: f64,
        flat: Option<&[u8]>,
    ) -> Vec<Vec<Point>> {
        let n = self.trace(field, level, flat);
        let mut loops = Vec::new();
        let mut start = 0;
        for k in 0..n {
            if self.ends[k] - start >= 6 {
                loops.push(points_of(&self.traced, start, self.ends[k], None));
            }
            start = self.ends[k];
        }
        loops
    }

    /// The loops of contourLoops, each simplified, without those left with under three points
    /// (`contourOutlines`).
    pub fn contour_outlines(
        &mut self,
        field: &Field,
        level: f64,
        tolerance: f64,
        flat: Option<&[u8]>,
    ) -> Vec<Vec<Point>> {
        let n = self.trace(field, level, flat);
        let mut loops = Vec::new();
        let mut start = 0;
        let traced = std::mem::take(&mut self.traced);
        for k in 0..n {
            let end = self.ends[k];
            let points = (end - start) >> 1;
            if points >= 3 {
                if points <= 8 {
                    loops.push(points_of(&traced, start, end, None));
                } else {
                    self.simplify(&traced, start, points, tolerance);
                    let kept: usize = self.kept[..points].iter().map(|&v| v as usize).sum();
                    if kept >= 3 {
                        loops.push(points_of(&traced, start, end, Some(&self.kept)));
                    }
                }
            }
            start = end;
        }
        self.traced = traced;
        loops
    }

    /// Ramer-Douglas-Peucker on a closed loop of `n` points at xy[start] (`simplify`). Leaves
    /// the flags in `self.kept[..n]`.
    fn simplify(&mut self, xy: &[f64], start: usize, n: usize, tolerance: f64) {
        if self.kept.len() < n {
            self.kept = vec![0; 2 * n];
        }
        let keep = &mut self.kept;
        keep[..n].fill(0);
        let x0 = xy[start];
        let y0 = xy[start + 1];
        let (mut far, mut top, mut next) = (0usize, -1.0f64, -1.0f64);
        for k in 1..n {
            let dx = xy[start + 2 * k] - x0;
            let dy = xy[start + 2 * k + 1] - y0;
            let d = dx * dx + dy * dy;
            if d > top {
                next = top;
                top = d;
                far = k;
            } else if d > next {
                next = d;
            }
        }
        if next >= top * (1.0 - CLEAR) {
            let mut best = -1.0;
            far = 0;
            for k in 1..n {
                let d = jsmath::hypot(xy[start + 2 * k] - x0, xy[start + 2 * k + 1] - y0);
                if d > best {
                    best = d;
                    far = k;
                }
            }
        }
        keep[0] = 1;
        keep[far] = 1;
        let chains = &mut self.chains;
        chains.clear();
        chains.extend_from_slice(&[0, far, far, n]);
        while let (Some(e), Some(s)) = (chains.pop(), chains.pop()) {
            let ax = xy[start + 2 * s];
            let ay = xy[start + 2 * s + 1];
            let dx = xy[start + 2 * (e % n)] - ax;
            let dy = xy[start + 2 * (e % n) + 1] - ay;
            let (mut idx, mut lead, mut second) = (-1i64, -1.0f64, -1.0f64);
            for k in s + 1..e {
                let d = (dx * (ay - xy[start + 2 * k + 1]) - dy * (ax - xy[start + 2 * k])).abs();
                if d > lead {
                    second = lead;
                    lead = d;
                    idx = k as i64;
                } else if d > second {
                    second = d;
                }
            }
            let bar = tolerance * (dx * dx + dy * dy).sqrt();
            if lead <= bar * (1.0 - CLEAR) && bar > 0.0 {
                idx = -1;
            } else if !(lead > bar * (1.0 + CLEAR) && second < lead * (1.0 - CLEAR)) {
                let len = jsmath::hypot(dx, dy);
                let mut max = tolerance;
                idx = -1;
                for k in s + 1..e {
                    let px = xy[start + 2 * k];
                    let py = xy[start + 2 * k + 1];
                    let d = if len == 0.0 {
                        jsmath::hypot(px - ax, py - ay)
                    } else {
                        (dx * (ay - py) - dy * (ax - px)).abs() / len
                    };
                    if d > max {
                        max = d;
                        idx = k as i64;
                    }
                }
            }
            if idx >= 0 {
                let idx = idx as usize;
                keep[idx] = 1;
                chains.extend_from_slice(&[s, idx, idx, e]);
            }
        }
    }

    /// `simplifiedLoop`: simplifyLoop for a loop given as `n` points in `xy`, x and y in turn.
    pub fn simplified_loop(&mut self, xy: &[f64], n: usize, tolerance: f64) -> Vec<Point> {
        if n <= 8 {
            points_of(xy, 0, 2 * n, None)
        } else {
            self.simplify(xy, 0, n, tolerance);
            points_of(xy, 0, 2 * n, Some(&self.kept))
        }
    }
}

fn points_of(xy: &[f64], start: usize, end: usize, keep: Option<&[u8]>) -> Vec<Point> {
    let mut out = Vec::with_capacity((end - start) / 2);
    let mut k = start;
    while k < end {
        if keep.is_none_or(|keep| keep[(k - start) >> 1] != 0) {
            out.push([xy[k], xy[k + 1]]);
        }
        k += 2;
    }
    out
}

/// Path numbers are hundredths; vertices snap to even ones (`UNIT`).
const UNIT: f64 = 100.0;

/// A grid value as a path number, as short as SVG allows: no leading zero, no `-0` (`write`).
fn write_grid(out: &mut String, k: i64) {
    let a = k.unsigned_abs();
    let whole = a / 100;
    let frac = a % 100;
    if k < 0 {
        out.push('-');
    }
    if whole != 0 || frac == 0 {
        out.push_str(&whole.to_string());
    }
    if frac != 0 {
        if frac.is_multiple_of(10) {
            out.push('.');
            out.push_str(&(frac / 10).to_string());
        } else if frac < 10 {
            out.push_str(".0");
            out.push_str(&frac.to_string());
        } else {
            out.push('.');
            out.push_str(&frac.to_string());
        }
    }
}

/// Turns sharper than this (radians) stay corners (`CORNER`).
const CORNER: f64 = 0.6;
/// A gentle turn between long edges stays sharp once its curve would leave the loop by more (`SAG`).
const SAG: f64 = 0.1;

/// The compact `d` text and the same outline as absolute commands, from `loopsToPath`.
pub struct LoopPath {
    pub d: String,
    pub cmds: Vec<Segment>,
}

struct Emitter {
    d: String,
    cmd: u8,
    last: String,
    tmp: String,
}

impl Emitter {
    fn emit(&mut self, c: u8, x: i64, y: i64) {
        if c != self.cmd {
            self.d.push(c as char);
            self.cmd = c;
            self.last.clear();
        }
        for v in [x, y] {
            self.tmp.clear();
            write_grid(&mut self.tmp, v);
            let t0 = self.tmp.as_bytes()[0];
            if !self.last.is_empty() && t0 != b'-' && !(t0 == b'.' && self.last.contains('.')) {
                self.d.push(' ');
            }
            self.d.push_str(&self.tmp);
            std::mem::swap(&mut self.last, &mut self.tmp);
        }
    }
}

#[inline]
fn grid(v: i64) -> f64 {
    v as f64 / UNIT
}

/// Compact closed path from loops, drawn with fill-rule evenodd (`loopsToPath`). Gentle turns are
/// drawn as quadratics between edge midpoints; real corners stay sharp.
pub fn loops_to_path(loops: &[Vec<Point>]) -> LoopPath {
    let mut em = Emitter {
        d: String::new(),
        cmd: 0,
        last: String::new(),
        tmp: String::new(),
    };
    let mut cmds = Vec::new();
    let mut v: Vec<i64> = Vec::new();
    let mut corner: Vec<u8> = Vec::new();
    for lp in loops {
        let n = lp.len();
        v.clear();
        v.resize(2 * n, 0);
        corner.clear();
        corner.resize(n, 0);
        for k in 0..n {
            let p = lp[k];
            let a = lp[(k + n - 1) % n];
            let b = lp[(k + 1) % n];
            v[2 * k] = 2 * jsmath::round(p[0] * UNIT / 2.0) as i64;
            v[2 * k + 1] = 2 * jsmath::round(p[1] * UNIT / 2.0) as i64;
            let cross = (p[0] - a[0]) * (b[1] - p[1]) - (p[1] - a[1]) * (b[0] - p[0]);
            let dot = (p[0] - a[0]) * (b[0] - p[0]) + (p[1] - a[1]) * (b[1] - p[1]);
            let turn = cross.abs();
            let steep = dot * jsmath::TAN_0_6;
            let sharp = if dot > 0.0 {
                turn > steep * (1.0 + CLEAR)
                    || (turn >= steep * (1.0 - CLEAR) && jsmath::atan2(cross, dot).abs() > CORNER)
            } else {
                jsmath::atan2(cross, dot).abs() > CORNER
            };
            if sharp {
                corner[k] = 1;
                continue;
            }
            let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
            let chord = (4.0 * SAG) * (4.0 * SAG) * (ex * ex + ey * ey);
            corner[k] = u8::from(
                turn * turn > chord * (1.0 + CLEAR)
                    || (turn * turn >= chord * (1.0 - CLEAR)
                        && turn > 4.0 * SAG * jsmath::hypot(ex, ey)),
            );
        }
        let i1 = 2 * (1 % n);
        let mut cx = (v[0] + v[i1]) / 2;
        let mut cy = (v[1] + v[i1 + 1]) / 2;
        em.emit(b'M', cx, cy);
        cmds.push(Segment::M {
            x: grid(cx),
            y: grid(cy),
        });
        let mut smooth = false;
        for k in 1..=n {
            let at = 2 * (k % n);
            let after = 2 * ((k + 1) % n);
            let (px, py) = (v[at], v[at + 1]);
            let (mx, my) = ((px + v[after]) / 2, (py + v[after + 1]) / 2);
            if corner[k % n] != 0 {
                em.emit(b'l', px - cx, py - cy);
                cx = px;
                cy = py;
                cmds.push(Segment::L {
                    x: grid(cx),
                    y: grid(cy),
                });
                if k < n && corner[(k + 1) % n] == 0 {
                    em.emit(b'l', mx - cx, my - cy);
                    cx = mx;
                    cy = my;
                    cmds.push(Segment::L {
                        x: grid(cx),
                        y: grid(cy),
                    });
                }
                smooth = false;
            } else {
                if smooth {
                    em.emit(b't', mx - cx, my - cy);
                } else {
                    em.emit(b'q', px - cx, py - cy);
                    em.emit(b'q', mx - cx, my - cy);
                }
                cmds.push(Segment::Q {
                    x1: grid(px),
                    y1: grid(py),
                    x: grid(mx),
                    y: grid(my),
                });
                cx = mx;
                cy = my;
                smooth = true;
            }
        }
        em.d.push('Z');
        em.cmd = b'Z';
        em.last.clear();
        cmds.push(Segment::Z);
    }
    LoopPath { d: em.d, cmds }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_numbers() {
        let w = |k| {
            let mut s = String::new();
            write_grid(&mut s, k);
            s
        };
        assert_eq!(w(0), "0");
        assert_eq!(w(1234), "12.34");
        assert_eq!(w(-50), "-.5");
        assert_eq!(w(5), ".05");
        assert_eq!(w(-200), "-2");
        assert_eq!(w(120), "1.2");
    }
}
