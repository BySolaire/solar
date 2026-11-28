//! Implied volume for a flat silhouette (port of `volume.ts`).
//!
//! Every filled subpath becomes a part with a front-surface height field (inflated slab, or a
//! solid of revolution about a vertical or a shared horizontal axis), cut once into two stacks
//! of plain cross-section paths: z-slices facing the viewer and x-slices facing sideways.

use std::sync::Arc;

use crate::contour::{loops_to_path, run_range, runs_of, Field, Tracer, RUN};
use crate::distance::{seg_dist2, EdgeIndex, SampleGrid};
use crate::geom::{BoxF, Outline};
use crate::jsmath;
use crate::path::{bounds, is_inside, Bounds, EdgeBands, Point};
use crate::validate::{
    analyze_body, drawn_eyes_fit, eye_centers, eye_clearance, gaze_range, AvatarDefinition,
    AvatarEyes, BodyAnalysis, GazeRange, EYE_H, EYE_W, GAZE_X, GAZE_Y,
};

/// How a part gets its depth (`PartKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartKind {
    Inflate,
    RevolveX,
    RevolveY,
}

impl PartKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PartKind::Inflate => "inflate",
            PartKind::RevolveX => "revolve-x",
            PartKind::RevolveY => "revolve-y",
        }
    }
}

/// Which stack a slice belongs to: `Z` the plane z = at (path in x, y); `X` the plane x = at
/// (path in u = -z, y).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Z,
    X,
}

impl Axis {
    pub fn as_char(self) -> char {
        match self {
            Axis::Z => 'z',
            Axis::X => 'x',
        }
    }
}

/// A cross-section outline, shared by the slices that draw it (the ± levels share one).
#[derive(Debug)]
pub struct SliceGeom {
    /// The path text exactly as the TS writes it (`VolumeSlice.d`) and the same outline as
    /// absolute commands.
    pub outline: Arc<Outline>,
    /// The cross-section as polygons in its plane's coordinates.
    pub loops: Vec<Vec<Point>>,
    /// Contour paths use evenodd; the core slice reuses the original nonzero outline.
    pub even_odd: bool,
    /// Path bounds in its plane's coordinates.
    pub bbox: BoxF,
}

/// One cross-section (`VolumeSlice`).
#[derive(Clone, Debug)]
pub struct VolumeSlice {
    pub axis: Axis,
    pub at: f64,
    /// z-slices: smallest distance from this cross-section to the outline (0 for the core and walls).
    pub inset: f64,
    pub geom: Arc<SliceGeom>,
}

/// One part of the body (`VolumePart`, without the build-time `height` closure).
#[derive(Clone, Debug)]
pub struct VolumePart {
    pub kind: PartKind,
    /// Outline plus the holes that cut it, serialized.
    pub d: String,
    pub polys: Vec<Vec<Point>>,
    pub bounds: Bounds,
    pub center: Point,
    /// Twice the largest inscribed radius (inflate) or the largest diameter (revolved).
    pub thickness: f64,
    /// Front-to-back extent, centered on z = 0.
    pub depth: f64,
    pub rxy: f64,
    pub rz: f64,
    /// Revolved parts: the axis (y for revolve-x, x for revolve-y).
    pub axis: Option<f64>,
    pub z_slices: Vec<VolumeSlice>,
    pub x_slices: Vec<VolumeSlice>,
}

/// Tangent plane at an eye center (`EyePlane`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EyePlane {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub dzdx: f64,
    pub dzdy: f64,
}

/// The implied volume (`Volume`).
#[derive(Clone, Debug)]
pub struct Volume {
    pub parts: Vec<VolumePart>,
    pub bounds: Bounds,
    /// Index of the part carrying the eyes.
    pub eye_part: usize,
    /// Highest host surface under each eye and its gaze travel.
    pub eye_z: [f64; 2],
    /// Drawn eye scale: the definition's scale enlarged as far as the body allows.
    pub eye_scale: f64,
    /// Tangent plane at each eye center.
    pub eye_planes: [EyePlane; 2],
}

/// Largest depth an inflated part may take, viewBox units.
pub const MAX_PART_DEPTH: f64 = 30.0;
/// Drawn eyes are up to this much larger than the contract's pill when the body has room.
pub const EYE_BOOST: f64 = 1.45;
/// Gap every drawn eye shape keeps from the outline before the renderer enlarges it.
pub const EYE_BOOST_PAD: f64 = 3.0;
/// Design target for generated avatars (advisory).
pub const FACE_ROOM_TARGET: f64 = 4.5;
const MAX_X_SLICES: f64 = 40.0;
const THIN: f64 = 4.0;
const THIN_REACH: f64 = 5.0;
const THIN_MARGIN: f64 = 2.0;
const FINE_Z: f64 = 0.6;

/// Distance from a point to the nearest edge of the polygons (`edgeDistance`).
pub fn edge_distance(polys: &[Vec<Point>], x: f64, y: f64) -> f64 {
    let mut best = f64::INFINITY;
    for poly in polys {
        let n = poly.len();
        let mut j = n.wrapping_sub(1);
        for i in 0..n {
            let d2 = seg_dist2(x, y, poly[j][0], poly[j][1], poly[i][0], poly[i][1]);
            if d2 < best {
                best = d2;
            }
            j = i;
        }
    }
    best.sqrt()
}

