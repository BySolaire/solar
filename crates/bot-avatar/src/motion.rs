//! Deterministic pose model (port of `motion.ts`). A pose is a pure function of (seed, time,
//! state, mood, gaze, reduced motion): no wall clock, no randomness, so any frame can be
//! reproduced exactly.

use std::fmt;
use std::sync::Arc;

use crate::jsmath::{self, clamp, cos, exp, imul, sin, smooth, to_int32};
use crate::marks::{bang, blend_marks, dots, pose_marks, PoseMark};

const TAU: f64 = std::f64::consts::PI * 2.0;
const PI: f64 = std::f64::consts::PI;

/// What the bot is doing (`AvatarState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AvatarState {
    Idle,
    Thinking,
    Working,
    Waiting,
    Blocked,
    Done,
}

/// Every state, in the order hosts list them (`STATES`).
pub const STATES: [AvatarState; 6] = [
    AvatarState::Idle,
    AvatarState::Thinking,
    AvatarState::Working,
    AvatarState::Waiting,
    AvatarState::Blocked,
    AvatarState::Done,
];

impl AvatarState {
    pub fn as_str(self) -> &'static str {
        match self {
            AvatarState::Idle => "idle",
            AvatarState::Thinking => "thinking",
            AvatarState::Working => "working",
            AvatarState::Waiting => "waiting",
            AvatarState::Blocked => "blocked",
            AvatarState::Done => "done",
        }
    }

    /// `isState` for a name.
    pub fn parse(s: &str) -> Option<AvatarState> {
        STATES.into_iter().find(|st| st.as_str() == s)
    }
}

/// How the bot feels (`AvatarMood`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum AvatarMood {
    #[default]
    Neutral,
    Happy,
}

/// Every mood (`MOODS`).
pub const MOODS: [AvatarMood; 2] = [AvatarMood::Neutral, AvatarMood::Happy];

impl AvatarMood {
    pub fn as_str(self) -> &'static str {
        match self {
            AvatarMood::Neutral => "neutral",
            AvatarMood::Happy => "happy",
        }
    }

    /// `isMood` for a name.
    pub fn parse(s: &str) -> Option<AvatarMood> {
        MOODS.into_iter().find(|m| m.as_str() == s)
    }
}

/// The single option that came before `state` and `mood` (`AvatarMotion`, deprecated upstream).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AvatarMotion {
    Idle,
    Thinking,
    Happy,
}

impl AvatarMotion {
    pub fn parse(s: &str) -> Option<AvatarMotion> {
        match s {
            "idle" => Some(AvatarMotion::Idle),
            "thinking" => Some(AvatarMotion::Thinking),
            "happy" => Some(AvatarMotion::Happy),
            _ => None,
        }
    }
}

/// How the eyes are drawn, as numbers that blend (`EyeLook`).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct EyeLook {
    /// 0 = the plain pill, 1 = the happy arc.
    pub arc: f64,
    /// 0 = the plain pill, 1 = the flat slit of a waiting bot.
    pub slit: f64,
}

/// A pose (`Pose`).
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    /// View rotation in radians.
    pub yaw: f64,
    pub pitch: f64,
    /// In-plane rotation, degrees.
    pub roll: f64,
    /// Vertical offset (negative = up), viewBox units.
    pub lift: f64,
    /// Squash and stretch about the base of the body.
    pub sx: f64,
    pub sy: f64,
    /// Eye look direction, -1..1 per axis.
    pub gaze_x: f64,
    pub gaze_y: f64,
    /// 0 = open, 1 = fully closed.
    pub blink: f64,
    /// Extra lid narrowing per eye, 0..1.
    pub squint_l: f64,
    pub squint_r: f64,
    pub eyes: EyeLook,
    /// Presence of the ribbons, 0..1.
    pub ribbons: f64,
    /// 0 a working turn's one hoop, 1 a celebration's cage of three.
    pub cage: f64,
    /// How far the body's turn has carried the ribbons round, radians.
    pub sweep: f64,
    /// The marks the body gives way to, in the order of their ids.
    pub marks: Vec<PoseMark>,
}

