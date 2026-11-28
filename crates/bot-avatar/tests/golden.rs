//! Golden tests: every fixture written by `tests/golden/gen.mjs` from the upstream TypeScript
//! engine must be reproduced: numbers within 1e-6, path texts byte for byte (compared through
//! a 64-bit hash and their length, or in full where the fixture keeps them), shape order exactly.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use bot_avatar::motion::compute_pose;
use bot_avatar::render::depth_option;
use bot_avatar::seams::{silhouette_seams, SeamSide};
use bot_avatar::validate::analyze_body;
use bot_avatar::volume::face_room;
use bot_avatar::volume::Axis;
use bot_avatar::{
    validate_avatar, Animator, AnimatorInput, AvatarDefinition, AvatarFrame, EyeMark, FrameShape,
    MotionBlend, PoseInput, PreparedAvatar, RenderOptions, ShapeKey, Tuning,
};
use serde_json::Value;

mod common;
use common::{mood_of, options_of, state_of};

const TOL: f64 = 1e-6;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/fixtures")
}

fn load(name: &str) -> Value {
    let text = std::fs::read_to_string(fixtures().join(name))
        .unwrap_or_else(|e| panic!("{name}: {e} (run tests/golden/gen.mjs)"));
    serde_json::from_str(&text).expect("fixture JSON")
}

/// The generator's `hash`: FNV-1a and djb2-xor over UTF-16 units, and the length.
fn hash(s: &str) -> String {
    let (mut a, mut b): (u32, u32) = (2166136261, 5381);
    let mut n = 0;
    for c in s.encode_utf16() {
        a = (a ^ c as u32).wrapping_mul(16777619);
        b = b.wrapping_mul(33) ^ c as u32;
        n += 1;
    }
    format!("{a:08x}{b:08x}:{n}")
}

/// Collects mismatches with their context.
struct Check {
    errors: Vec<String>,
    checks: usize,
    ctx: String,
}

impl Check {
    fn new() -> Check {
        Check {
            errors: Vec::new(),
            checks: 0,
            ctx: String::new(),
        }
    }

    fn fail(&mut self, what: &str, msg: String) {
        if self.errors.len() < 400 {
            self.errors
                .push(format!("{} :: {}: {}", self.ctx, what, msg));
        }
    }

    fn num(&mut self, what: &str, rust: f64, ts: &Value) {
        self.num_tol(what, rust, ts, TOL);
    }

    fn num_tol(&mut self, what: &str, rust: f64, ts: &Value, tol: f64) {
        self.checks += 1;
        let ok = match ts {
            Value::Number(n) => {
                let t = n.as_f64().unwrap_or(f64::NAN);
                (rust - t).abs() <= tol
            }
            Value::String(s) => {
                (s == "NaN" && rust.is_nan())
                    || (s == "Infinity" && rust == f64::INFINITY)
                    || (s == "-Infinity" && rust == f64::NEG_INFINITY)
            }
            Value::Null => false,
            _ => false,
        };
        if !ok {
            self.fail(what, format!("rust {rust:?} vs ts {ts}"));
        }
    }

    fn nums(&mut self, what: &str, rust: &[f64], ts: &Value) {
        let Some(arr) = ts.as_array() else {
            self.fail(what, format!("ts is not an array: {ts}"));
            return;
        };
        if arr.len() != rust.len() {
            self.fail(
                what,
                format!("length rust {} vs ts {}", rust.len(), arr.len()),
            );
            return;
        }
        for (k, (r, t)) in rust.iter().zip(arr).enumerate() {
            self.num(&format!("{what}[{k}]"), *r, t);
        }
    }

    fn eq<T: PartialEq + std::fmt::Debug>(&mut self, what: &str, rust: T, ts: T) {
        self.checks += 1;
        if rust != ts {
            self.fail(what, format!("rust {rust:?} vs ts {ts:?}"));
        }
    }
}