/// An inflated part's fillet (`Pick<VolumePart, 'depth' | 'rxy' | 'rz'>`).
#[derive(Clone, Copy, Debug)]
pub struct Fillet {
    pub depth: f64,
    pub rxy: f64,
    pub rz: f64,
}

/// Outline erosion of an inflated part at depth z (`erosionAt`).
pub fn erosion_at(part: Fillet, z: f64) -> f64 {
    let half = part.depth / 2.0;
    if part.rz <= 0.0 {
        return 0.0;
    }
    let u = (z.abs() - (half - part.rz)) / part.rz;
    if u <= 0.0 {
        return 0.0;
    }
    let m = jsmath::min(1.0, u);
    part.rxy * (1.0 - jsmath::max(0.0, 1.0 - m * m).sqrt())
}

/// How far inside the outline an inflated part's surface reaches `level` (`insetAt`).
pub fn inset_at(part: Fillet, level: f64) -> f64 {
    let half = part.depth / 2.0;
    if part.rxy <= 0.0 || part.rz <= 0.0 || level <= half - part.rz {
        return 0.0;
    }
    if level >= half {
        return part.rxy;
    }
    let s = (level - (half - part.rz)) / part.rz;
    part.rxy * (1.0 - jsmath::max(0.0, 1.0 - s * s).sqrt())
}

/// Front surface depth of an inflated part `dist` units inside the outline (`surfaceZ`).
pub fn surface_z(part: Fillet, dist: f64) -> f64 {
    let half = part.depth / 2.0;
    if part.rxy <= 0.0 || dist >= part.rxy {
        return half;
    }
    let k = 1.0 - jsmath::max(0.0, dist) / part.rxy;
    half - part.rz + jsmath::max(0.0, 1.0 - k * k).sqrt() * part.rz
}

/// Depth heuristic for inflated parts (`partDepth`).
pub fn part_depth(thickness: f64, b: Bounds, volume: f64) -> f64 {
    let long = jsmath::max(b.max_x - b.min_x, b.max_y - b.min_y);
    let raw = 0.8 * jsmath::max(thickness, jsmath::min(0.4 * long, 2.4 * thickness));
    jsmath::min(MAX_PART_DEPTH, jsmath::max(3.0, raw)) * volume
}

/// Sorted crossings of the line (y = s if `rows`, else x = s) with the polygons (`crossings`).
fn crossings(polys: &[Vec<Point>], rows: bool, s: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let (a, b) = if rows { (1, 0) } else { (0, 1) };
    for poly in polys {
        let n = poly.len();
        let mut j = n.wrapping_sub(1);
        for i in 0..n {
            let p = poly[j];
            let q = poly[i];
            if (p[a] <= s && q[a] > s) || (q[a] <= s && p[a] > s) {
                out.push(p[b] + ((s - p[a]) * (q[b] - p[b])) / (q[a] - p[a]));
            }
            j = i;
        }
    }
    out.sort_by(|m, n| m.partial_cmp(n).unwrap_or(std::cmp::Ordering::Equal));
    out
}

#[derive(Clone, Debug)]
struct Profile {
    start: f64,
    step: f64,
    r: Vec<f64>,
}

/// `revolveProfile`.
fn revolve_profile(
    polys: &[Vec<Point>],
    rows: bool,
    lo: f64,
    hi: f64,
    center: f64,
) -> Option<Profile> {
    let step = 0.2;
    let n = jsmath::max(2.0, ((hi - lo) / step).ceil() + 1.0) as usize;
    let mut r = vec![0.0f64; n];
    let mut span = 0.0f64;
    let mut mids = Vec::new();
    for (k, rk) in r.iter_mut().enumerate() {
        let s = jsmath::min(hi - 0.01, jsmath::max(lo + 0.01, lo + k as f64 * step));
        let xs = crossings(polys, rows, s);
        if xs.is_empty() {
            continue;
        }
        if xs.len() != 2 {
            return None;
        }
        *rk = (xs[1] - xs[0]) / 2.0;
        span = jsmath::max(span, xs[1] - xs[0]);
        mids.push((xs[0] + xs[1]) / 2.0);
    }
    let tol = jsmath::max(0.8, 0.04 * span);
    if mids.is_empty() || mids.iter().any(|m| (m - center).abs() > tol) {
        return None;
    }
    Some(Profile { start: lo, step, r })
}

fn radius_at(p: &Profile, s: f64) -> f64 {
    let t = (s - p.start) / p.step;
    let last = (p.r.len() - 1) as f64;
    if t < 0.0 || t > last {
        return 0.0;
    }
    let k = jsmath::min((p.r.len() - 2) as f64, t.floor()) as usize;
    p.r[k] + (p.r[k + 1] - p.r[k]) * (t - k as f64)
}

/// What gives a part its depth (`Surface`).
struct Surface {
    kind: PartKind,
    fillet: Fillet,
    profile: Option<Profile>,
    axis: f64,
    v: f64,
}

