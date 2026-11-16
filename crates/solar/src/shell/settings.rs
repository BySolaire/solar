//! Les réglages (⌘,) : ce que reçoit chaque nouvelle conversation, la langue,
//! et l'état réel de chaque agent, relu à chaque ouverture.

use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::*;
use rust_i18n::t;
use solar_core::agent::{Health, Provider};
use solar_core::state::Settings;

use super::Shell;
use crate::session_view::{MODES, SessionView};
use crate::theme::caps;

/// Ce qu'un menu de réglage propose : la valeur notée (vide : par défaut) et
/// son libellé.
type Options = Vec<(String, String)>;

impl Shell {
    pub(super) fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.importing = None;
        self.card = None;
        self.settings_open = true;
        self.take_focus = true;
        // L'état des agents est relu à chaque ouverture : il a pu changer.
        self.health.clear();
        for provider in Provider::ALL {
            cx.spawn(async move |this, cx| {
                let health = cx.background_spawn(async move { provider.health() }).await;
                let _ = this.update(cx, |this, cx| {
                    this.health.insert(provider, health);
                    cx.notify();
                });
            })
            .detach();
        }
        self.sync_previews(cx);
        cx.notify();
    }

    /// Les réglages d'un brouillon neuf : agent, mode, et pour Claude, modèle
    /// et effort.
    pub(super) fn apply_defaults(
        settings: &Settings,
        view: &mut SessionView,
        window: &mut Window,
        cx: &mut Context<SessionView>,
    ) {
        view.set_provider(settings.provider, window, cx);
        let mode = settings
            .mode
            .as_deref()
            .and_then(crate::session_view::known_mode)
            .filter(|mode| settings.provider.supports(mode));
        if let Some(mode) = mode {
            view.set_mode(mode, cx);
        }
        if settings.provider == Provider::Claude {
            if let Some(model) = &settings.claude_model {
                view.set_model(model.clone(), cx);
            }
            if let Some(effort) = &settings.claude_effort {
                view.set_effort(effort.clone(), cx);
            }
        }
    }

    /// Note un réglage, et l'applique au brouillon de l'accueil s'il n'a pas
    /// commencé.
    pub(super) fn change(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        set: impl FnOnce(&mut Settings),
    ) {
        set(&mut self.state.settings);
        if let Err(error) = self.state.save(&self.state_path) {
            eprintln!("solar: réglages non sauvegardés : {error}");
        }
        let settings = self.state.settings.clone();
        self.home.update(cx, |home, cx| {
            if !home.started() {
                Self::apply_defaults(&settings, home, window, cx);
            }
        });
        cx.notify();
    }

    fn set_language(&mut self, language: String, window: &mut Window, cx: &mut Context<Self>) {
        self.change(window, cx, |settings| {
            settings.language = (!language.is_empty()).then_some(language)
        });
        crate::set_language(self.state.settings.language.as_deref());
        // Le brouillon neuf reprend ses textes dans la langue choisie.
        if !self.home.read(cx).started() {
            let project = self.home.read(cx).project.clone();
            let settings = self.state.settings.clone();
            self.home = Self::draft(project, &settings, window, cx);
        }
        cx.refresh_windows();
    }

    /// Les modèles annoncés par un Claude lancé (accueil ou onglet).
    fn claude_models(&self, cx: &App) -> Vec<solar_core::claude::Model> {
        std::iter::once(&self.home)
            .chain(&self.tabs)
            .map(|view| view.read(cx).models())
            .find(|models| !models.is_empty())
            .map(<[_]>::to_vec)
            .unwrap_or_default()
    }

    fn picker(
        &self,
        id: &'static str,
        options: Options,
        current: &str,
        pick: fn(&mut Shell, String, &mut Window, &mut Context<Shell>),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let label = options
            .iter()
            .find(|(value, _)| value == current)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| current.to_string());
        let (shell, current) = (cx.entity(), current.to_string());
        Button::new(id)
            .outline()
            .small()
            .label(label)
            .dropdown_menu(move |menu, _, _| {
                options.iter().fold(menu, |menu, (value, name)| {
                    let (shell, value) = (shell.clone(), value.clone());
                    menu.item(
                        PopupMenuItem::new(name.clone())
                            .checked(*value == current)
                            .on_click(move |_, window, cx| {
                                shell.update(cx, |shell, cx| pick(shell, value.clone(), window, cx))
                            }),
                    )
                })
            })
    }

    fn health_line(&self, provider: Provider) -> String {
        match self.health.get(&provider) {
            None => t!("settings.checking").to_string(),
            Some(Health { version: None, .. }) => t!("settings.missing").to_string(),
            Some(health) => {
                let mut parts = vec![health.version.clone().unwrap_or_default()];
                parts.push(match health.signed_in {
                    Some(true) => t!("settings.signed_in").to_string(),
                    Some(false) => {
                        t!("settings.signed_out", command = provider.login_command()).to_string()
                    }
                    None => t!("settings.sign_in_unknown").to_string(),
                });
                if let Some(model) = &health.model {
                    parts.push(t!("settings.model_in_use", model = model).to_string());
                }
                parts.join(" · ")
            }
        }
    }

    pub(super) fn render_settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (muted, border) = (theme.muted_foreground, theme.border);
        let settings = &self.state.settings;
        let section = |title: String| {
            div()
                .pt_6()
                .pb_2()
                .text_xs()
                .text_color(muted)
                .child(caps(&title))
        };
        let row = |label: String, note: Option<String>, control: AnyElement| {
            h_flex()
                .py_3()
                .gap_6()
                .border_b_1()
                .border_color(border)
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(div().font_weight(FontWeight::BOLD).child(label))
                        .children(note.map(|note| div().text_sm().text_color(muted).child(note))),
                )
                .child(control)
        };

        let agents: Options = Provider::ALL
            .into_iter()
            .map(|provider| (provider.program().to_string(), provider.name().to_string()))
            .collect();
        let modes: Options = MODES
            .into_iter()
            .filter(|mode| settings.provider.supports(mode))
            .map(|mode| (mode.to_string(), t!(format!("mode.{mode}")).to_string()))
            .collect();
        let languages: Options = vec![
            (String::new(), t!("settings.language.system").to_string()),
            ("fr".into(), "Français".into()),
            ("en".into(), "English".into()),
        ];
        let models = self.claude_models(cx);
        let claude_models: Options =
            std::iter::once((String::new(), t!("settings.claude_default").to_string()))
                .chain(
                    models
                        .iter()
                        .map(|model| (model.value.clone(), model.name.clone())),
                )
                .collect();
        let chosen = settings.claude_model.clone().unwrap_or_default();
        let efforts: Options =
            std::iter::once((String::new(), t!("settings.claude_default").to_string()))
                .chain(
                    models
                        .iter()
                        .find(|model| {
                            model.value == chosen || (chosen.is_empty() && model.value == "default")
                        })
                        .map(|model| model.efforts.clone())
                        .unwrap_or_default()
                        .into_iter()
                        .map(|effort| (effort.clone(), effort)),
                )
                .collect();

        let agent_section = |provider: Provider| {
            v_flex()
                .child(section(provider.name().to_string()))
                .child(row(
                    t!("settings.state").to_string(),
                    Some(self.health_line(provider)),
                    div().into_any_element(),
                ))
        };

        div().id("settings").size_full().overflow_y_scroll().child(
            v_flex()
                .w_full()
                .max_w(px(680.))
                .mx_auto()
                .px_8()
                .py_8()
                .child(
                    div()
                        .text_2xl()
                        .font_weight(FontWeight::BOLD)
                        .child(t!("settings.title").to_string()),
                )
                .child(section(t!("settings.new_conversations").to_string()))
                .child(row(
                    t!("settings.agent").to_string(),
                    Some(t!("settings.agent.note").to_string()),
                    self.picker(
                        "settings-agent",
                        agents,
                        settings.provider.program(),
                        |shell, value, window, cx| {
                            let provider = Provider::ALL
                                .into_iter()
                                .find(|provider| provider.program() == value)
                                .unwrap_or_default();
                            shell.change(window, cx, |settings| {
                                settings.provider = provider;
                                // Un mode que ce fournisseur ne tient pas est oublié.
                                if settings
                                    .mode
                                    .as_deref()
                                    .is_some_and(|mode| !provider.supports(mode))
                                {
                                    settings.mode = None;
                                }
                            })
                        },
                        cx,
                    )
                    .into_any_element(),
                ))
                .child(row(
                    t!("settings.permissions").to_string(),
                    None,
                    self.picker(
                        "settings-mode",
                        modes,
                        settings
                            .mode
                            .as_deref()
                            .and_then(crate::session_view::known_mode)
                            .unwrap_or(MODES[0]),
                        |shell, value, window, cx| {
                            shell.change(window, cx, |settings| {
                                settings.mode = (value != MODES[0]).then_some(value)
                            })
                        },
                        cx,
                    )
                    .into_any_element(),
                ))
                .child(section(t!("settings.language.section").to_string()))
                .child(row(
                    t!("settings.glass").to_string(),
                    Some(t!("settings.glass.note").to_string()),
                    {
                        let shell = cx.entity();
                        Switch::new("settings-glass")
                            .checked(settings.glass)
                            .on_click(move |on, window, cx| {
                                let on = *on;
                                shell.update(cx, |shell, cx| {
                                    shell.change(window, cx, |settings| settings.glass = on);
                                    crate::theme::set_frosted(on, cx);
                                })
                            })
                            .into_any_element()
                    },
                ))
                .child(row(
                    t!("settings.turn_line").to_string(),
                    Some(t!("settings.turn_line.note").to_string()),
                    {
                        let shell = cx.entity();
                        Switch::new("settings-turn-line")
                            .checked(settings.turn_line_above)
                            .on_click(move |on, window, cx| {
                                let on = *on;
                                shell.update(cx, |shell, cx| {
                                    shell.change(window, cx, |settings| {
                                        settings.turn_line_above = on
                                    });
                                    cx.set_global(crate::session_view::TurnLineAbove(on));
                                    cx.refresh_windows();
                                })
                            })
                            .into_any_element()
                    },
                ))
                .child(row(
                    t!("settings.queue").to_string(),
                    Some(t!("settings.queue.note").to_string()),
                    {
                        let shell = cx.entity();
                        Switch::new("settings-queue")
                            .checked(settings.queue_while_working)
                            .on_click(move |on, window, cx| {
                                let on = *on;
                                shell.update(cx, |shell, cx| {
                                    shell.change(window, cx, |settings| {
                                        settings.queue_while_working = on
                                    });
                                    cx.set_global(crate::session_view::QueueWhileWorking(on));
                                    cx.refresh_windows();
                                })
                            })
                            .into_any_element()
                    },
                ))
                .child(row(
                    t!("settings.language").to_string(),
                    None,
                    self.picker(
                        "settings-language",
                        languages,
                        settings.language.as_deref().unwrap_or_default(),
                        |shell, value, window, cx| shell.set_language(value, window, cx),
                        cx,
                    )
                    .into_any_element(),
                ))
                .child(agent_section(Provider::Claude))
                .child(row(
                    t!("settings.claude_model").to_string(),
                    models
                        .is_empty()
                        .then(|| t!("settings.claude_models_later").to_string()),
                    self.picker(
                        "settings-claude-model",
                        claude_models,
                        settings.claude_model.as_deref().unwrap_or_default(),
                        |shell, value, window, cx| {
                            shell.change(window, cx, |settings| {
                                settings.claude_model = (!value.is_empty()).then_some(value);
                                settings.claude_effort = None;
                            })
                        },
                        cx,
                    )
                    .into_any_element(),
                ))
                .child(row(
                    t!("settings.claude_effort").to_string(),
                    None,
                    self.picker(
                        "settings-claude-effort",
                        efforts,
                        settings.claude_effort.as_deref().unwrap_or_default(),
                        |shell, value, window, cx| {
                            shell.change(window, cx, |settings| {
                                settings.claude_effort = (!value.is_empty()).then_some(value)
                            })
                        },
                        cx,
                    )
                    .into_any_element(),
                ))
                .child(agent_section(Provider::Codex))
                .child(agent_section(Provider::Grok))
                .child(
                    div()
                        .pt_6()
                        .text_sm()
                        .text_color(muted)
                        .child(t!("settings.agents_note").to_string()),
                ),
        )
    }
}
