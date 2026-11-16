//! Les deux menus du composer : le modèle (avec son agent, son effort et les
//! favoris) et le mode de permission (titre et description de chaque mode).
//! Sous le composer, le projet et sa branche.

use std::collections::HashMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rust_i18n::t;
use solar_core::agent::Provider;
use solar_core::claude::Model;
use solar_core::search::Query;

use super::SessionView;
use crate::assets::CLAUDE_LOGO;
use crate::theme::CLAUDE_BRAND;

/// Les modèles de chaque fournisseur, connus de toute l'app : ceux de Claude
/// arrivent à son lancement, ceux de Codex et Grok sont lus dans leurs caches.
#[derive(Default)]
pub struct Catalogs(pub HashMap<Provider, Catalog>);

#[derive(Clone, Default)]
pub struct Catalog {
    pub models: Vec<Model>,
    /// Le modèle utilisé si l'on ne choisit rien (configuration du CLI).
    pub default: Option<String>,
}

impl Global for Catalogs {}

/// Les modèles favoris, notés « fournisseur:modèle ». La fenêtre les garde
/// dans les réglages.
#[derive(Default)]
pub struct Favorites(pub Vec<String>);

impl Global for Favorites {}

/// L'ordre du menu des permissions, du plus prudent au plus libre.
const MENU_MODES: [&str; 5] = ["plan", "manual", "acceptEdits", "auto", "bypassPermissions"];

fn mode_icon(mode: &str) -> IconName {
    match mode {
        "plan" => IconName::Lock,
        "manual" => IconName::LockOpen,
        "acceptEdits" => IconName::Pencil,
        "auto" => IconName::Sparkles,
        _ => IconName::ShieldAlert,
    }
}

/// Le logo d'un fournisseur ; Codex et Grok ont une icône neutre en
/// attendant leurs logos officiels.
pub(super) fn provider_logo(provider: Provider) -> AnyElement {
    match provider {
        Provider::Claude => svg()
            .path(CLAUDE_LOGO)
            .size_4()
            .flex_shrink_0()
            .text_color(rgb(CLAUDE_BRAND))
            .into_any_element(),
        _ => Icon::new(IconName::Bot).small().into_any_element(),
    }
}

fn favorite_key(provider: Provider, model: &str) -> String {
    format!("{}:{model}", provider.program())
}

