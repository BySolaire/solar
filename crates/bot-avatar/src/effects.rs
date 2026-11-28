//! What a state draws besides the body (port of `effects.ts`): the ribbons that circle a working
//! or celebrating bot, and the marks a thinking or blocked bot turns into.

use crate::geom::BoxF;
use crate::jsmath::{self, atan2, clamp, cos, hypot, sin, smooth};
use crate::marks::{mark_parts_of, MarkContext, MarkPart, PoseMark};
use crate::motion::noise;
use crate::path::{Bounds, Point};

const PI: f64 = std::f64::consts::PI;

/// One piece of a ribbon (`RibbonPiece`). `points` are the outline's vertices; [`RibbonPiece::d`]
/// writes them as the TS `d`.
#[derive(Clone, Debug, PartialEq)]
pub struct RibbonPiece {
    pub points: Vec<Point>,
    pub front: bool,
    pub bbox: BoxF,
    /// Which ribbon the piece belongs to.
    pub ribbon: usize,
    /// The stretch of its ribbon the piece covers, 0 (tail) to 1 (head).
    pub along: [f64; 2],
    /// The centre of the ribbon at either end of that stretch.
    pub from: Point,
    pub to: Point,
}

impl RibbonPiece {
    /// `M x yL x y…Z`, numbers through `f()`.
    pub fn d(&self) -> String {
        points_d(&self.points)
    }
}

/// `M${pts.map((p) => `${f(p[0])} ${f(p[1])}`).join('L')}Z`.
pub fn points_d(pts: &[Point]) -> String {
    let mut d = String::with_capacity(pts.len() * 12 + 2);
    d.push('M');
    for (i, p) in pts.iter().enumerate() {
        if i > 0 {
            d.push('L');
        }
        jsmath::push_f(&mut d, p[0]);
        d.push(' ');
        jsmath::push_f(&mut d, p[1]);
    }
    d.push('Z');
    d
}

const DESIGN: f64 = 228.541;
const BODY: f64 = 80.0;
const UNIT: f64 = BODY / DESIGN;
const RIM_MARGIN: f64 = 2.0;
const RING: f64 = 38.8 * (116.0 / 114.2705);
const RIM_STEPS: usize = 180;
const RIM_FANS: i64 = 180;

/// How far `outlines` reach from (cx, cy) along a direction, exactly (`reachFinder`).
#[derive(Clone, Debug)]
pub struct ReachFinder {
    e: Vec<f64>,
    start: Vec<usize>,
    items: Vec<usize>,
    count: usize,
    cx: f64,
    cy: f64,
}

impl ReachFinder {
    pub fn new(outlines: &[Vec<Vec<Point>>], cx: f64, cy: f64) -> ReachFinder {
        let count: usize = outlines
            .iter()
            .flat_map(|o| o.iter())
            .map(|p| p.len())
            .sum();
        let mut e = vec![0.0f64; 4 * count];
        let mut first = vec![0i64; count];
        let mut more = vec![0i64; count];
        let mut start = vec![0usize; RIM_FANS as usize + 1];
        let mut k = 0;
        for outline in outlines {
            for poly in outline {
                let n = poly.len();
                for i in 0..n {
                    let p = poly[i];
                    let q = poly[(i + 1) % n];
                    e[4 * k] = p[0];
                    e[4 * k + 1] = p[1];
                    e[4 * k + 2] = q[0] - p[0];
                    e[4 * k + 3] = q[1] - p[1];
                    let (px, py, qx, qy) = (p[0] - cx, p[1] - cy, q[0] - cx, q[1] - cy);
                    let from = atan2(py, px);
                    let turn = atan2(px * qy - py * qx, px * qx + py * qy);
                    let near = px * px + py * py < 1e-6
                        || qx * qx + qy * qy < 1e-6
                        || turn.abs() > PI - 0.1;
                    let lo = ((jsmath::min(from, from + turn) / PI) * RIM_FANS as f64).floor()
                        as i64
                        - 1;
                    let hi = ((jsmath::max(from, from + turn) / PI) * RIM_FANS as f64).floor()
                        as i64
                        + 1;
                    first[k] = if near { 0 } else { lo.rem_euclid(RIM_FANS) };
                    more[k] = if near || hi - lo >= RIM_FANS {
                        RIM_FANS - 1
                    } else {
                        hi - lo
                    };
                    for f in 0..=more[k] {
                        start[((first[k] + f) % RIM_FANS) as usize + 1] += 1;
                    }
                    k += 1;
                }
            }
        }
        for f in 0..RIM_FANS as usize {
            start[f + 1] += start[f];
        }
        let mut items = vec![0usize; start[RIM_FANS as usize]];
        let mut next: Vec<usize> = start[..RIM_FANS as usize].to_vec();
        for k in 0..count {
            for f in 0..=more[k] {
                let fan = ((first[k] + f) % RIM_FANS) as usize;
                items[next[fan]] = 4 * k;
                next[fan] += 1;
            }
        }
        ReachFinder {
            e,
            start,
            items,
            count,
            cx,
            cy,
        }
    }