/// `surfaceHeight`.
fn surface_height(s: &Surface, x: f64, y: f64, sd: f64) -> f64 {
    if s.kind == PartKind::Inflate {
        return surface_z(s.fillet, sd);
    }
    let Some(p) = s.profile.as_ref() else {
        return 0.0;
    };
    if s.kind == PartKind::RevolveX {
        let r = radius_at(p, x);
        let t = y - s.axis;
        s.v * jsmath::max(0.0, r * r - t * t).sqrt()
    } else {
        let r = radius_at(p, y);
        let t = x - s.axis;
        s.v * jsmath::max(0.0, r * r - t * t).sqrt()
    }
}

/// Horizontal axis of revolution, if the body looks like a turned object seen from the side
/// (`revolveAxis`).
pub fn revolve_axis(body: &BodyAnalysis) -> Option<f64> {
    let b = body.bounds;
    let outlines: Vec<_> = body.subpaths.iter().filter(|s| !s.hole).collect();
    if outlines.len() < 2 || b.max_x - b.min_x < 1.1 * (b.max_y - b.min_y) {
        return None;
    }
    let axis = (b.min_y + b.max_y) / 2.0;
    for o in &outlines {
        let ob = bounds(&o.polys);
        if ((ob.min_y + ob.max_y) / 2.0 - axis).abs() > 1.5 {
            return None;
        }
    }
    let (mut filled, mut off) = (0u64, 0u64);
    let mut xs = Vec::new();
    let mut x = b.min_x + 0.5;
    while x < b.max_x {
        xs.push(x);
        x += 1.0;
    }
    let mut top = vec![0u8; xs.len()];
    let mut bottom = vec![0u8; xs.len()];
    let index = EdgeBands::new(&body.polys);
    let mut y = b.min_y + 0.5;
    while y < axis {
        index.inside_row(y, &xs, &mut top);
        index.inside_row(2.0 * axis - y, &xs, &mut bottom);
        for k in 0..xs.len() {
            if top[k] != 0 || bottom[k] != 0 {
                filled += 1;
            }
            if top[k] != bottom[k] {
                off += 1;
            }
        }
        y += 1.0;
    }
    if filled > 0 && (off as f64) / (filled as f64) < 0.02 {
        Some(axis)
    } else {
        None
    }
}

/// Sampled signed distance (positive inside) and height over a part's bounds (`Grid`).
struct Grid {
    x0: f64,
    y0: f64,
    step: f64,
    nx: usize,
    ny: usize,
    sd: Vec<f64>,
    h: Vec<f64>,
    inside: Vec<u8>,
}

fn grid_size(b: Bounds, step: f64) -> (usize, usize) {
    let pad = 1.0;
    let nx = ((b.max_x - b.min_x + 2.0 * pad) / step).ceil() as usize + 1;
    let ny = ((b.max_y - b.min_y + 2.0 * pad) / step).ceil() as usize + 1;
    (nx, ny)
}

/// `sampleGrid`.
fn sample_grid(rows: &EdgeBands, b: Bounds, step: f64, index: &mut EdgeIndex) -> Grid {
    let pad = 1.0;
    let x0 = b.min_x - pad;
    let y0 = b.min_y - pad;
    let (nx, ny) = grid_size(b, step);
    let mut sd = index.distances(SampleGrid {
        x0,
        y0,
        step,
        nx,
        ny,
    });
    let mut inside = vec![0u8; nx * ny];
    let xs: Vec<f64> = (0..nx).map(|i| x0 + i as f64 * step).collect();
    let mut row = vec![0u8; nx];
    for j in 0..ny {
        rows.inside_row(y0 + j as f64 * step, &xs, &mut row);
        inside[j * nx..(j + 1) * nx].copy_from_slice(&row);
        for i in 0..nx {
            if row[i] == 0 {
                sd[j * nx + i] = -sd[j * nx + i];
            }
        }
    }
    Grid {
        x0,
        y0,
        step,
        nx,
        ny,
        sd,
        h: vec![0.0; nx * ny],
        inside,
    }
}

/// `gridThickness`.
fn grid_thickness(rows: &EdgeBands, b: Bounds, step: f64, index: &mut EdgeIndex) -> f64 {
    let pad = 1.0;
    let (nx, ny) = grid_size(b, step);
    2.0 * index.widest(
        SampleGrid {
            x0: b.min_x - pad,
            y0: b.min_y - pad,
            step,
            nx,
            ny,
        },
        rows,
    )
}

/// `sampleHeights`.
fn sample_heights(g: &mut Grid, surface: &Surface) {
    for j in 0..g.ny {
        let y = g.y0 + j as f64 * g.step;
        for i in 0..g.nx {
            let k = j * g.nx + i;
            if g.inside[k] != 0 {
                g.h[k] = surface_height(surface, g.x0 + i as f64 * g.step, y, g.sd[k]);
            }
        }
    }
}

fn loop_box(loops: &[Vec<Point>]) -> BoxF {
    let b = bounds(loops);
    BoxF {
        x: b.min_x,
        y: b.min_y,
        w: b.max_x - b.min_x,
        h: b.max_y - b.min_y,
    }
}