fn mark_check(c: &mut Check, what: &str, mark: &EyeMark, ts: &Value) {
    c.eq(
        &format!("{what}.shape"),
        mark.shape(),
        ts[0].as_str().unwrap_or(""),
    );
    match mark {
        EyeMark::Pill {
            x,
            y,
            width,
            height,
            radius,
        } => {
            let t = ts
                .as_array()
                .map(|a| Value::Array(a[1..].to_vec()))
                .unwrap_or(Value::Null);
            c.nums(
                &format!("{what}.pill"),
                &[*x, *y, *width, *height, *radius],
                &t,
            );
        }
        EyeMark::Arc { stroke_width, .. } => {
            c.eq(
                &format!("{what}.d"),
                hash(&mark.d().unwrap_or_default()),
                ts[1].as_str().unwrap_or("").to_string(),
            );
            c.num(&format!("{what}.strokeWidth"), *stroke_width, &ts[2]);
        }
        EyeMark::Path { .. } => {
            c.eq(
                &format!("{what}.d"),
                hash(&mark.d().unwrap_or_default()),
                ts[1].as_str().unwrap_or("").to_string(),
            );
        }
    }
}

/// Compares a frame against the fixture's (`table`: path id → its text and flags).
fn frame_check(
    c: &mut Check,
    prepared: &PreparedAvatar,
    frame: &AvatarFrame,
    ts: &Value,
    table: &Value,
) {
    c.eq(
        "name",
        frame.name.as_str(),
        ts["name"].as_str().unwrap_or(""),
    );
    c.eq(
        "color",
        frame.color.as_str(),
        ts["color"].as_str().unwrap_or(""),
    );
    c.eq("ink", frame.ink.as_str(), ts["ink"].as_str().unwrap_or(""));
    c.nums("root", &frame.root, &ts["root"]);
    let id_of = |v: &Value| -> String {
        match v {
            Value::Number(n) => format!("{}-{}", prepared.id(), n),
            Value::String(s) => s.clone(),
            _ => String::new(),
        }
    };
    let ts_paths: Vec<String> = ts["paths"]
        .as_array()
        .map(|a| a.iter().map(id_of).collect())
        .unwrap_or_default();
    let rust_paths: Vec<String> = frame.paths.iter().map(|p| p.id.to_string()).collect();
    c.eq("paths", &rust_paths, &ts_paths);
    for p in &frame.paths {
        let Some(entry) = table.get(&*p.id) else {
            c.fail("paths", format!("{} missing from the fixture table", p.id));
            continue;
        };
        if let Some(d) = entry.get("d").and_then(Value::as_str) {
            c.eq(&format!("path {} d", p.id), p.d(), d);
        }
        c.eq(
            &format!("path {} hash", p.id),
            hash(p.d()),
            entry["h"].as_str().unwrap_or("").to_string(),
        );
        c.eq(
            &format!("path {} evenOdd", p.id),
            p.even_odd,
            entry["e"].as_bool().unwrap_or(false),
        );
        c.eq(
            &format!("path {} round", p.id),
            p.round,
            entry["r"].as_bool().unwrap_or(false),
        );
        c.nums(
            &format!("path {} box", p.id),
            &[p.bbox.x, p.bbox.y, p.bbox.w, p.bbox.h],
            &entry["b"],
        );
    }

    // Shapes, back to front.
    let shapes = &frame.shapes;
    let mut at = 0usize;
    let mut last_ribbon_matrix: Option<[f64; 6]> = None;
    let empty = vec![];
    for (si, s) in ts["shapes"].as_array().unwrap_or(&empty).iter().enumerate() {
        let tag = s[0].as_str().unwrap_or("");
        match tag {
            "S" => {
                let stack = if s[1].as_str() == Some("x") {
                    Axis::X
                } else {
                    Axis::Z
                };
                let ids = s[3].as_array().cloned().unwrap_or_default();
                let parts = s[4].as_array().cloned().unwrap_or_default();
                let js = s[5].as_array().cloned().unwrap_or_default();
                let (mut se, mut sf, mut ke, mut kf) = (0.0, 0.0, 0.0, 0.0);
                let n = ids.len();
                for k in 0..n {
                    let what = format!("shape {} (run {si}.{k})", at);
                    let Some(FrameShape::Slice { key, path, matrix }) = shapes.get(at) else {
                        c.fail(
                            &what,
                            format!(
                                "expected a slice, rust has {:?}",
                                shapes.get(at).map(|s| s.key())
                            ),
                        );
                        at += 1;
                        continue;
                    };
                    let want_key = ShapeKey::Slice {
                        part: parts[k].as_u64().unwrap_or(0) as usize,
                        stack,
                        index: js[k].as_u64().unwrap_or(0) as usize,
                    };
                    c.eq(&format!("{what} key"), *key, want_key);
                    c.eq(
                        &format!("{what} path"),
                        frame.paths[*path].id.to_string(),
                        id_of(&ids[k]),
                    );
                    c.nums(&format!("{what} abcd"), &matrix[..4], &s[2]);
                    if k == 0 {
                        c.nums(&format!("{what} ef first"), &matrix[4..], &s[6]);
                    }
                    if k == n - 1 {
                        c.nums(&format!("{what} ef last"), &matrix[4..], &s[7]);
                    }
                    se += matrix[4];
                    sf += matrix[5];
                    ke += (k + 1) as f64 * matrix[4];
                    kf += (k + 1) as f64 * matrix[5];
                    at += 1;
                }
                let nn = n as f64;
                c.num_tol(&format!("run {si} Σe"), se, &s[8][0], TOL * nn);
                c.num_tol(&format!("run {si} Σf"), sf, &s[8][1], TOL * nn);
                c.num_tol(&format!("run {si} Σke"), ke, &s[8][2], TOL * nn * nn);
                c.num_tol(&format!("run {si} Σkf"), kf, &s[8][3], TOL * nn * nn);
            }
            "s" => {
                let what = format!("shape {at}");
                match shapes.get(at) {
                    Some(sh @ FrameShape::Slice { path, matrix, .. }) => {
                        c.eq(
                            &format!("{what} key"),
                            sh.key(),
                            s[1].as_str().unwrap_or("").to_string(),
                        );
                        c.eq(
                            &format!("{what} path"),
                            frame.paths[*path].id.to_string(),
                            s[2].as_str().unwrap_or("").to_string(),
                        );
                        c.nums(&format!("{what} matrix"), matrix, &s[3]);
                    }
                    other => c.fail(
                        &what,
                        format!(
                            "expected slice {}, rust has {:?}",
                            s[1],
                            other.map(|s| s.key())
                        ),
                    ),
                }
                at += 1;
            }
            "e" => {
                let what = format!("shape {at}");
                match shapes.get(at) {
                    Some(sh @ FrameShape::Eye { matrix, mark, .. }) => {
                        c.eq(
                            &format!("{what} key"),
                            sh.key(),
                            s[1].as_str().unwrap_or("").to_string(),
                        );
                        c.nums(&format!("{what} matrix"), matrix, &s[2]);
                        mark_check(c, &format!("{what} mark"), mark, &s[3]);
                    }
                    other => c.fail(
                        &what,
                        format!(
                            "expected eye {}, rust has {:?}",
                            s[1],
                            other.map(|s| s.key())
                        ),
                    ),
                }
                at += 1;
            }
            "r" => {
                let what = format!("shape {at}");
                match shapes.get(at) {
                    Some(sh @ FrameShape::Ribbon(r)) => {
                        c.eq(
                            &format!("{what} key"),
                            sh.key(),
                            s[1].as_str().unwrap_or("").to_string(),
                        );
                        if s[2].is_null() {
                            if let Some(m) = last_ribbon_matrix {
                                c.eq(&format!("{what} matrix (same)"), r.matrix, m);
                            }
                        } else {
                            c.nums(&format!("{what} matrix"), &r.matrix, &s[2]);
                        }
                        last_ribbon_matrix = Some(r.matrix);
                        c.eq(
                            &format!("{what} front"),
                            r.front,
                            s[3].as_bool().unwrap_or(false),
                        );
                        c.eq(
                            &format!("{what} fill"),
                            r.fill.as_str(),
                            s[4].as_str().unwrap_or(""),
                        );
                        let g = &r.gradient;
                        c.nums(
                            &format!("{what} gradient"),
                            &[g.x1, g.y1, g.x2, g.y2],
                            &s[5],
                        );
                        let stops: Vec<String> = s[6]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .map(|v| v.as_str().unwrap_or("").to_string())
                                    .collect()
                            })
                            .unwrap_or_default();
                        c.eq(&format!("{what} stops"), &g.stops, &stops);
                        c.eq(
                            &format!("{what} d"),
                            hash(&r.d()),
                            s[7].as_str().unwrap_or("").to_string(),
                        );
                        c.eq(
                            &format!("{what} points"),
                            r.points.len() as u64,
                            s[8].as_u64().unwrap_or(0),
                        );
                        if let Some(p0) = r.points.first() {
                            c.nums(&format!("{what} p0"), p0, &s[9]);
                        }
                    }
                    other => c.fail(
                        &what,
                        format!(
                            "expected ribbon {}, rust has {:?}",
                            s[1],
                            other.map(|s| s.key())
                        ),
                    ),
                }
                at += 1;
            }
            _ => c.fail("shape", format!("unknown tag {tag}")),
        }
    }
    c.eq("shape count", shapes.len(), at);

    let sv = &frame.surface.view;
    c.nums(
        "surface.view",
        &[sv.c_y, sv.s_y, sv.c_p, sv.s_p, sv.facing],
        &ts["surface"]["view"],
    );
    c.num(
        "surface.scale",
        frame.surface.scale,
        &ts["surface"]["scale"],
    );
    let eyes = ts["surface"]["eyes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    c.eq("surface.eyes", frame.surface.eyes.len(), eyes.len());
    for (k, (e, t)) in frame.surface.eyes.iter().zip(&eyes).enumerate() {
        mark_check(c, &format!("surface.eye{k}"), &e.mark, &t[0]);
        c.num(&format!("surface.eye{k}.dx"), e.dx, &t[1]);
        c.num(&format!("surface.eye{k}.dy"), e.dy, &t[2]);
    }
}

