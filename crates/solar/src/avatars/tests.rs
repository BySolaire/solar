use super::*;
use bot_avatar::Pose;
use gpui::{Context, Entity, Modifiers, Render, TestAppContext, VisualTestContext};

const TICK: Duration = Duration::from_millis(33);
const BLUE: u32 = 0x459cf7;

fn spec(look: Look, side: f32, motion: Motion) -> Spec {
    Spec {
        mascot: mascot_index("robot"),
        fill: soften(BLUE),
        look,
        side,
        motion,
        follow: false,
        gaze: None,
        fill_parent: false,
    }
}

fn pose(spec: &Spec, animator: &mut Animator, time: f64, reduced: bool) -> Pose {
    let frame = frame_options(spec, animator, time, false, None, reduced);
    prepared(spec.mascot).unwrap().pose(&frame.options)
}

/// The engine state a Mono avatar shows for a status, side and `calm`.
fn mono_state(status: &str, side: f32, calm: bool, reduced: bool) -> String {
    let look = Look::of_status(status);
    let motion = mono_motion(look, side, calm);
    shown_state(look, side, mode(motion, false), reduced)
        .id()
        .to_string()
}

#[test]
fn mono_states_follow_the_direction_at_every_size() {
    // Sidebar, tabs, rail, pins and transcript signatures (14–16 pt).
    assert_eq!(mono_motion(Look::Idle, 14., true), Motion::Blink);
    assert_eq!(mono_motion(Look::Idle, 16., false), Motion::Blink);
    assert_eq!(mono_motion(Look::Working, 16., true), Motion::Live);
    assert_eq!(mono_motion(Look::Waiting, 16., true), Motion::Still);
    assert_eq!(mono_state("idle", 14., true, false), "idle-blink");
    assert_eq!(mono_state("working", 14., true, false), "working-compact");
    assert_eq!(mono_state("working", 20., false, false), "working-compact");
    assert_eq!(mono_state("needs-you", 16., true, false), "waiting-rest");
    // Dashboard and floating list (28 pt), cards, headers and panels (48–56 pt).
    for side in [28., 44., 48., 56.] {
        assert_eq!(
            mono_motion(Look::Idle, side, false),
            Motion::BlinkLiveOnHover
        );
        assert_eq!(mono_motion(Look::Waiting, side, false), Motion::Live);
        // Idle rests and blinks until hovered, then lives (and follows).
        assert_eq!(mono_state("idle", side, false, false), "idle-blink");
        let hovered = mode(mono_motion(Look::Idle, side, false), true);
        assert_eq!(shown_state(Look::Idle, side, hovered, false).id(), "idle");
        assert_eq!(mono_state("working", side, false, false), "working");
        assert_eq!(mono_state("needs-you", side, false, false), "waiting");
    }
    // A calm line-up only blinks.
    assert_eq!(mono_state("idle", 40., true, false), "idle-blink");
    // Reduced motion: the plain states, which the engine freezes at rest.
    assert_eq!(mono_state("idle", 14., true, true), "idle");
    assert_eq!(mono_state("working", 14., true, true), "working-compact");
    assert_eq!(mono_state("needs-you", 16., true, true), "waiting");
    assert_eq!(mono_state("working", 56., false, true), "working");
    // Unknown statuses are idle; thinking, blocked and done never show.
    assert_eq!(Look::of_status("done"), Look::Idle);
    assert_eq!(Look::of_status("thinking"), Look::Idle);
}

#[test]
fn colours_soften_without_greying_and_eyes_stay_ink() {
    let soft = soften(BLUE);
    let mut base: Hsla = rgb(BLUE).into();
    assert!((soft.s - base.s * SOFTEN_SATURATION).abs() < 1e-6);
    assert!((soft.l - (base.l + (0.82 - base.l) * 0.55)).abs() < 1e-6);
    assert_eq!(soft.h, base.h);
    // The palette blue stays a clear blue, not the old blue-grey #97b9dd.
    assert_eq!(hex(soften(BLUE)), "#81b9f3");
    // A neutral grey stays neutral, lightened toward 0.82.
    assert_eq!(hex(soften(0x808080)), "#adadad");
    base.a = 1.;
    assert_eq!(hex(base), "#459cf7");

    let mut animator = Animator::new();
    for fill in [0xffffff, 0x000000, BLUE] {
        let spec = Spec {
            fill: soften(fill),
            ..spec(Look::Idle, 28., Motion::Live)
        };
        let frame = frame_options(&spec, &mut animator, 0.5, false, None, false);
        assert_eq!(frame.options.eye_color.as_deref(), Some("#151515"));
        assert_eq!(frame.options.mood, Some(AvatarMood::Neutral));
        assert_eq!(frame.options.color, Some(hex(soften(fill))));
        assert_eq!(frame.options.size, Some(28.));
        let avatar = prepared(spec.mascot).unwrap().frame(&frame.options);
        assert_eq!(avatar.ink, "#151515");
    }
}

