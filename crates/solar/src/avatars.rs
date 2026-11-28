//! The animated bot avatars: the open-bot-avatar engine (`bot-avatar`) drawn
//! as a GPUI element, following the art direction in
//! `04-maquettes/explorations/open-bot-avatar/direction-animation/DIRECTION.md`.
//!
//! - **Look.** Flat material, eyes always `#151515`, Solar ribbon tuning,
//!   mood neutral. A Mono's `idle`, `working` and `needs-you` show `idle`,
//!   `working` (`working-compact`, without ribbons, under 28 pt) and `waiting`.
//! - **Motion** ([`Motion`]): fully animated on the shared clock at 30 fps,
//!   a rest pose that blinks every few seconds, a still rest pose, or still
//!   until hovered. Resting is a state of its own (`idle-rest`, `idle-blink`,
//!   `waiting-rest`, …: the state's reduced-motion pose), so the engine's pose
//!   blending eases every change, a hover or a status, with no jump.
//!   Reduced motion draws the engine's rest pose and asks for no frames.
//! - **Pixels.** Each frame is painted at the real size × the window's scale
//!   factor with the root motion (roll, hop, squash) baked in, since GPUI
//!   sprites cannot rotate, and handed to GPUI as straight-alpha BGRA, the
//!   format its own image decoders produce.
//!
//! # Frames and GPU memory
//!
//! Every avatar keeps, in its element state, its animator, its canvas
//! surface and the image it shows (plus the one before, see [`Instance`]). A
//! frame equal to the one shown reuses its image; an equal frame another
//! avatar already painted is shared through a map of weak entries, so twin
//! avatars rasterize once. When the last avatar holding an image lets go of
//! it, the image goes to a reaper task that removes it from every window's
//! sprite atlas after the current draw: the frame on screen never loses its
//! sprite, and nothing an avatar stopped showing stays on the GPU.
//!
//! # Clock
//!
//! A still avatar asks for nothing. A blinking one sleeps until its next
//! blink ([`pulse::wake_at`]) and leases the clock for the blink alone,
//! about 0.2 s every 4.2 s on average. A live avatar, or any avatar easing
//! from one pose to another, leases the clock at [`rate`]. Measured (release,
//! Linux, `bot-avatar/tests/perf.rs`): a frame's geometry takes about 6 µs and
//! painting about 0.18 ms at 16 pt, 0.25 ms at 28 pt and 0.42 ms at 56 pt
//! on a 2× screen, so a live 56 pt avatar costs about 1.3% of one core at
//! 30 fps before GPUI redraws the window, and a blinking sidebar avatar
//! about 7 frames per blink (see the tests).
use crate::pulse::{self, Rate};
use bot_avatar::motion::{blink_at, noise};
use bot_avatar::{
    Animator, AnimatorInput, AvatarFrame, AvatarMood, AvatarState, CanvasSurface, EyeMark,
    FrameShape, PoseInput, PosePatch, PreparedAvatar, Rect, RenderOptions, State, StateContext,
    StateDefinition, compute_pose, pointer_gaze, solar_avatar, working_compact,
};
use gpui::{
    AnyElement, App, Bounds, Corners, DispatchPhase, Element, ElementId, Global, GlobalElementId,
    Hitbox, HitboxBehavior, Hsla, InspectorElementId, IntoElement, LayoutId, MouseMoveEvent,
    Pixels, RenderImage, Rgba, Role, SharedString, Style, Task, Window, div, point, prelude::*, px,
    relative, rgb, size,
};
use solar_core::project_appearance::MASCOTS;
use std::{
    cell::Cell,
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    rc::{Rc, Weak},
    sync::{Arc, OnceLock},
    time::Duration,
};

/// The eye ink, whatever the body (DIRECTION.md §1: never `autoEyeColor`).
pub(crate) const EYE_INK: &str = "#151515";
/// Below this side, in points, `working` has no ribbons (`working-compact`).
pub(crate) const RIBBONS_FROM: f32 = 28.;
/// From this side, in points, the eyes follow the pointer over the avatar:
/// a Mono's card, the conversation header and the panels (48–56 pt), and the
/// 44 pt head of a Mono's dashboard card.
pub(crate) const FOLLOW_FROM: f32 = 44.;
/// The engine's default volume.
const DEPTH: Option<f64> = None;

