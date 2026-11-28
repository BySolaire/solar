//! Definitions and validation (port of `types.ts` and `validate.ts`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::jsmath::{self, number_to_string};
use crate::path::{
    bounds, filled_area, flatten_path, js_trim, parse_path, serialize_path, split_subpaths, Bounds,
    EdgeBands, PathError, Point, Segment,
};

/// Placement of the eye pair (`AvatarEyes`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AvatarEyes {
    /// Center of the eye pair, viewBox units.
    pub x: f64,
    pub y: f64,
    /// Center-to-center distance between the two eyes, viewBox units.
    pub spacing: f64,
    /// 1 = each neutral eye is ~4 units wide and ~11 units tall.
    pub scale: f64,
}

/// The body path (`{ path: string }`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AvatarBody {
    pub path: String,
}

/// Avatar definition contract v1 (`AvatarDefinition`): one silhouette path in a fixed
/// 0 0 100 100 viewBox and the eye pair.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AvatarDefinition {
    pub version: u32,
    /// Lowercase slug, e.g. "controller".
    pub id: String,
    /// Human-readable display name.
    pub name: String,
    /// One filled path, nonzero fill rule; holes wind opposite to their outline.
    pub body: AvatarBody,
    pub eyes: AvatarEyes,
}

impl AvatarDefinition {
    /// The definition as the JSON value `validate_avatar` takes.
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// Neutral eye size at scale 1, viewBox units (`EYE_W`, `EYE_H`).
pub const EYE_W: f64 = 4.0;
pub const EYE_H: f64 = 11.0;
/// Hard limits; the contract's soft target for the body is 8..92.
pub const BODY_MIN: f64 = 6.0;
pub const BODY_MAX: f64 = 94.0;
const EYE_MARGIN: f64 = 1.2;
/// Max gaze travel of the eyes inside the face at scale 1 (`GAZE_X`, `GAZE_Y`).
pub const GAZE_X: f64 = 1.5;
pub const GAZE_Y: f64 = 1.1;

/// Happy-eye arc, as fractions of the drawn eye size (`HAPPY_ARC`).
pub mod happy_arc {
    pub const HALF_WIDTH: f64 = 0.72;
    pub const BASE: f64 = 0.1;
    pub const PEAK: f64 = 0.42;
    pub const STROKE: f64 = 0.6;
}

/// Gap the renderer keeps between every drawn eye shape and the outline while the eyes look around.
pub const EYE_CLEARANCE: f64 = 1.5;
/// Holes up to this area count as small details (`RECESS_MAX_AREA`).
pub const RECESS_MAX_AREA: f64 = 110.0;

const TOP_KEYS: [&str; 5] = ["version", "id", "name", "body", "eyes"];
const EYE_KEYS: [&str; 4] = ["x", "y", "spacing", "scale"];

/// A subpath that cuts a hole (`BodyHole`).
#[derive(Clone, Debug)]
pub struct BodyHole {
    pub d: String,
    pub area: f64,
}

/// One subpath of the body (`BodySubpath`).
#[derive(Clone, Debug)]
pub struct BodySubpath {
    /// The subpath, serialized.
    pub d: String,
    /// The subpath's own segments (the numbers `d` is written from).
    pub segs: Vec<Segment>,
    /// Its own flattened polygon(s).
    pub polys: Vec<Vec<Point>>,
    /// True when the subpath cuts a hole rather than adding fill.
    pub hole: bool,
    /// A point inside the subpath's own fill (for holes: inside the cut-out).
    pub probe: Option<Point>,
}

/// `BodyAnalysis`.
#[derive(Clone, Debug)]
pub struct BodyAnalysis {
    pub polys: Vec<Vec<Point>>,
    pub bounds: Bounds,
    pub area: f64,
    pub holes: Vec<BodyHole>,
    pub subpaths: Vec<BodySubpath>,
}

/// How far (0..1 of GAZE_X/GAZE_Y) the drawn eyes can travel each way (`GazeRange`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GazeRange {
    pub left: f64,
    pub right: f64,
    pub up: f64,
    pub down: f64,
}

/// Points on the outline of an eye pill expanded by `margin` (`eyeOutline` in validate.ts).
fn eye_outline(cx: f64, cy: f64, scale: f64, margin: f64, pts: &mut Vec<Point>) {
    let hw = (EYE_W * scale) / 2.0 + margin;
    let hh = (EYE_H * scale) / 2.0 + margin;
    let straight = hh - hw;
    for k in 0..16 {
        let a = (k as f64 / 16.0) * std::f64::consts::PI * 2.0;
        let x = jsmath::cos(a) * hw;
        let s = jsmath::sin(a);
        let y = s * hw + jsmath::sign(s) * jsmath::max(0.0, straight);
        pts.push([cx + x, cy + y]);
    }
    for f in [-0.5, 0.0, 0.5] {
        pts.push([cx - hw, cy + f * straight]);
        pts.push([cx + hw, cy + f * straight]);
    }
    pts.push([cx, cy]);
}