#[test]
fn pixels_are_straight_alpha_bgra_with_ink_eyes() {
    let spec = Spec {
        fill: rgb(0xffffff).into(),
        ..spec(Look::Idle, 56., Motion::Still)
    };
    let prepared = prepared(spec.mascot).unwrap();
    let frame = frame_options(&spec, &mut Animator::new(), 0., false, None, false);
    let mut surface = CanvasSurface::new(prepared);
    let pixmap = surface
        .paint(&prepared.frame(&frame.options), 56., 2., None, None)
        .unwrap();
    // Painted at the real size × the scale factor, never upscaled.
    let single = CanvasSurface::new(prepared)
        .paint(&prepared.frame(&frame.options), 56., 1., None, None)
        .unwrap()
        .width();
    assert!(
        pixmap.width().abs_diff(single * 2) <= 2,
        "{} at 2×",
        pixmap.width()
    );
    let bytes = bgra(pixmap, 1.);
    let pixels: Vec<&[u8; 4]> = bytes.as_chunks::<4>().0.iter().collect();
    assert!(pixels.contains(&&[0x15, 0x15, 0x15, 0xff]), "ink eyes");
    assert!(pixels.contains(&&[0xff, 0xff, 0xff, 0xff]), "white body");
    // Straight alpha: the white body's antialiased edge stays white.
    let edge = pixels
        .iter()
        .find(|p| p[3] > 16 && p[3] < 240 && p[0] > 0xf0)
        .expect("an antialiased edge");
    assert_eq!(&edge[..3], &[0xff, 0xff, 0xff]);
    // A faded body (a finished sub-agent's `content(0.45)`) fades as a whole.
    let faded = bgra(pixmap, 0.45);
    let top = faded.as_chunks::<4>().0.iter().map(|p| p[3]).max().unwrap();
    assert_eq!(top, (255. * 0.45f32).round() as u8);
}

#[test]
fn every_mascot_prepares_and_paints() {
    for (index, name) in MASCOTS.iter().enumerate() {
        assert_eq!(mascot_index(name), index);
        let prepared = prepared(index).unwrap_or_else(|| panic!("{name} prepares"));
        let spec = Spec {
            mascot: index,
            ..spec(Look::Working, 16., Motion::Live)
        };
        let frame = frame_options(&spec, &mut Animator::new(), 3., false, None, false);
        let pixmap = CanvasSurface::new(prepared)
            .paint(&prepared.frame(&frame.options), 16., 2., None, None)
            .cloned()
            .unwrap();
        assert!(pixmap.pixels().iter().any(|p| p.alpha() == 255), "{name}");
    }
    assert_eq!(mascot_index("unknown"), mascot_index("ghost"));
}

#[test]
fn reduced_motion_draws_the_rest_pose() {
    for (look, side) in [
        (Look::Idle, 14.),
        (Look::Working, 16.),
        (Look::Working, 56.),
        (Look::Waiting, 28.),
    ] {
        let spec = Spec {
            follow: true,
            ..spec(look, side, Motion::Live)
        };
        let mut animator = Animator::new();
        let first = pose(&spec, &mut animator, 0., true);
        for time in [0.4, 1.9, 3.3, 7.7, 12.1] {
            let pose = pose(&spec, &mut animator, time, true);
            assert_eq!(pose, first, "{look:?} {side} at {time}");
            assert_eq!(pose.blink, 0.);
            assert_eq!(pose.ribbons, 0.);
            assert_eq!(pose.lift, 0.);
        }
    }
    // Waiting still reads by its slit eyes.
    let waiting = pose(
        &spec(Look::Waiting, 16., Motion::Still),
        &mut Animator::new(),
        0.,
        true,
    );
    assert!(waiting.eyes.slit > 0.9);
}