/// What the avatar shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub(crate) enum Look {
    Idle,
    Working,
    Waiting,
}
impl Look {
    /// A Mono's status: `idle`, `working` or `needs-you`.
    pub(crate) fn of_status(status: &str) -> Look {
        match status {
            "working" => Look::Working,
            "needs-you" => Look::Waiting,
            _ => Look::Idle,
        }
    }
}

/// How much an avatar moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub(crate) enum Motion {
    /// The state's rest pose. Asks for no frames.
    Still,
    /// The rest pose with the engine's blinks, every 4.2 s on average.
    /// Sleeps between blinks.
    Blink,
    /// Fully animated on the shared clock.
    Live,
    /// Still, and live while the pointer is over it.
    LiveOnHover,
    /// Blinks, and live while the pointer is over it.
    BlinkLiveOnHover,
}

/// What a Mono avatar does at a size (DIRECTION.md §3). Under 28 pt an idle
/// Mono only blinks, a working one moves without ribbons and one that needs
/// you holds still; from 28 pt a working or waiting Mono moves, and an idle
/// one blinks and comes alive under the pointer. Idle Monos sit on screen for
/// hours: animating them all the time kept the window repainting at 30 fps
/// (about 30% of a core on the Mac, measured 2026-11-28), so they rest.
/// `calm` keeps an idle Mono blinking at any size, for illustrations that
/// should not draw the eye.
pub(crate) fn mono_motion(look: Look, side: f32, calm: bool) -> Motion {
    let small = side < RIBBONS_FROM;
    match look {
        Look::Working => Motion::Live,
        Look::Waiting if small => Motion::Still,
        Look::Waiting => Motion::Live,
        Look::Idle if small || calm => Motion::Blink,
        Look::Idle => Motion::BlinkLiveOnHover,
    }
}

/// How much of its saturation a colour keeps on an avatar. The pixel mascots
/// kept 0.55, which turned blues into the blue-grey Solar never uses (client
/// decision 2026-11-28: soften without greying).
pub(crate) const SOFTEN_SATURATION: f32 = 0.9;

/// A Mono's or a project's colour as the avatars wear it: lighter, hue and
/// most of its saturation kept: saturation × 0.9, lightness 55% of the way to
/// 0.82.
pub(crate) fn soften(color: u32) -> Hsla {
    let mut c: Hsla = rgb(color).into();
    c.s *= SOFTEN_SATURATION;
    c.l += (0.82 - c.l) * 0.55;
    c
}

/// The index in `MASCOTS` of a mascot name; unknown names show the ghost, as
/// the pixel mascots did.
pub(crate) fn mascot_index(name: &str) -> usize {
    MASCOTS.iter().position(|m| *m == name).unwrap_or(1)
}