fn volume_check(c: &mut Check, p: &PreparedAvatar, ts: &Value) {
    let v = p.volume();
    let b = v.bounds;
    c.nums(
        "bounds",
        &[b.min_x, b.min_y, b.max_x, b.max_y],
        &ts["bounds"],
    );
    c.eq(
        "eyePart",
        v.eye_part as u64,
        ts["eyePart"].as_u64().unwrap_or(99),
    );
    c.nums("eyeZ", &v.eye_z, &ts["eyeZ"]);
    c.num("eyeScale", v.eye_scale, &ts["eyeScale"]);
    for (k, pl) in v.eye_planes.iter().enumerate() {
        c.nums(
            &format!("eyePlane{k}"),
            &[pl.x, pl.y, pl.z, pl.dzdx, pl.dzdy],
            &ts["eyePlanes"][k],
        );
    }
    let e = p.extent();
    c.nums("extent", &[e.x, e.y, e.size], &ts["extent"]);
    let parts = ts["parts"].as_array().cloned().unwrap_or_default();
    c.eq("parts", v.parts.len(), parts.len());
    for (i, (part, t)) in v.parts.iter().zip(&parts).enumerate() {
        let w = |s: &str| format!("part{i}.{s}");
        c.eq(
            &w("kind"),
            part.kind.as_str(),
            t["kind"].as_str().unwrap_or(""),
        );
        c.num(&w("depth"), part.depth, &t["depth"]);
        c.num(&w("thickness"), part.thickness, &t["thickness"]);
        c.num(&w("rxy"), part.rxy, &t["rxy"]);
        c.num(&w("rz"), part.rz, &t["rz"]);
        match (part.axis, t["axis"].as_f64()) {
            (None, None) => {}
            (Some(a), Some(_)) => c.num(&w("axis"), a, &t["axis"]),
            (a, b) => c.fail(&w("axis"), format!("rust {a:?} vs ts {b:?}")),
        }
        c.nums(&w("center"), &part.center, &t["center"]);
        c.nums(
            &w("bounds"),
            &[
                part.bounds.min_x,
                part.bounds.min_y,
                part.bounds.max_x,
                part.bounds.max_y,
            ],
            &t["bounds"],
        );
        c.eq(
            &w("d"),
            hash(&part.d),
            t["d"].as_str().unwrap_or("").to_string(),
        );
        let zs = t["z"].as_array().cloned().unwrap_or_default();
        c.eq(&w("z slices"), part.z_slices.len(), zs.len());
        for (j, (s, tz)) in part.z_slices.iter().zip(&zs).enumerate() {
            let w = |x: &str| format!("part{i}.z{j}.{x}");
            c.num(&w("at"), s.at, &tz[0]);
            c.num(&w("inset"), s.inset, &tz[1]);
            c.eq(
                &w("d"),
                hash(&s.geom.outline.d),
                tz[2].as_str().unwrap_or("").to_string(),
            );
            c.eq(
                &w("evenOdd"),
                s.geom.even_odd,
                tz[3].as_bool().unwrap_or(false),
            );
            let bb = s.geom.bbox;
            c.nums(&w("box"), &[bb.x, bb.y, bb.w, bb.h], &tz[4]);
            c.eq(
                &w("points"),
                s.geom.loops.iter().map(|l| l.len() as u64).sum::<u64>(),
                tz[5].as_u64().unwrap_or(0),
            );
        }
        let xs = t["x"].as_array().cloned().unwrap_or_default();
        c.eq(&w("x slices"), part.x_slices.len(), xs.len());
        for (j, (s, tx)) in part.x_slices.iter().zip(&xs).enumerate() {
            let w = |x: &str| format!("part{i}.x{j}.{x}");
            c.num(&w("at"), s.at, &tx[0]);
            c.eq(
                &w("d"),
                hash(&s.geom.outline.d),
                tx[1].as_str().unwrap_or("").to_string(),
            );
            let bb = s.geom.bbox;
            c.nums(&w("box"), &[bb.x, bb.y, bb.w, bb.h], &tx[2]);
            c.eq(
                &w("points"),
                s.geom.loops.iter().map(|l| l.len() as u64).sum::<u64>(),
                tx[3].as_u64().unwrap_or(0),
            );
        }
    }
}