/// A rectangle of grid samples, inclusive, and the highest surface over its thin samples (`ThinBox`).
#[derive(Clone, Copy, Debug)]
struct ThinBox {
    i0: i64,
    i1: i64,
    j0: i64,
    j1: i64,
    top: f64,
}

/// A part's cross-section at a level (`LevelCut`): `insetCuts` or `revolveCuts`.
enum LevelCut {
    Inset {
        fillet: Fillet,
        min: Vec<f64>,
        max: Vec<f64>,
        flat: Vec<u8>,
    },
    Revolve {
        base: Vec<f64>,
        over: Vec<f64>,
        values: Vec<f64>,
        rise: (Vec<f64>, Vec<f64>),
        edge: (Vec<f64>, Vec<f64>),
        flat: Vec<u8>,
        v: f64,
    },
}

impl LevelCut {
    fn inset(g: &Grid, fillet: Fillet) -> LevelCut {
        let (min, max) = run_range(&g.sd, g.nx, g.ny);
        let flat = vec![0u8; min.len()];
        LevelCut::Inset {
            fillet,
            min,
            max,
            flat,
        }
    }

    fn revolve(g: &Grid, surface: &Surface) -> LevelCut {
        let (nx, ny) = (g.nx, g.ny);
        let along_x = surface.kind == PartKind::RevolveX;
        let mut base = vec![0.0f64; nx * ny];
        let mut over = vec![0.0f64; nx * ny];
        if let Some(p) = surface.profile.as_ref() {
            for j in 0..ny {
                let y = g.y0 + j as f64 * g.step;
                for i in 0..nx {
                    let x = g.x0 + i as f64 * g.step;
                    let r = radius_at(p, if along_x { x } else { y });
                    let t = if along_x {
                        y - surface.axis
                    } else {
                        x - surface.axis
                    };
                    base[j * nx + i] = r * r - t * t;
                    over[j * nx + i] = 2.0 * jsmath::max(r, 0.5);
                }
            }
        }
        let rise = run_range(&base, nx, ny);
        let edge = run_range(&g.sd, nx, ny);
        let flat = vec![0u8; rise.0.len()];
        LevelCut::Revolve {
            base,
            over,
            values: vec![0.0; nx * ny],
            rise,
            edge,
            flat,
            v: surface.v,
        }
    }

    fn cut(
        &mut self,
        tracer: &mut Tracer,
        g: &Grid,
        level: f64,
        bx: Option<ThinBox>,
    ) -> Vec<Vec<Point>> {
        match self {
            LevelCut::Inset {
                fillet,
                min,
                max,
                flat,
            } => {
                let drop = inset_at(*fillet, level);
                let Some(bx) = bx else {
                    for k in 0..flat.len() {
                        flat[k] = u8::from(min[k] - drop >= 0.0 || max[k] - drop < 0.0);
                    }
                    let whole = Field {
                        values: &g.sd,
                        nx: g.nx,
                        ny: g.ny,
                        x0: g.x0,
                        y0: g.y0,
                        step: g.step,
                    };
                    return tracer.contour_outlines(&whole, drop, 0.04, Some(flat));
                };
                let i0 = (bx.i0 - 1).max(0);
                let i1 = (g.nx as i64 - 1).min(bx.i1 + 1);
                let j0 = (bx.j0 - 1).max(0);
                let j1 = (g.ny as i64 - 1).min(bx.j1 + 1);
                let nx = (i1 - i0 + 1) as usize;
                let ny = (j1 - j0 + 1) as usize;
                let mut values = vec![0.0f64; nx * ny];
                for j in 0..ny as i64 {
                    for i in 0..nx as i64 {
                        let sd = g.sd[((j + j0) as usize) * g.nx + (i + i0) as usize];
                        let v = jsmath::min(sd - drop, sd);
                        let edge = (i + i0 - bx.i0)
                            .min(bx.i1 - i - i0)
                            .min(j + j0 - bx.j0)
                            .min(bx.j1 - j - j0);
                        values[j as usize * nx + i as usize] = jsmath::min(v, g.step * edge as f64);
                    }
                }
                let field = Field {
                    values: &values,
                    nx,
                    ny,
                    x0: g.x0 + i0 as f64 * g.step,
                    y0: g.y0 + j0 as f64 * g.step,
                    step: g.step,
                };
                tracer.contour_outlines(&field, 0.0, 0.04, None)
            }
            LevelCut::Revolve {
                base,
                over,
                values,
                rise,
                edge,
                flat,
                v,
            } => {
                let (nx, ny) = (g.nx, g.ny);
                let q = level / jsmath::max(*v, 1e-6);
                let drop = q * q;
                let runs = runs_of(nx);
                for k in 0..flat.len() {
                    let out = rise.1[k] - drop < 0.0 || edge.1[k] < 0.0;
                    let within = rise.0[k] - drop >= 0.0 && edge.0[k] >= 0.0;
                    flat[k] = u8::from(out || within);
                    if flat[k] != 0 {
                        continue;
                    }
                    let r = k % runs;
                    let j = (k - r) / runs;
                    let end = j * nx + (nx - 1).min((r + 1) * RUN);
                    for q in j * nx + r * RUN..=end {
                        values[q] = jsmath::min((base[q] - drop) / over[q], g.sd[q]);
                        values[q + nx] =
                            jsmath::min((base[q + nx] - drop) / over[q + nx], g.sd[q + nx]);
                    }
                }
                let field = Field {
                    values,
                    nx,
                    ny,
                    x0: g.x0,
                    y0: g.y0,
                    step: g.step,
                };
                tracer.contour_outlines(&field, 0.0, 0.04, Some(flat))
            }
        }
    }
}

