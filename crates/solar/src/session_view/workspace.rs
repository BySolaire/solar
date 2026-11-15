//! L'espace de travail d'une mission : une barre sous les onglets (titre, état,
//! vues, Arrêter), la vue Plan, qui montre le plan retenu et l'activité réelle
//! de Claude, et la vue Fichiers, en lecture seule. Une conversation ordinaire
//! n'a rien de tout cela.

use std::collections::BTreeMap;
use std::path::Path;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::list::ListItem;
use gpui_kit::component::text::TextView;
use gpui_kit::component::tree::{TreeItem, TreeState, tree};
use gpui_kit::component::{ActiveTheme, Icon, Selectable, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rust_i18n::t;
use solar_core::files::{self, Content};
use solar_core::state::MissionStatus;

use super::{Item, SessionView, Status, tool_icon};
use crate::theme::caps;

/// Les vues d'une mission. La conversation reste la vue par défaut.
#[derive(Clone, Copy, Default, PartialEq)]
pub(super) enum Pane {
    #[default]
    Conversation,
    Plan,
    Files,
    Changes,
    Preview,
}

/// Un fichier ouvert dans la vue Fichiers : au-delà, l'aperçu est coupé.
const PREVIEW_LIMIT: usize = 200_000;

/// L'arborescence d'une liste de chemins relatifs triés : dossiers d'abord,
/// identifiés par un `/` final, puis fichiers.
pub(super) fn file_tree(paths: &[String]) -> Vec<TreeItem> {
    fn level(prefix: &str, paths: &[&str]) -> Vec<TreeItem> {
        let mut folders: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let mut files = Vec::new();
        for path in paths {
            match path.split_once('/') {
                Some((folder, rest)) => folders.entry(folder).or_default().push(rest),
                None => files.push(*path),
            }
        }
        let folders = folders.into_iter().map(|(name, inside)| {
            let id = format!("{prefix}{name}/");
            let children = level(&id, &inside);
            TreeItem::new(id, name.to_string()).children(children)
        });
        let files = files
            .into_iter()
            .map(|name| TreeItem::new(format!("{prefix}{name}"), name.to_string()));
        folders.chain(files).collect()
    }
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    level("", &paths)
}

/// Un bloc de code Markdown dont la clôture ne peut pas apparaître dans le texte.
fn fenced(path: &str, text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    let language = Path::new(path)
        .extension()
        .map(|ext| ext.to_string_lossy().to_string())
        .unwrap_or_default();
    format!("{fence}{language}\n{text}\n{fence}")
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
    /// Ouvre la vue Fichiers ; la liste est lue une fois, en arrière-plan.
    pub(super) fn open_files(&mut self, cx: &mut Context<Self>) {
        self.pane = Pane::Files;
        cx.notify();
        if self.file_tree.is_some() {
            return;
        }
        let state = cx.new(|cx| TreeState::new(cx));
        cx.observe(&state, |this, state, cx| this.show_selected(&state, cx))
            .detach();
        self.file_tree = Some(state.clone());
        let root = self.workdir();
        cx.spawn(async move |this, cx| {
            let paths = cx.background_spawn(async move { files::list(&root) }).await;
            state.update(cx, |state, cx| state.set_items(file_tree(&paths), cx));
            let _ = this.update(cx, |this, cx| {
                this.files_listed = true;
                cx.notify();
            });
        })
        .detach();
    }

    /// Le fichier choisi dans l'arborescence est lu en arrière-plan.
    fn show_selected(&mut self, state: &Entity<TreeState>, cx: &mut Context<Self>) {
        let Some(item) = state.read(cx).selected_item() else {
            return;
        };
        let path = item.id.to_string();
        if item.is_folder() || self.opened.as_ref().is_some_and(|(open, _)| *open == path) {
            return;
        }
        self.opened = Some((
            path.clone(),
            Content::Text {
                text: String::new(),
                truncated: false,
            },
        ));
        let file = self.workdir().join(&path);
        cx.spawn(async move |this, cx| {
            let content = cx
                .background_spawn(async move { files::read(&file, PREVIEW_LIMIT) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.opened.as_ref().is_some_and(|(open, _)| *open == path) {
                    this.opened = Some((path, content));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// L'état de la mission, en clair. Une fin de tour n'est pas une mission
    /// terminée : c'est « Tour terminé », à examiner.
    /// Le numéro de tentative, s'il y en a plusieurs, puis l'état. Un agent
    /// bloqué est nommé avec la raison, tirée des événements réels.
    fn mission_state(&self, mission: &solar_core::state::Mission) -> String {
        let state = self.mission_status(mission.status);
        let state = match mission.next_attempt {
            Some(next) => t!("mission.status.superseded", n = next).to_string(),
            None => state,
        };
        if mission.attempt > 1 || mission.next_attempt.is_some() {
            t!("mission.attempt", n = mission.attempt, state = state).to_string()
        } else {
            state
        }
    }

    fn mission_status(&self, status: MissionStatus) -> String {
        let key = match (status, self.status) {
            _ if self.stopping => "mission.status.stopping",
            _ if self.interrupted => "mission.status.interrupted",
            _ if self.reviewing => "mission.status.reviewing",
            _ if self.reviewer_failed => "mission.status.reviewer_failed",
            (MissionStatus::Running, Status::Idle) if self.turn_failed => {
                "mission.status.turn_failed"
            }
            (MissionStatus::Planning, _) => "mission.status.planning",
            (MissionStatus::Ready, _) => "mission.status.ready",
            (MissionStatus::Cancelled, _) => "mission.status.cancelled",
            (MissionStatus::Accepted, _) => "mission.status.accepted",
            (MissionStatus::Restored, _) => "mission.status.restored",
            (MissionStatus::Running, Status::Working) => "mission.status.running",
            (MissionStatus::Running, Status::NeedsYou) => "mission.status.waiting",
            (MissionStatus::Running, Status::Idle) => "mission.status.turn_done",
        };
        t!(
            key,
            agent = self.provider.name(),
            reviewer = self.reviewer_provider.name()
        )
        .to_string()
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
                .on_click(cx.listener(move |this, _, window, cx| {
                    match target {
                        Pane::Files => this.open_files(cx),
                        Pane::Changes => this.open_changes(cx),
                        Pane::Preview => this.open_preview(window, cx),
                        _ => {
                            this.pane = target;
                            cx.notify();
                        }
                    }
                    // La page web, vue native, ne reste visible que dans sa vue.
                    this.set_preview_visible(true, cx);
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
                                .child(self.mission_state(mission)),
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
                        .child(pane("pane-plan", "pane.plan", Pane::Plan))
                        .child(pane("pane-changes", "pane.changes", Pane::Changes))
                        .child(pane("pane-files", "pane.files", Pane::Files))
                        .child(pane("pane-preview", "pane.preview", Pane::Preview)),
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
                })
                // Après une restauration ou un abandon : une nouvelle tentative,
                // dans une conversation neuve.
                .when(
                    self.status == Status::Idle
                        && mission.next_attempt.is_none()
                        && matches!(
                            mission.status,
                            MissionStatus::Restored | MissionStatus::Cancelled
                        )
                        && !mission.plans.is_empty(),
                    |bar| {
                        bar.child(
                            Button::new("relaunch-mission")
                                .ghost()
                                .small()
                                .icon(IconName::RefreshCw)
                                .label(t!("mission.relaunch", n = mission.attempt + 1).to_string())
                                .on_click(cx.listener(|this, _, _, cx| this.relaunch(cx))),
                        )
                    },
                ),
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

impl SessionView {
    /// Le fichier choisi à gauche, l'arborescence du projet à droite, sans
    /// aucune modification possible.
    /// Rend l'aperçu, puis l'arborescence, qui descend sur toute la hauteur à droite.
    pub(super) fn render_files_view(&self, cx: &mut Context<Self>) -> (AnyElement, AnyElement) {
        let theme = cx.theme();
        let (muted, border) = (theme.muted_foreground, theme.border);
        let note = |text: String| div().text_color(muted).child(text);
        let list = match &self.file_tree {
            Some(state) if self.files_listed && state.read(cx).entry(0).is_none() => {
                note(t!("files.none").to_string()).p_4().into_any_element()
            }
            Some(state) if self.files_listed => tree(state, move |ix, entry, selected, _, _| {
                let item = entry.item();
                let icon = match (entry.is_folder(), entry.is_expanded()) {
                    (true, true) => IconName::FolderOpen,
                    (true, false) => IconName::FolderClosed,
                    _ => IconName::File,
                };
                ListItem::new(ix)
                    .selected(selected)
                    .pl(px(8. + entry.depth() as f32 * 14.))
                    .child(
                        h_flex()
                            .gap_1p5()
                            .min_w_0()
                            .child(Icon::new(icon).xsmall().text_color(muted))
                            .child(div().min_w_0().truncate().child(item.label.clone())),
                    )
            })
            .into_any_element(),
            _ => note(t!("files.loading").to_string())
                .p_4()
                .into_any_element(),
        };
        let viewer = match &self.opened {
            None => note(t!("files.choose").to_string()).into_any_element(),
            Some((path, Content::Text { text, truncated })) => v_flex()
                .gap_3()
                .child(div().text_color(muted).child(path.clone()))
                .when(*truncated, |column| {
                    column.child(note(t!("files.truncated").to_string()))
                })
                .child(
                    TextView::markdown(
                        SharedString::from(format!("file:{path}")),
                        fenced(path, text),
                    )
                    .selectable(true),
                )
                .into_any_element(),
            Some((path, Content::Binary)) => v_flex()
                .gap_3()
                .child(div().text_color(muted).child(path.clone()))
                .child(note(t!("files.binary").to_string()))
                .into_any_element(),
            Some((path, Content::Unreadable(error))) => v_flex()
                .gap_3()
                .child(div().text_color(muted).child(path.clone()))
                .child(note(t!("files.unreadable", error = error).to_string()))
                .into_any_element(),
        };
        (
            div()
                .id("file-view")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px_6()
                .py_4()
                .child(viewer)
                .into_any_element(),
            div()
                .w(px(280.))
                .flex_shrink_0()
                .h_full()
                .py_2()
                .border_l_1()
                .border_color(border)
                .child(list)
                .into_any_element(),
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
            finished: None,
            open: false,
        }
    }

    #[test]
    fn paths_become_a_tree_with_folders_first() {
        let paths = ["README.md", "src/b/c.rs", "src/a.rs"].map(String::from);
        let tree = file_tree(&paths);
        let ids = |items: &[TreeItem]| items.iter().map(|i| i.id.to_string()).collect::<Vec<_>>();
        assert_eq!(ids(&tree), ["src/", "README.md"]);
        assert_eq!(ids(&tree[0].children), ["src/b/", "src/a.rs"]);
        assert_eq!(ids(&tree[0].children[0].children), ["src/b/c.rs"]);
        assert_eq!(tree[0].children[0].label.as_ref(), "b");
    }

    #[test]
    fn a_fence_never_appears_in_the_text_it_wraps() {
        assert_eq!(fenced("a.rs", "fn main() {}"), "```rs\nfn main() {}\n```");
        assert!(fenced("doc.md", "```rust\nx\n```").starts_with("````md\n"));
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