fn pose_check(c: &mut Check, o: &RenderOptions, ts: &Value) {
    let pose = compute_pose(&PoseInput {
        seed: 0,
        time: o.time.unwrap_or(0.0),
        state: o.state.as_ref(),
        mood: o.mood,
        motion: o.motion,
        state_age: o.state_age,
        gaze: o.gaze,
        reduced_motion: o.reduced_motion,
        view: o.view,
        blend: o.motion_blend.as_ref(),
    });
    let nums = [
        pose.yaw,
        pose.pitch,
        pose.roll,
        pose.lift,
        pose.sx,
        pose.sy,
        pose.gaze_x,
        pose.gaze_y,
        pose.blink,
        pose.squint_l,
        pose.squint_r,
        pose.eyes.arc,
        pose.eyes.slit,
        pose.ribbons,
        pose.cage,
        pose.sweep,
    ];
    let arr = ts.as_array().cloned().unwrap_or_default();
    c.nums(
        "pose",
        &nums,
        &Value::Array(arr[..16.min(arr.len())].to_vec()),
    );
    let marks = arr
        .get(16)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    c.eq("pose.marks", pose.marks.len(), marks.len());
    for (m, t) in pose.marks.iter().zip(&marks) {
        c.eq("pose.mark id", m.mark.id(), t[0].as_str().unwrap_or(""));
        c.num("pose.mark weight", m.weight, &t[1]);
    }
}