    /// The reach along angle `a` (radians), either way.
    pub fn at(&self, a: f64) -> f64 {
        let ux = cos(a);
        let uy = sin(a);
        let turns = atan2(uy, ux) / PI;
        let fan = ((turns - turns.floor()) * RIM_FANS as f64).floor();
        let known = fan >= 0.0 && fan < RIM_FANS as f64;
        let (lo, hi) = if known {
            let f = fan as usize;
            (self.start[f], self.start[f + 1])
        } else {
            (0, self.count)
        };
        let mut far = 0.0f64;
        for n in lo..hi {
            let at = if known { self.items[n] } else { 4 * n };
            let (x1, y1, ex, ey) = (self.e[at], self.e[at + 1], self.e[at + 2], self.e[at + 3]);
            let den = ux * ey - uy * ex;
            if den.abs() < 1e-12 {
                continue;
            }
            let t = ((x1 - self.cx) * ey - (y1 - self.cy) * ex) / den;
            let s = ((x1 - self.cx) * uy - (y1 - self.cy) * ux) / den;
            if (0.0..=1.0).contains(&s) {
                far = jsmath::max(far, t.abs());
            }
        }
        far
    }
}

/// Where a body's hoops are centred, and how far the body reaches from there (`Rim`).
#[derive(Clone, Debug)]
pub struct Rim {
    pub cx: f64,
    pub cy: f64,
    pub finder: std::sync::Arc<ReachFinder>,
    pub max: f64,
}

impl Rim {
    pub fn at(&self, angle: f64) -> f64 {
        self.finder.at(angle)
    }
}

/// `rimOf`.
pub fn rim_of(outlines: &[Vec<Vec<Point>>], cx: f64, cy: f64) -> Rim {
    let finder = ReachFinder::new(outlines, cx, cy);
    let max = jsmath::max_of((0..RIM_STEPS).map(|k| finder.at((k as f64 * PI) / RIM_STEPS as f64)));
    Rim {
        cx,
        cy,
        finder: std::sync::Arc::new(finder),
        max: max * 1.02,
    }
}

const HOOP_TILT: f64 = 0.386;
const HOOP_ROLL: f64 = 0.157;
const HOOP_JITTER: f64 = 0.24;
const HOOP_TILT_JITTER: f64 = 0.08;
const LADDER: f64 = 38.0;
const ARC: [f64; 2] = [2.2, 3.4];
const FOLLOW: [f64; 2] = [0.74, 0.94];
const DRIFT: [f64; 2] = [0.5, 1.1];
const WIDEN: f64 = 2.5;
const START: f64 = 0.18;
const SIZE_FEW: [f64; 2] = [8.0, 10.5];
const SIZE_FOUR: [f64; 2] = [6.6, 8.6];
const SIZE_MANY: [f64; 2] = [5.6, 7.4];
const WORK_LANES: [f64; 2] = [3.0, 5.0];
const BRUSH: f64 = 1.7 * 1.98;
const FAR: f64 = 0.72;
const GROW_S: f64 = 0.34;
const STAGGER: [f64; 2] = [0.055, 0.105];
const SWEEP_RATE: [f64; 2] = [6.3, 15.0];
const SWEEP_LONGEST: f64 = 4.0;
/// Ribbons per hoop in a celebration (`LANES`) and hoops (`HOOPS`).
pub const LANES: usize = 3;
pub const HOOPS: usize = 3;
const SAMPLES: usize = 24;
const STRETCH: usize = 6;
const SLIVER: f64 = 0.02;
const CAP: usize = 7;