/// Un effort lisible : « high » devient « High ».
fn effort_label(effort: &str) -> String {
    let mut chars = effort.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Ce que montre le menu du modèle quand il est ouvert.
#[derive(Clone, Copy, Default, PartialEq)]
pub(super) enum Panel {
    #[default]
    Models,
    Effort,
}

impl SessionView {
    /// Les modèles d'un fournisseur : pour Claude, ceux de cette conversation
    /// s'il est lancé, sinon ceux annoncés ailleurs dans l'app.
    fn models_of(&self, provider: Provider, cx: &App) -> Vec<Model> {
        if provider == Provider::Claude && !self.models.is_empty() {
            return self.models.clone();
        }
        cx.try_global::<Catalogs>()
            .and_then(|catalogs| catalogs.0.get(&provider))
            .map(|catalog| catalog.models.clone())
            .unwrap_or_default()
    }

    /// Le modèle en cours de cette conversation, s'il est connu.
    fn shown_model(&self, cx: &App) -> Option<Model> {
        if self.provider == Provider::Claude {
            return self.current_model().cloned();
        }
        let models = self.models_of(self.provider, cx);
        let wanted = self.model.clone().or_else(|| {
            cx.try_global::<Catalogs>()?
                .0
                .get(&self.provider)?
                .default
                .clone()
        })?;
        models.into_iter().find(|model| model.value == wanted)
    }

    fn model_label(model: &Model) -> String {
        // `default` s'appelle « Default (recommended) » : on montre le modèle qu'il désigne.
        if model.value == "default" && !model.description.is_empty() {
            model.description.clone()
        } else {
            model.name.clone()
        }
    }

    /// Choisit un modèle, et son agent tant que la conversation n'a pas commencé.
    fn choose_model(
        &mut self,
        provider: Provider,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if provider != self.provider {
            if self.started() {
                return;
            }
            self.set_provider(provider, window, cx);
        }
        self.set_model(value, cx);
        self.model_menu_open = false;
        cx.notify();
    }

    fn toggle_favorite(provider: Provider, value: &str, cx: &mut App) {
        let key = favorite_key(provider, value);
        let favorites = &mut cx.default_global::<Favorites>().0;
        match favorites.iter().position(|known| *known == key) {
            Some(ix) => {
                favorites.remove(ix);
            }
            None => favorites.push(key),
        }
    }

    /// Le bouton du modèle : logo de l'agent, modèle et effort ; son menu.
    pub(super) fn render_model_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let model = self.shown_model(cx);
        let mut label = model
            .as_ref()
            .map(Self::model_label)
            .unwrap_or_else(|| self.provider.name().to_string());
        if let Some(effort) = &self.effort {
            label = format!("{label} {}", effort_label(effort));
        }
        let view = cx.entity();
        let opener = view.clone();
        Popover::new("model-menu")
            .open(self.model_menu_open)
            .on_open_change(move |open, _, cx| {
                opener.update(cx, |this, cx| {
                    this.model_menu_open = *open;
                    if *open {
                        this.model_panel = Panel::Models;
                        this.model_tab = Some(this.provider);
                    }
                    // La page de preview, vue native, passerait devant le menu.
                    this.preview.menu_open = *open;
                    this.set_preview_visible(true, cx);
                    cx.notify();
                })
            })
            .trigger(
                Button::new("model").ghost().small().child(
                    h_flex()
                        .gap_1p5()
                        .child(provider_logo(self.provider))
                        .child(label)
                        .child(Icon::new(IconName::ChevronDown).xsmall()),
                ),
            )
            .appearance(false)
            .content(move |_, window, cx| {
                view.update(cx, |this, cx| this.render_model_panel(window, cx))
            })
    }

    fn render_model_panel(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (muted, border, ghost, popover) = (
            theme.muted_foreground,
            theme.border,
            theme.muted,
            theme.popover,
        );
        let model = self.shown_model(cx);
        let efforts = model
            .as_ref()
            .map(|m| m.efforts.clone())
            .unwrap_or_default();
        let box_style = |element: Div| {
            element
                .rounded(px(8.))
                .border_1()
                .border_color(border)
                .bg(popover)
                .p_1()
        };
        // Une ligne du premier menu : titre, valeur et flèche vers son panneau.
        let row = |id: &'static str, title: String, value: AnyElement, selected: bool| {
            h_flex()
                .id(id)
                .px_2()
                .py_1p5()
                .gap_3()
                .rounded(px(4.))
                .cursor_pointer()
                .when(selected, |row| row.bg(ghost))
                .hover(|row| row.bg(ghost))
                .child(div().flex_1().child(title))
                .child(value)
                .child(Icon::new(IconName::ChevronRight).xsmall().text_color(muted))
        };
        let current_effort = self
            .effort
            .as_deref()
            .map(effort_label)
            .unwrap_or_else(|| t!("models.effort_default").to_string());
        let main = box_style(v_flex().w(px(220.)))
            .when(!efforts.is_empty(), |column| {
                column.child(
                    row(
                        "panel-effort",
                        t!("models.effort").to_string(),
                        div()
                            .text_color(muted)
                            .child(current_effort)
                            .into_any_element(),
                        self.model_panel == Panel::Effort,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.model_panel = Panel::Effort;
                        cx.notify();
                    })),
                )
            })
            .child(
                row(
                    "panel-models",
                    t!("models.model").to_string(),
                    h_flex()
                        .gap_1p5()
                        .text_color(muted)
                        .child(provider_logo(self.provider))
                        .child(model.as_ref().map(Self::model_label).unwrap_or_default())
                        .into_any_element(),
                    self.model_panel == Panel::Models,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.model_panel = Panel::Models;
                    cx.notify();
                })),
            );

        let side = match self.model_panel {
            Panel::Effort => box_style(v_flex().w(px(220.)))
                .child(
                    h_flex()
                        .id("effort-default")
                        .px_2()
                        .py_1p5()
                        .rounded(px(4.))
                        .cursor_pointer()
                        .hover(|row| row.bg(ghost))
                        .child(
                            div()
                                .flex_1()
                                .child(t!("models.effort_default").to_string()),
                        )
                        .when(self.effort.is_none(), |row| {
                            row.child(Icon::new(IconName::Check).xsmall())
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.effort = None;
                            this.model_menu_open = false;
                            cx.notify();
                        })),
                )
                .children(efforts.into_iter().enumerate().map(|(ix, effort)| {
                    let chosen = self.effort.as_deref() == Some(effort.as_str());
                    h_flex()
                        .id(("effort", ix))
                        .px_2()
                        .py_1p5()
                        .rounded(px(4.))
                        .cursor_pointer()
                        .hover(|row| row.bg(ghost))
                        .child(div().flex_1().child(effort_label(&effort)))
                        .when(chosen, |row| row.child(Icon::new(IconName::Check).xsmall()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_effort(effort.clone(), cx);
                            this.model_menu_open = false;
                            cx.notify();
                        }))
                }))
                .into_any_element(),
            Panel::Models => self.render_models(cx),
        };

        h_flex()
            .items_start()
            .gap_1()
            .child(main)
            .child(side)
            .into_any_element()
    }

    /// Le panneau des modèles : les fournisseurs à gauche (favoris d'abord), la
    /// recherche et la liste à droite.
    fn render_models(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (muted, border, ghost, popover) = (
            theme.muted_foreground,
            theme.border,
            theme.muted,
            theme.popover,
        );
        let favorites = cx
            .try_global::<Favorites>()
            .map(|favorites| favorites.0.clone())
            .unwrap_or_default();
        let query = Query::new(&self.model_search.read(cx).value());
        let current = self.shown_model(cx).map(|model| model.value);

        // Les lignes du panneau : (fournisseur, modèle).
        let rows: Vec<(Provider, Model)> = match self.model_tab {
            None => favorites
                .iter()
                .filter_map(|key| {
                    let (program, value) = key.split_once(':')?;
                    let provider = Provider::ALL.into_iter().find(|p| p.program() == program)?;
                    let model = self
                        .models_of(provider, cx)
                        .into_iter()
                        .find(|m| m.value == value)?;
                    Some((provider, model))
                })
                .collect(),
            Some(provider) => self
                .models_of(provider, cx)
                .into_iter()
                .map(|model| (provider, model))
                .collect(),
        };
        let rows: Vec<(Provider, Model)> = rows
            .into_iter()
            .filter(|(_, model)| {
                query.matches(&Self::model_label(model)) || query.matches(&model.value)
            })
            .collect();

        let tab = |id: &'static str, selected: bool, content: AnyElement| {
            div()
                .id(id)
                .size_8()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .cursor_pointer()
                .when(selected, |tab| tab.bg(ghost))
                .hover(|tab| tab.bg(ghost))
                .child(content)
        };
        let tabs = v_flex()
            .p_1()
            .gap_1()
            .border_r_1()
            .border_color(border)
            .child(
                tab(
                    "tab-favorites",
                    self.model_tab.is_none(),
                    Icon::new(IconName::Star).small().into_any_element(),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.model_tab = None;
                    cx.notify();
                })),
            )
            .children(Provider::ALL.into_iter().map(|provider| {
                let id = match provider {
                    Provider::Claude => "tab-claude",
                    Provider::Codex => "tab-codex",
                    Provider::Grok => "tab-grok",
                };
                tab(
                    id,
                    self.model_tab == Some(provider),
                    provider_logo(provider),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.model_tab = Some(provider);
                    cx.notify();
                }))
            }));

        let started = self.started();
        let list = rows.into_iter().enumerate().map(|(ix, (provider, model))| {
            let chosen =
                provider == self.provider && current.as_deref() == Some(model.value.as_str());
            let favorite = favorites.contains(&favorite_key(provider, &model.value));
            // Une conversation commencée garde son agent : les autres sont grisés.
            let locked = started && provider != self.provider;
            let value = model.value.clone();
            let star_value = model.value.clone();
            h_flex()
                .id(("model-row", ix))
                .px_2()
                .py_1p5()
                .gap_2()
                .rounded(px(4.))
                .when(!locked, |row| {
                    row.cursor_pointer().hover(|row| row.bg(ghost))
                })
                .when(locked, |row| row.text_color(muted))
                .when(self.model_tab.is_none(), |row| {
                    row.child(provider_logo(provider))
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(Self::model_label(&model)),
                )
                .child(
                    div()
                        .id(("model-star", ix))
                        .cursor_pointer()
                        .text_color(if favorite { theme.foreground } else { muted })
                        .child(Icon::new(IconName::Star).xsmall())
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            SessionView::toggle_favorite(provider, &star_value, cx);
                        }),
                )
                .child(div().w_4().when(chosen, |cell| {
                    cell.child(Icon::new(IconName::Check).xsmall())
                }))
                .when(!locked, |row| {
                    row.on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_model(provider, value.clone(), window, cx)
                    }))
                })
        });
        let empty = match self.model_tab {
            None => t!("models.no_favorites").to_string(),
            Some(Provider::Claude) => t!("models.claude_later").to_string(),
            Some(_) => t!("models.none").to_string(),
        };
        let list: Vec<_> = list.collect();

        h_flex()
            .items_start()
            .w(px(320.))
            .rounded(px(8.))
            .border_1()
            .border_color(border)
            .bg(popover)
            .child(tabs)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div().px_2().border_b_1().border_color(border).child(
                            Input::new(&self.model_search)
                                .appearance(false)
                                .prefix(Icon::new(IconName::Search).small().text_color(muted)),
                        ),
                    )
                    .child(
                        v_flex()
                            .id("model-list")
                            .p_1()
                            .max_h(px(360.))
                            .overflow_y_scroll()
                            .when(list.is_empty(), |column| {
                                column.child(div().p_2().text_sm().text_color(muted).child(empty))
                            })
                            .children(list),
                    ),
            )
            .into_any_element()
    }

    /// Le « + » : joindre des images, et Planifier tant que la conversation n'a
    /// pas commencé. Chaque entrée a son icône, son titre et sa description.
    pub(super) fn render_plus_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (view, planning, started) = (cx.entity(), self.planning, self.started());
        let muted = cx.theme().muted_foreground;
        let entry = move |icon: IconName, title: String, note: String| {
            PopupMenuItem::element(move |_, _| {
                h_flex()
                    .gap_3()
                    .items_start()
                    .py_1()
                    .child(div().pt_0p5().child(Icon::new(icon).small()))
                    .child(
                        v_flex()
                            .child(div().font_weight(FontWeight::BOLD).child(title.clone()))
                            .child(div().text_xs().text_color(muted).child(note.clone())),
                    )
            })
        };
        Button::new("plus")
            .ghost()
            .small()
            .icon(IconName::Plus)
            .tooltip(t!("composer.add"))
            .dropdown_menu(move |menu, _, _| {
                let attach = view.clone();
                let menu = menu
                    .min_w(px(280.))
                    .label(crate::theme::caps(&t!("composer.add")))
                    .item(
                        entry(
                            IconName::Paperclip,
                            t!("composer.attach").to_string(),
                            t!("composer.attach.note").to_string(),
                        )
                        .on_click(move |_, window, cx| {
                            attach.update(cx, |this, cx| this.pick_images(window, cx))
                        }),
                    );
                if started {
                    return menu;
                }
                let plan = view.clone();
                menu.item(
                    entry(
                        IconName::ListChecks,
                        t!("composer.plan").to_string(),
                        t!("composer.plan.tooltip").to_string(),
                    )
                    .checked(planning)
                    .on_click(move |_, _, cx| {
                        plan.update(cx, |this, cx| {
                            this.planning = !this.planning;
                            cx.notify();
                        })
                    }),
                )
            })
            .on_open_change(self.page_under_menu(cx))
    }

    /// Le bouton du mode de permission, et son menu : icône, titre et description.
    pub(super) fn render_mode_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (view, mode, provider) = (cx.entity(), self.mode, self.provider);
        let muted = cx.theme().muted_foreground;
        Button::new("mode")
            .ghost()
            .small()
            .child(
                h_flex()
                    .gap_1p5()
                    .child(Icon::new(mode_icon(mode)).small())
                    .child(t!(format!("mode.{mode}")).to_string())
                    .child(Icon::new(IconName::ChevronDown).xsmall()),
            )
            .dropdown_menu(move |menu, _, _| {
                MENU_MODES
                    .into_iter()
                    .fold(menu.min_w(px(300.)), |menu, value| {
                        let view = view.clone();
                        menu.item(
                            PopupMenuItem::element(move |_, _| {
                                h_flex()
                                    .gap_3()
                                    .items_start()
                                    .py_1()
                                    .child(
                                        div().pt_0p5().child(Icon::new(mode_icon(value)).small()),
                                    )
                                    .child(
                                        v_flex()
                                            .child(
                                                div()
                                                    .font_weight(FontWeight::BOLD)
                                                    .child(t!(format!("mode.{value}")).to_string()),
                                            )
                                            .child(div().text_xs().text_color(muted).child(
                                                t!(format!("mode.{value}.note")).to_string(),
                                            )),
                                    )
                            })
                            .checked(mode == value)
                            .disabled(!provider.supports(value))
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| this.set_mode(value, cx))
                            }),
                        )
                    })
            })
            .on_open_change(self.page_under_menu(cx))
    }

    /// Sous le composer : le projet (à choisir tant que la conversation n'a pas
    /// commencé) et sa branche git.
    pub(super) fn render_under_composer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let project_label = self
            .project
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| t!("home.project.none").to_string());
        let projects = self
            .projects
            .iter()
            .map(|path| {
                let name = path.file_name().unwrap_or(path.as_os_str());
                (
                    path.to_string_lossy().to_string(),
                    name.to_string_lossy().to_string().into(),
                )
            })
            .chain([(String::new(), t!("home.project.choose").into())])
            .collect::<Vec<_>>();
        h_flex()
            .w_full()
            .max_w(px(760.))
            .px_1()
            .gap_4()
            .text_sm()
            .text_color(muted)
            .child(if self.started() {
                h_flex()
                    .gap_1p5()
                    .child(Icon::new(IconName::Folder).small())
                    .child(project_label)
                    .into_any_element()
            } else {
                self.render_picker(
                    "project",
                    IconName::Folder,
                    project_label.into(),
                    projects,
                    Some(self.project.to_string_lossy().to_string()),
                    |this, path, window, cx| {
                        // La dernière entrée, sans chemin, ouvre le sélecteur de dossier.
                        if path.is_empty() {
                            this.choose_folder(false, window, cx);
                        } else {
                            this.set_project(path.into(), cx);
                        }
                    },
                    cx,
                )
                .into_any_element()
            })
            .children(self.branch.clone().map(|branch| {
                h_flex()
                    .gap_1p5()
                    .child(Icon::new(IconName::GitBranch).small())
                    .child(branch)
            }))
    }
}

#[cfg(test)]
mod tests {
    use core::prelude::v1::test;

    use super::effort_label;

    #[test]
    fn efforts_read_well() {
        assert_eq!(effort_label("high"), "High");
        assert_eq!(effort_label("xhigh"), "Xhigh");
        assert_eq!(effort_label(""), "");
    }
}
