use gpui::{AnyElement, Div, Hsla, Role, Stateful, Svg, div, img, prelude::*, px, rgb, rgba, svg};
// Shared appearance tokens. Interaction fills use translucent content ink
// rather than opaque replacement greys.
pub(crate) const CHAT: u32 = 0x141414;
pub(crate) const TEXT: u32 = 0xebebeb;
pub(crate) const MUTED: u32 = 0x969696;
pub(crate) const ACCENT: u32 = 0x4da3f5;
pub(crate) const COLUMN: f32 = 832.;
/// The transcript's side padding inside `COLUMN`: the docked composer's
/// 10px inset, 1px border and 12px text padding, so prose lines up with the
/// composer's text as it scrolls past.
pub(crate) const COLUMN_PAD: f32 = 23.;
pub(crate) const TITLE_BAR_HEIGHT: f32 = 40.;
pub(crate) const TITLE_TAB_TEXT_SIZE: f32 = 13.;
pub(crate) const TITLE_TAB_MAX_WIDTH: f32 = 224.;
pub(crate) const PANE_HEADER_HEIGHT: f32 = 36.;
/// Every tab's plate: title, file and terminal tabs, and the new-tab button.
pub(crate) const TAB_PLATE_HEIGHT: f32 = 27.;

/// Attachment previews keep a fixed crop, independent of image dimensions.
pub(crate) fn attachment_thumbnail(source: impl Into<gpui::ImageSource>) -> gpui::AnyElement {
    div()
        .size(px(36.))
        .min_w(px(36.))
        .min_h(px(36.))
        .max_w(px(36.))
        .max_h(px(36.))
        .flex_shrink_0()
        .rounded_lg()
        .overflow_hidden()
        .child(
            img(source)
                .size(px(36.))
                .aspect_square()
                .flex_shrink_0()
                .rounded_lg()
                .object_fit(gpui::ObjectFit::Cover),
        )
        .into_any_element()
}

#[derive(Clone, Copy, PartialEq)]
struct Tokens {
    light: bool,
    ink: Hsla,
    accent: Hsla,
    user_accent: Option<Hsla>,
    shell: Hsla,
    pane: Hsla,
    inset_pane: Hsla,
    /// The palette's opaque `solid` role: Solar's graphite field.
    solid: Hsla,
    /// The Solar palette in dark mode: graphite composer and menus, Solar
    /// Blue send button, and the horizon on the empty home.
    solar: bool,
    /// Solar on see-through glass: secondary ink is lifted so small labels
    /// keep their contrast over a bright desktop.
    glass_ink: bool,
    syntax: [u32; 11],
    diff: [u32; 4],
    diff_strong: bool,
}
thread_local! {
    /// What views paint with: the target, or a step of the tween toward it.
    static TOKENS: std::cell::RefCell<Option<Tokens>> = const { std::cell::RefCell::new(None) };
    static TARGET: std::cell::RefCell<Option<Tokens>> = const { std::cell::RefCell::new(None) };
    static TWEEN: std::cell::Cell<Option<(Tokens, std::time::Instant)>> = const { std::cell::Cell::new(None) };
}
/// Switching to a project with other colors blends into them over this long.
const TWEEN_LENGTH: std::time::Duration = std::time::Duration::from_millis(360);