#[test]
fn ribbons_show_only_from_28_points() {
    let max_ribbons = |side: f32| {
        let spec = spec(Look::Working, side, Motion::Live);
        let mut animator = Animator::new();
        (0..400)
            .map(|i| pose(&spec, &mut animator, i as f64 * 0.033, false).ribbons)
            .fold(0., f64::max)
    };
    assert!(max_ribbons(28.) > 0.9, "a working turn within 12 s");
    assert_eq!(max_ribbons(16.), 0.);
}

#[test]
fn resting_states_hold_the_rest_pose_and_blink_on_the_engine_clock() {
    let spec = spec(Look::Idle, 14., Motion::Blink);
    let prepared = prepared(spec.mascot).unwrap();
    let seed = prepared.seed();
    let rest = pose(
        &Spec {
            motion: Motion::Still,
            ..spec
        },
        &mut Animator::new(),
        0.,
        false,
    );
    let mut animator = Animator::new();
    let mut blinked = false;
    for i in 0..600 {
        let time = i as f64 * 0.02;
        let pose = pose(&spec, &mut animator, time, false);
        let expected = blink_at(seed, engine_time(seed, time));
        assert!((pose.blink - expected).abs() < 1e-12, "at {time}");
        blinked |= pose.blink > 0.9;
        assert_eq!(
            Pose {
                blink: rest.blink,
                ..pose
            },
            rest
        );
    }
    assert!(blinked);
}

#[test]
fn next_blink_finds_the_blinks_the_engine_draws() {
    for seed in [0, 7, 0x9e37_79b1, 123_456_789] {
        let mut t = 0.;
        while t < 120. {
            let (start, end) = next_blink(seed, t);
            assert!(end > t && (end - start - BLINK_LENGTH).abs() < 1e-9);
            if blink_at(seed, t) > 0. {
                assert!(start <= t, "seed {seed} at {t}: blinking before {start}");
            } else if start > t {
                // Nothing in between.
                let mut u = t;
                while u < start {
                    assert_eq!(blink_at(seed, u), 0., "seed {seed} at {u}");
                    u += 0.01;
                }
            }
            assert!(blink_at(seed, (start + end) / 2.) > 0.99);
            t += 0.037;
        }
    }
}

#[test]
fn the_eyes_follow_the_pointer_only_where_asked() {
    let follow = Spec {
        follow: true,
        ..spec(Look::Idle, 56., Motion::Live)
    };
    let gaze = |spec: &Spec, hovered: bool, reduced: bool| {
        let mut animator = Animator::new();
        let mut frame = None;
        for i in 0..40 {
            let pointer = hovered.then_some((0.8, 0.));
            frame = Some(frame_options(
                spec,
                &mut animator,
                i as f64 / 30.,
                hovered,
                pointer,
                reduced,
            ));
        }
        frame.unwrap().options.gaze
    };
    let looking = gaze(&follow, true, false).expect("a gaze while hovered");
    assert!(looking.x > 0.7 && looking.weight.unwrap() > 0.9);
    assert_eq!(gaze(&follow, false, false), None);
    assert_eq!(gaze(&follow, true, true), None, "reduced motion");
    let no_follow = Spec {
        follow: false,
        ..follow
    };
    assert_eq!(gaze(&no_follow, true, false), None);

    // The composer's runner holds a look where it runs, live only.
    let runner = Spec {
        gaze: Some((-0.8, 0.)),
        ..spec(Look::Working, 16., Motion::Live)
    };
    let ahead = gaze(&runner, false, false).expect("a held gaze while live");
    assert!(ahead.x < -0.7);
    assert_eq!(gaze(&runner, false, true), None, "reduced motion");
    let resting = Spec {
        motion: Motion::Still,
        ..runner
    };
    assert_eq!(gaze(&resting, false, false), None);
}

// ---------------------------------------------------------------------------
// In a window

struct Host {
    build: Rc<dyn Fn() -> AnyElement>,
    show: bool,
    renders: Rc<Cell<usize>>,
}
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div()
            .size_full()
            .flex()
            .items_start()
            .when(self.show, |d| d.child((self.build)()))
    }
}