/// What a state's pose returns: the fields that differ from the base (`Partial<Pose>`). A field
/// left out, or not a finite number, keeps the base's.
#[derive(Clone, Debug, Default)]
pub struct PosePatch {
    pub yaw: Option<f64>,
    pub pitch: Option<f64>,
    pub roll: Option<f64>,
    pub lift: Option<f64>,
    pub sx: Option<f64>,
    pub sy: Option<f64>,
    pub gaze_x: Option<f64>,
    pub gaze_y: Option<f64>,
    pub blink: Option<f64>,
    pub squint_l: Option<f64>,
    pub squint_r: Option<f64>,
    pub eye_arc: Option<f64>,
    pub eye_slit: Option<f64>,
    pub ribbons: Option<f64>,
    pub cage: Option<f64>,
    pub sweep: Option<f64>,
    pub marks: Option<Vec<PoseMark>>,
}

impl From<Pose> for PosePatch {
    fn from(p: Pose) -> PosePatch {
        PosePatch {
            yaw: Some(p.yaw),
            pitch: Some(p.pitch),
            roll: Some(p.roll),
            lift: Some(p.lift),
            sx: Some(p.sx),
            sy: Some(p.sy),
            gaze_x: Some(p.gaze_x),
            gaze_y: Some(p.gaze_y),
            blink: Some(p.blink),
            squint_l: Some(p.squint_l),
            squint_r: Some(p.squint_r),
            eye_arc: Some(p.eyes.arc),
            eye_slit: Some(p.eyes.slit),
            ribbons: Some(p.ribbons),
            cage: Some(p.cage),
            sweep: Some(p.sweep),
            marks: Some(p.marks),
        }
    }
}

/// A state of the host's own (`AvatarStateDefinition`, experimental upstream).
pub trait StateDefinition: Send + Sync {
    /// Names the state: two definitions with one id are the same state.
    fn id(&self) -> &str;
    /// The pose at one moment: what differs from `ctx.base`. A pure function of the context.
    fn pose(&self, ctx: &StateContext) -> PosePatch;
    /// The age the state has when an avatar mounts in it, seconds (default 0).
    fn start_age(&self) -> Option<f64> {
        None
    }
}

/// A state: one of the six by name, or a definition of the host's own.
#[derive(Clone)]
pub enum State {
    Named(AvatarState),
    Custom(Arc<dyn StateDefinition>),
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::Named(s) => write!(f, "{:?}", s),
            State::Custom(d) => write!(f, "Custom({:?})", d.id()),
        }
    }
}

impl From<AvatarState> for State {
    fn from(s: AvatarState) -> State {
        State::Named(s)
    }
}

impl State {
    /// What names a state (`stateId`).
    pub fn id(&self) -> &str {
        match self {
            State::Named(s) => s.as_str(),
            State::Custom(d) => d.id(),
        }
    }

    /// `stateDefinition(state).startAge`.
    pub fn start_age(&self) -> Option<f64> {
        match self {
            State::Named(AvatarState::Done) => Some(DONE_TURN_S),
            State::Named(_) => None,
            State::Custom(d) => d.start_age(),
        }
    }
}

/// The state and mood to show (`resolveLook` / `lookOf`): a given state wins; else `motion`
/// `thinking` is a state and `happy` a mood.
pub fn look_of(
    state: Option<&State>,
    mood: Option<AvatarMood>,
    motion: Option<AvatarMotion>,
) -> (State, AvatarMood) {
    let state = match state {
        Some(s) => s.clone(),
        None if motion == Some(AvatarMotion::Thinking) => State::Named(AvatarState::Thinking),
        None => State::Named(AvatarState::Idle),
    };
    let mood = match mood {
        Some(m) => m,
        None if motion == Some(AvatarMotion::Happy) => AvatarMood::Happy,
        None => AvatarMood::Neutral,
    };
    (state, mood)
}

