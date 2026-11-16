//! La vue Preview d'une mission : la page du projet web, dans une webview
//! (WKWebView sur macOS). L'adresse vient d'un serveur que Claude a déjà lancé,
//! ou d'un serveur que SOLAR lance à la demande et arrête lui-même ; un serveur
//! lancé par quelqu'un d'autre n'est jamais arrêté. Aucune console : un état en
//! clair, et les dernières lignes du serveur en cas d'échec.

use std::collections::VecDeque;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use gpui_webview::WebView;
use rust_i18n::t;
use solar_core::preview::{Server, ServerEvent, dev_command, find_url};

use super::{Item, SessionView};

/// Injecté dans chaque page : Échap que la page n'a pas déjà traité prévient SOLAR.
const LEAVE_ON_ESCAPE: &str = "window.addEventListener('keydown', function (event) { \
    if (event.key === 'Escape' && !event.defaultPrevented) { window.ipc.postMessage('solar:escape'); } });";

/// Lignes du serveur gardées pour expliquer un échec.
const KEPT_LINES: usize = 40;

#[derive(Clone, Copy, Default, PartialEq)]
pub(super) enum PreviewState {
    #[default]
    Idle,
    Starting,
    Ready,
    Failed,
    Stopped,
}

#[derive(Default)]
pub(super) struct Preview {
    pub state: PreviewState,
    pub url: Option<String>,
    /// Le serveur que SOLAR a lancé ; le lâcher l'arrête.
    pub server: Option<Server>,
    /// SOLAR a demandé l'arrêt : la fin du serveur n'est pas un échec.
    pub stopping: bool,
    pub lines: VecDeque<String>,
    pub webview: Option<Entity<WebView>>,
    /// L'adresse saisie à la main, pour un projet sans serveur détecté.
    pub address: Option<Entity<InputState>>,
    /// Un menu de la vue est ouvert : la page reste cachée jusqu'à sa fermeture,
    /// même si la fenêtre redemande de l'afficher entre-temps.
    pub menu_open: bool,
}

/// La dernière adresse locale qu'une commande de Claude a fait apparaître.
fn url_from_activity(items: &[Item]) -> Option<String> {
    items.iter().rev().find_map(|item| match item {
        Item::Tool {
            name,
            result: Some((_, output)),
            ..
        } if name == "Bash" || name == "BashOutput" => find_url(output),
        _ => None,
    })
}