/// One avatar. Build it with [`Avatar::new`] where it is drawn: the call site,
/// mascot and size (within the enclosing element ids) name the avatar across
/// frames, so its animation carries over, and eases, while its look, motion
/// and colour change.
pub(crate) struct Avatar {
    key: u64,
    spec: Spec,
    label: Option<SharedString>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Spec {
    mascot: usize,
    /// The body colour; its alpha fades the whole avatar.
    fill: Hsla,
    look: Look,
    side: f32,
    motion: Motion,
    follow: bool,
    /// A look the eyes hold while live, -1..1 per axis.
    gaze: Option<(f32, f32)>,
    /// Lays out over its whole parent, the avatar centred in it.
    fill_parent: bool,
}

impl Avatar {
    #[track_caller]
    pub(crate) fn new(mascot: &str, fill: Hsla, side: f32) -> Self {
        let mascot = mascot_index(mascot);
        let mut hasher = DefaultHasher::new();
        std::panic::Location::caller().hash(&mut hasher);
        mascot.hash(&mut hasher);
        side.to_bits().hash(&mut hasher);
        Self {
            key: hasher.finish(),
            spec: Spec {
                mascot,
                fill,
                look: Look::Idle,
                side,
                motion: Motion::Still,
                follow: false,
                gaze: None,
                fill_parent: false,
            },
            label: None,
        }
    }
    pub(crate) fn look(mut self, look: Look) -> Self {
        self.spec.look = look;
        self
    }
    pub(crate) fn motion(mut self, motion: Motion) -> Self {
        self.spec.motion = motion;
        self
    }
    /// The eyes follow the pointer while it is over the avatar (live only).
    pub(crate) fn follow_pointer(mut self, follow: bool) -> Self {
        self.spec.follow = follow;
        self
    }
    /// The eyes look this way while live (-1..1 per axis), as the
    /// composer's runner looks where it runs. A followed pointer wins.
    pub(crate) fn gaze(mut self, gaze: Option<(f32, f32)>) -> Self {
        self.spec.gaze = gaze;
        self
    }
    /// Takes its parent's whole box and centres the avatar in it, so a
    /// pointer anywhere over the parent hovers it.
    pub(crate) fn fill_parent(mut self) -> Self {
        self.spec.fill_parent = true;
        self
    }
    /// Names the avatar for assistive technology, where no name shows beside
    /// it. Unlabeled avatars are decorative.
    #[allow(dead_code)]
    pub(crate) fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl IntoElement for Avatar {
    type Element = AnyElement;
    fn into_element(self) -> AnyElement {
        let element = AvatarElement {
            id: ElementId::NamedInteger("bot-avatar".into(), self.key),
            spec: self.spec,
        };
        match self.label {
            Some(label) => div()
                .id(ElementId::NamedInteger("bot-avatar-label".into(), self.key))
                .role(Role::Image)
                .aria_label(label)
                .flex_shrink_0()
                .child(element)
                .into_any_element(),
            None => element.into_any_element(),
        }
    }
}

// ---------------------------------------------------------------------------
// States

/// What an avatar's motion amounts to on this frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Rest,
    Blink,
    Live,
}
fn mode(motion: Motion, hovered: bool) -> Mode {
    match motion {
        Motion::Still => Mode::Rest,
        Motion::Blink => Mode::Blink,
        Motion::Live => Mode::Live,
        Motion::LiveOnHover if hovered => Mode::Live,
        Motion::LiveOnHover => Mode::Rest,
        Motion::BlinkLiveOnHover if hovered => Mode::Live,
        Motion::BlinkLiveOnHover => Mode::Blink,
    }
}

/// A state's reduced-motion pose as a state of its own, blinking or not, so
/// the animator blends into and out of it like any other state.
struct Resting {
    id: &'static str,
    of: State,
    blink: bool,
}
impl StateDefinition for Resting {
    fn id(&self) -> &str {
        self.id
    }
    fn pose(&self, ctx: &StateContext) -> PosePatch {
        let mut pose = compute_pose(&PoseInput {
            seed: ctx.seed,
            state: Some(&self.of),
            mood: Some(ctx.mood),
            reduced_motion: true,
            ..PoseInput::default()
        });
        if self.blink && !ctx.reduced_motion {
            pose.blink = blink_at(ctx.seed, ctx.t);
        }
        pose.into()
    }
}

fn named(look: Look, compact: bool) -> State {
    match look {
        Look::Idle => AvatarState::Idle.into(),
        Look::Working if compact => working_compact(),
        Look::Working => AvatarState::Working.into(),
        Look::Waiting => AvatarState::Waiting.into(),
    }
}

fn resting(look: Look, compact: bool, blink: bool) -> State {
    static STATES: OnceLock<Vec<State>> = OnceLock::new();
    let states = STATES.get_or_init(|| {
        let rest = |id: &'static str, of: State, blink: bool| {
            State::Custom(Arc::new(Resting { id, of, blink }))
        };
        vec![
            rest("idle-rest", AvatarState::Idle.into(), false),
            rest("idle-blink", AvatarState::Idle.into(), true),
            rest("working-rest", AvatarState::Working.into(), false),
            rest("working-compact-rest", working_compact(), false),
            rest("waiting-rest", AvatarState::Waiting.into(), false),
            rest("waiting-blink", AvatarState::Waiting.into(), true),
            rest("working-blink", AvatarState::Working.into(), true),
            rest("working-compact-blink", working_compact(), true),
        ]
    });
    let index = match (look, compact, blink) {
        (Look::Idle, _, false) => 0,
        (Look::Idle, _, true) => 1,
        (Look::Working, false, false) => 2,
        (Look::Working, true, false) => 3,
        (Look::Waiting, _, false) => 4,
        (Look::Waiting, _, true) => 5,
        (Look::Working, false, true) => 6,
        (Look::Working, true, true) => 7,
    };
    states[index].clone()
}

