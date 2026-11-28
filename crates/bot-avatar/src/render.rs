//! Frames for a validated silhouette with implied volume (port of `render.ts`, flat material):
//! [`PreparedAvatar`] is the TS `Prepared` cache entry (validated definition, volume, gaze range,
//! seed, path ids, lazily measured ribbon rims), and [`PreparedAvatar::frame`] /
//! [`PreparedAvatar::avatar_frame`] are `liveFrame` / `avatarFrame`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, OnceLock};

use crate::color::{mix, oklch, parse_hex, to_hex, to_oklch, Rgb};
use crate::effects::{body_scale, mark_parts, ribbon_pieces, ribbon_reach, rim_of, Rim};
use crate::eyes::{eye_path_cmds, plain_look, EyeSpot, PlainLook};
use crate::geom::{about, invert, mul, translate, BoxF, Matrix, Outline, PathCmd};
use crate::jsmath::{self, clamp, cos, hypot, hypot3, number_to_string, sin, to_base36};
use crate::marks::MarkPart;
use crate::motion::{
    compute_pose, hash_string, noise, AvatarMood, AvatarMotion, EyeLook, Gaze, MotionBlend, Pose,
    PoseInput, State, View,
};
use crate::path::{Bounds, EdgeBands, Point};
use crate::validate::{
    analyze_body, eye_centers, gaze_range, happy_arc, validate_avatar, AvatarDefinition,
    AvatarEyes, GazeRange, EYE_H, EYE_W, GAZE_X, GAZE_Y,
};
use crate::volume::{build_volume, Axis, EyePlane, Volume, VolumeSlice};

/// `DEFAULT_COLOR`.
pub const DEFAULT_COLOR: &str = "#ff6a1a";
/// `DEFAULT_EYE_COLOR`.
pub const DEFAULT_EYE_COLOR: &str = "#151515";
/// `depth` option value that maps to the natural inferred volume.
pub const DEFAULT_DEPTH: f64 = 6.0;
const MAX_DEPTH: f64 = 14.0;
const CSS_EYE_LIFT: f64 = 1.5;

/// What a frame is computed from (`RenderOptions`). Every field may be left out.
#[derive(Clone, Debug, Default)]
pub struct RenderOptions {
    /// Body color as #rgb or #rrggbb. Invalid values fall back to the default.
    pub color: Option<String>,
    /// Eye color as #rgb or #rrggbb.
    pub eye_color: Option<String>,
    /// Rendered width in CSS pixels: live frames skip cross-sections less than a pixel apart.
    pub size: Option<f64>,
    /// Volume amount 0..14 (default 6). Only the free functions read it; a [`PreparedAvatar`]
    /// has its own depth.
    pub depth: Option<f64>,
    /// Extra view rotation in radians.
    pub view: Option<View>,
    /// Animation clock in seconds.
    pub time: Option<f64>,
    /// Look target, each axis -1..1.
    pub gaze: Option<Gaze>,
    /// What the bot is doing (default idle).
    pub state: Option<State>,
    /// How the bot feels (default neutral).
    pub mood: Option<AvatarMood>,
    /// Deprecated single option (see [`AvatarMotion`]).
    pub motion: Option<AvatarMotion>,
    /// Seconds since `state` began.
    pub state_age: Option<f64>,
    /// Ease into `state` and `mood` from what was shown before.
    pub motion_blend: Option<MotionBlend>,
    /// Freeze body motion, blinking, ribbons and marks; gaze still applies.
    pub reduced_motion: bool,
    /// Upstream colours, or Solar's ribbon tuning (the default; see [`Tuning`]).
    pub tuning: Tuning,
}

/// The ribbon colouring. [`Tuning::Upstream`] is the engine exactly as published (the golden
/// tests use it). [`Tuning::Solar`], the default for the app, applies Solar's art direction
/// (DIRECTION.md, 2026-11-27) and nothing else, three changes to `ribbonPalette`:
///
/// - `NEUTRAL_TINT` 0.025 → 0: a white, gray or black body gets strictly neutral gray ribbons;
/// - `h = turnToward(base, step > 0 ? WARM : WARM + 180, …)` → `step > 0 ? turnToward(base, WARM, …) : base`;
/// - `hue = turnToward(h, away > 0 ? WARM : WARM + 180, …)` → `away > 0 ? turnToward(h, WARM, …) : h`.
///
/// So lighter steps keep their turn toward warm and darker ones keep the body's hue instead of
/// turning toward blue. Geometry, motion and eyes are the same either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Tuning {
    #[default]
    Solar,
    Upstream,
}

/// `depthOption`: clamped to 0..14 and rounded to halves.
pub fn depth_option(depth: Option<f64>) -> f64 {
    let d = clamp(
        depth.filter(|d| d.is_finite()).unwrap_or(DEFAULT_DEPTH),
        0.0,
        MAX_DEPTH,
    );
    jsmath::round(d * 2.0) / 2.0
}

/// The view's rotation (`ViewBasis`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewBasis {
    pub c_y: f64,
    pub s_y: f64,
    pub c_p: f64,
    pub s_p: f64,
    /// cos(yaw)·cos(pitch): > 0 when the front faces the viewer.
    pub facing: f64,
}

/// `viewBasis`.
pub fn view_basis(yaw: f64, pitch: f64) -> ViewBasis {
    let c_y = cos(yaw);
    let c_p = cos(pitch);
    ViewBasis {
        c_y,
        s_y: sin(yaw),
        c_p,
        s_p: sin(pitch),
        facing: c_y * c_p,
    }
}

/// Toward-viewer component of a model-space direction (`facingOf`).
pub fn facing_of(v: &ViewBasis, [x, y, z]: [f64; 3]) -> f64 {
    v.s_p * y + v.c_p * (-v.s_y * x + v.c_y * z)
}

fn eye_visible(v: &ViewBasis, plane: &EyePlane) -> bool {
    facing_of(v, [-plane.dzdx, -plane.dzdy, 1.0]) > 1e-6
}