#[cfg(test)]
pub(crate) fn configure(p: &crate::settings::Preferences, system_light: bool) -> bool {
    configure_project(p, None, system_light, false)
}
/// Paint with `project`'s colors in place of the app-wide ones. With
/// `animate`, a change of colors blends from what is on screen; call again
/// each frame while [`tweening`] so the blend advances. Returns whether the
/// painted colors changed.
pub(crate) fn configure_project(
    p: &crate::settings::Preferences,
    project: Option<&solar_core::project_appearance::ProjectTheme>,
    system_light: bool,
    animate: bool,
) -> bool {
    let light = p.theme == "light" || (p.theme == "system" && system_light);
    let (workspace_theme, accent_color, hue, saturation, dark_lightness) = match project {
        Some(t) => (
            t.workspace_theme.as_str(),
            t.accent_color.as_deref(),
            t.hue,
            t.saturation,
            t.dark_lightness,
        ),
        None => (
            p.workspace_theme.as_str(),
            p.accent_color.as_deref(),
            p.theme_hue,
            p.theme_saturation,
            p.theme_dark_lightness,
        ),
    };
    let palette = crate::settings::palette(workspace_theme, light);
    let glass = window_glass(p, system_light);
    let solar = workspace_theme == "solar" && !light;
    let glass_alpha = if glass {
        glass_opacity(p.sidebar_opacity, solar)
    } else {
        1.
    };
    // Both layouts derive their surface roles from the same saved settings.
    let shell = gpui::hsla(
        hue.min(360) as f32 / 360.,
        saturation.min(100) as f32 / 100.,
        if light {
            0.97
        } else {
            dark_lightness.min(30) as f32 / 100.
        },
        glass_alpha,
    );
    let mut pane = shell;
    pane.a = if glass && p.body_glass { 0.28 } else { 1. };
    // The window already paints its glass tint. A glass content card reveals
    // that layer directly; adding another tint would hide the desktop twice.
    // Solid content keeps its deeper dark tone or white light appearance.
    let mut inset_pane = shell;
    inset_pane.l = if light { 1. } else { shell.l * 0.45 };
    inset_pane.a = if glass && p.body_glass { 0. } else { 1. };
    let custom = accent_color
        .filter(|s| s.len() == 7)
        .and_then(|s| u32::from_str_radix(s.trim_start_matches('#'), 16).ok());
    let tokens = Tokens {
        light,
        ink: rgb(palette.content).into(),
        accent: rgb(custom.unwrap_or(palette.accent)).into(),
        user_accent: custom.map(|c| rgb(c).into()),
        shell,
        pane,
        inset_pane,
        solid: rgb(palette.solid).into(),
        solar,
        glass_ink: solar && glass_alpha < 1.,
        diff: diff_palette(&p.diff_palette, light),
        diff_strong: p.diff_palette == "high-contrast",
        syntax: syntax_palette(workspace_theme, light),
    };
    let now = std::time::Instant::now();
    if TARGET.with(|t| t.replace(Some(tokens))) != Some(tokens) {
        let shown = TOKENS.with(|t| *t.borrow());
        // A light/dark flip repaints too much at once to blend well.
        TWEEN.set(
            shown
                .filter(|shown| animate && shown.light == light)
                .map(|shown| (shown, now)),
        );
    }
    let painted = match TWEEN.get() {
        Some((from, start)) => {
            let t = (now - start).as_secs_f32() / TWEEN_LENGTH.as_secs_f32();
            if t >= 1. {
                TWEEN.set(None);
                tokens
            } else {
                // Ease out: quick to respond, gentle to settle.
                from.mix(&tokens, 1. - (1. - t).powi(3))
            }
        }
        None => tokens,
    };
    TOKENS.with(|current| current.replace(Some(painted)) != Some(painted))
}
/// Whether colors are still blending toward a project's theme.
pub(crate) fn tweening() -> bool {
    TWEEN.get().is_some()
}
impl Tokens {
    fn mix(&self, to: &Self, t: f32) -> Self {
        let color = |a: Hsla, b: Hsla| -> Hsla {
            let (a, b) = (a.to_rgb(), b.to_rgb());
            gpui::Rgba {
                r: a.r + (b.r - a.r) * t,
                g: a.g + (b.g - a.g) * t,
                b: a.b + (b.b - a.b) * t,
                a: a.a + (b.a - a.a) * t,
            }
            .into()
        };
        let hex = |a: u32, b: u32| -> u32 {
            [16, 8, 0].iter().fold(0, |out, shift| {
                let (a, b) = (((a >> shift) & 0xff) as f32, ((b >> shift) & 0xff) as f32);
                out | (((a + (b - a) * t).round() as u32) << shift)
            })
        };
        Self {
            ink: color(self.ink, to.ink),
            accent: color(self.accent, to.accent),
            // Going to or from the theme's own accent blends through it.
            user_accent: match (self.user_accent, to.user_accent) {
                (None, None) => None,
                (from, to_accent) => Some(color(
                    from.unwrap_or(self.accent),
                    to_accent.unwrap_or(to.accent),
                )),
            },
            shell: color(self.shell, to.shell),
            pane: color(self.pane, to.pane),
            inset_pane: color(self.inset_pane, to.inset_pane),
            solid: color(self.solid, to.solid),
            syntax: std::array::from_fn(|i| hex(self.syntax[i], to.syntax[i])),
            diff: std::array::from_fn(|i| hex(self.diff[i], to.diff[i])),
            ..*to
        }
    }
}