/// The engine state an avatar shows. Under reduced motion it is the plain
/// state, which the engine itself freezes into its rest pose.
fn shown_state(look: Look, side: f32, mode: Mode, reduced: bool) -> State {
    let compact = side < RIBBONS_FROM;
    if reduced {
        return named(look, compact);
    }
    match mode {
        Mode::Live => named(look, compact),
        Mode::Rest => resting(look, compact, false),
        Mode::Blink => resting(look, compact, true),
    }
}

/// The clock time a resting state's blinks run on, `statePose`'s `t`.
fn engine_time(seed: u32, time: f64) -> f64 {
    time + noise(seed, 7.0) * 10.0
}

const BLINK_SLOT: f64 = 4.2;
const BLINK_LENGTH: f64 = 0.19;
/// How early a wake may land and still start its blink, seconds.
const BLINK_SLACK: f64 = 0.001;

/// The blink under way at engine time `t`, or the next one: its start and
/// end, as `blinkAt` draws them (one per 4.2 s slot, sometimes doubled).
fn next_blink(seed: u32, t: f64) -> (f64, f64) {
    let slot = (t / BLINK_SLOT).floor();
    let mut next: Option<(f64, f64)> = None;
    for s in [slot - 1.0, slot, slot + 1.0] {
        let start = s * BLINK_SLOT + 0.5 + noise(seed, s * 2.0) * 2.9;
        let double = noise(seed, s * 2.0 + 1.0) > 0.8;
        for begin in [start, start + 0.34]
            .into_iter()
            .take(1 + usize::from(double))
        {
            let end = begin + BLINK_LENGTH;
            if end > t && next.is_none_or(|(b, _)| begin < b) {
                next = Some((begin, end));
            }
        }
    }
    // The next slot's blink starts after `t` whatever the noise.
    next.expect("a blink in the next slot")
}

// ---------------------------------------------------------------------------
// Prepared avatars

/// One prepared avatar per mascot, shared by every window and thread.
fn prepared(mascot: usize) -> Option<&'static PreparedAvatar> {
    static PREPARED: [OnceLock<Option<PreparedAvatar>>; MASCOTS.len()] =
        [const { OnceLock::new() }; MASCOTS.len()];
    PREPARED
        .get(mascot)?
        .get_or_init(|| {
            let prepared = solar_avatar(MASCOTS[mascot]).map(|def| PreparedAvatar::new(def, DEPTH));
            match prepared {
                Some(Ok(prepared)) => Some(prepared),
                Some(Err(errors)) => {
                    log::error!("avatar {}: {errors:?}", MASCOTS[mascot]);
                    None
                }
                None => {
                    log::error!("avatar {} is missing", MASCOTS[mascot]);
                    None
                }
            }
        })
        .as_ref()
}

/// Builds the ten avatars' volumes in the background (a few ms each), so the
/// first window to show them does not wait.
pub(crate) fn init(cx: &mut App) {
    cx.background_executor()
        .spawn(async {
            for mascot in 0..MASCOTS.len() {
                prepared(mascot);
            }
        })
        .detach();
}

// ---------------------------------------------------------------------------
// Images

#[derive(Default)]
struct Counters {
    /// Images some avatar still holds.
    live: Cell<usize>,
    /// Frames painted since the app started.
    painted: Cell<u64>,
    /// Images given back to the sprite atlases.
    freed: Cell<u64>,
}

/// A painted frame and where it lies.
struct Painted {
    image: Arc<RenderImage>,
    /// Device pixels from the viewBox origin to the bitmap's corner.
    left: f32,
    top: f32,
    width: u32,
    height: u32,
    reaper: async_channel::Sender<Arc<RenderImage>>,
    counters: Rc<Counters>,
}
impl Drop for Painted {
    fn drop(&mut self) {
        self.counters.live.set(self.counters.live.get() - 1);
        // Freed after this draw, once no frame on screen paints it.
        self.reaper.try_send(self.image.clone()).ok();
    }
}

