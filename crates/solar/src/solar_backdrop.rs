//! Solar's empty-home backdrop: a faint neutral veil and the monochrome
//! night horizon, oblique and anchored bottom right. Only the empty session
//! paints it; a session with messages, code or a terminal never does.
//!
//! Both images are baked from the validated mockup (Studio Design,
//! 04-maquettes/hf/valide/accueil.html): its `.main::before` box is 112% of
//! the pane wide at 16:9, set 48px past the right edge and 14px past the
//! bottom, turned -14°, greyscale ×1.1, masked to the limb and at 87%. The
//! bake keeps only the visible band, so this module places that band where
//! the turned box would put it.
use gpui::{
    AnyElement, Bounds, ImgResourceLoader, IntoElement, Pixels, Resource, Styled, canvas, point,
    px, size,
};

pub(crate) const HORIZON: &str = "solar-horizon.png";
pub(crate) const VEIL: &str = "solar-veil.png";

/// The width the horizon was baked at, for the box before turning.
const BAKE_WIDTH: f32 = 1600.;
/// The turned box's bounding size at the bake width.
const TURNED: (f32, f32) = (1772., 1262.);
/// The visible band inside the turned box, at the bake width.
const BAND: (f32, f32, f32, f32) = (160., 639., 1611., 621.);

/// Where the baked horizon band lands in a pane of `pane` bounds.
pub(crate) fn horizon_bounds(pane: Bounds<Pixels>) -> Bounds<Pixels> {
    let (w, h) = (pane.size.width.as_f32(), pane.size.height.as_f32());
    let box_width = w * 1.12;
    let scale = box_width / BAKE_WIDTH;
    let box_height = box_width * 9. / 16.;
    // Turning keeps the box's center.
    let center = (w + 48. - box_width / 2., h + 14. - box_height / 2.);
    let left = center.0 - TURNED.0 * scale / 2. + BAND.0 * scale;
    let top = center.1 - TURNED.1 * scale / 2. + BAND.1 * scale;
    Bounds::new(
        point(pane.left() + px(left), pane.top() + px(top)),
        size(px(BAND.2 * scale), px(BAND.3 * scale)),
    )
}

/// The backdrop, filling its (relative) parent and clipped to it.
pub(crate) fn backdrop() -> AnyElement {
    canvas(
        |_, _, _| {},
        |bounds, _, window, cx| {
            for (path, placed) in [(VEIL, bounds), (HORIZON, horizon_bounds(bounds))] {
                if let Some(Ok(image)) =
                    window.use_asset::<ImgResourceLoader>(&Resource::Embedded(path.into()), cx)
                {
                    let _ = window.paint_image(bounds, placed, Default::default(), image, 0, false);
                }
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_horizon_hugs_the_bottom_right_as_in_the_mockup() {
        // The mockup's pane: 1252×914 below the title bar.
        let pane = Bounds::new(point(px(260.), px(40.)), size(px(1252.), px(914.)));
        let band = horizon_bounds(pane);
        // It starts left of the pane, runs past its right edge and ends below
        // its bottom edge; the paint clips both.
        assert!(band.left() < pane.left());
        assert!(band.right() > pane.right());
        assert!(band.bottom() > pane.bottom());
        // Its top stays in the lower half, clear of the composer.
        assert!(band.top() > pane.top() + pane.size.height / 2.);
        // Wider panes scale it, keeping the anchor.
        let wide = Bounds::new(pane.origin, size(px(2504.), px(914.)));
        let scaled = horizon_bounds(wide);
        assert!((scaled.size.width.as_f32() - band.size.width.as_f32() * 2.).abs() < 0.5);
    }
}