/// Resting view (`BASE_YAW`, `BASE_PITCH`).
pub const BASE_YAW: f64 = -0.3;
pub const BASE_PITCH: f64 = 0.2;
/// Following a look target (`LOOK_YAW`, `LOOK_PITCH`, `LOOK_EYES`).
pub const LOOK_YAW: f64 = 0.32;
pub const LOOK_PITCH: f64 = 0.18;
pub const LOOK_EYES: f64 = 0.7;

/// FNV-1a over UTF-16 code units (`hashString`).
pub fn hash_string(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for c in s.encode_utf16() {
        h ^= c as u32;
        h = imul(h, 16777619);
    }
    h
}

/// Stateless hash noise in [0, 1) (`noise`).
pub fn noise(seed: u32, n: f64) -> f64 {
    let mut h = seed ^ imul(to_int32(n) as u32, 0x9e3779b1);
    h = imul(h ^ (h >> 16), 0x85ebca6b);
    h = imul(h ^ (h >> 13), 0xc2b2ae35);
    h ^= h >> 16;
    h as f64 / 4294967296.0
}

#[inline]
fn mix(a: f64, b: f64, k: f64) -> f64 {
    a + (b - a) * k
}

/// Blink closure 0..1 (`blinkAt`).
pub fn blink_at(seed: u32, t: f64) -> f64 {
    const SLOT: f64 = 4.2;
    const DUR: f64 = 0.19;
    let slot = (t / SLOT).floor();
    let mut closure = 0.0f64;
    for s in [slot - 1.0, slot] {
        let start = s * SLOT + 0.5 + noise(seed, s * 2.0) * 2.9;
        let double = noise(seed, s * 2.0 + 1.0) > 0.8;
        let starts = [start, start + 0.34];
        for &b in &starts[..if double { 2 } else { 1 }] {
            let u = (t - b) / DUR;
            if u > 0.0 && u < 1.0 {
                closure = jsmath::max(closure, sin(PI * u));
            }
        }
    }
    closure
}

/// Idle eye wander (`wanderAt`).
pub fn wander_at(seed: u32, t: f64) -> [f64; 2] {
    const SLOT: f64 = 1.9;
    let s = (t / SLOT).floor();
    let target = |k: f64| -> [f64; 2] {
        if noise(seed, 1000.0 + k * 3.0) < 0.4 {
            return [0.0, 0.0];
        }
        [
            (noise(seed, 1001.0 + k * 3.0) * 2.0 - 1.0) * 0.6,
            (noise(seed, 1002.0 + k * 3.0) * 2.0 - 1.0) * 0.35,
        ]
    };
    let a = target(s - 1.0);
    let b = target(s);
    let e = smooth((t - s * SLOT) / 0.16);
    [a[0] + (b[0] - a[0]) * e, a[1] + (b[1] - a[1]) * e]
}

/// Working: the eyes dart from one thing to the next (`dartAt`).
fn dart_at(seed: u32, t: f64) -> [f64; 2] {
    const SLOT: f64 = 0.62;
    let s = (t / SLOT).floor();
    let target = |k: f64| -> [f64; 2] {
        [
            (noise(seed, 2001.0 + k * 3.0) * 2.0 - 1.0) * 0.8,
            0.1 + noise(seed, 2002.0 + k * 3.0) * 0.5,
        ]
    };
    let a = target(s - 1.0);
    let b = target(s);
    let e = smooth((t - s * SLOT) / 0.12);
    [mix(a[0], b[0], e), mix(a[1], b[1], e)]
}

/// A look target, -1..1 per axis; `weight` 0..1 fades it in (default 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gaze {
    pub x: f64,
    pub y: f64,
    pub weight: Option<f64>,
}

/// Extra view rotation in radians.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct View {
    pub yaw: f64,
    pub pitch: f64,
}