fn eye_matrix(v: &ViewBasis, plane: &EyePlane, pivot: Point) -> Matrix {
    let mut m = plane_matrix(
        v,
        plane.z - plane.x * plane.dzdx - plane.y * plane.dzdy,
        pivot,
    );
    m[0] += v.s_y * plane.dzdx;
    m[1] -= v.c_y * v.s_p * plane.dzdx;
    m[2] += v.s_y * plane.dzdy;
    m[3] -= v.c_y * v.s_p * plane.dzdy;
    m
}

/// Which slice stack faces the viewer more (`activeStack`).
pub fn active_stack(v: &ViewBasis) -> Axis {
    if v.c_y.abs() >= v.s_y.abs() {
        Axis::Z
    } else {
        Axis::X
    }
}

/// Orthographic projection of the plane at depth z, pivoting on `pivot` (`planeMatrix`).
pub fn plane_matrix(v: &ViewBasis, z: f64, pivot: Point) -> Matrix {
    let [cx, cy] = pivot;
    [
        v.c_y,
        v.s_y * v.s_p,
        0.0,
        v.c_p,
        cx - v.c_y * cx + v.s_y * z,
        cy - v.c_p * cy - v.s_y * v.s_p * cx - v.c_y * v.s_p * z,
    ]
}

/// Projection of the side plane x = x0, local coordinates (u = -z, y) (`sidePlaneMatrix`).
pub fn side_plane_matrix(v: &ViewBasis, x0: f64, pivot: Point) -> Matrix {
    let [cx, cy] = pivot;
    [
        -v.s_y,
        v.c_y * v.s_p,
        0.0,
        v.c_p,
        cx + v.c_y * (x0 - cx),
        cy - v.c_p * cy + v.s_y * v.s_p * (x0 - cx),
    ]
}

/// Project a 3D point: screen x, screen y and depth toward the viewer (`projectPoint`).
pub fn project_point(v: &ViewBasis, [x, y, z]: [f64; 3], pivot: Point) -> [f64; 3] {
    let m = plane_matrix(v, z, pivot);
    let xx = x - pivot[0];
    let yy = y - pivot[1];
    [
        m[0] * x + m[2] * y + m[4],
        m[1] * x + m[3] * y + m[5],
        v.s_p * yy + v.c_p * (-v.s_y * xx + v.c_y * z),
    ]
}

fn root_matrix(pose: &Pose, b: &Bounds) -> Matrix {
    let bx = (b.min_x + b.max_x) / 2.0;
    let by = b.max_y;
    let r = (pose.roll * std::f64::consts::PI) / 180.0;
    let rot: Matrix = [cos(r), sin(r), -sin(r), cos(r), 0.0, 0.0];
    mul(
        &translate(0.0, pose.lift),
        &about(bx, by, &mul(&rot, &[pose.sx, 0.0, 0.0, pose.sy, 0.0, 0.0])),
    )
}

fn pivot_of(b: &Bounds) -> Point {
    [(b.min_x + b.max_x) / 2.0, (b.min_y + b.max_y) / 2.0]
}

const POSE_LIFT: f64 = 4.0;
const POSE_SWAY: f64 = 0.2;
const SLICE_SCREEN_GAP_PX: f64 = 0.55;
const SLICE_GAP_PX: f64 = 0.75;

/// How far the body reaches from the middle of its box, for a working turn's hoop and for a
/// celebration's cage (`ribbonRims`).
pub fn ribbon_rims(polys: &[Vec<Point>], b: &Bounds) -> (Rim, Rim) {
    let [cx, cy] = pivot_of(b);
    let bx = (b.min_x + b.max_x) / 2.0;
    let by = b.max_y;
    let posed = |rolls: &[f64], sys: &[f64]| -> Vec<Vec<Vec<Point>>> {
        let mut out = Vec::new();
        for &roll in rolls {
            let r = (roll * std::f64::consts::PI) / 180.0;
            let c = cos(r);
            let sn = sin(r);
            for &k in sys {
                out.push(
                    polys
                        .iter()
                        .map(|poly| {
                            poly.iter()
                                .map(|&[x, y]| {
                                    let lx = bx + c * (x - bx) - sn * (y - by);
                                    let ly = by + sn * (x - bx) + c * (y - by);
                                    [cx + 1.01 * (lx - cx), cy + k * (ly - cy)]
                                })
                                .collect()
                        })
                        .collect(),
                );
            }
        }
        out
    };
    (
        rim_of(&posed(&[2.4, 4.5, 6.6], &[0.98, 1.0]), cx, cy),
        rim_of(&posed(&[-1.7, 0.0, 1.7], &[0.99, 1.055]), cx, cy),
    )
}

/// The rim moved to where the hoops' centre is in this pose (`ribbonsFollow`).
fn ribbons_follow(rim: &Rim, pose: &Pose, b: &Bounds) -> Rim {
    let bx = (b.min_x + b.max_x) / 2.0;
    let by = b.max_y;
    Rim {
        cx: bx + pose.sx * (rim.cx - bx),
        cy: by + pose.sy * (rim.cy - by) + pose.lift,
        ..rim.clone()
    }
}

/// The last ribbon palette asked for: body colour, tuning, and the colour runs per ribbon.
type PaletteCache = Option<(Rgb, Tuning, Arc<Vec<Vec<Rgb>>>)>;

/// A distinct cross-section path of the volume, with its stable id.
struct DistinctPath {
    id: Arc<str>,
    slice: VolumeSlice,
}

/// A definition validated, its volume built and everything per-definition the frames need
/// (the TS `Prepared`). Keep one per (definition, depth) and call [`PreparedAvatar::frame`] per
/// frame.
pub struct PreparedAvatar {
    def: AvatarDefinition,
    depth: f64,
    vol: Volume,
    range: GazeRange,
    seed: u32,
    id: String,
    distinct: Vec<DistinctPath>,
    /// Distinct path index per part and slice: z stack, x stack.
    z_paths: Vec<Vec<usize>>,
    x_paths: Vec<Vec<usize>>,
    polys: Vec<Vec<Point>>,
    rims: OnceLock<(Rim, Rim)>,
    palette: Mutex<PaletteCache>,
}

impl fmt::Debug for PreparedAvatar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedAvatar")
            .field("id", &self.id)
            .field("def", &self.def.id)
            .field("depth", &self.depth)
            .finish()
    }
}

