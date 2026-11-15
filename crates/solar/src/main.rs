use gpui_kit::component::TitleBar;
use gpui_kit::*;

mod assets;
mod eclipse;
mod session_view;
mod shell;
mod theme;

rust_i18n::i18n!("locales", fallback = "en");

actions!(solar, [Quit]);

fn main() {
    // `SOLAR_LANG` force la langue ; sinon celle du système.
    let locale = std::env::var("SOLAR_LANG")
        .ok()
        .or_else(sys_locale::get_locale)
        .unwrap_or_default();
    rust_i18n::set_locale(if locale.starts_with("fr") { "fr" } else { "en" });

    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            theme::apply(cx);
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.bind_keys([
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("cmd-n", shell::NewConversation, None),
                KeyBinding::new("cmd-b", shell::ToggleSidebar, None),
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
