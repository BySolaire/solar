//! SubagentMascot: a sub-agent's avatar beside its row in a reply, one of the
//! ten mascots picked by its name, in the theme's neutral ink (DIRECTION.md
//! §3: running `content(0.70)` and working, done `content(0.45)` and still,
//! failed `#F87171` and still; 12 pt, never with ribbons).
use crate::avatars::{Avatar, Look, Motion};
use crate::theme::content;
use gpui::{AnyElement, Hsla, div, prelude::*, px, rgb};
use solar_core::{project_appearance::MASCOTS, tool::ToolState};

/// A 12px avatar centered in the same 14px icon slot as the React row.
/// Activity is independent of state: a pending run can already have settled.
#[track_caller]
pub(super) fn subagent_mascot(name: &str, state: &ToolState, active: bool) -> AnyElement {
    let (fill, look, motion) = subagent_look(state, active);
    div()
        .size(px(14.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(
            Avatar::new(MASCOTS[mascot_index(name)], fill, 12.)
                .look(look)
                .motion(motion),
        )
        .into_any_element()
}

/// The body colour, state and motion of a sub-agent's avatar.
fn subagent_look(state: &ToolState, active: bool) -> (Hsla, Look, Motion) {
    let fill = match state {
        ToolState::Failed | ToolState::Cancelled => rgb(0xf87171).into(),
        ToolState::Running => content(0.70),
        ToolState::Completed => content(0.45),
    };
    if active {
        (fill, Look::Working, Motion::Live)
    } else {
        (fill, Look::Idle, Motion::Still)
    }
}

/// projectMascots.ts's name hash, onto `MASCOTS`.
fn mascot_index(name: &str) -> usize {
    // JavaScript charCodeAt iterates UTF-16 code units, rather than UTF-8 bytes
    // or Unicode scalars. Wrapping u32 arithmetic mirrors `>>> 0` exactly.
    let hash = name.encode_utf16().fold(0u32, |hash, ch| {
        hash.wrapping_mul(131).wrapping_add(u32::from(ch))
    });
    hash as usize % MASCOTS.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub_agents_wear_the_theme_ink_and_move_only_while_running() {
        assert_eq!(
            subagent_look(&ToolState::Running, true),
            (content(0.70), Look::Working, Motion::Live)
        );
        // A run that settled before the reply did holds still.
        assert_eq!(
            subagent_look(&ToolState::Running, false),
            (content(0.70), Look::Idle, Motion::Still)
        );
        assert_eq!(
            subagent_look(&ToolState::Completed, false),
            (content(0.45), Look::Idle, Motion::Still)
        );
        for failed in [ToolState::Failed, ToolState::Cancelled] {
            assert_eq!(
                subagent_look(&failed, false),
                (rgb(0xf87171).into(), Look::Idle, Motion::Still)
            );
        }
    }

    #[test]
    fn names_pick_the_same_mascots_as_the_pixel_faces_did() {
        // `hash * 131 + code unit` over UTF-16, modulo the ten mascots in
        // projectMascots.ts order.
        assert_eq!(mascot_index(""), 0);
        assert_eq!(mascot_index("a"), 97 % 10);
        assert_eq!(mascot_index("ab"), (97 * 131 + 98) % 10);
        let emoji = "\u{1F600}";
        let units: Vec<u16> = emoji.encode_utf16().collect();
        assert_eq!(
            mascot_index(emoji),
            (units[0] as usize * 131 + units[1] as usize) % 10
        );
        assert_eq!(MASCOTS[mascot_index("ab")], MASCOTS[(97 * 131 + 98) % 10]);
    }
}
