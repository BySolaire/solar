//! Distance from points to the nearest edge of a set of polygons (port of `distance.ts`).
//!
//! Every answer is the smallest squared distance to a segment over all edges, then its square
//! root; the index only skips edges it can prove are farther, so it returns the same bits a pass
//! over every edge would.

use crate::jsmath;
use crate::path::{Bounds, EdgeBands, Point};

/// Margin on every bound that lets an edge be skipped, viewBox units.
const SLACK: f64 = 1e-6;
/// Side of a bucket of the coarse edge grid, viewBox units.
const CELL: f64 = 4.0;
/// Side, in samples, of the tiles a sample grid is cut into.
const TILE: usize = 16;

/// `segDist2`: squared distance from (px, py) to the segment a-b.
#[inline]
pub fn seg_dist2(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx = bx - ax;
    let dy = by - ay;
    let len = dx * dx + dy * dy;
    let mut t = if len > 0.0 {
        ((px - ax) * dx + (py - ay) * dy) / len
    } else {
        0.0
    };
    t = t.clamp(0.0, 1.0);
    let ex = ax + t * dx - px;
    let ey = ay + t * dy - py;
    ex * ex + ey * ey
}

/// A regular grid of sample points: x0 + i * step, y0 + j * step (`SampleGrid`).
#[derive(Clone, Copy, Debug)]
pub struct SampleGrid {
    pub x0: f64,
    pub y0: f64,
    pub step: f64,
    pub nx: usize,
    pub ny: usize,
}

/// Edges bucketed on a coarse grid (`EdgeIndex`).
pub struct EdgeIndex {
    e: Vec<f64>,
    start: Vec<usize>,
    items: Vec<usize>,
    ox: f64,
    oy: f64,
    span: i64,
    count: usize,
    // Walk scratch (module globals in the TS).
    stamp: Vec<u32>,
    tags: u32,
    lists: Vec<usize>,
    d2: Vec<f64>,
    vx: Vec<f64>,
    vy: Vec<f64>,
    bx: f64,
    by: f64,
    gx0: f64,
    gy0: f64,
    gstep: f64,
    gnx: usize,
}

impl EdgeIndex {
    pub fn new(polys: &[Vec<Point>], b: Bounds) -> EdgeIndex {
        let count: usize = polys.iter().map(|p| p.len()).sum();
        let ox = b.min_x - 4.0;
        let oy = b.min_y - 4.0;
        let span =
            ((jsmath::max(b.max_x - b.min_x, b.max_y - b.min_y) + 8.0) / CELL).ceil() as i64 + 1;
        let mut idx = EdgeIndex {
            e: vec![0.0; 5 * count],
            start: vec![0; (span * span) as usize + 1],
            items: Vec::new(),
            ox,
            oy,
            span,
            count,
            stamp: Vec::new(),
            tags: 0,
            lists: Vec::new(),
            d2: Vec::new(),
            vx: Vec::new(),
            vy: Vec::new(),
            bx: 0.0,
            by: 0.0,
            gx0: 0.0,
            gy0: 0.0,
            gstep: 0.0,
            gnx: 0,
        };
        let ss = (span * span) as usize;
        let mut next: Vec<usize> = Vec::new();
        for place in [false, true] {
            if place {
                for q in 0..ss {
                    idx.start[q + 1] += idx.start[q];
                }
                idx.items = vec![0; idx.start[ss]];
                next = idx.start[..ss].to_vec();
            }
            let mut k = 0;
            for poly in polys {
                let n = poly.len();
                let mut j = n.wrapping_sub(1);
                for i in 0..n {
                    let (ax, ay, bx, by) = (poly[j][0], poly[j][1], poly[i][0], poly[i][1]);
                    let dx = bx - ax;
                    let dy = by - ay;
                    idx.e[k] = ax;
                    idx.e[k + 1] = ay;
                    idx.e[k + 2] = dx;
                    idx.e[k + 3] = dy;
                    idx.e[k + 4] = dx * dx + dy * dy;
                    let cy0 = idx.cy(jsmath::min(ay, by));
                    let cy1 = idx.cy(jsmath::max(ay, by));
                    for cy in cy0..=cy1 {
                        let mut x0 = jsmath::min(ax, bx);
                        let mut x1 = jsmath::max(ax, bx);
                        if cy0 < cy1 && cy > 0 && cy < span - 1 {
                            let t0 = jsmath::min(
                                1.0,
                                jsmath::max(0.0, (oy + cy as f64 * CELL - SLACK - ay) / dy),
                            );
                            let t1 = jsmath::min(
                                1.0,
                                jsmath::max(0.0, (oy + (cy + 1) as f64 * CELL + SLACK - ay) / dy),
                            );
                            x0 = jsmath::min(ax + t0 * dx, ax + t1 * dx) - SLACK;
                            x1 = jsmath::max(ax + t0 * dx, ax + t1 * dx) + SLACK;
                        }
                        let end = idx.cx(x1);
                        let mut cx = idx.cx(x0);
                        while cx <= end {
                            let bucket = (cy * span + cx) as usize;
                            if place {
                                idx.items[next[bucket]] = k;
                                next[bucket] += 1;
                            } else {
                                idx.start[bucket + 1] += 1;
                            }
                            cx += 1;
                        }
                    }
                    k += 5;
                    j = i;
                }
            }
        }
        idx
    }