/// Boxes around the thin parts of a part's outline (`thinBoxes`).
fn thin_boxes(g: &Grid) -> Vec<ThinBox> {
    let (nx, ny) = (g.nx, g.ny);
    let reach = (THIN_REACH / g.step).ceil() as usize;
    let mut wide = vec![0u8; nx * ny];
    let mut near = vec![0u8; nx * ny];
    let mut count = vec![0i32; nx.max(ny) + 1];
    for j in 0..ny {
        for i in 0..nx {
            count[i + 1] = count[i] + i32::from(g.sd[j * nx + i] >= THIN);
        }
        for i in 0..nx {
            wide[j * nx + i] =
                u8::from(count[nx.min(i + reach + 1)] > count[i.saturating_sub(reach)]);
        }
    }
    for i in 0..nx {
        for j in 0..ny {
            count[j + 1] = count[j] + wide[j * nx + i] as i32;
        }
        for j in 0..ny {
            near[j * nx + i] =
                u8::from(count[ny.min(j + reach + 1)] > count[j.saturating_sub(reach)]);
        }
    }
    let thin = |k: usize| g.sd[k] > 0.0 && near[k] == 0;
    let mut seen = vec![0u8; nx * ny];
    let grow = (THIN_MARGIN / g.step).ceil() as i64;
    let mut boxes: Vec<ThinBox> = Vec::new();
    for start in 0..nx * ny {
        if seen[start] != 0 || !thin(start) {
            continue;
        }
        let mut bx = ThinBox {
            i0: nx as i64,
            i1: -1,
            j0: ny as i64,
            j1: -1,
            top: 0.0,
        };
        let mut cells = 0usize;
        let mut stack = vec![start];
        seen[start] = 1;
        while let Some(k) = stack.pop() {
            let i = k % nx;
            let j = (k - i) / nx;
            cells += 1;
            bx.top = jsmath::max(bx.top, g.h[k]);
            bx.i0 = bx.i0.min(i as i64);
            bx.i1 = bx.i1.max(i as i64);
            bx.j0 = bx.j0.min(j as i64);
            bx.j1 = bx.j1.max(j as i64);
            let neighbors = [
                if i > 0 { Some(k - 1) } else { None },
                if i < nx - 1 { Some(k + 1) } else { None },
                if j > 0 { Some(k - nx) } else { None },
                if j < ny - 1 { Some(k + nx) } else { None },
            ];
            for n in neighbors.into_iter().flatten() {
                if seen[n] == 0 && thin(n) {
                    seen[n] = 1;
                    stack.push(n);
                }
            }
        }
        if (cells as f64) * g.step * g.step < 2.0 {
            continue;
        }
        boxes.push(ThinBox {
            i0: (bx.i0 - grow).max(0),
            i1: (nx as i64 - 1).min(bx.i1 + grow),
            j0: (bx.j0 - grow).max(0),
            j1: (ny as i64 - 1).min(bx.j1 + grow),
            top: bx.top,
        });
    }
    let mut a = 0;
    while a < boxes.len() {
        let mut b = a + 1;
        while b < boxes.len() {
            let (p, q) = (boxes[a], boxes[b]);
            if p.i0 > q.i1 || q.i0 > p.i1 || p.j0 > q.j1 || q.j0 > p.j1 {
                b += 1;
                continue;
            }
            boxes[a] = ThinBox {
                i0: p.i0.min(q.i0),
                i1: p.i1.max(q.i1),
                j0: p.j0.min(q.j0),
                j1: p.j1.max(q.j1),
                top: jsmath::max(p.top, q.top),
            };
            boxes.remove(b);
            b = a + 1;
        }
        a += 1;
    }
    boxes
}

