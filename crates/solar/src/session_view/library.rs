//! La bibliothèque dans le composer : les modèles par `/`, le formulaire de
//! leurs trous, et ce qu'ils règlent (Planifier, agent conseillé).

use std::collections::HashMap;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::*;
use rust_i18n::t;
use solar_core::library::{self, Example, Template};

use super::SessionView;

/// Les exemples intégrés, dans la langue de l'interface.
pub(crate) fn examples() -> Vec<Example> {
    ["bug", "feature", "review", "tests"]
        .into_iter()
        .map(|name| Example {
            name: t!(format!("library.example.{name}.name")).to_string(),
            text: t!(format!("library.example.{name}.text")).to_string(),
        })
        .collect()
}

/// Le formulaire des trous d'un modèle, au-dessus du composer.
pub(super) struct TemplateForm {
    template: Template,
    fields: Vec<(String, Entity<InputState>)>,
    _subscriptions: Vec<Subscription>,
}

impl TemplateForm {
    #[cfg(test)]
    pub(super) fn fields(&self) -> &[(String, Entity<InputState>)] {
        &self.fields
    }
}

impl SessionView {
    /// Relit les modèles (projet, personnels, exemples), en arrière-plan.
    pub(crate) fn load_templates(&mut self, cx: &mut Context<Self>) {
        let project = (!self.project.as_os_str().is_empty()).then(|| self.project.clone());
        let examples = examples();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn(async move {
                    library::load(&library::personal_dir(), project.as_deref(), &examples)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.templates = found;
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn templates(&self) -> &[Template] {
        &self.templates
    }

    /// Utilise un modèle : il règle Planifier et l'agent d'une conversation
    /// neuve, puis s'insère, ou ouvre le formulaire de ses trous. Rien n'est
    /// envoyé.
    pub(crate) fn use_template(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(template) = self.templates.iter().find(|t| t.name == name).cloned() else {
            return;
        };
        if !self.started() {
            if template.mission {
                self.planning = true;
            }
            match template.agent {
                Some(agent) if agent != self.provider && agent.installed() => {
                    self.set_provider(agent, window, cx)
                }
                _ => {}
            }
        } else if template.agent.is_some_and(|agent| agent != self.provider) {
            self.note(
                t!(
                    "library.agent_kept",
                    agent = template.agent.map(|a| a.name()).unwrap_or_default(),
                    current = self.provider.name()
                ),
                false,
                cx,
            );
        }
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        let holes = library::holes(&template.body);
        if holes.is_empty() {
            self.fill(library::fill(&template.body, &HashMap::new()), window, cx);
            return;
        }
        let mut subscriptions = Vec::new();
        let fields: Vec<(String, Entity<InputState>)> = holes
            .into_iter()
            .map(|hole| {
                let input = cx.new(|cx| InputState::new(window, cx).placeholder(hole.clone()));
                subscriptions.push(cx.subscribe_in(
                    &input,
                    window,
                    |this, _, event: &InputEvent, window, cx| {
                        if let InputEvent::PressEnter { .. } = event {
                            this.insert_template(window, cx);
                        }
                    },
                ));
                (hole, input)
            })
            .collect();
        if let Some((_, first)) = fields.first() {
            first.update(cx, |input, cx| input.focus(window, cx));
        }
        self.template_form = Some(TemplateForm {
            template,
            fields,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }

    /// Les trous remplis : le texte complété va dans le composer.
    pub(super) fn insert_template(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.template_form.take() else {
            return;
        };
        let values: HashMap<String, String> = form
            .fields
            .iter()
            .map(|(hole, input)| (hole.clone(), input.read(cx).value().trim().to_string()))
            .collect();
        self.fill(library::fill(&form.template.body, &values), window, cx);
    }

    pub(super) fn render_template_form(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let form = self.template_form.as_ref()?;
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        Some(
            v_flex()
                .w_full()
                .max_w(px(760.))
                .p_3()
                .gap_3()
                .rounded(theme.radius)
                .border_1()
                .border_color(theme.input)
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            div()
                                .font_weight(FontWeight::BOLD)
                                .child(form.template.title.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(muted)
                                .child(form.template.description.clone()),
                        ),
                )
                .children(form.fields.iter().map(|(hole, input)| {
                    v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(muted)
                                .child(crate::theme::caps(hole)),
                        )
                        .child(Input::new(input).small())
                }))
                .child(
                    h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("template-cancel")
                                .ghost()
                                .small()
                                .label(t!("library.cancel").to_string())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.template_form = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("template-insert")
                                .primary()
                                .small()
                                .label(t!("library.insert").to_string())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.insert_template(window, cx)
                                })),
                        ),
                )
                .into_any_element(),
        )
    }
}