impl PreparedAvatar {
    /// Validates `definition` and builds its volume at `depth` (0..14, default 6; rounded to
    /// halves as the TS does). Errors are the validation messages.
    pub fn new(
        definition: &AvatarDefinition,
        depth: Option<f64>,
    ) -> Result<PreparedAvatar, Vec<String>> {
        let def = validate_avatar(&definition.to_value())?;
        Self::from_valid(def, depth_option(depth))
    }

    fn from_valid(def: AvatarDefinition, depth: f64) -> Result<PreparedAvatar, Vec<String>> {
        let body =
            analyze_body(&def.body.path, true).map_err(|e| vec![format!("body.path: {}", e.0)])?;
        let vol = build_volume(&def, depth / DEFAULT_DEPTH, &body);
        let id = format!(
            "oba-{}",
            to_base36(hash_string(&format!(
                "{}|{}|{}",
                def.id,
                def.body.path,
                number_to_string(depth)
            )) as u64)
        );
        let mut distinct: Vec<DistinctPath> = Vec::new();
        let mut by_d: HashMap<String, usize> = HashMap::new();
        let mut z_paths = Vec::new();
        let mut x_paths = Vec::new();
        for part in &vol.parts {
            let mut add = |s: &VolumeSlice| -> usize {
                let d = &s.geom.outline.d;
                if let Some(&k) = by_d.get(d) {
                    return k;
                }
                let k = distinct.len();
                by_d.insert(d.clone(), k);
                distinct.push(DistinctPath {
                    id: Arc::from(format!("{}-{}", id, k)),
                    slice: s.clone(),
                });
                k
            };
            let z: Vec<usize> = part.z_slices.iter().map(&mut add).collect();
            let x: Vec<usize> = part.x_slices.iter().map(&mut add).collect();
            z_paths.push(z);
            x_paths.push(x);
        }
        let whole = EdgeBands::new(&body.polys);
        let range = gaze_range(
            &whole,
            &AvatarEyes {
                scale: vol.eye_scale,
                ..def.eyes
            },
        );
        let seed = hash_string(&def.id);
        Ok(PreparedAvatar {
            def,
            depth,
            vol,
            range,
            seed,
            id,
            distinct,
            z_paths,
            x_paths,
            polys: body.polys,
            rims: OnceLock::new(),
            palette: Mutex::new(None),
        })
    }

    /// The validated (normalized) definition.
    pub fn definition(&self) -> &AvatarDefinition {
        &self.def
    }

    /// The depth option this avatar was built at (after `depthOption`).
    pub fn depth(&self) -> f64 {
        self.depth
    }

    /// The implied volume.
    pub fn volume(&self) -> &Volume {
        &self.vol
    }

    /// The avatar's seed (`hashString(def.id)`).
    pub fn seed(&self) -> u32 {
        self.seed
    }

    /// The SVG id prefix (`oba-…`).
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The drawn eyes' gaze travel.
    pub fn gaze_range(&self) -> GazeRange {
        self.range
    }

    fn rims(&self) -> &(Rim, Rim) {
        self.rims
            .get_or_init(|| ribbon_rims(&self.polys, &self.vol.bounds))
    }

    /// A square of viewBox units that holds the avatar at every view and pose (`avatarExtent`).
    pub fn extent(&self) -> Square {
        let b = self.vol.bounds;
        let [cx, cy] = pivot_of(&b);
        let max_depth = jsmath::max_of(self.vol.parts.iter().map(|p| p.depth));
        let radius = hypot3(
            b.max_x - b.min_x,
            b.max_y - b.min_y,
            max_depth + 2.0 * CSS_EYE_LIFT,
        ) / 2.0;
        let (work, cage) = self.rims();
        let reach = |rim: &Rim| {
            let r = ribbon_reach(Some(rim.max));
            hypot(rim.cx - cx, rim.cy - cy)
                + POSE_LIFT
                + POSE_SWAY * (b.max_y - rim.cy).abs()
                + r.radius
                + r.band
                + 1.2
        };
        let half = jsmath::max(
            jsmath::max(
                radius + POSE_LIFT + POSE_SWAY * (radius + b.max_y - cy),
                reach(work),
            ),
            reach(cage),
        );
        Square {
            x: cx - half,
            y: cy - half,
            size: 2.0 * half,
        }
    }

    fn pose_of(&self, o: &RenderOptions) -> Pose {
        compute_pose(&PoseInput {
            seed: self.seed,
            time: o.time.filter(|t| t.is_finite()).unwrap_or(0.0),
            state: o.state.as_ref(),
            mood: o.mood,
            motion: o.motion,
            state_age: o.state_age,
            gaze: o.gaze,
            reduced_motion: o.reduced_motion,
            view: o.view,
            blend: o.motion_blend.as_ref(),
        })
    }

    /// The pose a frame with these options shows (`computePose` as render.ts calls it).
    pub fn pose(&self, o: &RenderOptions) -> Pose {
        self.pose_of(o)
    }

    /// `avatarFrame`: one flat frame (every cross-section, whatever the size).
    pub fn avatar_frame(&self, o: &RenderOptions) -> AvatarFrame {
        self.flat_frame(o, None)
    }

    /// `liveFrame`: the frame a live canvas paints at `o.size` CSS pixels, skipping
    /// cross-sections that would sit less than a pixel apart.
    pub fn frame(&self, o: &RenderOptions) -> AvatarFrame {
        self.flat_frame(o, o.size)
    }