    fn ready(&mut self) {
        if self.stamp.len() >= self.count && !self.stamp.is_empty() {
            return;
        }
        let len = 2 * self.count.max(1);
        self.stamp = vec![0; len];
        self.lists = vec![0; (2 + 4) * len];
        self.d2 = vec![0.0; len];
        self.vx = vec![0.0; len];
        self.vy = vec![0.0; len];
    }

    #[inline]
    fn cx(&self, x: f64) -> i64 {
        clamp_cell(((x - self.ox) / CELL).floor(), self.span)
    }

    #[inline]
    fn cy(&self, y: f64) -> i64 {
        clamp_cell(((y - self.oy) / CELL).floor(), self.span)
    }

    /// Distance to the nearest edge, searching rings of buckets outward (`distance`).
    pub fn distance(&self, x: f64, y: f64) -> f64 {
        let span = self.span;
        let bx = self.cx(x);
        let by = self.cy(y);
        let mut best = f64::INFINITY;
        let e = &self.e;
        for r in 0..span {
            for cy in by - r..=by + r {
                if cy < 0 || cy >= span {
                    continue;
                }
                let edge_row = cy == by - r || cy == by + r;
                let stride = if edge_row || r == 0 { 1 } else { 2 * r };
                let mut cx = bx - r;
                while cx <= bx + r {
                    if cx >= 0 && cx < span {
                        let b = (cy * span + cx) as usize;
                        for &k in &self.items[self.start[b]..self.start[b + 1]] {
                            let (ax, ay, dx, dy, len) =
                                (e[k], e[k + 1], e[k + 2], e[k + 3], e[k + 4]);
                            let mut t = if len > 0.0 {
                                ((x - ax) * dx + (y - ay) * dy) / len
                            } else {
                                0.0
                            };
                            t = t.clamp(0.0, 1.0);
                            let ex = ax + t * dx - x;
                            let ey = ay + t * dy - y;
                            let d = ex * ex + ey * ey;
                            if d < best {
                                best = d;
                            }
                        }
                    }
                    cx += stride;
                }
            }
            let reach = r as f64 * CELL - SLACK;
            if reach > 0.0 && best <= reach * reach {
                break;
            }
        }
        best.sqrt()
    }

    /// Distance to the nearest edge at every sample of a grid, row-major (`distances`).
    pub fn distances(&mut self, g: SampleGrid) -> Vec<f64> {
        let mut out = vec![0.0f64; g.nx * g.ny];
        self.ready();
        self.gx0 = g.x0;
        self.gy0 = g.y0;
        self.gstep = g.step;
        self.gnx = g.nx;
        let mut j0 = 0;
        while j0 < g.ny {
            let mut i0 = 0;
            while i0 < g.nx {
                let i1 = g.nx.min(i0 + TILE) - 1;
                let j1 = g.ny.min(j0 + TILE) - 1;
                let im = (i0 + i1) >> 1;
                let jm = (j0 + j1) >> 1;
                let x = g.x0 + im as f64 * g.step;
                let y = g.y0 + jm as f64 * g.step;
                let reach = self.distance(x, y) + 2.0 * radius_of(g.step, i0, i1, j0, j1) + SLACK;
                let hi = self.collect(x, y, reach);
                self.fill(&mut out, i0, i1, j0, j1, 0, hi);
                i0 += TILE;
            }
            j0 += TILE;
        }
        out
    }