/// Platforms whose compositor blurs the desktop behind the window: macOS
/// vibrancy and Windows Acrylic. Linux blur is compositor-dependent, so it
/// stays opaque rather than exposing the raw desktop.
pub(crate) const GLASS_PLATFORM: bool = cfg!(any(target_os = "macos", target_os = "windows"));
pub(crate) fn window_glass(p: &crate::settings::Preferences, system_light: bool) -> bool {
    GLASS_PLATFORM && p.theme != "light" && !(p.theme == "system" && system_light)
}
/// What the compositor puts behind a window that does or doesn't paint glass.
pub(crate) fn window_backdrop(glass: bool) -> gpui::WindowBackgroundAppearance {
    if glass && GLASS_PLATFORM {
        gpui::WindowBackgroundAppearance::Blurred
    } else {
        gpui::WindowBackgroundAppearance::Opaque
    }
}
pub(crate) fn accent_color() -> Hsla {
    TOKENS.with(|t| {
        t.borrow()
            .as_ref()
            .map(|t| t.accent)
            .unwrap_or_else(|| rgb(ACCENT).into())
    })
}
pub(crate) fn content(alpha: f32) -> Hsla {
    let (mut color, glass_ink, solar): (Hsla, bool, bool) = TOKENS.with(|t| {
        t.borrow()
            .as_ref()
            .map(|t| (t.ink, t.glass_ink, t.solar))
            .unwrap_or_else(|| (rgb(TEXT).into(), false, false))
    });
    // Solar never uses blue-grey (client decision, 2026-11-24): its icy
    // #EAF2FF is for full-strength text only. Every faded ink (secondary
    // text, counts, times, shortcuts, placeholders, washes and hairlines)
    // fades pure white instead, so its grey is neutral.
    if solar && alpha < 1. {
        color = white(1.);
    }
    color.a = if glass_ink {
        glass_ink_alpha(alpha)
    } else {
        alpha
    };
    color
}

/// Solar never lets the desktop show through more than a quarter: at the
/// slider's 15% the window is still 75% black, so a bright wallpaper reads
/// as dark slate rather than milky grey and the home horizon stays visible.
/// The slider's 15–100% range maps onto that 75–100%.
pub(crate) const SOLAR_GLASS_FLOOR: f32 = 0.75;

/// How opaque the window's tint is for a saved sidebar opacity.
pub(crate) fn glass_opacity(sidebar_opacity: u32, solar: bool) -> f32 {
    let saved = sidebar_opacity.clamp(15, 100) as f32 / 100.;
    if solar {
        SOLAR_GLASS_FLOOR + (1. - SOLAR_GLASS_FLOOR) * (saved - 0.15) / 0.85
    } else {
        saved
    }
}

/// Secondary ink (section labels, counts, times, hints: 30–70% of the ink)
/// moves a fifth of the way toward full ink on Solar glass. Fills and
/// hairlines (under 30%) and full ink are left as they are.
fn glass_ink_alpha(alpha: f32) -> f32 {
    if (0.3..0.7).contains(&alpha) {
        alpha + (1. - alpha) * 0.2
    } else {
        alpha
    }
}

/// `[data-composer-box]::before` uses content ink at 3% over its backdrop.
/// Light mode instead gives the box the solid `background-base` surface.
pub(crate) fn composer_fill() -> Hsla {
    if solar() {
        return solid_fill();
    }
    let light = TOKENS.with(|tokens| tokens.borrow().as_ref().is_some_and(|t| t.light));
    if light {
        let mut background = shell_fill();
        background.a = 1.;
        background
    } else {
        content(0.03)
    }
}

/// The inset composer's frost; the solid light surface has no backdrop blur.
pub(crate) fn composer_blur() -> f32 {
    let light = TOKENS.with(|tokens| tokens.borrow().as_ref().is_some_and(|t| t.light));
    if light || solar() { 0. } else { 24. }
}

/// Whether the Solar palette paints in dark mode.
pub(crate) fn solar() -> bool {
    TOKENS.with(|tokens| tokens.borrow().as_ref().is_some_and(|t| t.solar))
}
/// The palette's opaque `solid` role; Solar's graphite field.
pub(crate) fn solid_fill() -> Hsla {
    TOKENS
        .with(|tokens| tokens.borrow().as_ref().map(|t| t.solid))
        .unwrap_or_else(|| rgb(CHAT).into())
}
/// Solar's control outline on focus: neutral white at 45% (≈ #737373 on
/// black). Solar never uses blue-grey (client decision, 2026-11-24).
const SOLAR_CONTROL_BORDER_ALPHA: f32 = 0.45;