/// Outline points of every shape the renderer may draw for one eye (`drawnEyeOutline`).
fn drawn_eye_outline(cx: f64, cy: f64, scale: f64, margin: f64, pts: &mut Vec<Point>) {
    use happy_arc::*;
    eye_outline(cx, cy, scale, margin, pts);
    let w = EYE_W * scale;
    let h = EYE_H * scale;
    let x0 = cx - HALF_WIDTH * w;
    let x1 = cx + HALF_WIDTH * w;
    let yb = cy + BASE * h;
    let yc = cy - PEAK * h;
    let r = (STROKE * w) / 2.0 + margin;
    for i in 0..=8 {
        let t = i as f64 / 8.0;
        let m = 1.0 - t;
        let x = m * m * x0 + 2.0 * m * t * cx + t * t * x1;
        let y = (m * m + t * t) * yb + 2.0 * m * t * yc;
        for k in 0..8 {
            let a = (k as f64 / 8.0) * std::f64::consts::PI * 2.0;
            pts.push([x + jsmath::cos(a) * r, y + jsmath::sin(a) * r]);
        }
    }
}

/// `eyeCenters`.
pub fn eye_centers(eyes: &AvatarEyes) -> [Point; 2] {
    [
        [eyes.x - eyes.spacing / 2.0, eyes.y],
        [eyes.x + eyes.spacing / 2.0, eyes.y],
    ]
}

/// `eyesFit`: the first pill outline point outside the fill, if any.
pub fn eyes_fit(
    index: &EdgeBands,
    eyes: &AvatarEyes,
    dx: f64,
    dy: f64,
    margin: f64,
) -> Option<Point> {
    let mut pts = Vec::with_capacity(32);
    for [cx, cy] in eye_centers(eyes) {
        pts.clear();
        eye_outline(cx + dx, cy + dy, eyes.scale, margin, &mut pts);
        for &[x, y] in &pts {
            if index.winding(x, y) == 0 {
                return Some([x, y]);
            }
        }
    }
    None
}

/// `drawnEyesFit`: like eyes_fit, for the pill and the wider happy arc.
pub fn drawn_eyes_fit(
    index: &EdgeBands,
    eyes: &AvatarEyes,
    dx: f64,
    dy: f64,
    margin: f64,
) -> Option<Point> {
    let mut pts = Vec::with_capacity(112);
    for [cx, cy] in eye_centers(eyes) {
        pts.clear();
        drawn_eye_outline(cx + dx, cy + dy, eyes.scale, margin, &mut pts);
        for &[x, y] in &pts {
            if index.winding(x, y) == 0 {
                return Some([x, y]);
            }
        }
    }
    None
}