fn size_for(count: usize) -> [f64; 2] {
    if count <= 3 {
        SIZE_FEW
    } else if count == 4 {
        SIZE_FOUR
    } else {
        SIZE_MANY
    }
}

#[inline]
fn mixr(range: [f64; 2], u: f64) -> f64 {
    range[0] + (range[1] - range[0]) * u
}

/// How each hoop is rolled in the picture (`hoopRolls`).
fn hoop_rolls(seed: u32) -> [f64; HOOPS] {
    std::array::from_fn(|i| {
        HOOP_ROLL
            + (i as f64 * PI) / HOOPS as f64
            + (noise(seed, 450.0 + i as f64) - 0.5) * HOOP_JITTER
    })
}

/// How far ribbons reach (`ribbonReach`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RibbonReach {
    pub radius: f64,
    pub width: f64,
    pub band: f64,
}

/// `ribbonReach`.
pub fn ribbon_reach(rim_max: Option<f64>) -> RibbonReach {
    let width = SIZE_FEW[1] * BRUSH * UNIT;
    let ring = match rim_max {
        None => RING,
        Some(m) => m + RIM_MARGIN,
    };
    RibbonReach {
        radius: ring + (LADDER + WIDEN * SWEEP_LONGEST) * UNIT,
        width,
        band: width / 2.0,
    }
}