/// The app's painted frames, shared between avatars showing the same one.
struct Images {
    shared: HashMap<u64, Weak<Painted>>,
    reaper: async_channel::Sender<Arc<RenderImage>>,
    counters: Rc<Counters>,
    _reaper: Task<()>,
}
impl Global for Images {}

fn images(cx: &mut App) -> &mut Images {
    if !cx.has_global::<Images>() {
        let (reaper, dropped) = async_channel::unbounded::<Arc<RenderImage>>();
        let counters = Rc::new(Counters::default());
        let task = cx.spawn({
            let counters = counters.clone();
            async move |cx| {
                while let Ok(image) = dropped.recv().await {
                    let mut batch = vec![image];
                    while let Ok(image) = dropped.try_recv() {
                        batch.push(image);
                    }
                    counters
                        .freed
                        .set(counters.freed.get() + batch.len() as u64);
                    cx.update(|cx| {
                        for image in batch {
                            cx.drop_image(image, None);
                        }
                    });
                }
            }
        });
        cx.set_global(Images {
            shared: HashMap::new(),
            reaper,
            counters,
            _reaper: task,
        });
    }
    cx.global_mut::<Images>()
}

/// Everything that decides a frame's pixels.
fn frame_key(spec: &Spec, frame: &AvatarFrame, dpr: f32) -> u64 {
    let mut h = DefaultHasher::new();
    spec.mascot.hash(&mut h);
    spec.side.to_bits().hash(&mut h);
    dpr.to_bits().hash(&mut h);
    spec.fill.a.to_bits().hash(&mut h);
    frame.color.hash(&mut h);
    frame.ink.hash(&mut h);
    let numbers = |values: &[f64], h: &mut DefaultHasher| {
        for value in values {
            value.to_bits().hash(h);
        }
    };
    numbers(&frame.root, &mut h);
    for shape in &frame.shapes {
        match shape {
            FrameShape::Slice { path, matrix, .. } => {
                0u8.hash(&mut h);
                if let Some(path) = frame.paths.get(*path) {
                    path.outline.d.hash(&mut h);
                    path.even_odd.hash(&mut h);
                }
                numbers(matrix, &mut h);
            }
            FrameShape::Eye { key, matrix, mark } => {
                1u8.hash(&mut h);
                key.hash(&mut h);
                numbers(matrix, &mut h);
                match mark {
                    EyeMark::Pill {
                        x,
                        y,
                        width,
                        height,
                        radius,
                    } => numbers(&[*x, *y, *width, *height, *radius], &mut h),
                    EyeMark::Arc { stroke_width, .. } => {
                        numbers(&[*stroke_width], &mut h);
                        mark.d().hash(&mut h);
                    }
                    EyeMark::Path { .. } => mark.d().hash(&mut h),
                }
            }
            FrameShape::Ribbon(ribbon) => {
                2u8.hash(&mut h);
                for p in &ribbon.points {
                    numbers(p, &mut h);
                }
                numbers(&ribbon.matrix, &mut h);
                ribbon.front.hash(&mut h);
                ribbon.fill.hash(&mut h);
                let g = &ribbon.gradient;
                numbers(&[g.x1, g.y1, g.x2, g.y2], &mut h);
                g.stops.hash(&mut h);
            }
        }
    }
    h.finish()
}

/// The pixmap as GPUI wants an image: BGRA with straight alpha, as its
/// decoders hand it over, faded by `opacity`.
fn bgra(pixmap: &tiny_skia::Pixmap, opacity: f32) -> Vec<u8> {
    let opacity = opacity.clamp(0., 1.);
    let mut bytes = Vec::with_capacity(pixmap.data().len());
    for pixel in pixmap.pixels() {
        let c = pixel.demultiply();
        let alpha = (c.alpha() as f32 * opacity).round() as u8;
        bytes.extend_from_slice(&[c.blue(), c.green(), c.red(), alpha]);
    }
    bytes
}

fn hex(color: Hsla) -> String {
    let c = Rgba::from(color);
    let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(c.r), byte(c.g), byte(c.b))
}

// ---------------------------------------------------------------------------
// The element

