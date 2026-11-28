//! Timing of the per-frame work, for the budget of ~30 small live avatars at 60 fps.
//! Run with `cargo test -p bot-avatar --release --test perf -- --ignored --nocapture`.

use std::time::Instant;

use bot_avatar::{solar_avatars, AvatarState, CanvasSurface, PreparedAvatar, RenderOptions};

#[test]
#[ignore = "timing; run in release"]
fn thirty_avatars_one_second() {
    let t0 = Instant::now();
    let prepared: Vec<PreparedAvatar> = solar_avatars()
        .iter()
        .map(|d| PreparedAvatar::new(d, None).expect("valid"))
        .collect();
    let prep = t0.elapsed();
    eprintln!(
        "prepare 10 avatars: {:.1} ms ({:.1} ms each)",
        prep.as_secs_f64() * 1e3,
        prep.as_secs_f64() * 1e2
    );

    let states = [
        AvatarState::Idle,
        AvatarState::Working,
        AvatarState::Waiting,
    ];
    let mut surfaces: Vec<CanvasSurface> = (0..30)
        .map(|i| CanvasSurface::new(&prepared[i % 10]))
        .collect();
    for (size, dpr) in [(16.0, 2.0), (28.0, 2.0), (56.0, 2.0)] {
        let (mut frame_time, mut paint_time, mut reused) = (0.0, 0.0, 0usize);
        for n in 0..60 {
            for i in 0..30 {
                let p = &prepared[i % 10];
                let t = n as f64 / 60.0 + i as f64 * 0.37;
                let o = RenderOptions {
                    color: Some("#7aa2f7".into()),
                    state: Some(states[i % 3].into()),
                    time: Some(t),
                    state_age: Some(2.0 + t),
                    size: Some(size),
                    ..Default::default()
                };
                let a = Instant::now();
                let tr = p.live_transform(&o);
                let frame = tr.build_frame();
                frame_time += a.elapsed().as_secs_f64();
                std::hint::black_box(&frame);
                let b = Instant::now();
                let out = surfaces[i].paint_live(&tr, size, dpr, None, None);
                paint_time += b.elapsed().as_secs_f64();
                reused += out.reused as usize;
            }
        }
        eprintln!(
            "{size} pt @{dpr}x, 30 avatars × 60 frames: frames {:.1} ms, painting {:.1} ms (per second of animation), {} reused bitmaps",
            frame_time * 1e3,
            paint_time * 1e3,
            reused
        );
    }
}