/// The accent picked in Settings; `None` keeps the theme's neutral ink.
fn user_accent() -> Option<Hsla> {
    TOKENS.with(|t| t.borrow().as_ref().and_then(|t| t.user_accent))
}
/// `var(--user-accent-color, var(--color-accent))`.
pub(crate) fn picked_accent() -> Hsla {
    user_accent().unwrap_or_else(accent_color)
}
/// `.user-message-bubble`: a picked accent tints sent messages at 24%.
pub(crate) fn user_bubble_fill() -> Hsla {
    user_accent().map_or(content(0.1), |a| a.opacity(0.24))
}
/// `.user-message-bubble[data-draft]`: an 8% wash inside a 38% dashed edge.
pub(crate) fn draft_bubble_fill() -> Hsla {
    user_accent().map_or(content(0.04), |a| a.opacity(0.08))
}
pub(crate) fn draft_bubble_edge() -> Hsla {
    user_accent().map_or(content(0.3), |a| a.opacity(0.38))
}
/// `.primary-action` under `has-user-accent`: the accent, with black or white
/// ink by its relative luminance; `None` keeps the default solid button.
pub(crate) fn primary_action(enabled: bool) -> Option<(Hsla, Hsla)> {
    let accent = match user_accent() {
        Some(accent) => accent,
        // Solar Blue lights the send button only once it can send, with
        // black ink; at rest it is a graphite key with muted ink.
        None if solar() => {
            return Some(if enabled {
                (accent_color(), black(1.))
            } else {
                (solid_fill(), content(0.55))
            });
        }
        None => return None,
    };
    if !enabled {
        return Some((accent.opacity(0.28), content(0.35)));
    }
    let rgba = accent.to_rgb();
    let linear = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * linear(rgba.r) + 0.7152 * linear(rgba.g) + 0.0722 * linear(rgba.b);
    Some((
        accent,
        if luminance > 0.179 {
            black(1.)
        } else {
            white(1.)
        },
    ))
}

/// Selected text is tinted with the accent; light surfaces get a paler wash
/// so dark ink stays readable on top.
pub(crate) fn text_selection() -> Hsla {
    accent_color().opacity(if is_light() { 0.2 } else { 0.35 })
}

/// Light mode lifts the solid composer off the matching page with a soft
/// two-layer drop; dark mode's frosted box needs no shadow.
pub(crate) fn composer_shadow() -> Vec<gpui::BoxShadow> {
    if !is_light() {
        return Vec::new();
    }
    vec![
        gpui::BoxShadow::new(px(0.), px(1.), black(0.04)).blur_radius(px(2.)),
        gpui::BoxShadow::new(px(0.), px(6.), black(0.07))
            .blur_radius(px(20.))
            .spread_radius(px(-4.)),
    ]
}

/// Light themes paint floating surfaces opaque instead of frosting them.
pub(crate) fn is_light() -> bool {
    TOKENS.with(|tokens| tokens.borrow().as_ref().is_some_and(|t| t.light))
}
/// The opaque card behind popovers and modals in light themes, and wherever
/// the renderer has no backdrop blur.
pub(crate) fn floating_fill() -> Hsla {
    if solar() {
        return solid_fill();
    }
    let mut fill = if is_light() {
        inset_pane_fill()
    } else {
        pane_fill()
    };
    if is_light() {
        fill.l = fill.l.max(0.995);
    }
    fill.a = 1.;
    fill
}

pub(crate) fn composer_border(focused: bool) -> Hsla {
    if solar() {
        // White 12% at rest, the control outline while typing.
        return if focused {
            white(SOLAR_CONTROL_BORDER_ALPHA)
        } else {
            white(0.12)
        };
    }
    content(if focused { 0.2 } else { 0.1 })
}

pub(crate) fn title_tab_fill(selected: bool, hovered: bool) -> Hsla {
    content(if selected {
        0.1
    } else if hovered {
        0.07
    } else {
        0.04
    })
}