fn host(
    cx: &mut TestAppContext,
    reduced: bool,
    build: impl Fn() -> AnyElement + 'static,
) -> (Entity<Host>, Rc<Cell<usize>>, &mut VisualTestContext) {
    cx.update(|cx| cx.set_reduce_motion(reduced));
    let renders = Rc::new(Cell::new(0));
    let (view, cx) = cx.add_window_view({
        let renders = renders.clone();
        move |_, _| Host {
            build: Rc::new(build),
            show: true,
            renders,
        }
    });
    cx.run_until_parked();
    (view, renders, cx)
}

fn advance(cx: &mut VisualTestContext, total: Duration) {
    let mut gone = Duration::ZERO;
    while gone < total {
        cx.executor().advance_clock(TICK);
        cx.run_until_parked();
        gone += TICK;
    }
}

fn idle(cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| !pulse::running(cx) && pulse::pending_wakes(cx) == 0)
}

fn content_like(alpha: f32) -> Hsla {
    let mut ink: Hsla = rgb(0xffffff).into();
    ink.a = alpha;
    ink
}

#[gpui::test]
fn still_avatars_ask_for_no_frames(cx: &mut TestAppContext) {
    let (_, renders, cx) = host(cx, false, || {
        let appearance = solar_core::project_appearance::ProjectAppearance::default();
        div()
            .flex()
            .child(crate::project_settings::project_icon(
                &appearance,
                "/work/solar",
                12.,
            ))
            .child(
                Avatar::new("crab", content_like(0.45), 12.)
                    .look(Look::Idle)
                    .motion(Motion::Still),
            )
            .child(crate::monos::avatar::avatar(
                "cat",
                BLUE,
                "needs-you",
                16.,
                true,
            ))
            .into_any_element()
    });
    assert!(idle(cx));
    let (live, painted, _) = cx.update(|_, cx| stats(cx));
    assert_eq!((live, painted), (3, 3));
    let before = renders.get();
    advance(cx, Duration::from_secs(3));
    cx.executor().advance_clock(Duration::from_secs(30));
    cx.run_until_parked();
    assert_eq!(renders.get(), before, "nothing repaints");
    assert!(idle(cx));
    assert_eq!(cx.update(|_, cx| stats(cx)).1, 3);
}

#[gpui::test]
fn reduced_motion_leases_nothing_and_holds_the_rest_pose(cx: &mut TestAppContext) {
    let (view, _, cx) = host(cx, true, || {
        div()
            .flex()
            .child(crate::monos::avatar::avatar(
                "robot", BLUE, "working", 56., false,
            ))
            .child(crate::monos::avatar::avatar("cat", BLUE, "idle", 14., true))
            .child(crate::monos::avatar::avatar(
                "dino", BLUE, "working", 16., true,
            ))
            .child(
                div()
                    .size(px(40.))
                    .child(crate::monos::avatar::picker_avatar("frog", BLUE, 32.)),
            )
            .into_any_element()
    });
    assert!(idle(cx));
    let painted = cx.update(|_, cx| stats(cx)).1;
    // Over the 56 pt avatar, which would follow the pointer, then the option.
    cx.simulate_mouse_move(point(px(40.), px(20.)), None, Modifiers::default());
    cx.simulate_mouse_move(point(px(110.), px(20.)), None, Modifiers::default());
    for _ in 0..10 {
        cx.executor().advance_clock(Duration::from_millis(700));
        view.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
    }
    assert!(idle(cx));
    assert_eq!(
        cx.update(|_, cx| stats(cx)).1,
        painted,
        "every repaint draws the same rest frame"
    );
}

#[gpui::test]
fn small_idle_avatars_wake_only_to_blink(cx: &mut TestAppContext) {
    let (_, renders, cx) = host(cx, false, || {
        crate::monos::avatar::avatar("cat", BLUE, "idle", 14., true)
    });
    let seed = prepared(mascot_index("cat")).unwrap().seed();
    let offset = engine_time(seed, 0.);
    // Its blinks over the next 42 s of the shared clock.
    let span = 42.;
    let mut blinks = 0;
    let mut t = 0.;
    loop {
        let (start, end) = next_blink(seed, offset + t);
        if start - offset > span {
            break;
        }
        blinks += 1;
        t = end - offset + 1e-6;
    }
    // About one every 4.2 s, "every 5 s or so".
    assert!((7..=16).contains(&blinks), "{blinks} blinks");
    let first = renders.get();
    advance(cx, Duration::from_secs_f64(span));
    let frames = renders.get() - first;
    // Each blink: the wake, ~6 frames of 0.19 s at 30 fps, the open eyes.
    assert!(
        frames >= blinks && frames <= blinks * 10,
        "{frames} frames for {blinks} blinks"
    );
    // A live avatar would have drawn ~1270 frames over the same span.
    assert!(frames * 10 < (span * 30.) as usize);
    // Between blinks it sleeps on one timer.
    cx.update(|_, cx| assert_eq!(pulse::pending_wakes(cx), 1));
}

