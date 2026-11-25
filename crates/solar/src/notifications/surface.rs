//! Stationary notification glass, matching GlassBackdrop and the native popover.
use crate::theme::*;
use gpui::{Div, canvas, div, prelude::*, px};

pub(crate) fn backdrop(radius: f32) -> impl IntoElement {
    let (light, opaque) = (is_light(), floating_fill());
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            let quad = if light {
                gpui::fill(bounds, opaque)
            } else {
                gpui::fill(bounds, content(0.02))
                    .border_color(gpui::backdrop_blur_marker(24. * window.scale_factor()))
            };
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let _ = (light, opaque);
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let quad = gpui::fill(bounds, pane_fill());
            window.paint_quad(quad.corner_radii(px(radius)));
        },
    )
    .absolute()
    .size_full()
}
/// A card floating in the top-right notification stack. It blocks the
/// mouse: the stack overlaps the title bar, and a click on the card must not
/// also reach the control under it (it used to press the diff counter).
pub(crate) fn glass_frame() -> Div {
    div()
        .occlude()
        .relative()
        .w(px(340.))
        .max_w_full()
        .overflow_hidden()
        .rounded(px(12.))
        .border_1()
        .border_color(content(0.1))
        .text_color(content(1.))
        .shadow_lg()
        .child(backdrop(12.))
}

/// Shared shell for the compact cards in the top-right notification stack.
pub(crate) fn toast_frame() -> Div {
    glass_frame()
        .w_full()
        .border_dashed()
        .border_color(content(0.2))
        .bg(content(0.1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, Render, TestAppContext, Window};
    use std::{cell::Cell, rc::Rc};

    /// A button with a notification card laid over it, as the stack lies
    /// over the title bar's diff counter.
    struct Overlap {
        pressed: Rc<Cell<bool>>,
    }
    impl Render for Overlap {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let pressed = self.pressed.clone();
            div()
                .size_full()
                .relative()
                .child(
                    div()
                        .id("under")
                        .debug_selector(|| "under".into())
                        .absolute()
                        .top(px(10.))
                        .left(px(10.))
                        .size(px(40.))
                        .on_click(move |_, _, _| pressed.set(true)),
                )
                .child(
                    div().absolute().top_0().left_0().child(
                        glass_frame()
                            .id("card")
                            .debug_selector(|| "card".into())
                            .h(px(80.)),
                    ),
                )
        }
    }

    #[gpui::test]
    fn a_click_on_a_notification_card_does_not_reach_what_is_under_it(cx: &mut TestAppContext) {
        let pressed = Rc::new(Cell::new(false));
        let (_, cx) = cx.add_window_view(|_, _| Overlap {
            pressed: pressed.clone(),
        });
        cx.run_until_parked();
        let under = cx.debug_bounds("under").expect("button drawn");
        assert!(
            cx.debug_bounds("card")
                .expect("card drawn")
                .contains(&under.center())
        );
        cx.simulate_click(under.center(), Default::default());
        cx.run_until_parked();
        assert!(!pressed.get(), "the click went through the card");
    }
}