/// The plate for session tabs in the title bar.
/// Headline ink owns inactive dimming; the plate never fades its children.
/// Its icon tile sits as far from the left edge as from the top and bottom.
pub(crate) fn title_tab_plate(selected: bool, hovered: bool) -> Div {
    div()
        .h(px(TAB_PLATE_HEIGHT))
        .rounded_md()
        .pl(px(crate::tab_tile::tab_tile_inset()))
        .pr_2()
        .flex()
        .items_center()
        .bg(title_tab_fill(selected, hovered))
}
pub(crate) fn shell_fill() -> Hsla {
    if let Some(fill) = TOKENS.with(|t| t.borrow().as_ref().map(|t| t.shell)) {
        return fill;
    }
    if GLASS_PLATFORM {
        // #0d0d0d at 80%, with the stylesheet's #ebebeb 5% wash above it.
        rgba(0x1b1b1bcf).into()
    } else {
        rgb(0x181818).into()
    }
}
pub(crate) fn pane_fill() -> Hsla {
    if let Some(fill) = TOKENS.with(|t| t.borrow().as_ref().map(|t| t.pane)) {
        return fill;
    }
    if GLASS_PLATFORM {
        rgba(0x06060666).into()
    } else {
        rgb(CHAT).into()
    }
}
/// `--color-background-base`: the theme's opaque surface, also the ink on
/// solid `bg-content` buttons.
pub(crate) fn background_base() -> Hsla {
    let mut base = shell_fill();
    base.a = 1.;
    base
}
/// `--zen-rail`: ink 14% over the opaque base, the tree rails under tool
/// phases and orchestrator/operator rows. Opaque, so the curve and spine
/// overlapping where they meet don't stack into a brighter pixel.
pub(crate) fn rail_color() -> Hsla {
    // Solar mixes its rails from pure white, so they stay neutral grey.
    let ink = if solar() { white(1.) } else { content(1.) }.to_rgb();
    let base = background_base().to_rgb();
    let mix = |b: f32, i: f32| b + (i - b) * 0.14;
    gpui::Rgba {
        r: mix(base.r, ink.r),
        g: mix(base.g, ink.g),
        b: mix(base.b, ink.b),
        a: 1.,
    }
    .into()
}
/// Apply to surrounding chrome, leaving the content on the window's glass.
pub(crate) fn inset_chrome_wash() -> Hsla {
    content(0.05)
}
pub(crate) fn inset_pane_fill() -> Hsla {
    TOKENS
        .with(|tokens| tokens.borrow().as_ref().map(|t| t.inset_pane))
        .unwrap_or_else(pane_fill)
}
pub(crate) fn section_label(label: &'static str) -> Div {
    div()
        .h(px(28.))
        .px_2()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_1()
        .text_size(px(12.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(content(0.5))
        .child(div().flex_1().child(label))
}
pub(crate) fn icon(path: &'static str, side: f32) -> Svg {
    svg()
        .path(path)
        .size(px(side))
        .text_color(content(0.5))
        .flex_shrink_0()
}
/// A provider's brand fill for its icon tile, top and bottom of a gentle
/// gradient. `None` for a multicolour logo that keeps a light tile.
pub(crate) fn provider_tile(provider: &str) -> Option<(Hsla, Hsla)> {
    let (top, bottom) = match provider {
        "claude" => (0xE3876A, 0xD06A49),
        "antigravity" => return None,
        // The violet-to-pink of its mark.
        "omp" => (0xC45BE0, 0x8A5CF2),
        "cursor" | "grok" | "pi" | "fx" => (0x343434, 0x161616),
        "opencode" => (0x3B3636, 0x1F1C1C),
        "hermes" | "devin" => (0x3A3A3E, 0x1E1E22),
        // Codex: the blue of its mark's gradient.
        _ => (0x7A8CFF, 0x4048FF),
    };
    Some((rgb(top).into(), rgb(bottom).into()))
}

/// The provider's mark in white, to sit on its [`provider_tile`] colour.
/// A multicolour logo sits on a neutral tile in that tile's ink.
pub(crate) fn provider_glyph(provider: &str, side: f32) -> AnyElement {
    let (path, scale) = match provider {
        // A multicolour logo on a neutral tile: one colour, like the rest.
        "antigravity" => {
            return svg()
                .path("antigravity.svg")
                .size(px(side))
                .text_color(crate::tab_tile::tile_ink())
                .flex_shrink_0()
                .into_any_element();
        }
        // Codex's mark without the white disc behind it, so the prompt cut
        // out of it shows the tile's colour.
        "codex" => ("codex-glyph.svg", 1.),
        "claude" => ("claude.svg", 1.),
        "cursor" => ("cursor.svg", 1.),
        "grok" => ("grok.svg", 1.),
        "opencode" => ("opencode.svg", 1.),
        "pi" => ("pi.svg", 1.),
        "omp" => ("omp.svg", 1.),
        "fx" => ("fx.svg", 1.),
        "hermes" => ("hermes.svg", 0.72),
        "devin" => ("devin.svg", 0.88),
        _ => ("codex-glyph.svg", 1.),
    };
    svg()
        .path(path)
        .size(px(side * scale))
        .text_color(gpui::white())
        .flex_shrink_0()
        .into_any_element()
}

pub(crate) fn provider_icon(provider: &str, side: f32) -> Div {
    let (path, monochrome, scale) = match provider {
        "claude" => ("claude.svg", false, 1.),
        "cursor" => ("cursor.svg", true, 1.),
        "grok" => ("grok.svg", true, 1.),
        "opencode" => ("opencode.svg", true, 1.),
        "antigravity" => ("antigravity.svg", false, 1.),
        "pi" => ("pi.svg", true, 1.),
        "omp" => ("omp.svg", false, 1.),
        "fx" => ("fx.svg", true, 1.),
        "hermes" => ("hermes.svg", true, 0.72),
        "devin" => ("devin.svg", true, 0.88),
        _ => ("codex.svg", false, 1.),
    };
    // Like React's currentColor / CSS masks, GPUI's SVG mask follows the theme.
    // Image rendering preserves brand colors but has no inherited currentColor.
    let mark = if monochrome {
        svg()
            .path(path)
            .size(px(side * scale))
            .text_color(content(1.))
            .flex_shrink_0()
            .into_any_element()
    } else {
        img(path).size(px(side)).flex_shrink_0().into_any_element()
    };
    div()
        .size(px(side))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(mark)
}
pub(crate) fn icon_button(
    id: &'static str,
    label: &'static str,
    path: &'static str,
) -> Stateful<Div> {
    icon_button_hover(id, label, path, 0.1)
}
pub(crate) fn icon_button_hover(
    id: &'static str,
    label: &'static str,
    path: &'static str,
    hover_alpha: f32,
) -> Stateful<Div> {
    chrome_button(id, label, path, true, hover_alpha)
}
pub(crate) fn icon_button_enabled(
    id: &'static str,
    label: &'static str,
    path: &'static str,
    enabled: bool,
) -> Stateful<Div> {
    chrome_button(id, label, path, enabled, 0.1)
}
fn chrome_button(
    id: &'static str,
    label: &'static str,
    path: &'static str,
    enabled: bool,
    hover_alpha: f32,
) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .group("chrome-button")
        .cursor_pointer()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(24.))
        .rounded_md()
        .text_color(content(if enabled { 0.5 } else { 0.25 }))
        .when(enabled, |d| {
            d.hover(|s| s.bg(content(hover_alpha)).text_color(content(1.)))
        })
        .child(
            icon(path, 14.)
                .text_color(content(if enabled { 0.5 } else { 0.25 }))
                .when(enabled, |i| {
                    i.group_hover("chrome-button", |s| s.text_color(content(1.)))
                }),
        )
}
// `.primary-action` paints with literal white and black, not theme ink.
pub(crate) fn white(alpha: f32) -> Hsla {
    gpui::hsla(0., 0., 1., alpha)
}
pub(crate) fn black(alpha: f32) -> Hsla {
    gpui::hsla(0., 0., 0., alpha)
}

