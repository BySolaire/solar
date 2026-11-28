//! Raster sanity tests: a few frames painted to PNG under `target/tmp/bot-avatar/`, checked for
//! the body colour, the eye ink and the ribbons. (The browser painter cannot run here; the
//! painter mirrors `dom/paint.ts` line by line instead.)

use bot_avatar::paint::{eyes_cache_key, PaintBounds};
use bot_avatar::{
    example, fit_square, paint, solar_avatar, working_compact, AvatarFrame, AvatarMood,
    AvatarState, CanvasSurface, FrameShape, PreparedAvatar, RenderOptions, Square, Tuning,
};
use tiny_skia::Pixmap;

fn out_dir() -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("bot-avatar");
    std::fs::create_dir_all(&dir).expect("output dir");
    dir
}

/// Fully opaque pixels of exactly this colour.
fn count(p: &Pixmap, hex: &str) -> usize {
    let [r, g, b] = bot_avatar::color::parse_hex(hex).expect("hex");
    p.pixels()
        .iter()
        .filter(|px| {
            px.alpha() == 255
                && px.red() == r as u8
                && px.green() == g as u8
                && px.blue() == b as u8
        })
        .count()
}

fn opaque(p: &Pixmap) -> usize {
    p.pixels().iter().filter(|px| px.alpha() == 255).count()
}

fn save(p: &Pixmap, name: &str) {
    p.save_png(out_dir().join(name)).expect("png");
}

fn frame(def_id: &str, solar: bool, o: RenderOptions) -> (PreparedAvatar, AvatarFrame) {
    let def = if solar {
        solar_avatar(def_id).cloned()
    } else {
        example(def_id)
    }
    .expect("definition");
    let p = PreparedAvatar::new(&def, o.depth).expect("valid");
    let f = p.avatar_frame(&o);
    (p, f)
}

#[test]
fn idle_pill_eyes() {
    let (_, f) = frame(
        "controller",
        false,
        RenderOptions {
            color: Some("#ff6a1a".into()),
            time: Some(0.37),
            ..Default::default()
        },
    );
    let px = paint(&f, 256);
    save(&px, "controller-idle.png");
    assert!(
        count(&px, "#ff6a1a") > 20_000,
        "body colour fills most of the silhouette"
    );
    assert!(count(&px, "#151515") > 300, "two dark pill eyes");
    assert!(f.shapes.iter().any(|s| matches!(s, FrameShape::Eye { .. })));
}

#[test]
fn happy_arc_and_waiting_slits() {
    let o = RenderOptions {
        color: Some("#f8a01c".into()),
        mood: Some(AvatarMood::Happy),
        time: Some(1.0),
        ..Default::default()
    };
    let (_, f) = frame("mug", false, o);
    let px = paint(&f, 192);
    save(&px, "mug-happy.png");
    assert!(count(&px, "#f8a01c") > 8_000);
    assert!(count(&px, "#151515") > 80, "stroked arcs");

    let o = RenderOptions {
        color: Some("#8b5cf6".into()),
        state: Some(AvatarState::Waiting.into()),
        time: Some(2.0),
        ..Default::default()
    };
    let (_, f) = frame("ghost", true, o);
    let px = paint(&f, 192);
    save(&px, "ghost-waiting.png");
    assert!(count(&px, "#8b5cf6") > 8_000);
    assert!(count(&px, "#151515") > 40, "filled slits");
}

#[test]
fn working_ribbons_and_compact() {
    // A working turn under way (its timing is seeded per avatar): the moment of the first turn
    // with the most ribbon pieces. They are painted in shades of the body colour.
    let def = solar_avatar("robot").expect("robot");
    let p = PreparedAvatar::new(def, None).expect("valid");
    let at = |age: f64| RenderOptions {
        color: Some("#1f7cf2".into()),
        state: Some(AvatarState::Working.into()),
        time: Some(age),
        state_age: Some(age),
        ..Default::default()
    };
    let pieces = |f: &AvatarFrame| {
        f.shapes
            .iter()
            .filter(|s| matches!(s, FrameShape::Ribbon(_)))
            .count()
    };
    let best = (0..80)
        .map(|k| 1.0 + k as f64 * 0.05)
        .max_by_key(|&age| pieces(&p.avatar_frame(&at(age))))
        .expect("ages");
    let base = at(best);
    let f = p.avatar_frame(&base);
    assert!(pieces(&f) > 0, "the first working turn comes within 5 s");
    let px = paint(&f, 256);
    save(&px, "robot-working.png");
    let body = count(&px, "#1f7cf2");
    let ink = count(&px, "#151515");
    let other = opaque(&px) - body - ink;
    assert!(
        body > 5_000 && other > 2_000,
        "body {body}, ribbons and edges {other}"
    );

    // working-compact: the same pose without ribbons.
    let compact = RenderOptions {
        state: Some(working_compact()),
        ..base.clone()
    };
    let fc = p.avatar_frame(&compact);
    assert!(fc
        .shapes
        .iter()
        .all(|s| !matches!(s, FrameShape::Ribbon(_))));
    let (pw, pc) = (p.pose(&base), p.pose(&compact));
    assert!(pw.ribbons > 0.0 && pc.ribbons == 0.0);
    assert_eq!(
        (pw.yaw, pw.roll, pw.lift, pw.gaze_x),
        (pc.yaw, pc.roll, pc.lift, pc.gaze_x),
        "same body motion"
    );
    save(&paint(&fc, 256), "robot-working-compact.png");
}

