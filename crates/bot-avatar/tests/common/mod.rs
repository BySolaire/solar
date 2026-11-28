//! Shared by the golden and canvas tests: RenderOptions from the JSON options a fixture gives
//! the TypeScript engine.

#![allow(dead_code)]

use bot_avatar::{
    AvatarMood, AvatarMotion, AvatarState, BlendFrom, Gaze, MotionBlend, RenderOptions, State,
    Tuning, View,
};
use serde_json::Value;

pub fn opt_f(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}

pub fn state_of(v: Option<&Value>) -> Option<State> {
    v.and_then(Value::as_str)
        .and_then(AvatarState::parse)
        .map(State::Named)
}

pub fn mood_of(v: Option<&Value>) -> Option<AvatarMood> {
    v.and_then(Value::as_str).and_then(AvatarMood::parse)
}

pub fn motion_of(v: Option<&Value>) -> Option<AvatarMotion> {
    v.and_then(Value::as_str).and_then(AvatarMotion::parse)
}

pub fn blend_of(v: &Value) -> Option<MotionBlend> {
    if v.is_null() {
        return None;
    }
    let from = &v["from"];
    let from = if let Some(m) = from.as_str() {
        BlendFrom {
            motion: AvatarMotion::parse(m),
            ..BlendFrom::default()
        }
    } else {
        BlendFrom {
            state: state_of(from.get("state")),
            mood: mood_of(from.get("mood")),
            motion: motion_of(from.get("motion")),
            age: opt_f(from, "age"),
            blend: from.get("blend").and_then(blend_of).map(Box::new),
        }
    };
    Some(MotionBlend {
        from,
        t: v["t"].as_f64().unwrap_or(f64::NAN),
    })
}

pub fn options_of(o: &Value) -> RenderOptions {
    RenderOptions {
        color: o.get("color").and_then(Value::as_str).map(String::from),
        eye_color: o.get("eyeColor").and_then(Value::as_str).map(String::from),
        size: opt_f(o, "size"),
        depth: opt_f(o, "depth"),
        view: o.get("view").map(|v| View {
            yaw: opt_f(v, "yaw").unwrap_or(0.0),
            pitch: opt_f(v, "pitch").unwrap_or(0.0),
        }),
        time: opt_f(o, "time"),
        gaze: o.get("gaze").filter(|g| !g.is_null()).map(|g| Gaze {
            x: opt_f(g, "x").unwrap_or(f64::NAN),
            y: opt_f(g, "y").unwrap_or(f64::NAN),
            weight: opt_f(g, "weight"),
        }),
        state: state_of(o.get("state")),
        mood: mood_of(o.get("mood")),
        motion: motion_of(o.get("motion")),
        state_age: opt_f(o, "stateAge"),
        motion_blend: o.get("motionBlend").and_then(blend_of),
        reduced_motion: o
            .get("reducedMotion")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        tuning: Tuning::Upstream,
    }
}