const SYNTAX_KEYS: [&str; 11] = [
    "keyword", "callable", "string", "type", "tag", "number", "comment", "property", "meta",
    "invalid", "heading",
];
fn syntax_palette(theme: &str, light: bool) -> [u32; 11] {
    static PALETTES: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let palettes = PALETTES.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/editor-palettes.json"))
            .expect("bundled editor palettes")
    });
    let palette = palettes.get(theme).unwrap_or(&palettes["default"]);
    let palette = &palette[if light { "light" } else { "dark" }];
    SYNTAX_KEYS.map(|key| {
        u32::from_str_radix(
            palette[key]
                .as_str()
                .unwrap_or("#ebebeb")
                .trim_start_matches('#'),
            16,
        )
        .unwrap_or(0xebebeb)
    })
}
pub(crate) fn syntax_color(kind: &str) -> Hsla {
    let name = kind
        .strip_prefix("var(--syntax-")
        .and_then(|s| s.strip_suffix(')'));
    let index = if kind == "var(--color-markdown-heading)" {
        Some(10)
    } else {
        name.and_then(|name| SYNTAX_KEYS.iter().position(|key| *key == name))
    };
    index.map_or_else(
        || content(1.),
        |index| {
            let color = TOKENS
                .with(|t| t.borrow().as_ref().map(|t| t.syntax[index]))
                .unwrap_or_else(|| syntax_palette("default", false)[index]);
            rgb(color).into()
        },
    )
}

