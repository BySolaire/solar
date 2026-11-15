//! L'espace de travail d'une mission : une barre sous les onglets (titre, état,
//! vues, Arrêter) et la vue Plan, qui montre le plan retenu et l'activité réelle
//! de Claude. Une conversation ordinaire n'a ni l'une ni l'autre.

use std::path::Path;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme, Icon, Selectable, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rust_i18n::t;
use solar_core::state::MissionStatus;

use super::{Item, SessionView, Status, tool_icon};
use crate::theme::caps;

/// Les vues d'une mission. La conversation reste la vue par défaut.
#[derive(Clone, Copy, Default, PartialEq)]
pub(super) enum Pane {
    #[default]
    Conversation,
    Plan,
}

/// Ce que Claude a réellement fait, tiré des outils du fil.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Activity {
    /// Fichiers du projet modifiés, dans l'ordre de la première modification.
    pub edited: Vec<String>,
    pub read: Vec<String>,
    /// Commandes lancées, avec leur échec éventuel.
    pub commands: Vec<(String, bool)>,
}

/// Agrège les outils du fil. Seuls les fichiers du projet comptent : le plan
/// que Claude range dans `~/.claude/plans` n'est pas une modification du projet.
pub(super) fn activity(items: &[Item], root: &Path) -> Activity {
    let mut found = Activity::default();
    let push = |list: &mut Vec<String>, file: &str| {
        let Ok(inside) = Path::new(file).strip_prefix(root) else {
            return;
        };
        let file = inside.display().to_string();
        if !list.contains(&file) {
            list.push(file);
        }
    };
    for item in items {
        let Item::Tool {
            name,
            input,
            result,
            ..
        } = item
        else {
            continue;
        };
        let file = input["file_path"].as_str().unwrap_or_default();
        match name.as_str() {
            "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => push(&mut found.edited, file),
            "Read" => push(&mut found.read, file),
            "Bash" => {
                let command = input["command"].as_str().unwrap_or_default();
                let failed = matches!(result, Some((true, _)));
                found
                    .commands
                    .push((command.lines().next().unwrap_or_default().into(), failed));
            }
            _ => {}
        }
    }
    found
}

impl SessionView {
    /// L'état de la mission, en clair. Une fin de tour n'est pas une mission
    /// terminée : c'est « Tour terminé », à examiner.
    fn mission_state(&self, status: MissionStatus) -> String {
        let key = match (status, self.status) {
            _ if self.stopping => "mission.status.stopping",
            (MissionStatus::Planning, _) => "mission.status.planning",
            (MissionStatus::Ready, _) => "mission.status.ready",
            (MissionStatus::Cancelled, _) => "mission.status.cancelled",
            (MissionStatus::Running, Status::Working) => "mission.status.running",
            (MissionStatus::Running, Status::NeedsYou) => "mission.status.waiting",
            (MissionStatus::Running, Status::Idle) => "mission.status.turn_done",
        };
        t!(key).to_string()
    }