/// The ribbons at one moment, as filled outlines (`ribbonPieces`).
#[allow(clippy::needless_range_loop)]
pub fn ribbon_pieces(
    b: Bounds,
    presence: f64,
    seed: u32,
    sweep: f64,
    cage: f64,
    rim: Option<&Rim>,
) -> Vec<RibbonPiece> {
    let k = clamp(presence, 0.0, 1.0);
    let more = if cage >= 0.5 { 1.0 } else { 0.0 };
    if k <= 0.01 {
        return Vec::new();
    }
    let (cx, cy) = match rim {
        Some(r) => (r.cx, r.cy),
        None => ((b.min_x + b.max_x) / 2.0, (b.min_y + b.max_y) / 2.0),
    };
    let rolls = hoop_rolls(seed);
    let cage_ring = match rim {
        Some(r) if more != 0.0 => jsmath::max_of(rolls.iter().map(|&a| r.at(a))) + RIM_MARGIN,
        _ => RING,
    };
    let unit = UNIT;
    let dir = if sweep < 0.0 { -1.0 } else { 1.0 };
    let since = sweep.abs() / mixr(SWEEP_RATE, more);
    let mut pieces = Vec::new();
    let hoops = if more != 0.0 { HOOPS } else { 1 };
    for i in 0..hoops {
        let hoop_roll = rolls[i];
        let hoop_tilt = HOOP_TILT + (noise(seed, 460.0 + i as f64) - 0.5) * HOOP_TILT_JITTER;
        let ring = if more != 0.0 {
            cage_ring
        } else {
            match rim {
                Some(r) => r.at(hoop_roll) + RIM_MARGIN,
                None => RING,
            }
        };
        let lanes = if i != 0 || more > 0.0 {
            LANES
        } else {
            jsmath::round(mixr(WORK_LANES, noise(seed, 47.0))) as usize
        };
        for l in 0..lanes {
            let n = |m: f64| noise(seed, 500.0 + ((i * LANES + l) * 11) as f64 + m);
            let order = if more > 0.0 { l * HOOPS + i } else { l };
            let at = order as f64 * mixr(STAGGER, n(7.0));
            let life = since - at;
            if life <= 0.0 {
                continue;
            }
            let before = if since > 0.0 {
                sweep.abs() * (at / since)
            } else {
                0.0
            };
            let roll = hoop_roll;
            let tilt = hoop_tilt;
            let flat = sin(tilt);
            let depth = cos(tilt);
            let cr = cos(roll);
            let sr = sin(roll);
            let r = ring
                + (l as f64 * LADDER * unit) / (lanes as f64 - 1.0)
                + mixr([0.0, WIDEN], n(2.0)) * unit * life;
            let travel = (sweep.abs() - before) * mixr(FOLLOW, n(3.0)) + mixr(DRIFT, n(4.0)) * life;
            let head = dir * (before + travel - START * n(5.0));
            let ret = 1.0 - clamp(k * 1.25, 0.0, 1.0);
            let length =
                jsmath::min(mixr(ARC, n(6.0)), travel) * (1.0 - ret * ret * (3.0 - 2.0 * ret));
            if length <= 0.02 {
                continue;
            }
            let size =
                mixr(size_for(lanes), n(8.0)) * (1.0 - more) + mixr(SIZE_MANY, n(8.0)) * more;
            let near_head = clamp(cos(head) * depth, 0.0, 1.0);
            let mut full = size
                * BRUSH
                * unit
                * (FAR + (1.0 - FAR) * near_head)
                * smooth(life / GROW_S)
                * (1.0 - 0.72 * ret * ret);
            let lam_at = |u: f64| head - dir * length * (1.0 - u);
            let along = |lam: f64| 1.0 - (head - lam) / (dir * length);
            let sliver = SLIVER / length;
            let mut ss: Vec<f64> = (0..=SAMPLES).map(|j| j as f64 / SAMPLES as f64).collect();
            let mut extra = false;
            let lo = jsmath::min(head, head - dir * length);
            let hi = jsmath::max(head, head - dir * length);
            let mut side = ((lo - PI / 2.0) / PI).ceil();
            while PI / 2.0 + side * PI < hi {
                let u = along(PI / 2.0 + side * PI);
                extra = true;
                for d in [-sliver, 0.0, sliver] {
                    if u + d > 0.0 && u + d < 1.0 {
                        ss.push(u + d);
                    }
                }
                side += 1.0;
            }
            if extra {
                ss.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
            }
            let near: Vec<bool> = ss
                .iter()
                .map(|&u| cos(lam_at(u) + dir * 1e-6) > 0.0)
                .collect();
            let cnt = ss.len();
            let mut mid: Vec<Point> = Vec::with_capacity(cnt);
            let mut halfs: Vec<f64> = Vec::with_capacity(cnt);
            let mut norm: Vec<Point> = Vec::with_capacity(cnt);
            let mut bend: Vec<f64> = Vec::with_capacity(cnt);
            for &u in &ss {
                let lam = lam_at(u);
                let sn = sin(lam);
                let c = cos(lam);
                let nx = sn * flat;
                let ny = -c;
                let nl0 = hypot(nx, ny);
                let nl = if nl0 == 0.0 || nl0.is_nan() { 1.0 } else { nl0 };
                let ux = nx / nl;
                let uy = ny / nl;
                let y = ny * flat;
                mid.push([cx + r * (sn * cr - y * sr), cy + r * (sn * sr + y * cr)]);
                norm.push([ux * cr - uy * sr, ux * sr + uy * cr]);
                halfs.push(0.5 + 0.5 * u);
                bend.push((r * (c * c + flat * flat * sn * sn).powf(1.5)) / flat);
            }
            let last = cnt - 1;
            let mut on_screen = 0.0;
            for j in 1..=last {
                on_screen += hypot(mid[j][0] - mid[j - 1][0], mid[j][1] - mid[j - 1][1]);
            }
            full = jsmath::min(full, 0.34 * on_screen);
            for j in 0..=last {
                halfs[j] = jsmath::min(halfs[j] * (full / 2.0), 0.9 * bend[j]);
            }
            let side_pt = |j: usize, sign: f64| -> Point {
                [
                    mid[j][0] + sign * norm[j][0] * halfs[j],
                    mid[j][1] + sign * norm[j][1] * halfs[j],
                ]
            };
            let cap = |j: usize, ahead: bool, out: &mut Vec<Point>| {
                let o = if ahead { mid[j - 1] } else { mid[j + 1] };
                let mut tx = if ahead {
                    mid[j][0] - o[0]
                } else {
                    o[0] - mid[j][0]
                };
                let mut ty = if ahead {
                    mid[j][1] - o[1]
                } else {
                    o[1] - mid[j][1]
                };
                let tl0 = hypot(tx, ty);
                let tl = if tl0 == 0.0 || tl0.is_nan() { 1.0 } else { tl0 };
                tx /= tl;
                ty /= tl;
                let [nx, ny] = norm[j];
                let h = halfs[j];
                for c in 1..CAP {
                    let t = (PI * c as f64) / CAP as f64;
                    let e = if ahead { cos(t) } else { -cos(t) };
                    let g = if ahead { sin(t) } else { -sin(t) };
                    out.push([
                        mid[j][0] + (nx * e + tx * g) * h,
                        mid[j][1] + (ny * e + ty * g) * h,
                    ]);
                }
            };
            let mut from = 0usize;
            for j in 1..=last {
                let turns = j < last && near[j] != near[from];
                if j < last && !turns && j - from < STRETCH {
                    continue;
                }
                let to = j;
                let behind = !near[from];
                let end = if to < last && (!turns || behind) {
                    to + 1
                } else {
                    to
                };
                let start = if from > 0 && behind && near[from - 1] {
                    from - 1
                } else {
                    from
                };
                let mut pts: Vec<Point> = Vec::with_capacity(2 * (end - start + 1) + 2 * CAP);
                for q in start..=end {
                    pts.push(side_pt(q, 1.0));
                }
                if end == last {
                    cap(last, true, &mut pts);
                }
                for q in (start..=end).rev() {
                    pts.push(side_pt(q, -1.0));
                }
                if start == 0 {
                    cap(0, false, &mut pts);
                }
                let (mut min_x, mut max_x, mut min_y, mut max_y) = (
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                );
                for &[px, py] in &pts {
                    if px < min_x {
                        min_x = px;
                    }
                    if px > max_x {
                        max_x = px;
                    }
                    if py < min_y {
                        min_y = py;
                    }
                    if py > max_y {
                        max_y = py;
                    }
                }
                pieces.push(RibbonPiece {
                    points: pts,
                    front: near[from],
                    bbox: BoxF {
                        x: min_x,
                        y: min_y,
                        w: max_x - min_x,
                        h: max_y - min_y,
                    },
                    ribbon: i * LANES + l,
                    along: [ss[from], ss[to]],
                    from: mid[from],
                    to: mid[to],
                });
                from = j;
            }
        }
    }
    pieces
}

