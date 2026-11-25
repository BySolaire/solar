//! Solar's empty states: what is empty, why, and one way forward
//! (charles-design-tips 19; client exception to the unchanged MonoCode
//! structure, 2026-11-25). Copy: 03-contenu/etats-vides-en.md.
use crate::theme::*;
use gpui::{Div, ElementId, FontWeight, Role, SharedString, Stateful, div, prelude::*, px};

/// A title in primary ink (13 px semibold), a neutral secondary line
/// (12/17) and, 12 pt below it, the action.
pub(crate) fn empty_state(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    line: impl Into<SharedString>,
    action: impl IntoElement,
) -> Stateful<Div> {
    let title = title.into();
    let id = id.into();
    let selector = format!("{id}");
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .role(Role::Status)
        .aria_label(title.clone())
        .flex()
        .flex_col()
        .items_start()
        .child(
            div()
                .text_size(px(13.))
                .line_height(px(18.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(content(1.))
                .child(title),
        )
        .child(
            div()
                .mt(px(2.))
                .text_size(px(12.))
                .line_height(px(17.))
                .text_color(content(0.5))
                .child(line.into()),
        )
        .child(div().mt(px(12.)).child(action))
}

/// The compact secondary button: 28 pt with a hairline border, as in
/// Settings.
pub(crate) fn empty_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
) -> Stateful<Div> {
    let label = label.into();
    let id = id.into();
    let selector = format!("{id}");
    crate::settings::widgets::secondary_button(id, label.clone(), false, false)
        .debug_selector(move || selector.clone())
        .child(label)
}

/// A discreet text link, for places where a bordered button would crowd
/// (the session sidebar already has its New row at the top).
pub(crate) fn empty_link(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
) -> Stateful<Div> {
    let label = label.into();
    let id = id.into();
    let selector = format!("{id}");
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .role(Role::Button)
        .aria_label(label.clone())
        .cursor_pointer()
        .text_size(px(12.))
        .line_height(px(17.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(content(0.7))
        .hover(|s| s.text_color(content(1.)))
        .child(label)
}