/// The body analysis, faceRoom and silhouetteSeams (the strict-only checks).
fn strict_check(c: &mut Check, def: &AvatarDefinition, ts: &Value) {
    let body = analyze_body(&def.body.path, true).expect("valid path");
    c.num("area", body.area, &ts["area"]);
    let holes = ts["holes"].as_array().cloned().unwrap_or_default();
    c.eq("holes", body.holes.len(), holes.len());
    for (h, t) in body.holes.iter().zip(&holes) {
        c.eq(
            "hole d",
            hash(&h.d),
            t[0].as_str().unwrap_or("").to_string(),
        );
        c.num("hole area", h.area, &t[1]);
    }
    let subs = ts["subpaths"].as_array().cloned().unwrap_or_default();
    c.eq("subpaths", body.subpaths.len(), subs.len());
    for (sp, t) in body.subpaths.iter().zip(&subs) {
        c.eq(
            "subpath d",
            hash(&sp.d),
            t[0].as_str().unwrap_or("").to_string(),
        );
        c.eq("subpath hole", sp.hole, t[1].as_bool().unwrap_or(false));
        match (sp.probe, t[2].is_null()) {
            (None, true) => {}
            (Some(p), false) => c.nums("subpath probe", &p, &t[2]),
            (p, _) => c.fail("subpath probe", format!("rust {p:?} vs ts {}", t[2])),
        }
    }
    let room = face_room(def, Some(&body)).expect("face room");
    let fr = &ts["faceRoom"];
    c.num("faceRoom.boost", room.boost, &fr[0]);
    c.num("faceRoom.clearance", room.clearance, &fr[1]);
    c.nums(
        "faceRoom.gaze",
        &[
            room.gaze.left,
            room.gaze.right,
            room.gaze.up,
            room.gaze.down,
        ],
        &fr[2],
    );
    c.eq("faceRoom.ok", room.ok, fr[3].as_bool().unwrap_or(false));
    let seams = silhouette_seams(&body.polys);
    let want = ts["seams"].as_array().cloned().unwrap_or_default();
    c.eq("seams", seams.len(), want.len());
    for (sm, t) in seams.iter().zip(&want) {
        let side = match sm.side {
            SeamSide::Left => "left",
            SeamSide::Right => "right",
            SeamSide::Top => "top",
            SeamSide::Bottom => "bottom",
        };
        c.eq("seam side", side, t[0].as_str().unwrap_or(""));
        c.num("seam at", sm.at, &t[1]);
        c.num("seam size", sm.size, &t[2]);
    }
}