/// A mark starts to grow at this weight (`MARK_FROM`).
const MARK_FROM: f64 = 0.45;

fn grown(weight: f64) -> f64 {
    smooth((clamp(weight, 0.0, 1.0) - MARK_FROM) / (1.0 - MARK_FROM))
}

/// The unit every mark is sized in (`markUnit`).
fn mark_unit(b: Bounds) -> f64 {
    clamp(
        jsmath::min(b.max_x - b.min_x, b.max_y - b.min_y),
        34.0,
        78.0,
    )
}

/// The parts of a pose's marks at one moment, placed on the body (`markParts`).
pub fn mark_parts(
    b: Bounds,
    marks: &[PoseMark],
    seed: u32,
    time: f64,
    still: bool,
) -> Vec<MarkPart> {
    let cx = (b.min_x + b.max_x) / 2.0;
    let cy = (b.min_y + b.max_y) / 2.0;
    let ctx = MarkContext {
        unit: mark_unit(b),
        time: if still {
            0.0
        } else {
            time + noise(seed, 9.0) * 10.0
        },
        still,
    };
    let mut out = Vec::new();
    for m in marks {
        let g = grown(m.weight);
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !(g > 0.0) {
            continue;
        }
        for part in mark_parts_of(m.mark.parts(&ctx)) {
            out.push(MarkPart {
                x: cx + part.x * g,
                y: cy + part.y * g,
                scale: part.scale * g,
                ..part
            });
        }
    }
    out
}

/// How much of its size the body keeps while it gives way to its marks (`bodyScale`).
pub fn body_scale(marks: &[PoseMark]) -> f64 {
    let left = |weight: f64| 1.0 - smooth(clamp(weight, 0.0, 1.0) / 0.55);
    let scale = left(marks.iter().fold(0.0, |sum, m| sum + m.weight));
    if marks.iter().any(|m| grown(m.weight) > 0.0) {
        scale
    } else {
        jsmath::max(scale, left(MARK_FROM))
    }
}