/// Side profile for the slab of grid columns c0..c1 (`columnLoops`).
#[allow(clippy::needless_range_loop)]
fn column_loops(
    tracer: &mut Tracer,
    g: &Grid,
    c0: usize,
    c1: usize,
    surface: &Surface,
) -> Vec<Vec<Point>> {
    let mut loops = Vec::new();
    let mut sd = vec![f64::NEG_INFINITY; g.ny];
    let mut h = vec![0.0f64; g.ny];
    for j in 0..g.ny {
        for c in c0..=c1 {
            let k = j * g.nx + c;
            if g.sd[k] > sd[j] {
                sd[j] = g.sd[k];
            }
            if g.sd[k] > 0.0 && g.h[k] > h[j] {
                h[j] = g.h[k];
            }
        }
    }
    let mut xy = vec![0.0f64; 4 * (g.ny + 2)];
    let mut j = 0;
    while j < g.ny {
        if sd[j] <= 0.0 {
            j += 1;
            continue;
        }
        let j0 = j;
        while j < g.ny && sd[j] > 0.0 {
            j += 1;
        }
        let j1 = j - 1;
        let mut n = 0;
        if j0 > 0 {
            n = rim_point(g, c0, c1, surface, &sd, &h, j0, j0 - 1, &mut xy, n);
        } else {
            xy[n] = h[j0];
            xy[n + 1] = g.y0 + j0 as f64 * g.step;
            n += 2;
        }
        for k in j0..=j1 {
            xy[n] = h[k];
            xy[n + 1] = g.y0 + k as f64 * g.step;
            n += 2;
        }
        if j1 < g.ny - 1 {
            n = rim_point(g, c0, c1, surface, &sd, &h, j1, j1 + 1, &mut xy, n);
        } else {
            xy[n] = h[j1];
            xy[n + 1] = g.y0 + j1 as f64 * g.step;
            n += 2;
        }
        let mut top = f64::NEG_INFINITY;
        let mut k = 0;
        while k < n {
            top = jsmath::max(top, xy[k]);
            k += 2;
        }
        if top < 0.05 {
            continue;
        }
        let mut m = n;
        let mut k = n as i64 - 2;
        while k >= 0 {
            xy[m] = -xy[k as usize];
            xy[m + 1] = xy[k as usize + 1];
            m += 2;
            k -= 2;
        }
        loops.push(tracer.simplified_loop(&xy, n, 0.04));
    }
    loops
}

/// `rimPoint`.
#[allow(clippy::too_many_arguments)]
fn rim_point(
    g: &Grid,
    c0: usize,
    c1: usize,
    surface: &Surface,
    sd: &[f64],
    h: &[f64],
    inside: usize,
    outside: usize,
    xy: &mut [f64],
    n: usize,
) -> usize {
    let y_in = g.y0 + inside as f64 * g.step;
    let y =
        y_in + ((g.y0 + outside as f64 * g.step - y_in) * sd[inside]) / (sd[inside] - sd[outside]);
    let mut top = 0.0f64;
    for c in c0..=c1 {
        top = jsmath::max(
            top,
            surface_height(surface, g.x0 + c as f64 * g.step, y, 0.0),
        );
    }
    xy[n] = jsmath::min(h[inside], top);
    xy[n + 1] = y;
    n + 2
}

/// Depth levels in (flat, flat + round), even in angle (`levels`).
fn levels(flat: f64, round: f64, steps: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let mut k = 1.0;
    while k < steps {
        out.push(flat + round * jsmath::sin((k / steps) * (std::f64::consts::PI / 2.0)));
        k += 1.0;
    }
    out
}

/// A part's height at a point, for the eyes (the TS `part.height` closure).
struct PartHeight {
    surface: Surface,
    filled: EdgeBands,
    index: EdgeIndex,
}

impl PartHeight {
    fn height(&self, x: f64, y: f64) -> f64 {
        if self.filled.winding(x, y) != 0 {
            let sd = if self.surface.kind == PartKind::Inflate {
                self.index.distance(x, y)
            } else {
                0.0
            };
            surface_height(&self.surface, x, y, sd)
        } else {
            0.0
        }
    }
}