fn report(c: Check, what: &str) {
    if !c.errors.is_empty() {
        panic!(
            "{what}: {} mismatches (of {} checks), first ones:\n{}",
            c.errors.len(),
            c.checks,
            c.errors.join("\n")
        );
    }
    eprintln!("{what}: {} checks passed", c.checks);
}

#[test]
fn golden_avatars() {
    let index = load("index.json");
    let files: Vec<String> = index["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| f.as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(
        files.len(),
        33,
        "7 examples, 16 gallery avatars and 10 Solar avatars"
    );
    let only = std::env::var("GOLDEN_ONLY").ok();
    let mut c = Check::new();
    let mut frames = 0;
    for file in &files {
        if only.as_ref().is_some_and(|o| !file.contains(o.as_str())) {
            continue;
        }
        let fx = load(file);
        c.ctx = file.clone();
        // Validation and normalization.
        let normalized = validate_avatar(&fx["definition"]);
        let want: AvatarDefinition =
            serde_json::from_value(fx["normalized"].clone()).expect("normalized");
        match normalized {
            Ok(def) => {
                c.eq(
                    "normalized path",
                    def.body.path.as_str(),
                    want.body.path.as_str(),
                );
                c.eq("normalized name", def.name.as_str(), want.name.as_str());
                c.eq("normalized eyes", def.eyes, want.eyes);
            }
            Err(e) => c.fail("validate", format!("{e:?}")),
        }
        let def: AvatarDefinition =
            serde_json::from_value(fx["definition"].clone()).expect("definition");
        strict_check(&mut c, &want, &fx["strict"]);
        let mut prepared: HashMap<u64, Arc<PreparedAvatar>> = HashMap::new();
        let mut prep = |depth: Option<f64>| -> Arc<PreparedAvatar> {
            let d = depth_option(depth);
            prepared
                .entry(d.to_bits())
                .or_insert_with(|| {
                    Arc::new(PreparedAvatar::new(&def, Some(d)).expect("valid definition"))
                })
                .clone()
        };
        for v in fx["volumes"].as_array().expect("volumes") {
            let depth = v["depth"].as_f64();
            c.ctx = format!("{file} volume depth {:?}", depth);
            let p = prep(depth);
            volume_check(&mut c, &p, v);
        }
        for case in fx["cases"].as_array().expect("cases") {
            let name = case["name"].as_str().unwrap_or("");
            c.ctx = format!("{file} case {name}");
            let o = options_of(&case["o"]);
            let p = prep(o.depth);
            let frame = if case["live"].as_bool() == Some(true) {
                p.frame(&o)
            } else {
                p.avatar_frame(&o)
            };
            let depth_key = case["o"]
                .get("depth")
                .map(|d| d.to_string())
                .unwrap_or_else(|| "6".into());
            let table = &fx["tables"][depth_key.as_str()];
            frame_check(&mut c, &p, &frame, &case["frame"], table);
            pose_check(&mut c, &o, &case["pose"]);
            frames += 1;
        }
    }
    eprintln!("{frames} frames");
    report(c, "golden avatars");
}