/// What a change eases in from (`AvatarMotionBlend.from`): a state and mood that may themselves
/// still have been easing in. A legacy plain motion is `BlendFrom { motion: Some(..), .. }`.
#[derive(Clone, Debug, Default)]
pub struct BlendFrom {
    pub state: Option<State>,
    pub mood: Option<AvatarMood>,
    pub motion: Option<AvatarMotion>,
    /// Seconds that state had been showing when the change began.
    pub age: Option<f64>,
    pub blend: Option<Box<MotionBlend>>,
}

/// A change easing in over what was shown before it (`AvatarMotionBlend`), `t` 0..1.
#[derive(Clone, Debug)]
pub struct MotionBlend {
    pub from: BlendFrom,
    pub t: f64,
}

/// Inputs of a pose (`PoseInput`).
#[derive(Clone, Debug, Default)]
pub struct PoseInput<'a> {
    pub seed: u32,
    pub time: f64,
    pub state: Option<&'a State>,
    pub mood: Option<AvatarMood>,
    pub motion: Option<AvatarMotion>,
    pub state_age: Option<f64>,
    pub gaze: Option<Gaze>,
    pub reduced_motion: bool,
    pub view: Option<View>,
    pub blend: Option<&'a MotionBlend>,
}

/// Pose part way from `a` to `b` (`blendPoses`).
pub fn blend_poses(a: &Pose, b: &Pose, t: f64) -> Pose {
    let k = clamp(t, 0.0, 1.0);
    let m = |x: f64, y: f64| mix(x, y, k);
    let sweep = if a.ribbons > 0.0 && b.ribbons > 0.0 {
        m(a.sweep, b.sweep)
    } else if a.ribbons > 0.0 {
        a.sweep
    } else {
        b.sweep
    };
    let cage = if a.ribbons > 0.0 && b.ribbons > 0.0 {
        m(a.cage, b.cage)
    } else if a.ribbons > 0.0 {
        a.cage
    } else {
        b.cage
    };
    Pose {
        yaw: m(a.yaw, b.yaw),
        pitch: m(a.pitch, b.pitch),
        roll: m(a.roll, b.roll),
        lift: m(a.lift, b.lift),
        sx: m(a.sx, b.sx),
        sy: m(a.sy, b.sy),
        gaze_x: m(a.gaze_x, b.gaze_x),
        gaze_y: m(a.gaze_y, b.gaze_y),
        blink: m(a.blink, b.blink),
        squint_l: m(a.squint_l, b.squint_l),
        squint_r: m(a.squint_r, b.squint_r),
        eyes: EyeLook {
            arc: m(a.eyes.arc, b.eyes.arc),
            slit: m(a.eyes.slit, b.eyes.slit),
        },
        ribbons: m(a.ribbons, b.ribbons),
        cage,
        sweep,
        marks: blend_marks(&a.marks, &b.marks, m),
    }
}

/// The pose at one moment (`computePose`), eased in from `blend.from` while `blend.t < 1`.
pub fn compute_pose(input: &PoseInput) -> Pose {
    let pose = state_pose(input);
    let Some(blend) = input.blend else {
        return pose;
    };
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(blend.t < 1.0) {
        return pose;
    }
    let from = &blend.from;
    let (from_state, _) = look_of(from.state.as_ref(), from.mood, from.motion);
    let (to_state, _) = look_of(input.state, input.mood, input.motion);
    let state_age = if from_state.id() == to_state.id() {
        input.state_age
    } else {
        from.age
    };
    let before = compute_pose(&PoseInput {
        state: from.state.as_ref(),
        mood: from.mood,
        motion: from.motion,
        state_age,
        blend: from.blend.as_deref(),
        ..input.clone()
    });
    blend_poses(
        &before,
        &pose,
        if blend.t.is_finite() { blend.t } else { 1.0 },
    )
}

