//! La revue d'une mission : ce qui a changé depuis l'instantané pris à son
//! lancement, la validation tirée des tests que Claude a lancés, et la décision.
//! Accepter n'est qu'une décision : aucun commit. Restaurer ne remet que les
//! fichiers de la mission, après confirmation, et jamais s'ils ont bougé depuis
//! l'examen.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::diff::{Diff, DiffFile, DiffState};
use gpui_kit::component::{ActiveTheme, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rust_i18n::t;
use solar_core::checkpoint::{Change, ChangeKind, RestoreError, Store};
use solar_core::state::MissionStatus;

use super::{Item, SessionView, Status};

/// L'état examiné : l'arbre du projet à l'affichage, ses changements et leur diff.
pub(super) struct Review {
    pub tree: String,
    pub changes: Vec<Change>,
    pub diff: Entity<DiffState>,
}

/// Le dernier passage des tests, tel que Claude l'a lancé.
#[derive(Debug, PartialEq)]
pub(super) enum Tests {
    NotRun,
    Running(String),
    /// La commande, son échec, et « périmé » si un fichier a changé après.
    Done {
        command: String,
        failed: bool,
        stale: bool,
    },
}

/// Commandes reconnues comme des tests.
const TEST_COMMANDS: [&str; 17] = [
    "cargo test",
    "cargo nextest",
    "npm test",
    "npm run test",
    "pnpm test",
    "pnpm run test",
    "yarn test",
    "bun test",
    "pytest",
    "go test",
    "vitest",
    "jest",
    "swift test",
    "mvn test",
    "gradle test",
    "make test",
    "rspec",
];

pub(super) fn tests(items: &[Item]) -> Tests {
    let edits = |item: &Item| {
        matches!(item, Item::Tool { name, .. }
            if matches!(name.as_str(), "Write" | "Edit" | "MultiEdit" | "NotebookEdit"))
    };
    let last = items
        .iter()
        .enumerate()
        .rev()
        .find_map(|(ix, item)| match item {
            Item::Tool {
                name,
                input,
                result,
                ..
            } if name == "Bash" => {
                let command = input["command"].as_str().unwrap_or_default();
                TEST_COMMANDS
                    .iter()
                    .any(|test| command.contains(test))
                    .then(|| {
                        (
                            ix,
                            command.lines().next().unwrap_or_default().to_string(),
                            result,
                        )
                    })
            }
            _ => None,
        });
    match last {
        None => Tests::NotRun,
        Some((_, command, None)) => Tests::Running(command),
        Some((ix, command, Some((failed, _)))) => Tests::Done {
            command,
            failed: *failed,
            stale: items[ix + 1..].iter().any(edits),
        },
    }
}

impl SessionView {
    fn store(&self) -> Store {
        Store::for_project(&self.checkpoints, &self.project)
    }

    pub(super) fn open_changes(&mut self, cx: &mut Context<Self>) {
        self.pane = super::workspace::Pane::Changes;
        self.refresh_review(cx);
    }

    /// Relit l'état du projet et le compare à l'instantané, en arrière-plan.
    pub(super) fn refresh_review(&mut self, cx: &mut Context<Self>) {
        let Some(base) = self.mission.as_ref().and_then(|m| m.checkpoint.clone()) else {
            cx.notify();
            return;
        };
        self.review_busy = true;
        cx.notify();
        let store = self.store();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_spawn(async move {
                    let tree = store.snapshot()?;
                    let changes = store.changes(&base, &tree)?;
                    let patch = store.patch(&base, &tree)?;
                    std::io::Result::Ok((tree, changes, patch))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.review_busy = false;
                match read {
                    Ok((tree, changes, patch)) => {
                        let files = DiffFile::parse(&patch).unwrap_or_default();
                        let diff = match this.review.take() {
                            Some(review) => {
                                review.diff.update(cx, |diff, cx| diff.set_files(files, cx));
                                review.diff
                            }
                            None => cx.new(|cx| DiffState::new(files, cx)),
                        };
                        this.review = Some(Review {
                            tree,
                            changes,
                            diff,
                        });
                    }
                    Err(error) => {
                        this.review_note = Some(t!("review.failed", error = error).into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Accepte le résultat examiné, s'il n'a pas changé depuis : sinon la revue
    /// est relue et la décision attend un nouvel examen.
    pub(super) fn accept_result(&mut self, cx: &mut Context<Self>) {
        let Some(reviewed) = self.review.as_ref().map(|review| review.tree.clone()) else {
            return;
        };
        let store = self.store();
        cx.spawn(async move |this, cx| {
            let now = cx.background_spawn(async move { store.snapshot() }).await;
            let _ = this.update(cx, |this, cx| match now {
                Ok(now) if now == reviewed => {
                    if let Some(mission) = this.mission.as_mut() {
                        mission.status = MissionStatus::Accepted;
                        mission.accepted = Some(now);
                    }
                    this.review_note = None;
                    cx.notify();
                }
                Ok(_) => {
                    this.review_note = Some(t!("review.stale").into());
                    this.refresh_review(cx);
                }
                Err(error) => {
                    this.review_note = Some(t!("review.failed", error = error).into());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Remet les fichiers de la mission dans leur état d'avant, et eux seuls.
    pub(super) fn restore_result(&mut self, cx: &mut Context<Self>) {
        let (Some(base), Some(review)) = (
            self.mission.as_ref().and_then(|m| m.checkpoint.clone()),
            self.review.as_ref(),
        ) else {
            return;
        };
        let (reviewed, paths) = (
            review.tree.clone(),
            review
                .changes
                .iter()
                .map(|change| change.path.clone())
                .collect::<Vec<_>>(),
        );
        self.confirm_restore = false;
        let store = self.store();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_spawn(async move {
                    store.restore(&base, &reviewed, &paths).map(|_| paths.len())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.review_note = Some(match done {
                    Ok(count) => {
                        if let Some(mission) = this.mission.as_mut() {
                            mission.status = MissionStatus::Restored;
                        }
                        t!("review.restored", count = count).into()
                    }
                    Err(RestoreError::ChangedSinceReview(paths)) => {
                        t!("review.changed", files = paths.join(", ")).into()
                    }
                    Err(RestoreError::Io(error)) => t!("review.failed", error = error).into(),
                });
                this.refresh_review(cx);
            });
        })
        .detach();
    }

    /// La vue Modifications : le diff à gauche, la liste des fichiers à droite.
    pub(super) fn render_changes_view(&self, cx: &mut Context<Self>) -> (AnyElement, AnyElement) {
        let theme = cx.theme();
        let (muted, border) = (theme.muted_foreground, theme.border);
        let (success, danger, warning) = (theme.success, theme.danger, theme.warning);
        let note = |text: String| div().text_color(muted).child(text);
        let mission = self.mission.as_ref();
        let status = mission.map(|m| m.status);

        let tests_line = match tests(&self.items) {
            Tests::NotRun => note(t!("review.tests.none").to_string()),
            Tests::Running(command) => {
                note(t!("review.tests.running", command = command).to_string())
            }
            Tests::Done {
                command,
                failed,
                stale,
            } => {
                let key = match (failed, stale) {
                    (_, true) => "review.tests.stale",
                    (true, false) => "review.tests.failed",
                    (false, false) => "review.tests.passed",
                };
                let color = match (failed, stale) {
                    (_, true) => muted,
                    (true, false) => danger,
                    (false, false) => success,
                };
                div()
                    .text_color(color)
                    .child(t!(key, command = command).to_string())
            }
        };

        let changes = self.review.as_ref().map(|r| r.changes.len()).unwrap_or(0);
        let can_decide = changes > 0
            && self.status == Status::Idle
            && !self.review_busy
            && status == Some(MissionStatus::Running);
        let decision = match status {
            Some(MissionStatus::Accepted) => Some(note(t!("review.accepted").to_string())),
            Some(MissionStatus::Restored) => Some(note(t!("review.restored_state").to_string())),
            _ => None,
        };
        let actions = h_flex()
            .gap_2()
            .when(can_decide && !self.confirm_restore, |row| {
                row.child(
                    Button::new("accept-result")
                        .primary()
                        .small()
                        .icon(IconName::Check)
                        .label(t!("review.accept").to_string())
                        .on_click(cx.listener(|this, _, _, cx| this.accept_result(cx))),
                )
                .child(
                    Button::new("restore-result")
                        .ghost()
                        .small()
                        .icon(IconName::Undo2)
                        .label(t!("review.restore").to_string())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirm_restore = true;
                            cx.notify();
                        })),
                )
            })
            .when(can_decide && self.confirm_restore, |row| {
                row.child(
                    div()
                        .flex_1()
                        .child(t!("review.restore.confirm", count = changes).to_string()),
                )
                .child(
                    Button::new("restore-cancel")
                        .ghost()
                        .small()
                        .label(t!("review.restore.cancel").to_string())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirm_restore = false;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("restore-confirm")
                        .primary()
                        .small()
                        .label(t!("review.restore.go").to_string())
                        .on_click(cx.listener(|this, _, _, cx| this.restore_result(cx))),
                )
            });

        let header = v_flex()
            .flex_shrink_0()
            .px_6()
            .py_3()
            .gap_2()
            .border_b_1()
            .border_color(border)
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_color(muted)
                            .child(t!("review.base").to_string()),
                    )
                    .child(
                        Button::new("review-refresh")
                            .ghost()
                            .xsmall()
                            .icon(IconName::RefreshCw)
                            .tooltip(t!("review.refresh"))
                            .on_click(cx.listener(|this, _, _, cx| this.refresh_review(cx))),
                    ),
            )
            .child(tests_line)
            .children(decision)
            .children(
                self.review_note
                    .clone()
                    .map(|text| div().text_color(warning).child(text)),
            )
            .child(actions);

        let body = match (&self.review, mission.and_then(|m| m.checkpoint.as_ref())) {
            (_, None) => note(t!("review.no_base").to_string())
                .p_6()
                .into_any_element(),
            (None, _) => note(t!("review.loading").to_string())
                .p_6()
                .into_any_element(),
            (Some(review), _) if review.changes.is_empty() => {
                note(t!("review.none").to_string()).p_6().into_any_element()
            }
            (Some(review), _) => Diff::new(&review.diff)
                .flex_1()
                .min_h_0()
                .into_any_element(),
        };

        let rows = self.review.iter().flat_map(|review| {
            review.changes.iter().enumerate().map(|(ix, change)| {
                let (icon, color) = match change.kind {
                    ChangeKind::Added => (IconName::Plus, success),
                    ChangeKind::Deleted => (IconName::Minus, danger),
                    ChangeKind::Modified => (IconName::Pencil, muted),
                };
                let lines = match change.lines {
                    Some((added, removed)) => format!("+{added} −{removed}"),
                    None => t!("review.binary").to_string(),
                };
                let (diff, path) = (review.diff.clone(), change.path.clone());
                h_flex()
                    .id(("change", ix))
                    .px_3()
                    .py_1()
                    .gap_2()
                    .cursor_pointer()
                    .hover(|row| row.bg(cx.theme().list_hover))
                    .child(Icon::new(icon).xsmall().text_color(color))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(change.path.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(lines),
                    )
                    .on_click(move |_, _, cx| {
                        diff.update(cx, |diff, cx| diff.scroll_to_file(&path, cx))
                    })
            })
        });

        (
            v_flex()
                .flex_1()
                .min_h_0()
                .child(header)
                .child(body)
                .into_any_element(),
            v_flex()
                .id("changes-list")
                .w(px(280.))
                .flex_shrink_0()
                .h_full()
                .py_2()
                .border_l_1()
                .border_color(border)
                .overflow_y_scroll()
                .children(rows)
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
            open: false,
        }
    }

    #[test]
    fn the_last_test_run_tells_the_validation_state() {
        let edit = || tool("Edit", json!({"file_path": "/p/a.rs"}), Some(false));
        let run = |failed| tool("Bash", json!({"command": "cargo test --workspace"}), failed);
        let other = tool("Bash", json!({"command": "ls"}), Some(false));
        let done = |failed, stale| Tests::Done {
            command: "cargo test --workspace".into(),
            failed,
            stale,
        };
        assert_eq!(tests(&[edit(), other]), Tests::NotRun);
        assert_eq!(
            tests(&[edit(), run(None)]),
            Tests::Running("cargo test --workspace".into())
        );
        assert_eq!(tests(&[edit(), run(Some(true))]), done(true, false));
        assert_eq!(tests(&[run(Some(false)), edit()]), done(false, true));
        assert_eq!(
            tests(&[run(Some(true)), edit(), run(Some(false))]),
            done(false, false)
        );
    }
}