    /// `liveTransform`: pose, view, root matrix and eyes, without building the frame.
    pub fn live_transform(&self, o: &RenderOptions) -> LiveTransform<'_> {
        let pose = self.pose_of(o);
        let (color, ink) = colors(o);
        let view = view_basis(pose.yaw, pose.pitch);
        let root = root_matrix(&pose, &self.vol.bounds);
        let has_overlay = pose.ribbons > 0.01 || !pose.marks.is_empty();
        let states = self.eye_states(&pose);
        let eyes = surface_eyes(&states);
        LiveTransform {
            view,
            root,
            eyes,
            color,
            ink,
            has_overlay,
            prepared: self,
            pose,
            states,
            options: o.clone(),
        }
    }

    fn flat_frame(&self, o: &RenderOptions, size: Option<f64>) -> AvatarFrame {
        let (color, ink) = colors(o);
        let pose = self.pose_of(o);
        let v = view_basis(pose.yaw, pose.pitch);
        let root = root_matrix(&pose, &self.vol.bounds);
        let states = self.eye_states(&pose);
        let eyes = surface_eyes(&states);
        self.flat_frame_from(&pose, &v, root, &states, eyes, color, ink, o, size)
    }

    fn eye_states(&self, pose: &Pose) -> [EyeState; 2] {
        let s = self.vol.eye_scale;
        let w = EYE_W * s;
        let h = EYE_H * s;
        let r = self.range;
        let dx = pose.gaze_x * (if pose.gaze_x >= 0.0 { r.right } else { r.left }) * GAZE_X * s;
        let dy = pose.gaze_y * (if pose.gaze_y >= 0.0 { r.down } else { r.up }) * GAZE_Y * s;
        let centers = eye_centers(&self.def.eyes);
        let plain = plain_look(pose.eyes);
        [0usize, 1].map(|i| {
            let squint = if i == 0 { pose.squint_l } else { pose.squint_r };
            let open = jsmath::max(
                h * jsmath::max(0.09, (1.0 - pose.blink) * (1.0 - squint * 0.4)),
                w * 0.55,
            );
            EyeState {
                x: centers[i][0],
                y: centers[i][1],
                w,
                h,
                dx,
                dy,
                open,
                shift: (h - open) * 0.18,
                look: pose.eyes,
                plain,
            }
        })
    }

    /// The ribbon colors on a body this color (`ribbonPalette`), kept for the last color asked.
    fn palette(&self, color: Option<&str>, tuning: Tuning) -> Arc<Vec<Vec<Rgb>>> {
        let rgb = color
            .and_then(parse_hex)
            .or_else(|| parse_hex(DEFAULT_COLOR))
            .unwrap_or([255.0, 106.0, 26.0]);
        if let Ok(guard) = self.palette.lock() {
            if let Some((c, k, p)) = guard.as_ref() {
                if *c == rgb && *k == tuning {
                    return p.clone();
                }
            }
        }
        let p = Arc::new(ribbon_palette(rgb, self.seed, tuning));
        if let Ok(mut guard) = self.palette.lock() {
            *guard = Some((rgb, tuning, p.clone()));
        }
        p
    }

    fn effects_of(&self, pose: &Pose, o: &RenderOptions) -> Effects {
        let b = self.vol.bounds;
        let time = o.time.filter(|t| t.is_finite()).unwrap_or(0.0);
        let still = o.reduced_motion;
        let [cx, cy] = pivot_of(&b);
        let ribbons = if still || pose.ribbons <= 0.01 {
            Vec::new()
        } else {
            let (work, cage) = self.rims();
            let rim = ribbons_follow(if pose.cage >= 0.5 { cage } else { work }, pose, &b);
            let pieces = ribbon_pieces(
                b,
                pose.ribbons,
                self.seed,
                pose.sweep,
                pose.cage,
                Some(&rim),
            );
            let palette = if pieces.is_empty() {
                None
            } else {
                Some(self.palette(o.color.as_deref(), o.tuning))
            };
            pieces
                .into_iter()
                .map(|r| {
                    let hue = &palette
                        .as_ref()
                        .map(|p| p[r.ribbon].clone())
                        .unwrap_or_default();
                    let [s0, s1] = r.along;
                    let stops: Vec<String> = [s0, (s0 + s1) / 2.0, s1]
                        .iter()
                        .map(|&t| run_at(hue, t))
                        .collect();
                    let long = hypot(r.to[0] - r.from[0], r.to[1] - r.from[1]) > 0.01;
                    let fill = stops[1].clone();
                    let gradient = if long {
                        Gradient {
                            x1: r.from[0],
                            y1: r.from[1],
                            x2: r.to[0],
                            y2: r.to[1],
                            stops,
                        }
                    } else {
                        Gradient {
                            x1: r.from[0],
                            y1: r.from[1],
                            x2: r.from[0] + 0.01,
                            y2: r.from[1],
                            stops: vec![fill.clone(), fill.clone(), fill.clone()],
                        }
                    };
                    EffectRibbon {
                        points: r.points,
                        bbox: r.bbox,
                        front: r.front,
                        fill,
                        gradient,
                    }
                })
                .collect()
        };
        let marks = mark_parts(b, &pose.marks, self.seed, time, still)
            .into_iter()
            .map(|part: MarkPart| {
                let turn: Matrix = [
                    cos(part.turn),
                    sin(part.turn),
                    -sin(part.turn),
                    cos(part.turn),
                    0.0,
                    0.0,
                ];
                let matrix = mul(
                    &about(cx, cy, &turn),
                    &mul(
                        &translate(part.x, part.y),
                        &[part.scale, 0.0, 0.0, part.scale, 0.0, 0.0],
                    ),
                );
                EffectMark {
                    id: format!("{}-{}", self.id, mark_id(&part.shape)),
                    outline: Outline::from_cmds((*part.cmds).clone()),
                    bbox: part.bbox,
                    round: part.round,
                    matrix,
                }
            })
            .collect();
        Effects {
            body: body_scale(&pose.marks),
            ribbons,
            marks,
        }
    }

    /// Per slice of a stack, in order: whether it is kept at this rendered size (`sliceDetail`).
    fn slice_detail(&self, stack: Axis, size: Option<f64>) -> Vec<bool> {
        let px = match size {
            Some(s) if s.is_finite() && s > 0.0 => jsmath::round(s),
            _ => 0.0,
        };
        let gap = if px != 0.0 {
            (SLICE_GAP_PX * 100.0) / px
        } else {
            0.0
        };
        let mut keep = Vec::new();
        for part in &self.vol.parts {
            let slices = if stack == Axis::Z {
                &part.z_slices
            } else {
                &part.x_slices
            };
            let mut last = f64::NEG_INFINITY;
            for (j, s) in slices.iter().enumerate() {
                let core = s.axis == Axis::Z && s.at == 0.0 && s.inset == 0.0;
                let kept = core || j == 0 || j == slices.len() - 1 || s.at - last >= gap;
                if kept {
                    last = s.at;
                }
                keep.push(kept);
            }
        }
        keep
    }

    /// Every cross-section a view draws, with its projection (`drawnSlices`).
    fn drawn_slices(
        &self,
        v: &ViewBasis,
        size: Option<f64>,
        mut visit: impl FnMut(&VolumeSlice, Matrix, usize, usize, usize),
    ) {
        let stack = active_stack(v);
        let pivot = pivot_of(&self.vol.bounds);
        let sweep_all = hypot(v.s_y, v.c_y * v.s_p);
        let inset_scale = jsmath::min(v.c_y.abs(), v.c_p.abs());
        let drawn = |s: &VolumeSlice| {
            s.axis != Axis::Z || s.inset <= 0.0 || s.at.abs() * sweep_all > s.inset * inset_scale
        };
        let kept = self.slice_detail(stack, size);
        let px = match size {
            Some(s) if s.is_finite() && s > 0.0 => s,
            _ => 0.0,
        };
        let sweep = if stack == Axis::Z {
            hypot(v.s_y, v.c_y * v.s_p)
        } else {
            0.0
        };
        let min_shift = if px != 0.0 && stack == Axis::Z {
            if sweep > 1e-4 {
                (SLICE_SCREEN_GAP_PX * 100.0) / (px * sweep)
            } else {
                f64::INFINITY
            }
        } else {
            0.0
        };
        let mut k = 0;
        for (i, part) in self.vol.parts.iter().enumerate() {
            let (slices, paths) = if stack == Axis::Z {
                (&part.z_slices, &self.z_paths[i])
            } else {
                (&part.x_slices, &self.x_paths[i])
            };
            let mut last_at = f64::NEG_INFINITY;
            for (j, s) in slices.iter().enumerate() {
                let keep = kept[k];
                k += 1;
                if !keep || !drawn(s) {
                    continue;
                }
                if min_shift > 0.0 {
                    let essential =
                        (s.at == 0.0 && s.inset == 0.0) || j == 0 || j == slices.len() - 1;
                    if !essential && s.at - last_at < min_shift {
                        continue;
                    }
                    last_at = s.at;
                }
                let m = if stack == Axis::Z {
                    plane_matrix(v, s.at, pivot)
                } else {
                    side_plane_matrix(v, s.at, pivot)
                };
                visit(s, m, i, j, paths[j]);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn flat_frame_from(
        &self,
        pose: &Pose,
        v: &ViewBasis,
        root: Matrix,
        states: &[EyeState; 2],
        eyes: Vec<SurfaceEye>,
        color: String,
        ink: String,
        o: &RenderOptions,
        size: Option<f64>,
    ) -> AvatarFrame {
        let vol = &self.vol;
        let b = vol.bounds;
        let pivot = pivot_of(&b);
        let stack = active_stack(v);
        let fx = self.effects_of(pose, o);
        let shrink_matrix = if fx.body > 0.0 && fx.body < 1.0 {
            Some(about(
                pivot[0],
                pivot[1],
                &[fx.body, 0.0, 0.0, fx.body, 0.0, 0.0],
            ))
        } else {
            None
        };
        let shrink = |m: Matrix| match &shrink_matrix {
            Some(s) => mul(s, &m),
            None => m,
        };

        let mut paths: Vec<FramePath> = Vec::new();
        let mut frame_index: Vec<Option<usize>> = vec![None; self.distinct.len()];
        let mut items: Vec<(f64, FrameShape)> = Vec::new();
        if fx.body > 0.0 {
            self.drawn_slices(v, size, |s, m, i, j, distinct| {
                let path = *frame_index[distinct].get_or_insert_with(|| {
                    let dp = &self.distinct[distinct];
                    paths.push(FramePath {
                        id: dp.id.clone(),
                        outline: dp.slice.geom.outline.clone(),
                        even_odd: dp.slice.geom.even_odd,
                        bbox: dp.slice.geom.bbox,
                        round: false,
                    });
                    paths.len() - 1
                });
                let depth = if stack == Axis::Z {
                    v.facing * s.at
                } else {
                    -v.s_y * v.c_p * s.at
                };
                items.push((
                    depth,
                    FrameShape::Slice {
                        key: ShapeKey::Slice {
                            part: i,
                            stack,
                            index: j,
                        },
                        path,
                        matrix: shrink(m),
                    },
                ));
            });
            for (idx, e) in states.iter().enumerate() {
                let plane = &vol.eye_planes[idx];
                if !eye_visible(v, plane) {
                    continue;
                }
                let half_w = e.w
                    * if e.plain == Some(PlainLook::Pill) {
                        0.5
                    } else {
                        happy_arc::HALF_WIDTH + 0.5 * happy_arc::STROKE
                    };
                let x_depth = -v.s_y * v.c_p;
                let z = plane.z + e.dx * plane.dzdx + e.dy * plane.dzdy;
                let depth = if stack == Axis::Z {
                    v.facing * z
                        + v.facing.abs()
                            * (plane.dzdx.abs() * half_w + (plane.dzdy.abs() * e.h) / 2.0)
                } else {
                    x_depth * (e.x + e.dx) + x_depth.abs() * half_w
                };
                items.push((
                    depth + 1e-6,
                    FrameShape::Eye {
                        key: idx,
                        matrix: shrink(eye_matrix(v, plane, pivot)),
                        mark: eye_mark(e),
                    },
                ));
            }
            items.sort_by(|a, c| a.0.partial_cmp(&c.0).unwrap_or(std::cmp::Ordering::Equal));
        }

        let fixed = if !fx.ribbons.is_empty() || !fx.marks.is_empty() {
            invert(&root)
        } else {
            [0.0; 6]
        };
        let back = fx.ribbons.iter().filter(|r| !r.front).count();
        let total = items.len() + fx.ribbons.len() + fx.marks.len();
        let mut back_shapes: Vec<FrameShape> = Vec::with_capacity(back);
        let mut front_shapes: Vec<FrameShape> = Vec::new();
        for (i, r) in fx.ribbons.into_iter().enumerate() {
            let front = r.front;
            let shape = FrameShape::Ribbon(RibbonShape {
                key: i,
                points: r.points,
                bbox: r.bbox,
                matrix: fixed,
                front: r.front,
                fill: r.fill,
                gradient: r.gradient,
            });
            if front {
                front_shapes.push(shape);
            } else {
                back_shapes.push(shape);
            }
        }
        let mut mark_ids: Vec<(String, usize)> = Vec::new();
        for (i, mark) in fx.marks.into_iter().enumerate() {
            let taken = mark_ids
                .iter()
                .find(|(id, _)| *id == mark.id)
                .map(|&(_, k)| k);
            let id = match taken {
                Some(k) if paths[k].outline.d != mark.outline.d => {
                    format!(
                        "{}-{}",
                        mark.id,
                        to_base36(hash_string(&mark.outline.d) as u64)
                    )
                }
                _ => mark.id.clone(),
            };
            let path = match mark_ids.iter().find(|(known, _)| *known == id) {
                Some(&(_, k)) => k,
                None => {
                    paths.push(FramePath {
                        id: Arc::from(id.as_str()),
                        outline: Arc::new(mark.outline),
                        even_odd: false,
                        bbox: mark.bbox,
                        round: mark.round,
                    });
                    mark_ids.push((id, paths.len() - 1));
                    paths.len() - 1
                }
            };
            front_shapes.push(FrameShape::Slice {
                key: ShapeKey::Mark(i),
                path,
                matrix: mul(&fixed, &mark.matrix),
            });
        }
        let mut shapes = Vec::with_capacity(total);
        shapes.extend(back_shapes);
        shapes.extend(items.into_iter().map(|(_, s)| s));
        shapes.extend(front_shapes);
        AvatarFrame {
            name: self.def.name.clone(),
            color,
            ink,
            paths,
            root,
            shapes,
            surface: FrameSurface {
                view: *v,
                scale: fx.body,
                eyes: if fx.body > 0.0 { eyes } else { Vec::new() },
            },
        }
    }
}

/// A square in viewBox units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Square {
    pub x: f64,
    pub y: f64,
    pub size: f64,
}

/// `markId`: a part's name in a path id.
fn mark_id(shape: &str) -> String {
    let b = shape.as_bytes();
    if !b.is_empty() && b[0].is_ascii_alphabetic() && b.iter().all(|c| c.is_ascii_alphanumeric()) {
        shape.to_string()
    } else {
        format!("m-{}", to_base36(hash_string(shape) as u64))
    }
}

#[derive(Clone, Debug)]
struct EyeState {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    dx: f64,
    dy: f64,
    open: f64,
    shift: f64,
    look: EyeLook,
    plain: Option<PlainLook>,
}

fn eye_spot(e: &EyeState, gaze: bool) -> EyeSpot {
    EyeSpot {
        x: e.x + if gaze { e.dx } else { 0.0 },
        y: e.y + if gaze { e.dy } else { 0.0 },
        w: e.w,
        h: e.h,
        open: e.open,
        shift: e.shift,
    }
}

fn happy_arc_cmds(e: &EyeState) -> Vec<PathCmd> {
    let ex = e.x + e.dx;
    let ey = e.y + e.dy;
    let hw = e.w * happy_arc::HALF_WIDTH;
    let base = ey + e.h * happy_arc::BASE;
    vec![
        PathCmd::M {
            x: ex - hw,
            y: base,
        },
        PathCmd::Q {
            x1: ex,
            y1: ey - e.h * happy_arc::PEAK,
            x: ex + hw,
            y: base,
        },
    ]
}

fn eye_mark(e: &EyeState) -> EyeMark {
    match e.plain {
        Some(PlainLook::Arc) => EyeMark::Arc {
            cmds: happy_arc_cmds(e),
            stroke_width: e.w * happy_arc::STROKE,
        },
        None => EyeMark::Path {
            cmds: eye_path_cmds(e.look, &eye_spot(e, true)),
        },
        Some(PlainLook::Pill) => {
            let cx = e.x + e.dx;
            let cy = e.y + e.dy + e.shift;
            EyeMark::Pill {
                x: cx - e.w / 2.0,
                y: cy - e.open / 2.0,
                width: e.w,
                height: e.open,
                radius: jsmath::min(e.w, e.open) / 2.0,
            }
        }
    }
}

fn surface_eyes(states: &[EyeState; 2]) -> Vec<SurfaceEye> {
    states
        .iter()
        .map(|e| SurfaceEye {
            mark: eye_mark(&EyeState {
                dx: 0.0,
                dy: 0.0,
                ..e.clone()
            }),
            dx: e.dx,
            dy: e.dy,
        })
        .collect()
}

/// `normalizedColor`.
fn normalized_color(input: Option<&str>, fallback: &str) -> String {
    match input {
        Some(s) if !s.is_empty() => parse_hex(s)
            .map(to_hex)
            .unwrap_or_else(|| fallback.to_string()),
        _ => fallback.to_string(),
    }
}

fn colors(o: &RenderOptions) -> (String, String) {
    (
        normalized_color(o.color.as_deref(), DEFAULT_COLOR),
        normalized_color(o.eye_color.as_deref(), DEFAULT_EYE_COLOR),
    )
}

/// How one eye is drawn, in its face plane's coordinates (`EyeMark`).
#[derive(Clone, Debug, PartialEq)]
pub enum EyeMark {
    /// The plain pill: a rounded rectangle.
    Pill {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        radius: f64,
    },
    /// The plain happy arc: a quadratic stroked `stroke_width` wide with round caps.
    Arc {
        cmds: Vec<PathCmd>,
        stroke_width: f64,
    },
    /// Every other look: a filled outline.
    Path { cmds: Vec<PathCmd> },
}

impl EyeMark {
    /// `"pill" | "arc" | "path"`.
    pub fn shape(&self) -> &'static str {
        match self {
            EyeMark::Pill { .. } => "pill",
            EyeMark::Arc { .. } => "arc",
            EyeMark::Path { .. } => "path",
        }
    }

    /// The SVG text of an arc or path mark (`d`), None for a pill.
    pub fn d(&self) -> Option<String> {
        match self {
            EyeMark::Pill { .. } => None,
            EyeMark::Arc { cmds, .. } | EyeMark::Path { cmds } => Some(crate::geom::path_d(cmds)),
        }
    }
}