/// Build the implied volume (`buildVolume`). `volume` scales every depth (0 = flat); `body` is
/// the analysis of the definition's path.
pub fn build_volume(def: &AvatarDefinition, volume: f64, body: &BodyAnalysis) -> Volume {
    let outlines: Vec<_> = body.subpaths.iter().filter(|s| !s.hole).collect();
    let holes: Vec<_> = body.subpaths.iter().filter(|s| s.hole).collect();
    let v = if volume.is_finite() {
        jsmath::max(0.0, volume)
    } else {
        1.0
    };
    let x_axis = revolve_axis(body);
    let mut tracer = Tracer::new();
    let mut heights: Vec<PartHeight> = Vec::new();

    let mut parts = Vec::new();
    for o in &outlines {
        let cuts: Vec<_> = holes
            .iter()
            .filter(|h| h.probe.is_some_and(|p| is_inside(&o.polys, p[0], p[1])))
            .collect();
        let mut polys = o.polys.clone();
        for h in &cuts {
            polys.extend(h.polys.iter().cloned());
        }
        let b = bounds(&o.polys);
        let center: Point = [(b.min_x + b.max_x) / 2.0, (b.min_y + b.max_y) / 2.0];
        let min_dim = jsmath::min(b.max_x - b.min_x, b.max_y - b.min_y);
        let step = jsmath::max(0.25, jsmath::min(0.45, min_dim / 24.0));

        let mut kind = PartKind::Inflate;
        let mut profile: Option<Profile> = None;
        if let Some(xa) = x_axis {
            profile = revolve_profile(&polys, false, b.min_x, b.max_x, xa);
            if profile.is_some() {
                kind = PartKind::RevolveX;
            }
        } else if outlines.len() > 1 && cuts.is_empty() {
            profile = revolve_profile(&polys, true, b.min_y, b.max_y, center[0]);
            if profile.is_some() {
                kind = PartKind::RevolveY;
            }
        }

        let mut index = EdgeIndex::new(&polys, b);
        let filled = EdgeBands::new(&polys);
        let (thickness, depth);
        let (mut rxy, mut rz, mut axis) = (0.0, 0.0, 0.0);
        match (&profile, kind) {
            (Some(p), PartKind::RevolveX | PartKind::RevolveY) => {
                axis = if kind == PartKind::RevolveX {
                    x_axis.unwrap_or(0.0)
                } else {
                    center[0]
                };
                let radius = jsmath::max_of(p.r.iter().copied());
                thickness = 2.0 * radius;
                depth = 2.0 * radius * v;
            }
            _ => {
                thickness = grid_thickness(&filled, b, jsmath::max(step, 0.6), &mut index);
                depth = part_depth(thickness, b, v);
                rxy = jsmath::min(thickness / 2.0, depth / 2.0) * 0.9;
                rz = (depth / 2.0) * 0.85;
            }
        }
        let surface = Surface {
            kind,
            fillet: Fillet { depth, rxy, rz },
            profile,
            axis,
            v,
        };

        let mut core_d = o.d.clone();
        let mut core_cmds = o.segs.clone();
        for h in &cuts {
            core_d.push_str(&h.d);
            core_cmds.extend_from_slice(&h.segs);
        }
        let core = VolumeSlice {
            axis: Axis::Z,
            at: 0.0,
            inset: 0.0,
            geom: Arc::new(SliceGeom {
                outline: Arc::new(Outline::with_d(core_d.clone(), core_cmds)),
                loops: polys.clone(),
                even_odd: false,
                bbox: BoxF {
                    x: b.min_x,
                    y: b.min_y,
                    w: b.max_x - b.min_x,
                    h: b.max_y - b.min_y,
                },
            }),
        };
        let mut z_slices = vec![core];
        let mut x_slices = Vec::new();
        let half = depth / 2.0;
        if half > 0.05 {
            let mut g = sample_grid(&filled, b, step, &mut index);
            sample_heights(&mut g, &surface);
            let steps = if kind == PartKind::Inflate {
                jsmath::max(4.0, jsmath::min(14.0, jsmath::round(rz / 0.8)))
            } else {
                jsmath::max(5.0, jsmath::min(18.0, jsmath::round(half / 1.1)))
            };
            let ls: Vec<f64> = if kind == PartKind::Inflate {
                let mut l = vec![half - rz];
                l.extend(levels(half - rz, rz, steps));
                l
            } else {
                levels(0.0, half, steps)
            };
            let mut cut = if kind == PartKind::Inflate {
                LevelCut::inset(&g, surface.fillet)
            } else {
                LevelCut::revolve(&g, &surface)
            };
            let mut add_level = |level: f64,
                                 bx: Option<ThinBox>,
                                 z_slices: &mut Vec<VolumeSlice>,
                                 tracer: &mut Tracer|
             -> bool {
                let loops = cut.cut(tracer, &g, level, bx);
                if loops.is_empty() {
                    return false;
                }
                let mut inset = f64::INFINITY;
                let mut k = 0usize;
                for lp in &loops {
                    for p in lp {
                        if k.is_multiple_of(3) {
                            inset = jsmath::min(inset, index.distance(p[0], p[1]));
                        }
                        k += 1;
                    }
                }
                let path = loops_to_path(&loops);
                let bbox = loop_box(&loops);
                let geom = Arc::new(SliceGeom {
                    outline: Arc::new(Outline::with_d(path.d, path.cmds)),
                    loops,
                    even_odd: true,
                    bbox,
                });
                z_slices.push(VolumeSlice {
                    axis: Axis::Z,
                    at: level,
                    inset,
                    geom: geom.clone(),
                });
                z_slices.push(VolumeSlice {
                    axis: Axis::Z,
                    at: -level,
                    inset,
                    geom,
                });
                true
            };
            let mut used = vec![0.0f64];
            for &level in &ls {
                if level >= 0.02 && add_level(level, None, &mut z_slices, &mut tracer) {
                    used.push(level);
                }
            }
            let thin = if kind == PartKind::Inflate {
                thin_boxes(&g)
            } else {
                Vec::new()
            };
            for bx in &thin {
                let mut k = 1;
                while k < used.len() && used[k - 1] < bx.top {
                    let a = used[k - 1];
                    let gap = used[k] - a;
                    let m = (gap / FINE_Z - 1e-9).ceil();
                    let mut q = 1.0;
                    while q < m && a + (gap * q) / m < bx.top {
                        add_level(a + (gap * q) / m, Some(*bx), &mut z_slices, &mut tracer);
                        q += 1.0;
                    }
                    k += 1;
                }
            }
            z_slices.sort_by(|m, n| m.at.partial_cmp(&n.at).unwrap_or(std::cmp::Ordering::Equal));

            let width = b.max_x - b.min_x;
            let every = jsmath::max(
                1.0,
                jsmath::round(jsmath::max(1.1, width / MAX_X_SLICES) / g.step),
            ) as usize;
            let before = (every - 1) / 2;
            let mut i = 1;
            while i + 1 < g.nx {
                let c0 = i.saturating_sub(before);
                let c1 = (g.nx - 1).min((i + every - 1).saturating_sub(before));
                let loops = column_loops(&mut tracer, &g, c0, c1, &surface);
                if !loops.is_empty() {
                    let path = loops_to_path(&loops);
                    let bbox = loop_box(&loops);
                    x_slices.push(VolumeSlice {
                        axis: Axis::X,
                        at: g.x0 + i as f64 * g.step,
                        inset: 0.0,
                        geom: Arc::new(SliceGeom {
                            outline: Arc::new(Outline::with_d(path.d, path.cmds)),
                            loops,
                            even_odd: true,
                            bbox,
                        }),
                    });
                }
                i += every;
            }
        }

        parts.push(VolumePart {
            kind,
            d: core_d,
            polys,
            bounds: b,
            center,
            thickness,
            depth,
            rxy,
            rz,
            axis: match kind {
                PartKind::RevolveX => x_axis,
                PartKind::RevolveY => Some(center[0]),
                PartKind::Inflate => None,
            },
            z_slices,
            x_slices,
        });
        heights.push(PartHeight {
            surface,
            filled,
            index,
        });
    }

    let whole = EdgeBands::new(&body.polys);
    let eye_scale = boosted_eye_scale(&whole, &def.eyes);
    let centers = eye_centers(&def.eyes);
    let mut eye_part = 0;
    let mut best_z = f64::NEG_INFINITY;
    for (i, part) in heights.iter().enumerate() {
        let zs = centers.map(|[x, y]| part.height(x, y));
        if zs.iter().any(|&z| z <= 0.0) {
            continue;
        }
        let z = jsmath::min_of(zs);
        if z > best_z {
            best_z = z;
            eye_part = i;
        }
    }
    let host = heights.get(eye_part);
    let w = (EYE_W * eye_scale) / 2.0 + GAZE_X * eye_scale;
    let hh = (EYE_H * eye_scale) / 2.0 + GAZE_Y * eye_scale;
    let eye_z = centers.map(|[x, y]| {
        let mut top = 0.0f64;
        for fx in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            for fy in [-1.0, -0.5, 0.0, 0.5, 1.0] {
                top = jsmath::max(top, host.map_or(0.0, |h| h.height(x + fx * w, y + fy * hh)));
            }
        }
        top
    });
    let eye_planes = centers.map(|[x, y]| match host {
        None => EyePlane {
            x,
            y,
            z: 0.0,
            dzdx: 0.0,
            dzdy: 0.0,
        },
        Some(host) => {
            let e = 0.75;
            let dzdx = (host.height(x + e, y) - host.height(x - e, y)) / (2.0 * e);
            let dzdy = (host.height(x, y + e) - host.height(x, y - e)) / (2.0 * e);
            EyePlane {
                x,
                y,
                z: host.height(x, y),
                dzdx,
                dzdy,
            }
        }
    });

    Volume {
        parts,
        bounds: body.bounds,
        eye_part,
        eye_z,
        eye_scale,
        eye_planes,
    }
}

