//! L'accueil : ce que montre une conversation avant son premier message. Le
//! message d'accueil et le composer, centrés, rien d'autre : les conversations
//! sont dans la barre latérale. La planète passe derrière, en travers.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Icon, Sizable, h_flex, v_flex};
use gpui_kit::*;
use rust_i18n::t;

use super::{HOME_WIDTH, SessionView};
use crate::eclipse::{PLANET_SIZE, planet};

/// Hauteur dont la planète dépasse au-dessus du message d'accueil : son limbe
/// monte à droite du message et passe sous lui.
const PLANET_RISE: Pixels = px(230.);
const PLANET_FADE: std::time::Duration = std::time::Duration::from_millis(1200);

impl SessionView {
    pub(super) fn render_home(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (muted, danger) = (cx.theme().muted_foreground, cx.theme().danger);

        let (color, status) = if self.launch_error.is_some() {
            (danger, t!("home.status.offline"))
        } else if self.project.as_os_str().is_empty() {
            (muted, t!("home.status.no_project"))
        } else if self.claude.is_some() && !self.models.is_empty() {
            (cx.theme().success, t!("home.status.online"))
        } else {
            (muted, t!("home.status.connecting"))
        };
        let greeting = match first_name(std::env::var("USER").ok().as_deref()) {
            Some(name) => t!("home.greeting", name = name),
            None => t!("home.greeting.anonymous"),
        };

        div().id("home").size_full().overflow_y_scroll().child(
            v_flex()
                .min_h_full()
                .items_center()
                .justify_center()
                .px_6()
                .py_8()
                .gap_6()
                .child(
                    // Le message d'accueil, à gauche : la planète monte en travers à sa droite.
                    v_flex()
                        .relative()
                        .w_full()
                        .max_w(HOME_WIDTH)
                        .gap_2()
                        // La planète est calée sur le message d'accueil, qu'elle suit
                        // partout ; dessinée en premier, elle reste sous le texte et
                        // sous le composer, qui porte un voile.
                        .child(
                            div()
                                .absolute()
                                .top(-PLANET_RISE)
                                .left_0()
                                .w(PLANET_SIZE.width)
                                .h(PLANET_SIZE.height)
                                .child(planet())
                                // Respecte « Réduire les animations » : GPUI saute alors l'effet.
                                .with_animation(
                                    "planet-rise",
                                    Animation::new(PLANET_FADE).with_easing(ease_out_quint()),
                                    |planet, delta| planet.opacity(delta),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(40.))
                                .font_weight(FontWeight::BOLD)
                                .line_height(relative(1.1))
                                .child(greeting.to_string()),
                        )
                        .child(
                            div()
                                .text_lg()
                                .text_color(muted)
                                .child(t!("home.title").to_string()),
                        )
                        .child(
                            h_flex()
                                .mt_1()
                                .gap_2()
                                .text_sm()
                                .font_weight(FontWeight::BOLD)
                                .child(div().size_2().rounded_full().bg(color))
                                .child(status.to_string()),
                        ),
                )
                .child(
                    v_flex()
                        .mt_8()
                        .w_full()
                        .max_w(HOME_WIDTH)
                        .gap_2()
                        .children(self.render_suggestions(cx))
                        .child(if self.project.as_os_str().is_empty() {
                            h_flex()
                                .child(
                                    Button::new("choose-folder")
                                        .primary()
                                        .icon(IconName::FolderOpen)
                                        .label(t!("home.project.none").to_string())
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.choose_folder(false, window, cx)
                                        })),
                                )
                                .into_any_element()
                        } else {
                            self.render_composer(cx).into_any_element()
                        })
                        .children(self.launch_error.clone().map(|error| {
                            h_flex()
                                .gap_3()
                                .px_1()
                                .text_xs()
                                .text_color(danger)
                                .child(Icon::new(IconName::CircleAlert).xsmall())
                                .child(div().flex_1().child(error))
                                .child(
                                    Button::new("retry")
                                        .ghost()
                                        .xsmall()
                                        .label(t!("home.retry").to_string())
                                        .on_click(cx.listener(|this, _, _, cx| this.launch(cx))),
                                )
                        })),
                ),
        )
    }
}

/// Le prénom pour le message d'accueil, tiré du nom de session : « camille »
/// donne « Camille ».
// shortcut: le nom de session n'est pas toujours le prénom (« jdupont ») ; lire
// le nom complet du compte (`id -F` sur macOS) si l'écart gêne.
fn first_name(login: Option<&str>) -> Option<String> {
    let mut letters = login?.trim().chars();
    let first = letters.next()?;
    Some(first.to_uppercase().chain(letters).collect())
}

#[cfg(test)]
mod tests {
    use core::prelude::v1::test;

    use super::first_name;

    #[test]
    fn first_name_capitalizes_the_login() {
        assert_eq!(first_name(Some("camille")).as_deref(), Some("Camille"));
        assert_eq!(first_name(Some("élodie")).as_deref(), Some("Élodie"));
        assert_eq!(first_name(Some("  ")), None);
        assert_eq!(first_name(None), None);
    }
}