/// Done celebrates in cycles (`DONE_TURN_S`, `DONE_CYCLE_S`).
pub const DONE_TURN_S: f64 = 3.87;
pub const DONE_CYCLE_S: f64 = 6.2;
const DONE_SWEEPS: f64 = 9.0;
const DONE_DRAW_IN_S: f64 = 0.5;

const WORK_FIRST_S: f64 = 1.2;
const WORK_FIRST_JITTER_S: f64 = 1.2;
const WORK_GAP_S: f64 = 7.5;
const WORK_JITTER_S: f64 = 1.5;
const WORK_TURN_S: f64 = 1.5;
const WORK_EASE: f64 = 6.2;

/// How far into the current working turn, or None between turns (`workTurn`).
fn work_turn(seed: u32, age: f64) -> Option<f64> {
    let first = WORK_FIRST_S + WORK_FIRST_JITTER_S * noise(seed, 41.0);
    if age < first {
        return None;
    }
    let n = ((age - first) / WORK_GAP_S).floor();
    for k in [n, n - 1.0] {
        if k < 0.0 {
            continue;
        }
        let start = first
            + k * WORK_GAP_S
            + if k != 0.0 {
                WORK_JITTER_S * noise(seed, 60.0 + k)
            } else {
                0.0
            };
        if age >= start && age < start + WORK_TURN_S {
            return Some(age - start);
        }
    }
    None
}

/// An angle as the nearest way round to it, -π..π (`near`).
fn near(angle: f64) -> f64 {
    angle - TAU * jsmath::round(angle / TAU)
}

const CELEBRATION_END_S: f64 = 3.79;

/// How far a celebration's ribbons have gone round `t` seconds in (`celebration`).
fn celebration(t: f64) -> f64 {
    let v = (DONE_SWEEPS * TAU + 0.5) / 2.4625;
    if t <= 0.0 {
        return 0.0;
    }
    if t < 0.24 {
        return -0.25 * (1.0 - cos((PI * t) / 0.24));
    }
    if t < 0.54 {
        let d = t - 0.24;
        return -0.5 + (v * (d * d)) / 0.6;
    }
    if t < 2.54 {
        return -0.5 + v * (t - 0.39);
    }
    if t < CELEBRATION_END_S {
        return -0.5 + v * 2.15 + (1.25 * v * (1.0 - (1.0 - (t - 2.54) / 1.25).powf(4.0))) / 4.0;
    }
    DONE_SWEEPS * TAU
}

/// What a state works its pose out from (`StateContext`).
#[derive(Clone, Debug)]
pub struct StateContext {
    pub seed: u32,
    /// The animation clock as given, seconds.
    pub time: f64,
    /// The clock offset per avatar; 0 under reduced motion.
    pub t: f64,
    /// Seconds since the state began, where the host gave it.
    pub state_age: Option<f64>,
    pub mood: AvatarMood,
    pub reduced_motion: bool,
    /// 1, or 0 under reduced motion.
    pub live: f64,
    /// How much of its idle turning the body keeps.
    pub sway: f64,
    /// The pose a state starts from.
    pub base: Pose,
}

impl StateContext {
    /// A sine of this period (seconds) on the clock; 0 under reduced motion (`wave`).
    pub fn wave(&self, period: f64, phase: f64) -> f64 {
        if self.reduced_motion {
            0.0
        } else {
            sin((TAU * self.t) / period + phase)
        }
    }

    /// The calm pose of a bot with nothing to do (`rest`); `pace` stretches its periods.
    pub fn rest(&self, pace: f64) -> Pose {
        let mut pose = self.base.clone();
        pose.yaw += 0.07 * self.sway * self.wave(6.1 * pace, 0.0);
        pose.pitch += 0.04 * self.sway * self.wave(4.7 * pace, 1.0);
        pose.roll = 1.6 * self.wave(5.3 * pace, 2.0);
        pose.lift = if self.reduced_motion {
            0.0
        } else {
            -0.45 * (1.0 - cos((TAU * self.t) / (3.4 * pace)))
        };
        let breath = self.wave(3.4 * pace, -PI / 2.0);
        pose.sy = 1.0 + 0.012 * breath;
        pose.sx = 1.0 - 0.008 * breath;
        if !self.reduced_motion {
            pose.blink = blink_at(self.seed, self.t);
            [pose.gaze_x, pose.gaze_y] = wander_at(self.seed, self.t);
        }
        pose
    }
}

