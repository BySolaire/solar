//! The per-avatar state a live renderer keeps between frames (ports of `mood.ts`, `follow.ts`
//! and `animator.ts`): the eased look toward a target and the eased change between states and
//! moods. Feed [`Animator::step`] once per frame; it returns the `gaze`, `motion_blend` and
//! `state_age` render options for that frame.

use crate::jsmath::{self, exp, hypot, smooth};
use crate::motion::{look_of, AvatarMood, AvatarMotion, BlendFrom, Gaze, MotionBlend, State};

// ---------------------------------------------------------------------------
// mood.ts

/// Length of a mood change, ms (`MOOD_MS`).
pub const MOOD_MS: f64 = 420.0;

/// Eased progress of a mood change from elapsed time (`moodProgress`).
pub fn mood_progress(elapsed_ms: f64) -> f64 {
    smooth(elapsed_ms / MOOD_MS)
}

/// What a new change eases in from: the look on screen with any change still under way frozen
/// at its progress; older changes that have all but faded (or beyond a few levels) are dropped
/// (`settleMood`).
pub fn settle_mood(state: &BlendFrom, share: f64, depth: u32) -> BlendFrom {
    let Some(b) = state.blend.as_ref() else {
        return state.clone();
    };
    let settled = BlendFrom {
        blend: None,
        ..state.clone()
    };
    let rest = share * (1.0 - b.t);
    if rest < 0.02 || depth >= 3 {
        return settled;
    }
    BlendFrom {
        blend: Some(Box::new(MotionBlend {
            from: settle_mood(&b.from, rest, depth + 1),
            t: b.t,
        })),
        ..settled
    }
}

// ---------------------------------------------------------------------------
// follow.ts

/// The eased look (`FollowState`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FollowState {
    pub x: f64,
    pub y: f64,
    /// 0 = at rest, 1 = fully following.
    pub attention: f64,
}

/// `REST`.
pub const REST: FollowState = FollowState {
    x: 0.0,
    y: 0.0,
    attention: 0.0,
};

const FOLLOW_TAU: f64 = 0.12;
const ATTEND_TAU: f64 = 0.3;
const REACH: f64 = 1.0;
const MIN_SIZE: f64 = 160.0;

/// A box in CSS pixels (`{ left, top, width, height }`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

/// Look direction toward a viewport point, from the avatar's box (`pointerGaze`). `origin` is
/// where the look starts as a fraction of the box (default its centre).
pub fn pointer_gaze(pointer: (f64, f64), rect: Rect, origin: Option<(f64, f64)>) -> (f64, f64) {
    let (ox, oy) = origin.unwrap_or((0.5, 0.5));
    let reach = jsmath::max(jsmath::max(rect.width, rect.height), MIN_SIZE) * REACH;
    let dx = (pointer.0 - (rect.left + rect.width * ox)) / reach;
    let dy = (pointer.1 - (rect.top + rect.height * oy)) / reach;
    let m = hypot(dx, dy);
    if !m.is_finite() || m < 1e-9 {
        return (0.0, 0.0);
    }
    let k = jsmath::tanh(m) / m;
    (dx * k, dy * k)
}

/// One easing step toward `target` (None = back to rest) (`followStep`). Returns the next state
/// and whether it still needs frames.
pub fn follow_step(
    state: FollowState,
    target: Option<(f64, f64)>,
    dt: f64,
    reduced: bool,
) -> (FollowState, bool) {
    let goal = match target {
        Some((x, y)) => FollowState {
            x,
            y,
            attention: 1.0,
        },
        None => REST,
    };
    let step = jsmath::max(0.0, jsmath::min(dt, 0.1));
    let ease = |tau: f64| if reduced { 1.0 } else { 1.0 - exp(-step / tau) };
    let k = ease(if target.is_some() {
        FOLLOW_TAU
    } else {
        ATTEND_TAU
    });
    let next = FollowState {
        x: state.x + (goal.x - state.x) * k,
        y: state.y + (goal.y - state.y) * k,
        attention: state.attention + (goal.attention - state.attention) * ease(ATTEND_TAU),
    };
    let moving = (goal.x - next.x).abs() > 0.002
        || (goal.y - next.y).abs() > 0.002
        || (goal.attention - next.attention).abs() > 0.002;
    (if moving { next } else { goal }, moving)
}