impl SessionView {
    pub(super) fn open_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pane = super::workspace::Pane::Preview;
        if self.preview.address.is_none() {
            let address =
                cx.new(|cx| InputState::new(window, cx).placeholder(t!("preview.address")));
            cx.subscribe_in(
                &address,
                window,
                |this, address, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        let url = address.read(cx).value().trim().to_string();
                        if !url.is_empty() {
                            this.show_page(url, window, cx);
                        }
                    }
                },
            )
            .detach();
            self.preview.address = Some(address);
        }
        if self.preview.url.is_none()
            && self.preview.server.is_none()
            && let Some(url) = url_from_activity(&self.items)
        {
            self.show_page(url, window, cx);
        }
        self.set_preview_visible(true, cx);
        cx.notify();
    }

    /// Ouvre une adresse dans la webview, créée au premier usage.
    fn show_page(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        self.preview.state = PreviewState::Ready;
        match &self.preview.webview {
            Some(webview) => webview.update(cx, |webview, _| webview.load_url(&url)),
            None => {
                // La page garde le clavier : Échap, s'il n'est pas déjà pris par
                // elle, rend la main à SOLAR.
                let (tx, rx) = smol::channel::unbounded::<()>();
                let builder = wry::WebViewBuilder::new()
                    .with_url(&url)
                    .with_initialization_script(LEAVE_ON_ESCAPE)
                    .with_ipc_handler(move |request| {
                        if request.body() == "solar:escape" {
                            let _ = tx.try_send(());
                        }
                    });
                cx.spawn_in(window, async move |this, cx| {
                    while rx.recv().await.is_ok() {
                        if this
                            .update_in(cx, |this, window, cx| this.leave_page(window, cx))
                            .is_err()
                        {
                            break;
                        }
                    }
                })
                .detach();
                match WebView::build(builder, window, cx) {
                    Ok(webview) => self.preview.webview = Some(cx.new(|_| webview)),
                    Err(error) => {
                        self.preview.state = PreviewState::Failed;
                        self.preview.lines.push_back(error.to_string());
                    }
                }
            }
        }
        self.preview.url = Some(url);
        self.set_preview_visible(true, cx);
        // Le bouton « Lancer » vient de disparaître avec son focus : le clavier
        // reste dans SOLAR, au composer, tant qu'on ne clique pas dans la page.
        self.input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// Un menu passerait sous la page, vue native : elle est cachée tant qu'il
    /// est ouvert.
    pub(super) fn page_under_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
        let view = cx.entity().downgrade();
        move |open, _, cx| {
            let _ = view.update(cx, |this, cx| {
                this.preview.menu_open = *open;
                this.set_preview_visible(true, cx);
            });
        }
    }

    /// Échap dans la page : le focus revient à SOLAR, dans le composer.
    fn leave_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(webview) = &self.preview.webview {
            let _ = webview.read(cx).focus_parent();
        }
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    /// La webview est une vue native posée sur la fenêtre : elle recouvre tout
    /// et ne se cache que si on le lui dit. La fenêtre l'appelle à chaque
    /// changement d'onglet ou d'écran ; la vue, à chaque changement de vue.
    pub fn set_preview_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        let show =
            visible && self.pane == super::workspace::Pane::Preview && !self.preview.menu_open;
        if let Some(webview) = &self.preview.webview {
            webview.update(cx, |webview, _| {
                if show {
                    webview.show();
                } else {
                    webview.hide();
                }
            });
        }
    }

    fn start_server(&mut self, command: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
        let (tx, rx) = smol::channel::unbounded();
        match Server::start(&self.workdir(), &command, move |event| {
            let _ = tx.send_blocking(event);
        }) {
            Ok(server) => {
                self.preview.server = Some(server);
                self.preview.state = PreviewState::Starting;
                self.preview.stopping = false;
                self.preview.lines.clear();
            }
            Err(error) => {
                self.preview.state = PreviewState::Failed;
                self.preview.lines = [error.to_string()].into();
                cx.notify();
                return;
            }
        }
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = rx.recv().await {
                let alive =
                    this.update_in(cx, |this, window, cx| this.on_server(event, window, cx));
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn on_server(&mut self, event: ServerEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            ServerEvent::Line(line) => {
                if self.preview.state == PreviewState::Starting
                    && let Some(url) = find_url(&line)
                {
                    self.show_page(url, window, cx);
                }
                self.preview.lines.push_back(line);
                if self.preview.lines.len() > KEPT_LINES {
                    self.preview.lines.pop_front();
                }
            }
            ServerEvent::Exited => {
                self.preview.server = None;
                self.preview.state = if self.preview.stopping {
                    PreviewState::Stopped
                } else {
                    PreviewState::Failed
                };
                // La page d'un serveur arrêté n'est plus le résultat de la mission.
                self.preview.webview = None;
                self.preview.url = None;
            }
        }
        cx.notify();
    }

    fn stop_server(&mut self, cx: &mut Context<Self>) {
        if let Some(server) = self.preview.server.as_mut() {
            self.preview.stopping = true;
            server.stop();
        }
        cx.notify();
    }

    pub(super) fn render_preview_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (muted, border, danger) = (theme.muted_foreground, theme.border, theme.danger);
        let note = |text: String| div().text_color(muted).child(text);
        let preview = &self.preview;
        let command = dev_command(&self.workdir());
        let ours = preview.server.is_some();

        let state = match preview.state {
            PreviewState::Starting => t!("preview.state.starting").to_string(),
            PreviewState::Ready => match &preview.url {
                Some(url) if ours => t!("preview.state.ours", url = url).to_string(),
                Some(url) => t!("preview.state.theirs", url = url).to_string(),
                None => String::new(),
            },
            PreviewState::Failed => t!("preview.state.failed").to_string(),
            PreviewState::Stopped => t!("preview.state.stopped").to_string(),
            PreviewState::Idle => String::new(),
        };
        // Sans état ni page, pas de bandeau vide au-dessus de l'explication.
        let has_header = !state.is_empty() || preview.webview.is_some() || ours;
        let header = h_flex()
            .flex_shrink_0()
            .px_6()
            .py_3()
            .gap_2()
            .border_b_1()
            .border_color(border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(if preview.state == PreviewState::Failed {
                        danger
                    } else {
                        muted
                    })
                    .child(state),
            )
            .when(preview.webview.is_some(), |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(muted)
                        .child(t!("preview.escape").to_string()),
                )
                .child(
                    Button::new("preview-reload")
                        .ghost()
                        .small()
                        .icon(IconName::RefreshCw)
                        .label(t!("preview.reload").to_string())
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let (Some(webview), Some(url)) =
                                (&this.preview.webview, this.preview.url.clone())
                            {
                                webview.update(cx, |webview, _| webview.load_url(&url));
                            }
                        })),
                )
            })
            .when(ours, |row| {
                row.child(
                    Button::new("preview-stop")
                        .ghost()
                        .small()
                        .icon(IconName::Square)
                        .label(t!("preview.stop").to_string())
                        .on_click(cx.listener(|this, _, _, cx| this.stop_server(cx))),
                )
            });

        let launch = command.clone().map(|command| {
            let shown = command.join(" ");
            Button::new("preview-launch")
                .primary()
                .icon(IconName::Play)
                .label(t!("preview.launch", command = shown).to_string())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.start_server(command.clone(), window, cx)
                }))
        });
        let address = preview.address.as_ref().map(|address| {
            v_flex()
                .gap_2()
                .child(note(t!("preview.address.hint").to_string()))
                .child(div().w(px(420.)).child(Input::new(address)))
        });

        let body = match (&preview.webview, preview.state) {
            (Some(webview), _) => div()
                .flex_1()
                .min_h_0()
                .child(webview.clone())
                .into_any_element(),
            (None, PreviewState::Starting) => note(t!("preview.starting").to_string())
                .p_6()
                .into_any_element(),
            (None, PreviewState::Failed) => {
                let lines = preview.lines.iter().cloned().collect::<Vec<_>>().join("\n");
                v_flex()
                    .id("preview-failure")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_6()
                    .gap_4()
                    .child(note(t!("preview.failed").to_string()))
                    .when(!lines.is_empty(), |column| {
                        column.child(
                            TextView::markdown("preview-lines", format!("```\n{lines}\n```"))
                                .selectable(true),
                        )
                    })
                    .children(launch)
                    .into_any_element()
            }
            (None, _) => v_flex()
                .p_6()
                .gap_6()
                .child(note(
                    if command.is_some() {
                        t!("preview.ready_to_launch")
                    } else {
                        t!("preview.unavailable")
                    }
                    .to_string(),
                ))
                .children(launch)
                .children(address)
                .into_any_element(),
        };

        v_flex()
            .flex_1()
            .min_h_0()
            .when(has_header, |column| column.child(header))
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use core::prelude::v1::test;

    use serde_json::json;

    use super::*;

    #[test]
    fn the_address_comes_from_the_latest_command_output() {
        let bash = |output: &str| Item::Tool {
            id: String::new(),
            name: "Bash".into(),
            input: json!({"command": "npm run dev &"}),
            result: Some((false, output.into())),
            finished: None,
            open: false,
        };
        assert_eq!(url_from_activity(&[bash("compiled")]), None);
        assert_eq!(
            url_from_activity(&[
                bash("Local: http://localhost:3000/"),
                bash("Local: http://localhost:5173/")
            ]),
            Some("http://localhost:5173/".into())
        );
    }
}