/// What an avatar keeps between frames.
struct Instance {
    animator: Animator,
    surface: CanvasSurface,
    /// The frame shown and its image.
    key: Option<u64>,
    shown: Option<Rc<Painted>>,
    /// The image shown before, kept for one more paint: two avatars that
    /// share an element id take turns with this state within one frame, and
    /// each must keep its own image alive.
    previous: Option<Rc<Painted>>,
}

struct AvatarElement {
    id: ElementId,
    spec: Spec,
}

impl IntoElement for AvatarElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

/// The rate of a live avatar. 30 fps everywhere for now: DIRECTION.md
/// allows 60 fps from 48 pt if the Mac measurements permit; a faster
/// [`Rate`] returned here for `side >= 48.` is all that would take.
fn rate(_side: f32) -> Rate {
    Rate::Smooth
}

/// One frame's decisions, apart from the element so tests can check them.
struct Frame {
    options: RenderOptions,
    mode: Mode,
    moving: bool,
}

fn frame_options(
    spec: &Spec,
    animator: &mut Animator,
    time: f64,
    hovered: bool,
    pointer: Option<(f64, f64)>,
    reduced: bool,
) -> Frame {
    let mode = mode(spec.motion, hovered);
    let state = shown_state(spec.look, spec.side, mode, reduced);
    let step = animator.step(&AnimatorInput {
        now: time * 1000.,
        state: Some(state.clone()),
        mood: Some(AvatarMood::Neutral),
        target: pointer
            .filter(|_| spec.follow)
            .or(spec.gaze.map(|(x, y)| (x as f64, y as f64)))
            .filter(|_| mode == Mode::Live && !reduced),
        reduced_motion: reduced,
        ..AnimatorInput::default()
    });
    let mut options = RenderOptions {
        color: Some(hex(spec.fill)),
        eye_color: Some(EYE_INK.into()),
        size: Some(spec.side as f64),
        time: Some(time),
        state: Some(state),
        mood: Some(AvatarMood::Neutral),
        reduced_motion: reduced,
        ..RenderOptions::default()
    };
    step.apply(&mut options);
    Frame {
        options,
        mode,
        moving: step.moving,
    }
}

impl Element for AvatarElement {
    type RequestLayoutState = ();
    type PrepaintState = Option<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            size: if self.spec.fill_parent {
                size(relative(1.).into(), relative(1.).into())
            } else {
                size(px(self.spec.side).into(), px(self.spec.side).into())
            },
            flex_shrink: 0.,
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) -> Option<Hitbox> {
        let hover = self.spec.follow
            || matches!(
                self.spec.motion,
                Motion::LiveOnHover | Motion::BlinkLiveOnHover
            );
        hover.then(|| window.insert_hitbox(bounds, HitboxBehavior::Normal))
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        hitbox: &mut Option<Hitbox>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(id) = id else { return };
        let Some(prepared) = prepared(self.spec.mascot) else {
            return;
        };
        let spec = self.spec;
        let side = px(spec.side);
        let bounds = Bounds::new(
            bounds.origin
                + point(
                    (bounds.size.width - side) / 2.,
                    (bounds.size.height - side) / 2.,
                ),
            size(side, side),
        );
        let hovered = hitbox.as_ref().is_some_and(|h| h.is_hovered(window));
        if let Some(hitbox) = hitbox.clone() {
            // Hover starts or ends a live avatar's motion, and its gaze.
            let view = window.current_view();
            window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) != hovered {
                    cx.notify_paint(view);
                }
            });
        }
        let reduced = cx.reduce_motion();
        let time = pulse::elapsed(cx).as_secs_f64();
        let dpr = window.scale_factor();
        let pointer = hovered.then(|| {
            let mouse = window.mouse_position();
            pointer_gaze(
                (mouse.x.as_f32() as f64, mouse.y.as_f32() as f64),
                Rect {
                    left: bounds.origin.x.as_f32() as f64,
                    top: bounds.origin.y.as_f32() as f64,
                    width: bounds.size.width.as_f32() as f64,
                    height: bounds.size.height.as_f32() as f64,
                },
                None,
            )
        });
        let (painted, frame) = window.with_element_state(id, |instance, _| {
            let mut instance = instance.unwrap_or_else(|| Instance {
                animator: Animator::new(),
                surface: CanvasSurface::new(prepared),
                key: None,
                shown: None,
                previous: None,
            });
            let frame = frame_options(
                &spec,
                &mut instance.animator,
                time,
                hovered,
                pointer,
                reduced,
            );
            let avatar_frame = prepared.frame(&frame.options);
            let key = frame_key(&spec, &avatar_frame, dpr);
            if instance.key == Some(key) && instance.shown.is_some() {
                instance.previous = None;
            } else {
                let painted = share(&spec, key, &avatar_frame, &mut instance.surface, dpr, cx);
                instance.previous = instance.shown.take();
                instance.shown = painted;
                instance.key = Some(key);
            }
            ((instance.shown.clone(), frame), instance)
        });
        if let Some(painted) = painted {
            let scale = dpr.max(f32::EPSILON);
            let image_bounds = Bounds::new(
                bounds.origin + point(px(painted.left / scale), px(painted.top / scale)),
                size(
                    px(painted.width as f32 / scale),
                    px(painted.height as f32 / scale),
                ),
            );
            if let Err(error) = window.paint_image(
                image_bounds,
                image_bounds,
                Corners::default(),
                painted.image.clone(),
                0,
                false,
            ) {
                log::warn!("avatar: {error:?}");
            }
        }
        schedule(&spec, &frame, prepared.seed(), time, reduced, window, cx);
    }
}