    /// The largest distance to the nearest edge over the grid's samples inside the fill (`widest`).
    pub fn widest(&mut self, g: SampleGrid, rows: &EdgeBands) -> f64 {
        self.ready();
        self.gx0 = g.x0;
        self.gy0 = g.y0;
        self.gstep = g.step;
        self.gnx = g.nx;
        let mut tiles: Vec<[usize; 4]> = Vec::new();
        let mut j0 = 0;
        while j0 < g.ny {
            let mut i0 = 0;
            while i0 < g.nx {
                tiles.push([i0, g.nx.min(i0 + TILE) - 1, j0, g.ny.min(j0 + TILE) - 1]);
                i0 += TILE;
            }
            j0 += TILE;
        }
        let mut middles = vec![0.0f64; tiles.len()];
        let mut best = 0.0f64;
        for (k, t) in tiles.iter().enumerate() {
            let x = g.x0 + ((t[0] + t[1]) >> 1) as f64 * g.step;
            let y = g.y0 + ((t[2] + t[3]) >> 1) as f64 * g.step;
            let d = self.distance(x, y);
            middles[k] = d;
            if d > best && rows.inside_row_at(x, y) {
                best = d;
            }
        }
        for (k, t) in tiles.iter().enumerate() {
            let [i0, i1, j0, j1] = *t;
            let x = g.x0 + ((i0 + i1) >> 1) as f64 * g.step;
            let y = g.y0 + ((j0 + j1) >> 1) as f64 * g.step;
            let d = middles[k];
            let radius = radius_of(g.step, i0, i1, j0, j1);
            if d + radius + SLACK < best {
                continue;
            }
            let whole = d > radius + SLACK;
            if whole && !rows.inside_row_at(x, y) {
                continue;
            }
            let hi = self.collect(x, y, d + 2.0 * radius + SLACK);
            best = self.peak(rows, best, whole, i0, i1, j0, j1, 0, hi);
        }
        best
    }

    /// Lists every edge with a point in the square of `reach` around (x, y) (`collect`).
    fn collect(&mut self, x: f64, y: f64, reach: f64) -> usize {
        if self.tags >= 0x7fffff00 {
            self.stamp.fill(0);
            self.tags = 0;
        }
        self.tags += 1;
        let tag = self.tags;
        let mut n = 0;
        let cx1 = self.cx(x + reach);
        let cy1 = self.cy(y + reach);
        let cx0 = self.cx(x - reach);
        for cy in self.cy(y - reach)..=cy1 {
            for cx in cx0..=cx1 {
                let b = (cy * self.span + cx) as usize;
                for q in self.start[b]..self.start[b + 1] {
                    let k = self.items[q];
                    if self.stamp[k / 5] == tag {
                        continue;
                    }
                    self.stamp[k / 5] = tag;
                    self.put(n, k);
                    n += 1;
                }
            }
        }
        n
    }

    #[inline]
    fn put(&mut self, at: usize, k: usize) {
        if at >= self.lists.len() {
            self.lists.resize(2 * at + 16, 0);
        }
        self.lists[at] = k;
    }

    /// Squared distance from (x, y) to the nearest of the edges `lists[lo..hi)` (`nearest`).
    fn nearest(&self, lo: usize, hi: usize, x: f64, y: f64) -> f64 {
        let e = &self.e;
        let mut best = f64::INFINITY;
        for &k in &self.lists[lo..hi] {
            let (ax, ay, dx, dy, len) = (e[k], e[k + 1], e[k + 2], e[k + 3], e[k + 4]);
            let mut t = if len > 0.0 {
                ((x - ax) * dx + (y - ay) * dy) / len
            } else {
                0.0
            };
            t = t.clamp(0.0, 1.0);
            let ex = ax + t * dx - x;
            let ey = ay + t * dy - y;
            let d = ex * ex + ey * ey;
            if d < best {
                best = d;
            }
        }
        best
    }

    /// `nearest`, keeping each edge's squared distance and vector for `narrow` (`survey`).
    fn survey(&mut self, lo: usize, hi: usize, x: f64, y: f64) -> f64 {
        let mut best = f64::INFINITY;
        if self.d2.len() < hi - lo {
            self.d2.resize(hi - lo, 0.0);
            self.vx.resize(hi - lo, 0.0);
            self.vy.resize(hi - lo, 0.0);
        }
        for q in lo..hi {
            let k = self.lists[q];
            let e = &self.e;
            let (ax, ay, dx, dy, len) = (e[k], e[k + 1], e[k + 2], e[k + 3], e[k + 4]);
            let mut t = if len > 0.0 {
                ((x - ax) * dx + (y - ay) * dy) / len
            } else {
                0.0
            };
            t = t.clamp(0.0, 1.0);
            let ex = ax + t * dx - x;
            let ey = ay + t * dy - y;
            let d = ex * ex + ey * ey;
            self.d2[q - lo] = d;
            self.vx[q - lo] = ex;
            self.vy[q - lo] = ey;
            if d < best {
                best = d;
                self.bx = ex;
                self.by = ey;
            }
        }
        best
    }

    /// The edges that can be nearest to some point within `radius` of the surveyed one (`narrow`).
    fn narrow(&mut self, lo: usize, hi: usize, d: f64, radius: f64) -> usize {
        let ball = d + 2.0 * radius + SLACK;
        let bend = if d > 0.0 {
            (radius * radius) / (2.0 * d)
        } else {
            f64::INFINITY
        };
        let ux = if d > 0.0 { self.bx / d } else { 0.0 };
        let uy = if d > 0.0 { self.by / d } else { 0.0 };
        let mut end = hi;
        for q in lo..hi {
            let far2 = self.d2[q - lo];
            if far2 > ball * ball {
                continue;
            }
            if bend < 2.0 * radius {
                let far = far2.sqrt();
                let wx = self.vx[q - lo] / far - ux;
                let wy = self.vy[q - lo] / far - uy;
                if far - d > (wx * wx + wy * wy).sqrt() * radius + bend + SLACK {
                    continue;
                }
            }
            let k = self.lists[q];
            self.put(end, k);
            end += 1;
        }
        end
    }