/// Which shape a slice entry draws: a body cross-section or a mark's part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKey {
    /// `${part}${stack}${index}`.
    Slice {
        part: usize,
        stack: Axis,
        index: usize,
    },
    /// `mark-${i}`.
    Mark(usize),
}

impl fmt::Display for ShapeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShapeKey::Slice { part, stack, index } => {
                write!(f, "{}{}{}", part, stack.as_char(), index)
            }
            ShapeKey::Mark(i) => write!(f, "mark-{}", i),
        }
    }
}

/// A linear gradient along a whole ribbon, stops evenly spaced.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub stops: Vec<String>,
}

/// A piece of a ribbon.
#[derive(Clone, Debug, PartialEq)]
pub struct RibbonShape {
    /// `ribbon-${key}`.
    pub key: usize,
    /// The piece's outline (`ribbonPointsOf`); [`RibbonShape::d`] writes the TS `d`.
    pub points: Vec<Point>,
    pub bbox: BoxF,
    pub matrix: Matrix,
    pub front: bool,
    /// The ribbon's colour where it is plainest.
    pub fill: String,
    pub gradient: Gradient,
}

impl RibbonShape {
    /// The piece as SVG text (avatarFrame's `d`; live frames leave it empty in the TS).
    pub fn d(&self) -> String {
        crate::effects::points_d(&self.points)
    }
}