/// Asks the clock for what the frame needs next.
fn schedule(
    spec: &Spec,
    frame: &Frame,
    seed: u32,
    time: f64,
    reduced: bool,
    window: &mut Window,
    cx: &mut App,
) {
    if reduced {
        return;
    }
    let blinking = frame.mode == Mode::Blink;
    let (start, end) = if blinking {
        let offset = engine_time(seed, 0.);
        let (start, end) = next_blink(seed, engine_time(seed, time));
        (start - offset, end - offset)
    } else {
        (0., 0.)
    };
    // The clock's nanoseconds round `start` down; a wake at it counts as in.
    let in_blink = blinking && start - BLINK_SLACK <= time && time < end;
    if frame.mode == Mode::Live || frame.moving || in_blink {
        pulse::request(rate(spec.side), window, cx);
    } else if blinking {
        pulse::wake_at(Duration::from_secs_f64(start.max(0.)), window, cx);
    }
}

/// The image of a frame: one another avatar painted, or a new one.
fn share(
    spec: &Spec,
    key: u64,
    frame: &AvatarFrame,
    surface: &mut CanvasSurface,
    dpr: f32,
    cx: &mut App,
) -> Option<Rc<Painted>> {
    let images = images(cx);
    if let Some(painted) = images.shared.get(&key).and_then(Weak::upgrade) {
        return Some(painted);
    }
    let pixmap = surface.paint(frame, spec.side as f64, dpr as f64, None, None)?;
    let (width, height) = (pixmap.width(), pixmap.height());
    let bytes = bgra(pixmap, spec.fill.a);
    let buffer = image::RgbaImage::from_raw(width, height, bytes)?;
    let image = Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]));
    let counters = images.counters.clone();
    counters.live.set(counters.live.get() + 1);
    counters.painted.set(counters.painted.get() + 1);
    let painted = Rc::new(Painted {
        image,
        left: surface.left as f32,
        top: surface.top as f32,
        width,
        height,
        reaper: images.reaper.clone(),
        counters,
    });
    if images.shared.len() >= 64 {
        images
            .shared
            .retain(|_, painted| painted.strong_count() > 0);
    }
    images.shared.insert(key, Rc::downgrade(&painted));
    Some(painted)
}

/// Images avatars hold, frames painted and images freed, for tests.
#[cfg(test)]
fn stats(cx: &App) -> (usize, u64, u64) {
    cx.try_global::<Images>().map_or((0, 0, 0), |images| {
        let c = &images.counters;
        (c.live.get(), c.painted.get(), c.freed.get())
    })
}

/// The images some avatar holds, for tests.
#[cfg(test)]
fn held_images(cx: &App) -> Vec<Arc<RenderImage>> {
    cx.try_global::<Images>().map_or(Vec::new(), |images| {
        images
            .shared
            .values()
            .filter_map(Weak::upgrade)
            .map(|painted| painted.image.clone())
            .collect()
    })
}

#[cfg(test)]
mod tests;