    /// Measures the samples i0..i1 × j0..j1 into `out` (`fill`).
    #[allow(clippy::too_many_arguments)]
    fn fill(
        &mut self,
        out: &mut [f64],
        i0: usize,
        i1: usize,
        j0: usize,
        j1: usize,
        lo: usize,
        hi: usize,
    ) {
        let (x0, y0, step, nx) = (self.gx0, self.gy0, self.gstep, self.gnx);
        if (i1 - i0 < 2 && j1 - j0 < 2) || hi - lo < 2 {
            for j in j0..=j1 {
                for i in i0..=i1 {
                    out[j * nx + i] = self
                        .nearest(lo, hi, x0 + i as f64 * step, y0 + j as f64 * step)
                        .sqrt();
                }
            }
            return;
        }
        let im = (i0 + i1) >> 1;
        let jm = (j0 + j1) >> 1;
        let d = self
            .survey(lo, hi, x0 + im as f64 * step, y0 + jm as f64 * step)
            .sqrt();
        out[jm * nx + im] = d;
        let end = self.narrow(lo, hi, d, radius_of(step, i0, i1, j0, j1));
        self.fill(out, i0, im, j0, jm, hi, end);
        if im < i1 {
            self.fill(out, im + 1, i1, j0, jm, hi, end);
        }
        if jm < j1 {
            self.fill(out, i0, im, jm + 1, j1, hi, end);
            if im < i1 {
                self.fill(out, im + 1, i1, jm + 1, j1, hi, end);
            }
        }
    }

    /// `fill` for `widest` (`peak`).
    #[allow(clippy::too_many_arguments)]
    fn peak(
        &mut self,
        rows: &EdgeBands,
        mut best: f64,
        mut whole: bool,
        i0: usize,
        i1: usize,
        j0: usize,
        j1: usize,
        lo: usize,
        hi: usize,
    ) -> f64 {
        let (x0, y0, step) = (self.gx0, self.gy0, self.gstep);
        if (i1 - i0 < 2 && j1 - j0 < 2) || hi - lo < 2 {
            for j in j0..=j1 {
                for i in i0..=i1 {
                    let x = x0 + i as f64 * step;
                    let y = y0 + j as f64 * step;
                    let d = self.nearest(lo, hi, x, y).sqrt();
                    if d > best && (whole || rows.inside_row_at(x, y)) {
                        best = d;
                    }
                }
            }
            return best;
        }
        let im = (i0 + i1) >> 1;
        let jm = (j0 + j1) >> 1;
        let x = x0 + im as f64 * step;
        let y = y0 + jm as f64 * step;
        let d = self.survey(lo, hi, x, y).sqrt();
        let radius = radius_of(step, i0, i1, j0, j1);
        if d + radius + SLACK < best {
            return best;
        }
        if !whole && d > radius + SLACK {
            if !rows.inside_row_at(x, y) {
                return best;
            }
            whole = true;
        }
        if d > best && (whole || rows.inside_row_at(x, y)) {
            best = d;
        }
        let end = self.narrow(lo, hi, d, radius);
        best = self.peak(rows, best, whole, i0, im, j0, jm, hi, end);
        if im < i1 {
            best = self.peak(rows, best, whole, im + 1, i1, j0, jm, hi, end);
        }
        if jm < j1 {
            best = self.peak(rows, best, whole, i0, im, jm + 1, j1, hi, end);
            if im < i1 {
                best = self.peak(rows, best, whole, im + 1, i1, jm + 1, j1, hi, end);
            }
        }
        best
    }
}

#[inline]
fn clamp_cell(q: f64, span: i64) -> i64 {
    // Math.max(0, Math.min(span - 1, q)); NaN lands on 0 as in the TS (Math.max(0, NaN) is NaN,
    // which then indexes nothing: never reached with finite coordinates).
    let hi = (span - 1) as f64;
    let v = if q < hi { q } else { hi };
    if v > 0.0 {
        v as i64
    } else {
        0
    }
}

/// How far the samples i0..i1 × j0..j1 reach from the middle one (`radiusOf`).
fn radius_of(step: f64, i0: usize, i1: usize, j0: usize, j1: usize) -> f64 {
    let im = (i0 + i1) >> 1;
    let jm = (j0 + j1) >> 1;
    step * jsmath::hypot((im - i0).max(i1 - im) as f64, (jm - j0).max(j1 - jm) as f64)
}