#[gpui::test]
fn live_avatars_free_the_images_they_replace(cx: &mut TestAppContext) {
    let (view, _, cx) = host(cx, false, || {
        crate::monos::avatar::avatar("robot", BLUE, "working", 28., false)
    });
    cx.update(|_, cx| assert!(pulse::running(cx)));
    let first = cx.update(|_, cx| held_images(cx));
    assert_eq!(first.len(), 1);
    let first = first[0].clone();
    cx.update(|window, _| assert!(window.has_image_atlas_entry(&first)));
    advance(cx, Duration::from_secs(1));
    let (live, painted, freed) = cx.update(|_, cx| stats(cx));
    assert!(painted >= 20, "{painted} frames in a second");
    // The image on screen and, for one more paint, the one before.
    assert!(live <= 2, "{live} images held");
    assert_eq!(freed + live as u64, painted);
    cx.update(|window, cx| {
        assert!(!window.has_image_atlas_entry(&first), "replaced, freed");
        let held = held_images(cx);
        assert!(held.iter().any(|image| window.has_image_atlas_entry(image)));
    });
    // Gone from the window, gone from the GPU.
    let held = cx.update(|_, cx| held_images(cx));
    view.update(cx, |host, cx| {
        host.show = false;
        cx.notify();
    });
    cx.run_until_parked();
    let (live, painted, freed) = cx.update(|_, cx| stats(cx));
    assert_eq!((live, freed), (0, painted));
    cx.update(|window, _| {
        assert!(
            held.iter()
                .all(|image| !window.has_image_atlas_entry(image))
        );
    });
    advance(cx, Duration::from_millis(500));
    cx.update(|_, cx| assert!(!pulse::running(cx)));
}

#[gpui::test]
fn twin_avatars_paint_once(cx: &mut TestAppContext) {
    let (_, _, cx) = host(cx, false, || {
        div()
            .flex()
            .child(crate::monos::avatar::avatar(
                "skull",
                BLUE,
                "needs-you",
                16.,
                true,
            ))
            .child(crate::monos::avatar::avatar(
                "skull",
                BLUE,
                "needs-you",
                16.,
                true,
            ))
            .child(crate::monos::avatar::avatar(
                "skull",
                BLUE,
                "needs-you",
                16.,
                true,
            ))
            .into_any_element()
    });
    assert_eq!(cx.update(|_, cx| stats(cx)), (1, 1, 0));
}

#[gpui::test]
fn hovering_a_mascot_option_animates_it_until_the_pointer_leaves(cx: &mut TestAppContext) {
    let (_, _, cx) = host(cx, false, || {
        div()
            .size(px(40.))
            .flex()
            .items_center()
            .justify_center()
            .child(crate::monos::avatar::picker_avatar("frog", BLUE, 20.))
            .into_any_element()
    });
    // A test window's pointer starts at its corner, over the option.
    cx.simulate_mouse_move(point(px(300.), px(300.)), None, Modifiers::default());
    advance(cx, Duration::from_secs(2));
    assert!(idle(cx));
    // Over the option's corner, outside the avatar itself.
    cx.simulate_mouse_move(point(px(3.), px(3.)), None, Modifiers::default());
    cx.run_until_parked();
    cx.update(|_, cx| assert!(pulse::running(cx)));
    let painted = cx.update(|_, cx| stats(cx)).1;
    advance(cx, Duration::from_secs(1));
    assert!(cx.update(|_, cx| stats(cx)).1 > painted + 10);
    cx.simulate_mouse_move(point(px(300.), px(300.)), None, Modifiers::default());
    cx.run_until_parked();
    // It eases back to rest, then stops.
    advance(cx, Duration::from_secs(2));
    assert!(idle(cx));
    let (live, _, _) = cx.update(|_, cx| stats(cx));
    assert!(live <= 2);
}