    pub(super) fn render_mission_bar(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let mission = self.mission.as_ref()?;
        let theme = cx.theme();
        let (white, muted) = (theme.foreground, theme.muted_foreground);
        let pane = |id: &'static str, key: &str, target: Pane| {
            let active = self.pane == target;
            Button::new(id)
                .ghost()
                .small()
                .selected(active)
                .text_color(if active { white } else { muted })
                .label(t!(key).to_string())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.pane = target;
                    cx.notify();
                }))
        };
        Some(
            h_flex()
                .w_full()
                .flex_shrink_0()
                .h(px(40.))
                .px_4()
                .gap_4()
                .border_b_1()
                .border_color(theme.border)
                .child(
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_2()
                        .child(Icon::new(IconName::ListChecks).xsmall().text_color(muted))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .font_weight(FontWeight::BOLD)
                                .child(self.title.clone()),
                        )
                        .child(div().flex_shrink_0().text_color(muted).child("·"))
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(muted)
                                .child(self.mission_state(mission.status)),
                        ),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .child(pane(
                            "pane-conversation",
                            "pane.conversation",
                            Pane::Conversation,
                        ))
                        .child(pane("pane-plan", "pane.plan", Pane::Plan)),
                )
                // « Arrêter » envoie une interruption ; l'état dit « Arrêt demandé »
                // jusqu'à ce que Claude confirme la fin du tour.
                .when(self.status != Status::Idle && !self.stopping, |bar| {
                    bar.child(
                        Button::new("stop-mission")
                            .ghost()
                            .small()
                            .icon(IconName::Square)
                            .label(t!("mission.stop").to_string())
                            .on_click(cx.listener(|this, _, _, cx| this.interrupt(cx))),
                    )
                }),
        )
    }

    /// Le plan retenu, puis ce que Claude a fait : rien d'estimé, rien d'inventé.
    pub(super) fn render_plan_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (muted, danger) = (theme.muted_foreground, theme.danger);
        let plans = self
            .mission
            .as_ref()
            .map(|mission| mission.plans.as_slice())
            .unwrap_or_default();
        let done = activity(&self.items, &self.root);

        let heading = |text: String| {
            div()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(muted)
                .child(caps(&text))
        };
        let row = |icon: IconName, text: String, failed: bool| {
            h_flex()
                .gap_2()
                .min_w_0()
                .text_color(if failed { danger } else { theme.foreground })
                .child(
                    Icon::new(if failed { IconName::CircleAlert } else { icon })
                        .xsmall()
                        .text_color(if failed { danger } else { muted }),
                )
                .child(div().min_w_0().truncate().child(text))
        };
        let group = |key: &str, rows: Vec<Div>| {
            (!rows.is_empty()).then(|| {
                v_flex()
                    .gap_1p5()
                    .child(div().text_color(muted).child(t!(key).to_string()))
                    .children(rows)
            })
        };
        let edited = done
            .edited
            .iter()
            .map(|file| row(tool_icon("Edit"), file.clone(), false))
            .collect();
        let read = done
            .read
            .iter()
            .map(|file| row(tool_icon("Read"), file.clone(), false))
            .collect();
        let commands = done
            .commands
            .iter()
            .map(|(command, failed)| row(tool_icon("Bash"), command.clone(), *failed))
            .collect();
        let nothing = done == Activity::default();

        div()
            .id("plan-view")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                v_flex().w_full().items_center().px_6().py_6().child(
                    v_flex()
                        .w_full()
                        .max_w(px(760.))
                        .gap_8()
                        .child(
                            v_flex()
                                .gap_3()
                                .child(heading(match plans.len() {
                                    0 => t!("pane.plan").to_string(),
                                    count => {
                                        t!("plan.version", n = count, count = count).to_string()
                                    }
                                }))
                                .child(match plans.last() {
                                    Some(plan) => {
                                        TextView::markdown("plan-view-text", plan.clone())
                                            .selectable(true)
                                            .into_any_element()
                                    }
                                    None => div()
                                        .text_color(muted)
                                        .child(t!("plan.none").to_string())
                                        .into_any_element(),
                                }),
                        )
                        .child(
                            v_flex()
                                .gap_4()
                                .child(heading(t!("activity.title").to_string()))
                                .when(nothing, |column| {
                                    column.child(
                                        div()
                                            .text_color(muted)
                                            .child(t!("activity.none").to_string()),
                                    )
                                })
                                .children(group("activity.edited", edited))
                                .children(group("activity.commands", commands))
                                .children(group("activity.read", read)),
                        ),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use core::prelude::v1::test;

    use serde_json::json;

    use super::*;

    fn tool(name: &str, input: serde_json::Value, failed: Option<bool>) -> Item {
        Item::Tool {
            id: String::new(),
            name: name.into(),
            input,
            result: failed.map(|failed| (failed, String::new())),
            open: false,
        }
    }

    #[test]
    fn activity_keeps_project_files_once_and_every_command() {
        let items = [
            tool(
                "Write",
                json!({"file_path": "/home/me/.claude/plans/p.md"}),
                Some(false),
            ),
            tool(
                "Read",
                json!({"file_path": "/work/app/src/auth.rs"}),
                Some(false),
            ),
            tool(
                "Edit",
                json!({"file_path": "/work/app/src/auth.rs"}),
                Some(false),
            ),
            tool(
                "Edit",
                json!({"file_path": "/work/app/src/auth.rs"}),
                Some(false),
            ),
            tool(
                "Bash",
                json!({"command": "cargo test\necho done"}),
                Some(true),
            ),
            tool("Bash", json!({"command": "cargo test"}), None),
        ];
        let found = activity(&items, Path::new("/work/app"));
        assert_eq!(found.edited, ["src/auth.rs"]);
        assert_eq!(found.read, ["src/auth.rs"]);
        assert_eq!(
            found.commands,
            [
                ("cargo test".to_string(), true),
                ("cargo test".to_string(), false)
            ]
        );
    }
}