fn diff_palette(name: &str, light: bool) -> [u32; 4] {
    match (name, light) {
        ("colorblind", false) => [0x388bfd, 0xdb6d28, 0x79c0ff, 0xffa657],
        ("colorblind", true) => [0x0969da, 0xbc4c00, 0x0550ae, 0x953800],
        ("high-contrast", false) => [0x58a6ff, 0xf0883e, 0xcae8ff, 0xffdfb6],
        ("high-contrast", true) => [0x0550ae, 0x953800, 0x032563, 0x471700],
        (_, true) => [0x10b981, 0xf43f5e, 0x047857, 0xbe123c],
        _ => [0x10b981, 0xf43f5e, 0x6ee7b7, 0xfda4af],
    }
}
pub(crate) fn diff_foreground(add: bool) -> Hsla {
    let index = if add { 2 } else { 3 };
    let color = TOKENS
        .with(|t| t.borrow().as_ref().map(|t| t.diff[index]))
        .unwrap_or_else(|| diff_palette("default", false)[index]);
    rgb(color).into()
}
pub(crate) fn diff_background(add: bool, gutter: bool) -> Hsla {
    let index = if add { 0 } else { 1 };
    let (color, strong) = TOKENS
        .with(|t| t.borrow().as_ref().map(|t| (t.diff[index], t.diff_strong)))
        .unwrap_or((diff_palette("default", false)[index], false));
    let mut color: Hsla = rgb(color).into();
    color.a = match (gutter, strong) {
        (true, true) => 0.45,
        (true, false) => 0.25,
        (false, true) => 0.28,
        _ => 0.15,
    };
    color
}

#[cfg(test)]
mod composer_tests {
    use super::*;

    #[test]
    fn composer_surface_matches_react_dark_ink_and_solid_light_background() {
        struct Restore(Option<Tokens>);
        impl Drop for Restore {
            fn drop(&mut self) {
                TOKENS.with(|tokens| *tokens.borrow_mut() = self.0);
            }
        }
        let _restore = Restore(TOKENS.with(|tokens| *tokens.borrow()));
        let mut preferences = crate::settings::Preferences::default();
        // MonoCode's own presets keep its frosted composer.
        preferences.apply_workspace_theme("default".into());
        configure(&preferences, false);
        assert!(!solar());
        assert_eq!(primary_action(true), None);
        assert_eq!(composer_fill(), content(0.03));
        assert_eq!(composer_blur(), 24.);
        assert_eq!(composer_border(false), content(0.1));
        assert_eq!(composer_border(true), content(0.2));

        preferences.theme = "light".into();
        preferences.theme_hue = 180;
        preferences.theme_saturation = 20;
        configure(&preferences, false);
        assert_eq!(composer_fill(), gpui::hsla(0.5, 0.2, 0.97, 1.));
        assert_eq!(composer_blur(), 0.);
        assert_eq!(composer_border(false), content(0.1));
        assert_eq!(composer_border(true), content(0.2));
    }

    #[test]
    fn solar_is_the_default_continuous_black_with_blue_kept_for_sending() {
        struct Restore(Option<Tokens>);
        impl Drop for Restore {
            fn drop(&mut self) {
                TOKENS.with(|tokens| *tokens.borrow_mut() = self.0);
            }
        }
        let _restore = Restore(TOKENS.with(|tokens| *tokens.borrow()));
        let mut preferences = crate::settings::Preferences::default();
        assert_eq!(preferences.workspace_theme, "solar");
        configure(&preferences, false);
        assert!(solar());
        // One opaque black for the sidebar, the pane and its inset card.
        let black: Hsla = rgb(0x000000).into();
        assert_eq!(background_base(), black);
        assert_eq!(pane_fill().l, 0.);
        assert_eq!(pane_fill().a, 1.);
        assert_eq!(inset_pane_fill().l, 0.);
        // Icy ink; graphite composer and menus, without frost.
        assert_eq!(content(1.), rgb(0xeaf2ff).into());
        assert_eq!(composer_fill(), rgb(0x171717).into());
        assert_eq!(floating_fill(), rgb(0x171717).into());
        assert_eq!(composer_blur(), 0.);
        assert_eq!(composer_border(true), white(0.45));
        // Solar Blue with black ink once the send button can send.
        assert_eq!(accent_color(), rgb(0xa9c9ff).into());
        assert_eq!(primary_action(true), Some((rgb(0xa9c9ff).into(), black)));
        let (rest, _) = primary_action(false).unwrap();
        assert_eq!(rest, rgb(0x171717).into());
        // A picked accent still wins, as in MonoCode.
        preferences.accent_color = Some("#ef4444".into());
        configure(&preferences, false);
        assert_eq!(primary_action(true).unwrap().0, rgb(0xef4444).into());
        // Light mode has no Solar variant yet.
        preferences.theme = "light".into();
        configure(&preferences, false);
        assert!(!solar());
    }