/// The six states' poses (`STATE_DEFINITIONS`).
fn named_pose(state: AvatarState, ctx: &StateContext) -> PosePatch {
    let (seed, t, reduced, live, sway) = (ctx.seed, ctx.t, ctx.reduced_motion, ctx.live, ctx.sway);
    let w = |period: f64, phase: f64| ctx.wave(period, phase);
    match state {
        AvatarState::Idle => {
            if ctx.mood != AvatarMood::Happy {
                return ctx.rest(1.0).into();
            }
            let mut pose = ctx.base.clone();
            let period = 0.92;
            let p = if reduced {
                0.5
            } else {
                ((t % period) + period) % period / period
            };
            let air = 4.0 * p * (1.0 - p);
            let land = if reduced {
                0.0
            } else {
                clamp(1.0 - jsmath::min(p, 1.0 - p) / 0.12, 0.0, 1.0)
            };
            pose.lift = if reduced { 0.0 } else { -3.2 * air };
            pose.sy = 1.0 - 0.09 * land + 0.03 * air * live;
            pose.sx = 1.0 + 0.07 * land - 0.015 * air * live;
            pose.roll = 4.5 * w(period * 2.0, 0.0);
            pose.yaw += 0.1 * w(period * 2.0, 0.6);
            pose.into()
        }
        AvatarState::Thinking => {
            let mut pose = ctx.base.clone();
            pose.yaw += 0.12 + 0.05 * sway * w(7.3, 0.0);
            pose.pitch += 0.08;
            pose.roll = -5.0 + 1.4 * w(4.1, 0.0);
            pose.lift = -0.5 * (1.0 - cos((TAU * t) / 5.2)) * live;
            pose.gaze_x = 0.55 + 0.3 * w(5.6, 0.0);
            pose.gaze_y = -0.8;
            pose.squint_l = 0.22;
            pose.squint_r = 0.32 + 0.1 * w(3.1, 0.0);
            if !reduced {
                pose.blink = blink_at(seed, t * 0.7);
            }
            pose.marks = vec![PoseMark {
                mark: dots(),
                weight: 1.0,
            }];
            pose.into()
        }
        AvatarState::Working => {
            let mut pose = ctx.base.clone();
            let beat = w(0.7, 0.0);
            pose.yaw += 0.08 * sway * w(2.3, 0.4);
            pose.pitch -= 0.05;
            pose.roll = 3.0 + 0.6 * beat;
            pose.lift = -0.3 * (0.5 + 0.5 * beat) * live;
            pose.sy = 0.99 - 0.008 * beat;
            pose.sx = 1.005 + 0.005 * beat;
            pose.squint_l = 0.25;
            pose.squint_r = 0.25;
            if reduced {
                pose.gaze_x = 0.15;
                pose.gaze_y = 0.3;
            } else {
                [pose.gaze_x, pose.gaze_y] = dart_at(seed, t);
                pose.blink = blink_at(seed, t * 1.3);
            }
            let age = match ctx.state_age {
                Some(a) if a.is_finite() => jsmath::max(0.0, a),
                _ => jsmath::max(0.0, ctx.time),
            };
            let into = if reduced { None } else { work_turn(seed, age) };
            if let Some(into) = into {
                let e = WORK_EASE * into;
                let turn = TAU * (1.0 - (1.0 + e) * exp(-e));
                let swell = sin(turn / 2.0);
                pose.ribbons =
                    clamp(into / 0.16, 0.0, 1.0) * clamp((WORK_TURN_S - into) / 0.5, 0.0, 1.0);
                pose.sweep = turn;
                pose.yaw += 0.22 * swell;
                pose.roll += 3.0 * swell;
                pose.lift -= 1.2 * swell;
                pose.gaze_x = clamp(pose.gaze_x * (1.0 - swell) - 0.85 * sin(turn), -1.0, 1.0);
                pose.gaze_y = pose.gaze_y * (1.0 - swell) - 0.3 * swell;
            }
            pose.into()
        }
        AvatarState::Waiting => {
            let mut pose = ctx.base.clone();
            let p = if reduced {
                0.0
            } else {
                ((t % 6.5) + 6.5) % 6.5 / 6.5
            };
            let sigh = sin(TAU * clamp(p / 0.33, 0.0, 1.0));
            pose.yaw += 0.04 * sway * w(7.4, 0.0);
            pose.pitch += 0.02 + 0.03 * sigh;
            pose.roll = 7.0 + 1.2 * w(6.4, 2.0);
            pose.lift = -0.3 * (1.0 - cos((TAU * t) / 6.5)) * live;
            pose.sy = 1.0 + 0.025 * sigh;
            pose.sx = 1.0 - 0.012 * sigh;
            pose.gaze_x = 0.5 + 0.15 * w(8.2, 0.0);
            pose.gaze_y = 0.1;
            pose.eyes = EyeLook {
                arc: ctx.base.eyes.arc,
                slit: 1.0 - ctx.base.eyes.arc,
            };
            if !reduced {
                pose.blink = blink_at(seed, t * 0.6);
            }
            pose.into()
        }
        AvatarState::Blocked => PosePatch {
            sy: Some(1.02),
            sx: Some(0.99),
            marks: Some(vec![PoseMark {
                mark: bang(),
                weight: 1.0,
            }]),
            ..PosePatch::default()
        },
        AvatarState::Done => {
            let mut pose = ctx.rest(1.25);
            let age = match ctx.state_age {
                Some(a) if !reduced && a.is_finite() => jsmath::max(0.0, a),
                _ => DONE_TURN_S,
            };
            let length = DONE_CYCLE_S * (1.0 + 0.15 * noise(seed, 31.0));
            let cycle = (age / length).floor();
            let since = age - cycle * length;
            if since < DONE_TURN_S {
                let swell = sin((PI * since) / DONE_TURN_S);
                let way = if cycle % 2.0 != 0.0 { -1.0 } else { 1.0 };
                let sweep = celebration(since) * way;
                pose.yaw += near(sweep / 3.0);
                pose.lift -= 2.2 * swell;
                pose.sy += 0.04 * swell;
                pose.ribbons = clamp(since / 0.1, 0.0, 1.0)
                    * clamp((DONE_TURN_S - since) / DONE_DRAW_IN_S, 0.0, 1.0);
                pose.cage = 1.0;
                pose.sweep = way * jsmath::max(0.0, celebration(since));
            }
            pose.into()
        }
    }
}