/// One entry of a frame's back-to-front list (`FrameShape`).
#[derive(Clone, Debug, PartialEq)]
pub enum FrameShape {
    /// A body cross-section or a mark's part: `frame.paths[path]` filled in the body color.
    Slice {
        key: ShapeKey,
        path: usize,
        matrix: Matrix,
    },
    /// An eye (`key` 0 or 1) in the ink.
    Eye {
        key: usize,
        matrix: Matrix,
        mark: EyeMark,
    },
    /// A ribbon piece.
    Ribbon(RibbonShape),
}

impl FrameShape {
    /// The shape's matrix.
    pub fn matrix(&self) -> &Matrix {
        match self {
            FrameShape::Slice { matrix, .. } | FrameShape::Eye { matrix, .. } => matrix,
            FrameShape::Ribbon(r) => &r.matrix,
        }
    }

    /// The TS `key` text.
    pub fn key(&self) -> String {
        match self {
            FrameShape::Slice { key, .. } => key.to_string(),
            FrameShape::Eye { key, .. } => format!("eye-{}", key),
            FrameShape::Ribbon(r) => format!("ribbon-{}", r.key),
        }
    }

    /// `"slice" | "eye" | "ribbon"`.
    pub fn kind(&self) -> &'static str {
        match self {
            FrameShape::Slice { .. } => "slice",
            FrameShape::Eye { .. } => "eye",
            FrameShape::Ribbon(_) => "ribbon",
        }
    }
}

