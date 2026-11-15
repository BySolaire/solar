//! La marque de SOLAR dans l'app, dessinée à partir des fichiers de `assets/logo`.

use gpui_kit::component::ActiveTheme;
use gpui_kit::*;

use crate::assets::{PLANET, WORDMARK};

/// Logo de la barre de titre : le mot SOLAR seul, comme le prévoit le kit
/// d'identité pour l'interface. Le fichier fait 1260 × 134.
pub fn brand(cx: &App) -> impl IntoElement {
    svg()
        .path(WORDMARK)
        .w(px(1260. / 134. * 10.))
        .h(px(10.))
        .text_color(cx.theme().foreground)
}

/// La planète de l'accueil : son limbe monte en travers, du bas à gauche vers le
/// haut à droite. Le fichier fait 1800 × 900 et s'efface déjà sur ses bords bas
/// et droit ; il s'affiche à sa taille, sans étirement.
pub fn planet() -> impl IntoElement {
    img(PLANET).size_full().object_fit(ObjectFit::Fill)
}

pub const PLANET_SIZE: Size<Pixels> = Size {
    width: px(900.),
    height: px(450.),
};