/// Solar's `working-compact` state (DIRECTION.md): the pose of `working` with `ribbons: 0`, for
/// avatars too small to show ribbons. As upstream would write it, a host's own state:
/// `{ id: 'working-compact', pose: (ctx) => ({ ...working.pose(ctx), ribbons: 0 }) }`.
pub struct WorkingCompact;

impl StateDefinition for WorkingCompact {
    fn id(&self) -> &str {
        "working-compact"
    }

    fn pose(&self, ctx: &StateContext) -> PosePatch {
        PosePatch {
            ribbons: Some(0.0),
            ..named_pose(AvatarState::Working, ctx)
        }
    }
}

/// The `working-compact` state (see [`WorkingCompact`]).
pub fn working_compact() -> State {
    static COMPACT: std::sync::OnceLock<Arc<dyn StateDefinition>> = std::sync::OnceLock::new();
    State::Custom(COMPACT.get_or_init(|| Arc::new(WorkingCompact)).clone())
}

/// What a state's pose returned, laid over the base pose (`laid`).
fn laid(base: &Pose, given: PosePatch) -> Pose {
    let number = |v: Option<f64>, or: f64| match v {
        Some(v) if v.is_finite() => v,
        _ => or,
    };
    Pose {
        yaw: number(given.yaw, base.yaw),
        pitch: number(given.pitch, base.pitch),
        roll: number(given.roll, base.roll),
        lift: number(given.lift, base.lift),
        sx: number(given.sx, base.sx),
        sy: number(given.sy, base.sy),
        gaze_x: number(given.gaze_x, base.gaze_x),
        gaze_y: number(given.gaze_y, base.gaze_y),
        blink: number(given.blink, base.blink),
        squint_l: number(given.squint_l, base.squint_l),
        squint_r: number(given.squint_r, base.squint_r),
        eyes: EyeLook {
            arc: number(given.eye_arc, base.eyes.arc),
            slit: number(given.eye_slit, base.eyes.slit),
        },
        ribbons: number(given.ribbons, base.ribbons),
        cage: number(given.cage, base.cage),
        sweep: number(given.sweep, base.sweep),
        marks: match given.marks {
            Some(m) => pose_marks(&m),
            None => base.marks.clone(),
        },
    }
}