#[test]
fn golden_validation() {
    let fx = load("validate.json");
    let mut c = Check::new();
    for (k, case) in fx.as_array().expect("cases").iter().enumerate() {
        c.ctx = format!("validate case {k}");
        let result = validate_avatar(&case["input"]);
        let ts = &case["result"];
        match result {
            Ok(def) => {
                c.eq("ok", true, ts["ok"].as_bool().unwrap_or(false));
                c.eq(
                    "path",
                    def.body.path.as_str(),
                    ts["value"]["body"]["path"].as_str().unwrap_or(""),
                );
            }
            Err(errors) => {
                c.eq("ok", false, ts["ok"].as_bool().unwrap_or(true));
                let want: Vec<String> = ts["errors"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|v| v.as_str().unwrap_or("").to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                c.eq("errors", errors, want);
            }
        }
    }
    report(c, "golden validation");
}

fn blend_check(c: &mut Check, what: &str, rust: Option<&MotionBlend>, ts: &Value) {
    match (rust, ts.is_null()) {
        (None, true) => {}
        (Some(b), false) => {
            c.num(&format!("{what}.t"), b.t, &ts["t"]);
            let f = &ts["from"];
            c.eq(
                &format!("{what}.state"),
                b.from.state.as_ref().map(|s| s.id().to_string()),
                f["state"].as_str().map(String::from),
            );
            c.eq(
                &format!("{what}.mood"),
                b.from.mood.map(|m| m.as_str()),
                f["mood"].as_str(),
            );
            match (b.from.age, f["age"].as_f64()) {
                (None, None) => {}
                (Some(a), Some(_)) => c.num(&format!("{what}.age"), a, &f["age"]),
                (a, t) => c.fail(&format!("{what}.age"), format!("rust {a:?} vs ts {t:?}")),
            }
            blend_check(
                c,
                &format!("{what}.from.blend"),
                b.from.blend.as_deref(),
                &f["blend"],
            );
        }
        (r, _) => c.fail(what, format!("rust {:?} vs ts {ts}", r.map(|b| b.t))),
    }
}

#[test]
fn golden_animator() {
    let fx = load("animator.json");
    let def = bot_avatar::example(fx["definition"].as_str().unwrap_or("")).expect("example");
    let prepared = PreparedAvatar::new(&def, None).expect("valid");
    let mut c = Check::new();
    for (name, steps) in fx["sequences"].as_object().expect("sequences") {
        let mut animator = Animator::new();
        for (k, step) in steps.as_array().expect("steps").iter().enumerate() {
            c.ctx = format!("animator {name} step {k}");
            let i = &step["input"];
            let target = i["target"]
                .as_array()
                .map(|t| (t[0].as_f64().unwrap_or(0.0), t[1].as_f64().unwrap_or(0.0)));
            let state = state_of(i.get("state"));
            let mood = mood_of(i.get("mood"));
            let reduced = i["reducedMotion"].as_bool().unwrap_or(false);
            let out = animator.step(&AnimatorInput {
                now: i["now"].as_f64().unwrap_or(0.0),
                state: state.clone(),
                mood,
                motion: None,
                target,
                hold: i["hold"].as_bool().unwrap_or(false),
                reduced_motion: reduced,
            });
            let ts = &step["step"];
            match (&out.gaze, ts["gaze"].is_null()) {
                (None, true) => {}
                (Some(g), false) => {
                    c.nums("gaze", &[g.x, g.y, g.weight.unwrap_or(1.0)], &ts["gaze"])
                }
                (g, _) => c.fail("gaze", format!("rust {g:?} vs ts {}", ts["gaze"])),
            }
            blend_check(
                &mut c,
                "motionBlend",
                out.motion_blend.as_ref(),
                &ts["motionBlend"],
            );
            c.num("stateAge", out.state_age, &ts["stateAge"]);
            c.eq(
                "moving",
                out.moving,
                ts["moving"].as_bool().unwrap_or(false),
            );
            let o = RenderOptions {
                color: Some("#1fbf68".into()),
                state,
                mood,
                time: Some(i["now"].as_f64().unwrap_or(0.0) / 1000.0),
                state_age: Some(out.state_age),
                gaze: out.gaze,
                motion_blend: out.motion_blend.clone(),
                reduced_motion: reduced,
                tuning: Tuning::Upstream,
                ..RenderOptions::default()
            };
            let frame = prepared.avatar_frame(&o);
            frame_check(&mut c, &prepared, &frame, &step["frame"], &step["table"]);
        }
    }
    report(c, "golden animator");
}

#[test]
fn golden_seams() {
    let fx = load("seams.json");
    let mut c = Check::new();
    let mut found = 0;
    for case in fx.as_array().expect("cases") {
        let path = case["path"].as_str().unwrap_or("");
        c.ctx = format!("seams {path}");
        let body = analyze_body(path, true).expect("path");
        let seams = silhouette_seams(&body.polys);
        let want = case["seams"].as_array().cloned().unwrap_or_default();
        found += want.len();
        c.eq("seams", seams.len(), want.len());
        for (sm, t) in seams.iter().zip(&want) {
            c.eq(
                "seam side",
                format!("{:?}", sm.side).to_lowercase(),
                t[0].as_str().unwrap_or("").to_string(),
            );
            c.num("seam at", sm.at, &t[1]);
            c.num("seam size", sm.size, &t[2]);
        }
    }
    assert!(found >= 3, "the fixture bodies have seams");
    report(c, "golden seams");
}