/// One cross-section outline, shared by every shape that draws it (`FramePath`).
#[derive(Clone, Debug, PartialEq)]
pub struct FramePath {
    pub id: Arc<str>,
    /// The outline: `outline.d` is the TS `d`, `outline.cmds` the numbers.
    pub outline: Arc<Outline>,
    pub even_odd: bool,
    /// Bounds of the outline in its own coordinates.
    pub bbox: BoxF,
    /// A mark's part that is a circle or an upright capsule filling `bbox`.
    pub round: bool,
}

impl FramePath {
    /// The SVG path text.
    pub fn d(&self) -> &str {
        &self.outline.d
    }
}

/// An eye on the body's surface, in face coordinates (`SurfaceEye`).
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceEye {
    pub mark: EyeMark,
    pub dx: f64,
    pub dy: f64,
}

/// The body as one surface (`AvatarFrame.surface`).
#[derive(Clone, Debug, PartialEq)]
pub struct FrameSurface {
    pub view: ViewBasis,
    /// The body's scale as it gives way to a mark.
    pub scale: f64,
    pub eyes: Vec<SurfaceEye>,
}

/// One frame flattened onto the screen (`AvatarFrame`): paint `shapes` in order inside `root`
/// and a 0 0 100 100 viewBox (slices in `color`, eyes in `ink`, ribbons in their gradients).
#[derive(Clone, Debug, PartialEq)]
pub struct AvatarFrame {
    pub name: String,
    pub color: String,
    pub ink: String,
    /// Distinct outlines this frame uses.
    pub paths: Vec<FramePath>,
    /// 2D motion (hop, roll, squash) about the base of the body.
    pub root: Matrix,
    /// Back to front.
    pub shapes: Vec<FrameShape>,
    pub surface: FrameSurface,
}

impl AvatarFrame {
    /// The outline a slice shape draws.
    pub fn path_of(&self, shape: &FrameShape) -> Option<&FramePath> {
        match shape {
            FrameShape::Slice { path, .. } => self.paths.get(*path),
            _ => None,
        }
    }
}

/// Pose, view, root matrix and eyes of a live frame, computed without building the frame
/// (`LiveTransform`). [`LiveTransform::build_frame`] materializes it.
pub struct LiveTransform<'a> {
    pub view: ViewBasis,
    pub root: Matrix,
    pub eyes: Vec<SurfaceEye>,
    pub color: String,
    pub ink: String,
    pub has_overlay: bool,
    prepared: &'a PreparedAvatar,
    pose: Pose,
    states: [EyeState; 2],
    options: RenderOptions,
}

impl LiveTransform<'_> {
    /// The full live frame from the pose already computed.
    pub fn build_frame(&self) -> AvatarFrame {
        self.prepared.flat_frame_from(
            &self.pose,
            &self.view,
            self.root,
            &self.states,
            self.eyes.clone(),
            self.color.clone(),
            self.ink.clone(),
            &self.options,
            self.options.size,
        )
    }

    /// The pose.
    pub fn pose(&self) -> &Pose {
        &self.pose
    }
}

struct EffectRibbon {
    points: Vec<Point>,
    bbox: BoxF,
    front: bool,
    fill: String,
    gradient: Gradient,
}

struct EffectMark {
    id: String,
    outline: Outline,
    bbox: BoxF,
    round: bool,
    matrix: Matrix,
}

struct Effects {
    body: f64,
    ribbons: Vec<EffectRibbon>,
    marks: Vec<EffectMark>,
}