/// Largest gap (to 0.1) between the drawn eyes at rest and the outline (`eyeClearance`).
pub fn eye_clearance(index: &EdgeBands, eyes: &AvatarEyes) -> f64 {
    if drawn_eyes_fit(index, eyes, 0.0, 0.0, 0.0).is_some() {
        return 0.0;
    }
    let (mut lo, mut hi) = (0.0f64, 20.0f64);
    while hi - lo > 0.05 {
        let mid = (lo + hi) / 2.0;
        if drawn_eyes_fit(index, eyes, 0.0, 0.0, mid).is_some() {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    (lo * 10.0).floor() / 10.0
}

/// `gazeRange`.
pub fn gaze_range(index: &EdgeBands, eyes: &AvatarEyes) -> GazeRange {
    let reach = |ux: f64, uy: f64| -> f64 {
        for k in [1.0, 0.8, 0.6, 0.4, 0.2] {
            if drawn_eyes_fit(
                index,
                eyes,
                ux * k * GAZE_X * eyes.scale,
                uy * k * GAZE_Y * eyes.scale,
                EYE_CLEARANCE,
            )
            .is_none()
            {
                return k;
            }
        }
        0.0
    };
    GazeRange {
        left: reach(-1.0, 0.0),
        right: reach(1.0, 0.0),
        up: reach(0.0, -1.0),
        down: reach(0.0, 1.0),
    }
}

/// `analyzeBody`.
pub fn analyze_body(path: &str, bounded: bool) -> Result<BodyAnalysis, PathError> {
    let segs = parse_path(path, bounded)?;
    let polys = flatten_path(&segs);
    let b = bounds(&polys);
    let whole = EdgeBands::new(&polys);
    let mut holes = Vec::new();
    let mut subpaths = Vec::new();
    for sub in split_subpaths(&segs) {
        let own = flatten_path(&sub);
        if own.is_empty() {
            continue;
        }
        let ob = bounds(&own);
        let (mut inside, mut cut) = (0.0f64, 0.0f64);
        let mut probe: Option<Point> = None;
        let mut cut_probe: Option<Point> = None;
        let step = if (ob.max_x - ob.min_x) * (ob.max_y - ob.min_y) < 900.0 {
            0.5
        } else {
            1.0
        };
        let alone = own.len() == polys.len();
        let mut xs = Vec::new();
        let mut x = ob.min_x.floor() + step / 2.0;
        while x < ob.max_x {
            xs.push(x);
            x += step;
        }
        let mut mine = vec![0u8; xs.len()];
        let mut all = vec![0u8; xs.len()];
        let own_bands = EdgeBands::new(&own);
        let mut y = ob.min_y.floor() + step / 2.0;
        while y < ob.max_y {
            own_bands.inside_row(y, &xs, &mut mine);
            if !alone {
                whole.inside_row(y, &xs, &mut all);
            }
            for k in 0..xs.len() {
                if mine[k] == 0 {
                    continue;
                }
                inside += 1.0;
                probe.get_or_insert([xs[k], y]);
                if !alone && all[k] == 0 {
                    cut += 1.0;
                    cut_probe.get_or_insert([xs[k], y]);
                }
            }
            y += step;
        }
        cut *= step * step * 4.0;
        inside *= step * step * 4.0;
        let d = serialize_path(&sub);
        let hole = inside > 0.0 && cut / inside > 0.6;
        if hole {
            holes.push(BodyHole {
                d: d.clone(),
                area: jsmath::round(cut / 4.0),
            });
        }
        subpaths.push(BodySubpath {
            d,
            segs: sub,
            polys: own,
            hole,
            probe: if hole { cut_probe } else { probe },
        });
    }
    let area = filled_area(&polys, b);
    Ok(BodyAnalysis {
        polys,
        bounds: b,
        area,
        holes,
        subpaths,
    })
}

fn r1(n: f64) -> String {
    number_to_string(jsmath::round(n * 10.0) / 10.0)
}

fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|x| x.is_finite())
}