#[test]
fn thinking_marks_replace_the_body() {
    let o = RenderOptions {
        color: Some("#ec4899".into()),
        state: Some(AvatarState::Thinking.into()),
        time: Some(1.0),
        ..Default::default()
    };
    let (_, f) = frame("cat", true, o);
    let px = paint(&f, 160);
    save(&px, "cat-thinking.png");
    assert!(
        count(&px, "#ec4899") > 1_000,
        "three dots in the body colour"
    );
    assert_eq!(count(&px, "#151515"), 0, "marks have no eyes");
}

#[test]
fn solar_tuning_keeps_grays_neutral() {
    let o = |tuning| RenderOptions {
        color: Some("#f4f4f4".into()),
        state: Some(AvatarState::Done.into()),
        time: Some(1.0),
        state_age: Some(1.0),
        tuning,
        ..Default::default()
    };
    let p = PreparedAvatar::new(solar_avatar("skull").expect("skull"), None).expect("valid");
    let stops = |f: &AvatarFrame| -> Vec<String> {
        f.shapes
            .iter()
            .filter_map(|s| {
                if let FrameShape::Ribbon(r) = s {
                    Some(r.gradient.stops.clone())
                } else {
                    None
                }
            })
            .flatten()
            .collect()
    };
    let solar = stops(&p.avatar_frame(&o(Tuning::Solar)));
    let upstream = stops(&p.avatar_frame(&o(Tuning::Upstream)));
    assert!(!solar.is_empty());
    assert!(
        solar
            .iter()
            .all(|c| c[1..3] == c[3..5] && c[3..5] == c[5..7]),
        "strict grays: {solar:?}"
    );
    assert!(
        upstream
            .iter()
            .any(|c| !(c[1..3] == c[3..5] && c[3..5] == c[5..7])),
        "upstream tints its grays"
    );
    assert_eq!(
        RenderOptions::default().tuning,
        Tuning::Solar,
        "Solar tuning is the default"
    );
}

#[test]
fn every_embedded_definition_validates() {
    assert_eq!(bot_avatar::examples().len(), 7);
    assert_eq!(bot_avatar::gallery().len(), 16);
    assert_eq!(bot_avatar::solar_avatars().len(), 10);
    for def in bot_avatar::examples()
        .iter()
        .chain(bot_avatar::gallery())
        .chain(bot_avatar::solar_avatars())
    {
        bot_avatar::validate_definition(def).unwrap_or_else(|e| panic!("{}: {e:?}", def.id));
    }
    for id in [
        "invader", "ghost", "robot", "cat", "skull", "crab", "mushroom", "rocket", "dino", "frog",
    ] {
        assert!(solar_avatar(id).is_some(), "{id}");
    }
    assert_eq!(bot_avatar::gallery_color("controller"), Some("#ff6a1a"));
}

#[test]
fn canvas_surface_lays_out_and_reuses() {
    let def = solar_avatar("frog").expect("frog");
    let p = PreparedAvatar::new(def, None).expect("valid");
    let mut surface = CanvasSurface::new(&p);
    // A still pose, reduced motion: the second frame only reuses the bitmap.
    let o = |t: f64| RenderOptions {
        color: Some("#1fbf68".into()),
        time: Some(t),
        size: Some(56.0),
        reduced_motion: true,
        ..Default::default()
    };
    let first = surface.paint_live(&p.live_transform(&o(0.0)), 56.0, 2.0, None, None);
    assert!(!first.reused);
    let px = surface.pixmap().expect("bitmap");
    // What is painted plus FIT_SLACK, at 1.12 device px per unit, never the whole extent.
    assert!(px.width() > 90 && px.width() < 200, "bitmap {}", px.width());
    assert!(count(px, "#1fbf68") > 1_000);
    save(px, "frog-live-56@2x.png");
    let second = surface.paint_live(&p.live_transform(&o(0.5)), 56.0, 2.0, None, None);
    assert!(second.reused);
    assert_eq!(first.eyes_key, second.eyes_key);
    assert_eq!(
        eyes_cache_key(&p.live_transform(&o(0.0)).eyes),
        first.eyes_key
    );

    // paintCanvas bakes the root into a bitmap covering what is painted.
    let f = p.frame(&RenderOptions {
        time: Some(3.0),
        size: Some(28.0),
        ..Default::default()
    });
    let painted = surface.paint(&f, 28.0, 1.0, None, None).expect("bitmap");
    assert!(painted.width() > 20);
}

#[test]
fn fit_square_follows_the_ts() {
    let cap = Square {
        x: -20.0,
        y: -20.0,
        size: 140.0,
    };
    let b = PaintBounds {
        x0: 10.0,
        y0: 20.0,
        x1: 90.0,
        y1: 80.0,
    };
    let s = fit_square(None, b, cap);
    assert_eq!(
        s,
        Square {
            x: 2.0,
            y: 2.0,
            size: 96.0
        }
    );
    // Still holds it: kept as it is.
    assert_eq!(
        fit_square(
            Some(s),
            PaintBounds {
                x0: 12.0,
                y0: 20.0,
                x1: 88.0,
                y1: 80.0
            },
            cap
        ),
        s
    );
    // Empty bounds: the previous square, else the cap.
    let empty = PaintBounds {
        x0: 1.0,
        y0: 0.0,
        x1: 0.0,
        y1: 0.0,
    };
    assert_eq!(fit_square(None, empty, cap), cap);
}

#[test]
fn default_eye_ink_is_neutral() {
    // DIRECTION.md: eyes are always #151515 (the engine's ink); autoEyeColor is never used.
    let p = PreparedAvatar::new(&example("controller").expect("controller"), None).expect("valid");
    for color in ["#ffffff", "#000000", "#1f7cf2", "#f4f4f4"] {
        let f = p.avatar_frame(&RenderOptions {
            color: Some(color.into()),
            ..Default::default()
        });
        assert_eq!(f.ink, "#151515", "body {color}");
    }
}