const RAMP_LIGHT: f64 = 0.09;
const RAMP_HUE: f64 = 14.0;
const RAMP_RANGE: [f64; 2] = [0.36, 0.9];
const WARM: f64 = 100.0;
const GRADIENT_LIGHT: f64 = 0.13;
const GRADIENT_HUE: f64 = 25.0;
const GRADIENT_STOPS: usize = 5;
const NEUTRAL: f64 = 0.04;
const NEUTRAL_TINT: f64 = 0.025;
const BAND_SHADES: [[usize; 3]; 3] = [[0, 1, 2], [2, 0, 3], [1, 2, 0]];

/// Turn hue `h` by up to `by` degrees toward hue `to` (`turnToward`).
fn turn_toward(h: f64, to: f64, by: f64) -> f64 {
    let d = ((to - h + 540.0) % 360.0) - 180.0;
    (h + jsmath::sign(d) * jsmath::min(d.abs(), by) + 360.0) % 360.0
}

/// The colour `t` of the way along a colour run (`runAt`).
fn run_at(run: &[Rgb], t: f64) -> String {
    if run.len() < 2 {
        return run
            .first()
            .map(|&c| to_hex(c))
            .unwrap_or_else(|| DEFAULT_COLOR.to_string());
    }
    let x = clamp(t, 0.0, 1.0) * (run.len() - 1) as f64;
    let n = jsmath::min(x.floor(), (run.len() - 2) as f64) as usize;
    to_hex(mix(run[n], run[n + 1], x - n as f64))
}

/// The colours of the ribbons on a body this colour (`ribbonPalette`).
fn ribbon_palette(rgb: Rgb, seed: u32, tuning: Tuning) -> Vec<Vec<Rgb>> {
    let solar = tuning == Tuning::Solar;
    let [light, chroma, hue] = to_oklch(rgb);
    let neutral = chroma < NEUTRAL;
    let base = if neutral {
        (130.0 + noise(seed, 500.0) * 310.0) % 360.0
    } else {
        hue
    };
    let c = if neutral {
        if solar {
            0.0
        } else {
            NEUTRAL_TINT
        }
    } else {
        jsmath::max(chroma, 0.17)
    };
    let from = jsmath::min(RAMP_RANGE[1], jsmath::max(RAMP_RANGE[0], light));
    let mut steps: Vec<f64> = Vec::new();
    let mut k = 1.0;
    while steps.len() < 5 && k < 10.0 {
        for step in [k, -k] {
            let l = from + step * RAMP_LIGHT;
            if steps.len() < 5 && (RAMP_RANGE[0] - 1e-9..=RAMP_RANGE[1] + 1e-9).contains(&l) {
                steps.push(step);
            }
        }
        k += 1.0;
    }
    while steps.len() < 5 {
        steps.push(*steps.last().unwrap_or(&1.0));
    }
    let shades: Vec<(f64, Vec<Rgb>)> = steps
        .iter()
        .map(|&step| {
            let l = from + step * RAMP_LIGHT;
            let h = if !solar {
                turn_toward(
                    base,
                    if step > 0.0 { WARM } else { WARM + 180.0 },
                    RAMP_HUE * step.abs(),
                )
            } else if step > 0.0 {
                turn_toward(base, WARM, RAMP_HUE * step.abs())
            } else {
                base
            };
            let away = if step > 0.0 { 1.0 } else { -1.0 };
            let run = (0..GRADIENT_STOPS)
                .map(|n| {
                    let t = n as f64 / (GRADIENT_STOPS - 1) as f64;
                    let hue = if !solar {
                        turn_toward(
                            h,
                            if away > 0.0 { WARM } else { WARM + 180.0 },
                            GRADIENT_HUE * t,
                        )
                    } else if away > 0.0 {
                        turn_toward(h, WARM, GRADIENT_HUE * t)
                    } else {
                        h
                    };
                    oklch(clamp(l + away * GRADIENT_LIGHT * t, 0.26, 0.9), c, hue)
                })
                .collect();
            (step, run)
        })
        .collect();
    let mut palette = Vec::new();
    for pick in BAND_SHADES {
        let mut band: Vec<&(f64, Vec<Rgb>)> = pick.iter().map(|&n| &shades[n]).collect();
        band.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
        palette.extend(band.into_iter().map(|(_, run)| run.clone()));
    }
    palette
}

// ---------------------------------------------------------------------------
// The TS module-level cache, for the free functions.

thread_local! {
    static CACHE: RefCell<Vec<(String, Arc<PreparedAvatar>)>> = const { RefCell::new(Vec::new()) };
}

/// The cache key render.ts uses: `JSON.stringify([id, name, path, eyes, depth])`.
fn cache_key(def: &AvatarDefinition, depth: f64) -> String {
    serde_json::to_string(&(&def.id, &def.name, &def.body.path, &def.eyes, depth))
        .unwrap_or_default()
}

/// The prepared avatar for a definition and depth option, from a per-thread cache of 64 entries
/// (render.ts's `prepare`). Errors are the validation messages.
pub fn prepare(
    def: &AvatarDefinition,
    depth: Option<f64>,
) -> Result<Arc<PreparedAvatar>, Vec<String>> {
    let depth = depth_option(depth);
    let key = cache_key(def, depth);
    if let Some(hit) = CACHE.with(|c| {
        c.borrow()
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, p)| p.clone())
    }) {
        return Ok(hit);
    }
    let valid = validate_avatar(&def.to_value())?;
    let prepared = Arc::new(PreparedAvatar::from_valid(valid, depth)?);
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() >= 64 {
            c.remove(0);
        }
        c.push((key, prepared.clone()));
    });
    Ok(prepared)
}

/// `avatarFrame(definition, options)`. Errors if the definition is invalid.
pub fn avatar_frame(def: &AvatarDefinition, o: &RenderOptions) -> Result<AvatarFrame, Vec<String>> {
    Ok(prepare(def, o.depth)?.avatar_frame(o))
}

/// `liveFrame(definition, options)`. Errors if the definition is invalid.
pub fn live_frame(def: &AvatarDefinition, o: &RenderOptions) -> Result<AvatarFrame, Vec<String>> {
    Ok(prepare(def, o.depth)?.frame(o))
}

/// `avatarExtent(definition, depth)`.
pub fn avatar_extent(def: &AvatarDefinition, depth: Option<f64>) -> Result<Square, Vec<String>> {
    Ok(prepare(def, depth)?.extent())
}