    #[test]
    fn solar_faded_ink_is_neutral_grey() {
        let mut preferences = crate::settings::Preferences::default();
        preferences.sidebar_opacity = 100;
        configure(&preferences, false);
        // Full-strength text keeps the icy white.
        assert_eq!(content(1.), rgb(0xeaf2ff).into());
        // Every faded ink is pure white faded: neutral on any grey.
        for alpha in [0.04, 0.1, 0.35, 0.5, 0.7, 0.9] {
            let ink = content(alpha).to_rgb();
            assert!(ink.r == ink.g && ink.g == ink.b && ink.r == 1., "{alpha}");
            assert_eq!(ink.a, alpha);
        }
        let rail = rail_color().to_rgb();
        assert!(rail.r == rail.g && rail.g == rail.b);
        // On Solar glass the lift still applies, still neutral.
        preferences.sidebar_opacity = 35;
        configure(&preferences, false);
        let ink = content(0.5).to_rgb();
        let lifted = if GLASS_PLATFORM { 0.6 } else { 0.5 };
        assert!((ink.a - lifted).abs() < 1e-6 && ink.r == ink.b);
    }

    #[test]
    fn solar_glass_is_on_by_default_and_never_milky() {
        let preferences = crate::settings::Preferences::default();
        assert_eq!(preferences.sidebar_opacity, 35);
        assert!(preferences.body_glass);
        // Solar keeps at least 75% of its black over the desktop; the
        // slider's range maps onto 75–100%, so 100% is still opaque.
        assert_eq!(glass_opacity(15, true), SOLAR_GLASS_FLOOR);
        assert_eq!(glass_opacity(0, true), SOLAR_GLASS_FLOOR);
        assert_eq!(glass_opacity(100, true), 1.);
        let default = glass_opacity(preferences.sidebar_opacity, true);
        assert!(default > 0.8 && default < 0.82, "{default}");
        // Other palettes keep MonoCode's plain mapping.
        assert_eq!(glass_opacity(35, false), 0.35);
        // Secondary ink is lifted a fifth of the way on Solar glass; fills,
        // hairlines and full ink are not.
        assert!((glass_ink_alpha(0.5) - 0.6).abs() < 1e-6);
        assert!((glass_ink_alpha(0.3) - 0.44).abs() < 1e-6);
        assert_eq!(glass_ink_alpha(0.1), 0.1);
        assert_eq!(glass_ink_alpha(1.), 1.);
    }

    #[test]
    fn project_colors_replace_the_app_colors_and_blend_in() {
        let preferences = crate::settings::Preferences::default();
        configure(&preferences, false);
        let app_accent = accent_color();
        let theme = solar_core::project_appearance::ProjectTheme {
            accent_color: Some("#ef4444".into()),
            hue: 20,
            saturation: 40,
            ..Default::default()
        };
        assert!(configure_project(&preferences, Some(&theme), false, true));
        assert!(tweening());
        // The blend starts from the colors on screen.
        assert_eq!(accent_color(), app_accent);
        std::thread::sleep(TWEEN_LENGTH);
        configure_project(&preferences, Some(&theme), false, true);
        assert!(!tweening());
        assert_eq!(accent_color(), rgb(0xef4444).into());
        assert_eq!(picked_accent(), rgb(0xef4444).into());
        assert_eq!(shell_fill().h, 20. / 360.);
        // Without animation the app colors return at once.
        assert!(configure_project(&preferences, None, false, false));
        assert!(!tweening());
        assert_eq!(accent_color(), app_accent);
    }
}