/// Check a definition you didn't write (`validateAvatar`): the normalized definition (its path
/// re-serialized from parsed numbers, its name trimmed), or every error, worded as in the TS.
///
/// Unknown-field errors are listed in the order `serde_json` keeps object keys (sorted, unless
/// the workspace enables `preserve_order`), where the TS lists them in source order.
pub fn validate_avatar(input: &Value) -> Result<AvatarDefinition, Vec<String>> {
    let mut errors: Vec<String> = Vec::new();
    let Some(obj) = input.as_object() else {
        return Err(vec!["definition must be a JSON object".into()]);
    };
    for k in obj.keys() {
        if !TOP_KEYS.contains(&k.as_str()) {
            errors.push(format!(
                "unknown field \"{}\" (allowed: {})",
                k,
                TOP_KEYS.join(", ")
            ));
        }
    }
    if obj.get("version").and_then(Value::as_f64) != Some(1.0) {
        errors.push("version must be 1".into());
    }
    let id = obj.get("id").and_then(Value::as_str);
    if !id.is_some_and(valid_slug) {
        errors.push("id must be a lowercase slug of 1-40 chars [a-z0-9-]".into());
    }
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .map(js_trim)
        .unwrap_or("");
    if name.is_empty()
        || name.encode_utf16().count() > 40
        || name
            .chars()
            .any(|c| (c as u32) < 0x20 || c == '<' || c == '>')
    {
        errors.push("name must be 1-40 printable characters without < or >".into());
    }

    let mut path: Option<&str> = None;
    match obj.get("body").and_then(Value::as_object) {
        None => errors.push("body must be an object { \"path\": \"...\" }".into()),
        Some(body) => {
            for k in body.keys() {
                if k != "path" {
                    errors.push(format!(
                        "unknown field \"body.{}\" (only \"path\" is allowed)",
                        k
                    ));
                }
            }
            match body.get("path").and_then(Value::as_str) {
                Some(p) => path = Some(p),
                None => errors.push("body.path must be a string".into()),
            }
        }
    }

    let mut eyes: Option<AvatarEyes> = None;
    match obj.get("eyes").and_then(Value::as_object) {
        None => errors.push("eyes must be an object { x, y, spacing, scale }".into()),
        Some(e) => {
            for k in e.keys() {
                if !EYE_KEYS.contains(&k.as_str()) {
                    errors.push(format!(
                        "unknown field \"eyes.{}\" (allowed: {})",
                        k,
                        EYE_KEYS.join(", ")
                    ));
                }
            }
            let bad: Vec<&str> = EYE_KEYS
                .iter()
                .copied()
                .filter(|k| num(e.get(*k)).is_none())
                .collect();
            if !bad.is_empty() {
                errors.push(format!(
                    "eyes.{} must be finite numbers",
                    bad.join(", eyes.")
                ));
            } else {
                let ev = AvatarEyes {
                    x: num(e.get("x")).unwrap_or(0.0),
                    y: num(e.get("y")).unwrap_or(0.0),
                    spacing: num(e.get("spacing")).unwrap_or(0.0),
                    scale: num(e.get("scale")).unwrap_or(0.0),
                };
                if ev.scale < 0.6 || ev.scale > 1.8 {
                    errors.push(format!(
                        "eyes.scale {} out of range 0.6..1.8",
                        number_to_string(ev.scale)
                    ));
                }
                let min_spacing = EYE_W * ev.scale * 1.75;
                if ev.spacing < min_spacing || ev.spacing > 40.0 {
                    errors.push(format!(
                        "eyes.spacing {} out of range {}..40 for scale {}",
                        number_to_string(ev.spacing),
                        r1(min_spacing),
                        number_to_string(ev.scale)
                    ));
                }
                eyes = Some(ev);
            }
        }
    }

    let mut normalized = String::new();
    let mut body: Option<BodyAnalysis> = None;
    if let Some(path) = path {
        match parse_path(path, true).and_then(|segs| {
            normalized = serialize_path(&segs);
            analyze_body(&normalized, true)
        }) {
            Ok(b) => body = Some(b),
            Err(e) => errors.push(format!("body.path: {}", e.0)),
        }
    }

    if let Some(body) = &body {
        let b = body.bounds;
        let mut out: Vec<String> = Vec::new();
        if b.min_x < BODY_MIN {
            out.push(format!("left edge x={}", r1(b.min_x)));
        }
        if b.min_y < BODY_MIN {
            out.push(format!("top edge y={}", r1(b.min_y)));
        }
        if b.max_x > BODY_MAX {
            out.push(format!("right edge x={}", r1(b.max_x)));
        }
        if b.max_y > BODY_MAX {
            out.push(format!("bottom edge y={}", r1(b.max_y)));
        }
        if !out.is_empty() {
            errors.push(format!(
                "body.path leaves the safe area ({}); keep geometry within 8..92",
                out.join(", ")
            ));
        }
        let w = b.max_x - b.min_x;
        let h = b.max_y - b.min_y;
        if jsmath::max(w, h) < 40.0 || jsmath::min(w, h) < 14.0 {
            errors.push(format!(
                "body is too small ({}x{}); use at least 40 units on the long side and 14 on the short side",
                r1(w),
                r1(h)
            ));
        }
        if body.area < 600.0 {
            errors.push(format!(
                "body fill area {} is too thin to carry eyes; must be at least 600 units²",
                number_to_string(body.area)
            ));
        }
        if let Some(eyes) = &eyes {
            let index = EdgeBands::new(&body.polys);
            if let Some(miss) = eyes_fit(&index, eyes, 0.0, 0.0, EYE_MARGIN) {
                errors.push(format!(
                    "eyes must sit inside the body with {} units margin; point ({}, {}) is outside or in a hole. \
                     Move eyes.x/y toward the widest solid area, reduce spacing or scale.",
                    number_to_string(EYE_MARGIN),
                    r1(miss[0]),
                    r1(miss[1])
                ));
            }
        }
    }

    match eyes {
        Some(eyes) if errors.is_empty() => Ok(AvatarDefinition {
            version: 1,
            id: id.unwrap_or_default().to_string(),
            name: name.to_string(),
            body: AvatarBody { path: normalized },
            eyes,
        }),
        _ => Err(errors),
    }
}

/// Validates a typed definition (see [`validate_avatar`]).
pub fn validate_definition(def: &AvatarDefinition) -> Result<AvatarDefinition, Vec<String>> {
    validate_avatar(&def.to_value())
}

/// `^[a-z0-9](?:[a-z0-9-]{0,38}[a-z0-9])?$`
fn valid_slug(id: &str) -> bool {
    let b = id.as_bytes();
    let edge = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    !b.is_empty()
        && b.len() <= 40
        && edge(b[0])
        && edge(b[b.len() - 1])
        && b.iter().all(|&c| edge(c) || c == b'-')
}
