//! A Mono's avatar: its mascot as an animated open-bot-avatar, in its colour
//! (see [`crate::avatars`] and DIRECTION.md §3 for what moves where).
use crate::avatars::{Avatar, FOLLOW_FROM, Look, Motion, mono_motion, soften};
use gpui::{AnyElement, prelude::*};

/// A Mono's avatar `side` points wide for its status (`idle`, `working` or
/// `needs-you`). Under 28 pt an idle Mono blinks, a working one works without
/// ribbons and one that needs you waits still; from 28 pt a working or
/// waiting Mono is animated and an idle one blinks until the pointer wakes
/// it, and from 44 pt its eyes follow the pointer over it. `calm` keeps an idle
/// Mono to blinking at any size, for decorative line-ups. Decorative: the
/// call sites show the Mono's name beside it or label their control.
#[track_caller]
pub(crate) fn avatar(name: &str, color: u32, status: &str, side: f32, calm: bool) -> AnyElement {
    let look = Look::of_status(status);
    let motion = mono_motion(look, side, calm);
    Avatar::new(name, soften(color), side)
        .look(look)
        .motion(motion)
        .follow_pointer(
            matches!(motion, Motion::Live | Motion::BlinkLiveOnHover) && side >= FOLLOW_FROM,
        )
        .into_any_element()
}

/// A mascot to choose in a Mono's look settings: still, idle and animated
/// while the pointer is over its option. It takes the option's whole box,
/// centred, so hovering anywhere over the option wakes it.
#[track_caller]
pub(crate) fn picker_avatar(name: &str, color: u32, side: f32) -> AnyElement {
    Avatar::new(name, soften(color), side)
        .look(Look::Idle)
        .motion(Motion::LiveOnHover)
        .fill_parent()
        .into_any_element()
}
