use gpui_kit::component::TitleBar;
use gpui_kit::*;

mod assets;
mod eclipse;
mod session_view;
mod shell;
mod theme;

rust_i18n::i18n!("locales", fallback = "en");

actions!(solar, [Quit]);

/// La langue de l'interface : `SOLAR_LANG` d'abord (essais), puis le réglage,
/// sinon celle du système.
pub fn set_language(chosen: Option<&str>) {
    let locale = std::env::var("SOLAR_LANG")
        .ok()
        .or_else(|| chosen.map(str::to_string))
        .or_else(sys_locale::get_locale)
        .unwrap_or_default();
    rust_i18n::set_locale(if locale.starts_with("fr") { "fr" } else { "en" });
}

/// Un nombre décimal écrit à la manière de la langue : « 2,5k » en français.
pub fn decimal(number: String) -> String {
    if rust_i18n::locale().starts_with("fr") {
        number.replace('.', ",")
    } else {
        number
    }
}

fn main() {
    set_language(None);

    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            // Le nom des notifications système ; sur macOS, l'identité vient du
            // paquet `.app` (sans lui, les notifications restent éteintes).
            cx.set_app_identity("com.bysolaire.solar", "SOLAR");
            theme::apply(cx);
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.bind_keys([
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("cmd-n", shell::NewConversation, None),
                KeyBinding::new("cmd-b", shell::ToggleSidebar, None),
                KeyBinding::new("cmd-k", shell::OpenSearch, None),
                KeyBinding::new("cmd-,", shell::OpenSettings, None),
            ]);

            let options = WindowOptions {
                // `SOLAR_WINDOW=1024x768` ouvre la fenêtre à cette taille, pour les revues visuelles.
                window_bounds: window_size()
                    .map(|size| WindowBounds::Windowed(Bounds::centered(None, size, cx))),
                window_min_size: Some(size(px(720.), px(480.))),
                // Verre dépoli : ce qui est derrière la fenêtre transparaît, flouté.
                window_background: WindowBackgroundAppearance::Blurred,
                ..TitleBar::window_options()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| shell::Shell::new(window, cx))
            })
            .expect("ouverture de la fenêtre impossible");
            cx.activate(true);
        });
}

fn window_size() -> Option<Size<Pixels>> {
    let wanted = std::env::var("SOLAR_WINDOW").ok()?;
    let (width, height) = wanted.split_once('x')?;
    Some(size(px(width.parse().ok()?), px(height.parse().ok()?)))
}