/// Largest eye scale up to EYE_BOOST × the definition's whose drawn eyes keep EYE_BOOST_PAD from
/// the outline (`boostedEyeScale`). `whole` indexes the body's polygons.
pub fn boosted_eye_scale(whole: &EdgeBands, eyes: &AvatarEyes) -> f64 {
    for k in [EYE_BOOST, 1.38, 1.3, 1.22, 1.15, 1.08] {
        let e = AvatarEyes {
            scale: eyes.scale * k,
            ..*eyes
        };
        if drawn_eyes_fit(whole, &e, 0.0, 0.0, EYE_BOOST_PAD).is_none() {
            return eyes.scale * k;
        }
    }
    eyes.scale
}

/// How much room the face has as the renderer will draw it (`FaceRoom`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceRoom {
    pub boost: f64,
    pub clearance: f64,
    pub gaze: GazeRange,
    pub ok: bool,
}

/// `faceRoom`.
pub fn face_room(def: &AvatarDefinition, body: Option<&BodyAnalysis>) -> Option<FaceRoom> {
    let owned;
    let body = match body {
        Some(b) => b,
        None => {
            owned = analyze_body(&def.body.path, true).ok()?;
            &owned
        }
    };
    let whole = EdgeBands::new(&body.polys);
    let scale = boosted_eye_scale(&whole, &def.eyes);
    let eyes = AvatarEyes { scale, ..def.eyes };
    let clearance = eye_clearance(&whole, &eyes);
    let gaze = gaze_range(&whole, &eyes);
    let boost = jsmath::round((scale / def.eyes.scale) * 100.0) / 100.0;
    let ok = boost >= EYE_BOOST - 1e-9
        && clearance >= FACE_ROOM_TARGET
        && jsmath::min(
            jsmath::min(gaze.left, gaze.right),
            jsmath::min(gaze.up, gaze.down),
        ) == 1.0;
    Some(FaceRoom {
        boost,
        clearance,
        gaze,
        ok,
    })
}
