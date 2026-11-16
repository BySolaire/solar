//! Les tokens de Solar, posés sur le thème sombre de gpui-kit. Couleurs et
//! boutons suivent le design system « SpaceX » d'open-design (voir
//! `docs/design/references/spacex-open-design.md`), la géométrie et la police
//! restent celles retenues d'après Starlink. Toute couleur de l'app vient d'ici
//! ou du thème.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

pub const BLACK: u32 = 0x000000;
/// Blanc « spectral », légèrement bleuté : tout le texte et ce qui est actif.
const SPECTRAL: u32 = 0xF0F0FA;
/// Blanc pur au survol d'un bouton principal, gris spectral quand on l'enfonce.
const HOVER: u32 = 0xFFFFFF;
const PRESSED: u32 = 0xD8D8E6;
const GREEN: u32 = 0x16A34A;
/// Couleur de marque de Claude (selon simple-icons), pour son seul logo : les
/// logos des fournisseurs gardent leur couleur, le reste de l'app est noir et blanc.
pub const CLAUDE_BRAND: u32 = 0xD97757;
const RED: u32 = 0xDC2626;
const YELLOW: u32 = 0xEAB308;

/// Opacité du texte secondaire.
const SECONDARY: f32 = 0.7;
/// Surface « fantôme » des boutons et des sélections, et filets de séparation.
const GHOST: f32 = 0.1;
/// Contour fantôme : ce qui doit se lire comme un contrôle (amorces, champ).
const GHOST_BORDER: f32 = 0.35;

/// Police de l'interface : Space Grotesk, recommandée par le kit d'identité,
/// embarquée depuis `assets/fonts/space-grotesk` (licence OFL 1.1).
pub const DISPLAY_FONT: &str = "Space Grotesk";
/// Police de lecture, pour les messages et la saisie : celle du système.
pub const READING_FONT: &str = ".SystemUIFont";
/// Le seul rayon de Solar, partout, d'après Monocode Native. Un élément posé dans
/// un autre prend ce rayon moins la marge qui les sépare, pour que les courbes
/// restent parallèles (gpui-kit en déduit 8 pour les lignes de menu).
pub const RADIUS: Pixels = px(12.);
/// Le rayon d'un élément posé dans un cadre : lignes de menu, boutons du
/// composer (un petit bouton carré à 12 deviendrait rond).
pub const INNER_RADIUS: Pixels = px(8.);

/// Opacité de la teinte noire posée sur le flou de la fenêtre.
const GLASS: f32 = 0.92;
/// Voile noir sous l'accueil, la conversation et la barre latérale.
const DEPTH: f32 = 0.55;
/// En verre dépoli, les deux teintes s'effacent : environ 60 % de noir au total
/// (contre 96 %), assez pour que le texte reste lisible sur un bureau clair.
const FROSTED_GLASS: f32 = 0.4;
const FROSTED_DEPTH: f32 = 0.3;

/// Le mode verre dépoli, réglé dans les réglages.
#[derive(Default)]
pub struct Frosted(pub bool);

impl Global for Frosted {}

/// Le voile sous l'accueil et la conversation, selon le mode.
pub fn depth(cx: &App) -> f32 {
    match cx.try_global::<Frosted>() {
        Some(Frosted(true)) => FROSTED_DEPTH,
        _ => DEPTH,
    }
}

/// Passe la fenêtre en verre dépoli, ou revient au noir.
pub fn set_frosted(frosted: bool, cx: &mut App) {
    cx.set_global(Frosted(frosted));
    let (glass, depth) = if frosted {
        (FROSTED_GLASS, FROSTED_DEPTH)
    } else {
        (GLASS, DEPTH)
    };
    Theme::update(cx, |theme| {
        let black: Hsla = rgb(BLACK).into();
        theme.colors.background = black.opacity(glass);
        theme.colors.sidebar = black.opacity(depth);
    });
    cx.refresh_windows();
}
/// Voile noir sous le composer, qui passe sur la planète de l'accueil : le texte
/// posé sur une image garde un fond sombre.
pub const VEIL: f32 = 0.8;

/// Petit intitulé en capitales : titres de section et noms d'état. Le reste de
/// l'interface garde sa casse.
pub fn caps(text: &str) -> String {
    text.to_uppercase()
}

pub fn apply(cx: &mut App) {
    let fonts = [
        include_bytes!("../../../assets/fonts/space-grotesk/SpaceGrotesk-Regular.otf").as_slice(),
        include_bytes!("../../../assets/fonts/space-grotesk/SpaceGrotesk-Medium.otf").as_slice(),
        include_bytes!("../../../assets/fonts/space-grotesk/SpaceGrotesk-Bold.otf").as_slice(),
    ];
    // Sans la police, l'app reste utilisable dans celle du système.
    if let Err(error) = cx
        .text_system()
        .add_fonts(fonts.map(std::borrow::Cow::Borrowed).to_vec())
    {
        eprintln!("solar: police Space Grotesk non chargée : {error}");
    }
    Theme::change(ThemeMode::Dark, None, cx);
    Theme::update(cx, |theme| {
        let white: Hsla = rgb(SPECTRAL).into();
        let black: Hsla = rgb(BLACK).into();
        let ghost = white.opacity(GHOST);
        let clear = black.opacity(0.0);

        theme.radius = RADIUS;
        theme.radius_lg = RADIUS;
        theme.font_family = DISPLAY_FONT.into();
        let c = &mut theme.colors;

        c.background = black.opacity(GLASS);
        c.foreground = white;
        c.muted = ghost;
        c.muted_foreground = white.opacity(SECONDARY);
        c.border = ghost;
        c.input = white.opacity(GHOST_BORDER);

        // Pas de couleur d'accent : ce qui est actif ou principal est blanc.
        c.primary = white;
        c.primary_hover = rgb(HOVER).into();
        c.primary_active = rgb(PRESSED).into();
        c.primary_foreground = black;
        c.button_primary = white;
        c.button_primary_hover = rgb(HOVER).into();
        c.button_primary_active = rgb(PRESSED).into();
        c.button_primary_foreground = black;
        // Boutons fantômes : un voile blanc à peine visible, sans ombre.
        c.button_secondary = ghost;
        c.button_secondary_hover = white.opacity(0.18);
        c.button_secondary_active = white.opacity(0.24);
        c.button_secondary_foreground = white;
        c.ring = white;
        c.selection = white.opacity(0.25);

        c.success = rgb(GREEN).into();
        c.danger = rgb(RED).into();
        c.warning = rgb(YELLOW).into();

        // Aucune carte : le composer et les listes reposent directement sur le noir.
        c.secondary = clear;
        c.secondary_hover = ghost;
        c.secondary_foreground = white;
        c.accent = ghost;
        c.accent_foreground = white;
        c.list_hover = ghost;
        // Les menus flottants restent opaques : ils passent au-dessus du texte.
        c.popover = rgb(0x0A0A0A).into();

        c.title_bar = clear;
        c.title_bar_border = ghost;
        c.tab_bar = clear;
        c.tab = clear;
        c.tab_foreground = white.opacity(SECONDARY);
        c.tab_active = clear;
        c.tab_active_foreground = white;
        c.sidebar = black.opacity(DEPTH);
        c.sidebar_foreground = white.opacity(SECONDARY);
        c.sidebar_border = ghost;
        c.sidebar_accent = clear;
        c.sidebar_accent_foreground = white;
    });
}
