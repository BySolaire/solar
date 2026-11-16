//! La revue d'une mission : ce qui a changé depuis l'instantané pris à son
//! lancement, la validation tirée des tests que Claude a lancés, et la décision.
//! Accepter n'est qu'une décision : aucun commit. Restaurer ne remet que les
//! fichiers de la mission, après confirmation, et jamais s'ils ont bougé depuis
//! l'examen.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::diff::{Diff, DiffFile, DiffState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rust_i18n::t;
use solar_core::agent::Provider;
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
        at: Option<chrono::DateTime<chrono::Local>>,
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
                finished,
                ..
            } if name == "Bash" => {
                let command = input["command"].as_str().unwrap_or_default();
                TEST_COMMANDS
                    .iter()
                    .any(|test| command.contains(test))
                    .then(|| {
                        let first = command.lines().next().unwrap_or_default().to_string();
                        (ix, first, result, *finished)
                    })
            }
            _ => None,
        });
    match last {
        None => Tests::NotRun,
        Some((_, command, None, _)) => Tests::Running(command),
        Some((ix, command, Some((failed, _)), at)) => Tests::Done {
            command,
            failed: *failed,
            stale: items[ix + 1..].iter().any(edits),
            at,
        },
    }
}

impl SessionView {
    /// Le dépôt privé des instantanés, sur le dossier où la mission travaille.
    pub(super) fn store(&self) -> Store {
        let store = Store::for_project(&self.checkpoints, &self.project);
        match self.mission.as_ref().and_then(|m| m.worktree.as_deref()) {
            Some(copy) => store.in_worktree(copy),
            None => store,
        }
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
        // Une copie retirée ne se relit plus : on montre ce qui a été appliqué.
        let frozen = self
            .mission
            .as_ref()
            .filter(|m| m.worktree.as_ref().is_some_and(|dir| !dir.is_dir()))
            .map(|m| m.accepted.clone());
        if let Some(None) = frozen {
            self.review = None;
            cx.notify();
            return;
        }
        self.review_busy = true;
        cx.notify();
        let store = self.store();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_spawn(async move {
                    let tree = match frozen.flatten() {
                        Some(tree) => tree,
                        None => store.snapshot()?,
                    };
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
                    if this.mission.as_ref().is_some_and(|m| m.worktree.is_some()) {
                        return this.apply_copy(now, cx);
                    }
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

    /// Applique au projet le résultat examiné dans la copie de travail, puis
    /// retire la copie. Si l'un des fichiers a changé dans le projet depuis le
    /// départ de la mission, rien n'est écrit.
    fn apply_copy(&mut self, reviewed: String, cx: &mut Context<Self>) {
        let (Some(base), Some(dir), Some(review)) = (
            self.mission.as_ref().and_then(|m| m.checkpoint.clone()),
            self.mission.as_ref().and_then(|m| m.worktree.clone()),
            self.review.as_ref(),
        ) else {
            return;
        };
        let paths: Vec<String> = review.changes.iter().map(|c| c.path.clone()).collect();
        let project = Store::for_project(&self.checkpoints, &self.project);
        self.review_busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_spawn(async move {
                    project.put(&reviewed, &base, &paths)?;
                    project.remove_worktree(&dir)?;
                    Ok((reviewed, paths.len()))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.review_busy = false;
                this.review_note = Some(match done {
                    Ok((reviewed, count)) => {
                        this.leave_copy();
                        if let Some(mission) = this.mission.as_mut() {
                            mission.status = MissionStatus::Accepted;
                            mission.accepted = Some(reviewed);
                        }
                        t!("review.applied", count = count).into()
                    }
                    Err(RestoreError::ChangedSinceReview(paths)) => {
                        t!("review.conflict", files = paths.join(", ")).into()
                    }
                    Err(RestoreError::Io(error)) => t!("review.failed", error = error).into(),
                });
                cx.notify();
            });
        })
        .detach();
    }

    /// La copie est retirée : ce qui y tournait s'arrête avec elle.
    pub(super) fn leave_copy(&mut self) {
        self.claude = None;
        self.reviewer = None;
        self.reviewing = false;
        self.preview.server = None;
    }

    /// Remet les fichiers de la mission dans leur état d'avant, et eux seuls.
    /// Une mission dans sa copie de travail : la copie est abandonnée, le
    /// projet n'a jamais été touché.
    pub(super) fn restore_result(&mut self, cx: &mut Context<Self>) {
        if let Some(dir) = self.mission.as_ref().and_then(|m| m.worktree.clone()) {
            self.confirm_restore = false;
            let project = Store::for_project(&self.checkpoints, &self.project);
            cx.spawn(async move |this, cx| {
                let done = cx
                    .background_spawn(async move { project.remove_worktree(&dir) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    match done {
                        Ok(()) => {
                            this.leave_copy();
                            if let Some(mission) = this.mission.as_mut() {
                                mission.status = MissionStatus::Restored;
                            }
                            this.review_note = Some(t!("review.discarded").into());
                        }
                        Err(error) => {
                            this.review_note = Some(t!("review.failed", error = error).into());
                        }
                    }
                    this.refresh_review(cx);
                });
            })
            .detach();
            return;
        }
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
                    store.put(&base, &reviewed, &paths).map(|_| paths.len())
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
                at,
            } => {
                // L'heure du jour, ou la date si le test n'est pas d'aujourd'hui.
                let at = at.map(|at| {
                    let today = at.date_naive() == chrono::Local::now().date_naive();
                    at.format(if today { "%H:%M" } else { "%d/%m %H:%M" })
                        .to_string()
                });
                let command = match at {
                    Some(at) => t!("review.tests.when", command = command, at = at).to_string(),
                    None => command,
                };
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
            && !self.reviewing
            && status == Some(MissionStatus::Running);
        // La dernière relecture d'un agent, périmée si le projet a changé depuis.
        let reviewed = self
            .review
            .as_ref()
            .zip(mission.and_then(|m| m.reviews.last()));
        let agent_review = reviewed.map(|(review, done)| {
            let stale = done.tree != review.tree;
            let (key, color) = match (stale, done.ok) {
                (true, _) => ("reviewer.verdict.stale", muted),
                (false, Some(true)) => ("reviewer.verdict.ok", success),
                (false, Some(false)) => ("reviewer.verdict.issues", danger),
                (false, None) => ("reviewer.verdict.unclear", warning),
            };
            let at = chrono::DateTime::parse_from_rfc3339(&done.at)
                .map(|at| at.with_timezone(&chrono::Local).format("%H:%M").to_string())
                .unwrap_or_default();
            v_flex()
                .gap_1()
                .child(
                    div()
                        .text_color(color)
                        .child(t!(key, agent = done.agent, at = at).to_string()),
                )
                .when(!stale, |column| {
                    column.child(
                        div()
                            .id("agent-review")
                            .max_h(px(160.))
                            .overflow_y_scroll()
                            .child(
                                TextView::markdown("agent-review-text", done.text.clone())
                                    .selectable(true),
                            ),
                    )
                })
                .when(!stale && done.ok == Some(false) && can_decide, |column| {
                    column.child(
                        h_flex().child(
                            Button::new("forward-review")
                                .ghost()
                                .small()
                                .icon(IconName::Send)
                                .label(t!("reviewer.send").to_string())
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.send_review(window, cx)),
                                ),
                        ),
                    )
                })
        });
        // Une mission dans sa copie : accepter l'applique, restaurer l'abandonne.
        let copy = mission.is_some_and(|m| m.worktree.is_some());
        let key =
            |in_place: &'static str, in_copy: &'static str| if copy { in_copy } else { in_place };
        let decision = match status {
            Some(MissionStatus::Accepted) => Some(note(
                t!(key("review.accepted", "review.applied_state")).to_string(),
            )),
            Some(MissionStatus::Restored) => Some(note(
                t!(key("review.restored_state", "review.discarded_state")).to_string(),
            )),
            _ => None,
        };
        let actions = h_flex()
            .flex_wrap()
            .gap_2()
            .when(can_decide && !self.confirm_restore, |row| {
                row.child(
                    Button::new("accept-result")
                        .primary()
                        .small()
                        .icon(IconName::Check)
                        .label(t!(key("review.accept", "review.apply")).to_string())
                        .on_click(cx.listener(|this, _, _, cx| this.accept_result(cx))),
                )
                .child({
                    let view = cx.entity();
                    Button::new("start-review")
                        .ghost()
                        .small()
                        .icon(IconName::ScanSearch)
                        .label(t!("reviewer.start").to_string())
                        .dropdown_menu(move |menu, _, _| {
                            Provider::ALL.into_iter().fold(menu, |menu, provider| {
                                let view = view.clone();
                                let installed = provider.installed();
                                let name = if installed {
                                    provider.name().to_string()
                                } else {
                                    t!("reviewer.missing", agent = provider.name()).to_string()
                                };
                                menu.item(PopupMenuItem::new(name).disabled(!installed).on_click(
                                    move |_, _, cx| {
                                        view.update(cx, |this, cx| this.start_review(provider, cx))
                                    },
                                ))
                            })
                        })
                        .on_open_change(self.page_under_menu(cx))
                })
                .child(
                    Button::new("restore-result")
                        .ghost()
                        .small()
                        .icon(IconName::Undo2)
                        .label(t!(key("review.restore", "review.discard")).to_string())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirm_restore = true;
                            cx.notify();
                        })),
                )
            })
            .when(can_decide && self.confirm_restore, |row| {
                row.child(
                    div().flex_1().child(
                        t!(
                            key("review.restore.confirm", "review.discard.confirm"),
                            count = changes
                        )
                        .to_string(),
                    ),
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
                        .label(t!(key("review.restore.go", "review.discard.go")).to_string())
                        .on_click(cx.listener(|this, _, _, cx| this.restore_result(cx))),
                )
            });

        // Dans une petite fenêtre, l'en-tête défile pour laisser place au diff ;
        // les décisions restent visibles dessous.
        let header = v_flex()
            .id("review-head")
            .flex_shrink_0()
            .max_h(relative(0.5))
            .overflow_y_scroll()
            .px_6()
            .pt_3()
            .gap_2()
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
            .when(self.reviewing, |column| {
                column.child(
                    h_flex()
                        .gap_2()
                        .child(
                            div().flex_1().text_color(muted).child(
                                t!("reviewer.running", reviewer = self.reviewer_provider.name())
                                    .to_string(),
                            ),
                        )
                        .child(
                            Button::new("stop-review")
                                .ghost()
                                .small()
                                .icon(IconName::Square)
                                .label(t!("reviewer.stop").to_string())
                                .on_click(cx.listener(|this, _, _, cx| this.stop_review(cx))),
                        ),
                )
            })
            .children(agent_review)
            .children(
                self.review_note
                    .clone()
                    .map(|text| div().text_color(warning).child(text)),
            );
        let actions = div()
            .flex_shrink_0()
            .px_6()
            .pt_2()
            .pb_3()
            .border_b_1()
            .border_color(border)
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
                .child(actions)
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
            finished: None,
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
            at: None,
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
        // L'heure du résultat accompagne la validation.
        let at = chrono::Local::now();
        let mut timed = run(Some(false));
        if let Item::Tool { finished, .. } = &mut timed {
            *finished = Some(at);
        }
        assert!(matches!(tests(&[timed]), Tests::Done { at: Some(when), .. } if when == at));
    }
}