/// The pose's look target for a follow state, nothing once at rest (`followGaze`).
pub fn follow_gaze(state: &FollowState) -> Option<Gaze> {
    let a = jsmath::min(1.0, jsmath::max(0.0, state.attention));
    if a > 0.0 {
        Some(Gaze {
            x: state.x,
            y: state.y,
            weight: Some(a * a * (3.0 - 2.0 * a)),
        })
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// animator.ts

/// One frame's input (`AnimatorInput`).
#[derive(Clone, Debug, Default)]
pub struct AnimatorInput {
    /// Wall clock, ms.
    pub now: f64,
    pub state: Option<State>,
    pub mood: Option<AvatarMood>,
    pub motion: Option<AvatarMotion>,
    /// Look target, -1..1 per axis; None eases back to rest.
    pub target: Option<(f64, f64)>,
    /// Keep the current look as it is.
    pub hold: bool,
    /// Snap instead of easing.
    pub reduced_motion: bool,
}

/// One frame's output (`AnimatorStep`): the render options it sets.
#[derive(Clone, Debug)]
pub struct AnimatorStep {
    pub gaze: Option<Gaze>,
    pub motion_blend: Option<MotionBlend>,
    /// Seconds since the state began.
    pub state_age: f64,
    /// True while the look or a change of state or mood is still easing.
    pub moving: bool,
}

impl AnimatorStep {
    /// Sets the render options this step decides (`gaze`, `motion_blend`, `state_age`), as
    /// mount.ts passes them to `liveTransform`.
    pub fn apply(&self, options: &mut crate::render::RenderOptions) {
        options.gaze = self.gaze;
        options.motion_blend = self.motion_blend.clone();
        options.state_age = Some(self.state_age);
    }
}

#[derive(Clone, Debug)]
struct Shown {
    state: State,
    mood: AvatarMood,
    blend: Option<MotionBlend>,
    since: f64,
    began: f64,
}

/// The per-frame host logic (`createAnimator`).
#[derive(Clone, Debug)]
pub struct Animator {
    follow: FollowState,
    last: Option<f64>,
    shown: Option<Shown>,
}

impl Default for Animator {
    fn default() -> Self {
        Animator::new()
    }
}

impl Animator {
    /// `createAnimator()`.
    pub fn new() -> Animator {
        Animator {
            follow: REST,
            last: None,
            shown: None,
        }
    }

    /// Back to rest with no history (`reset`).
    pub fn reset(&mut self) {
        *self = Animator::new();
    }

    /// One frame (`step`).
    pub fn step(&mut self, input: &AnimatorInput) -> AnimatorStep {
        let now = input.now;
        let dt = match self.last {
            None => 0.0,
            Some(last) => (now - last) / 1000.0,
        };
        self.last = Some(now);
        let mut moving = false;
        if !input.hold {
            let (next, m) = follow_step(self.follow, input.target, dt, input.reduced_motion);
            self.follow = next;
            moving = m;
        }

        let (state, mood) = look_of(input.state.as_ref(), input.mood, input.motion);
        let same = self
            .shown
            .as_ref()
            .is_some_and(|s| state.id() == s.state.id());
        if same {
            if let Some(shown) = self.shown.as_mut() {
                shown.state = state.clone();
            }
        }
        if let Some(shown) = self.shown.as_ref() {
            if !same || mood != shown.mood {
                let before = BlendFrom {
                    state: Some(shown.state.clone()),
                    mood: Some(shown.mood),
                    motion: None,
                    age: Some((now - shown.began) / 1000.0),
                    blend: shown.blend.as_ref().map(|b| {
                        Box::new(MotionBlend {
                            from: b.from.clone(),
                            t: mood_progress(now - shown.since),
                        })
                    }),
                };
                let began = if same { shown.began } else { now };
                self.shown = Some(Shown {
                    state: state.clone(),
                    mood,
                    blend: if input.reduced_motion {
                        None
                    } else {
                        Some(MotionBlend {
                            from: settle_mood(&before, 1.0, 0),
                            t: 0.0,
                        })
                    },
                    since: now,
                    began,
                });
            }
        }
        let shown = self.shown.get_or_insert_with(|| {
            let start = state
                .start_age()
                .filter(|a| a.is_finite())
                .map_or(0.0, |a| jsmath::max(0.0, a));
            Shown {
                state: state.clone(),
                mood,
                blend: None,
                since: now,
                began: now - start * 1000.0,
            }
        });
        let t = if input.reduced_motion {
            1.0
        } else {
            mood_progress(now - shown.since)
        };
        let mut motion_blend = None;
        match shown.blend.as_ref() {
            Some(b) if t < 1.0 => {
                motion_blend = Some(MotionBlend {
                    from: b.from.clone(),
                    t,
                })
            }
            _ => shown.blend = None,
        }
        AnimatorStep {
            gaze: follow_gaze(&self.follow),
            state_age: (now - shown.began) / 1000.0,
            moving: moving || motion_blend.is_some(),
            motion_blend,
        }
    }
}