/// `statePose`.
fn state_pose(input: &PoseInput) -> Pose {
    let (seed, time, reduced) = (input.seed, input.time, input.reduced_motion);
    let (state, mood) = look_of(input.state, input.mood, input.motion);
    let t = if reduced {
        0.0
    } else {
        time + noise(seed, 7.0) * 10.0
    };
    let given = input
        .gaze
        .filter(|g| g.x.is_finite() && g.y.is_finite())
        .map(|g| [clamp(g.x, -1.0, 1.0), clamp(g.y, -1.0, 1.0)]);
    let weight = match (given, input.gaze) {
        (Some(_), Some(g)) => match g.weight {
            Some(w) if w.is_finite() => clamp(w, 0.0, 1.0),
            _ => 1.0,
        },
        _ => 0.0,
    };
    let base = Pose {
        yaw: BASE_YAW,
        pitch: BASE_PITCH,
        roll: 0.0,
        lift: 0.0,
        sx: 1.0,
        sy: 1.0,
        gaze_x: 0.0,
        gaze_y: 0.0,
        blink: 0.0,
        squint_l: 0.0,
        squint_r: 0.0,
        eyes: EyeLook {
            arc: if mood == AvatarMood::Happy { 1.0 } else { 0.0 },
            slit: 0.0,
        },
        ribbons: 0.0,
        cage: 0.0,
        sweep: 0.0,
        marks: Vec::new(),
    };
    let ctx = StateContext {
        seed,
        time,
        t,
        state_age: input.state_age,
        mood,
        reduced_motion: reduced,
        live: if reduced { 0.0 } else { 1.0 },
        sway: 1.0 - 0.7 * weight,
        base: base.clone(),
    };
    let patch = match &state {
        State::Named(s) => named_pose(*s, &ctx),
        State::Custom(d) => d.pose(&ctx),
    };
    let mut pose = laid(&base, patch);

    if let Some(given) = given {
        if weight > 0.0 {
            let share = if reduced { 1.0 } else { LOOK_EYES };
            let follow = weight * (1.0 - pose.eyes.arc);
            pose.gaze_x = mix(pose.gaze_x, given[0] * share, follow);
            pose.gaze_y = mix(pose.gaze_y, given[1] * share, follow);
            if !reduced {
                pose.yaw += weight * (given[0] * LOOK_YAW - BASE_YAW);
                pose.pitch += weight * (-given[1] * LOOK_PITCH - BASE_PITCH / 2.0);
            }
        }
    }
    if let Some(view) = input.view {
        if view.yaw.is_finite() && view.pitch.is_finite() {
            pose.yaw += view.yaw;
            pose.pitch += view.pitch;
        }
    }
    pose
}
