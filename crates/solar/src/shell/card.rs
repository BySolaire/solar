//! La fiche d'un projet : ses conventions (modifiables), l'état de son dépôt
//! git, ses missions et les copies de travail qu'elles ont laissées sur le
//! disque.

use std::path::PathBuf;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rust_i18n::t;
use solar_core::checkpoint::{self, Store};
use solar_core::project::{self, Convention, Copy, GitSummary, SaveError};
use solar_core::state::{MissionStatus, Session};

use super::{Shell, mission_words, short_age};
use crate::theme::caps;

pub(super) struct Card {
    pub(super) path: PathBuf,
    conventions: Vec<Convention>,
    /// `None` tant que l'état git est lu ; `Some(None)` hors d'un dépôt.
    git: Option<Option<GitSummary>>,
    pub(super) copies: Option<Vec<Copy>>,
    pub(super) editing: Option<Editing>,
    /// La copie dont le retrait attend confirmation.
    confirm_remove: Option<PathBuf>,
    pub(super) note: Option<String>,
}

/// Une convention ouverte dans l'éditeur, avec ce qu'elle contenait à
/// l'ouverture : l'enregistrement refuse d'écraser un changement fait ailleurs.
pub(super) struct Editing {
    path: &'static str,
    pub(super) editor: Entity<TextareaState>,
    opened: Option<String>,
}

/// Une mission qui se sert encore de sa copie : en préparation, prête ou en cours.
fn alive(status: MissionStatus) -> bool {
    matches!(
        status,
        MissionStatus::Planning | MissionStatus::Ready | MissionStatus::Running
    )
}

impl Shell {
    pub(super) fn open_card(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.importing = None;
        self.settings_open = false;
        self.card = Some(Card {
            conventions: project::conventions(&path),
            path,
            git: None,
            copies: None,
            editing: None,
            confirm_remove: None,
            note: None,
        });
        self.reload_card(cx);
        self.sync_previews(cx);
        cx.notify();
    }

    /// Relit l'état git et les copies en arrière-plan.
    fn reload_card(&mut self, cx: &mut Context<Self>) {
        let Some(card) = self.card.as_mut() else {
            return;
        };
        card.conventions = project::conventions(&card.path);
        let path = card.path.clone();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_spawn(async move {
                    let worktrees = checkpoint::worktrees(&Store::default_dir());
                    (
                        project::git_summary(&path),
                        project::copies(&worktrees, &path),
                        path,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let (git, copies, path) = read;
                if let Some(card) = this.card.as_mut().filter(|card| card.path == path) {
                    card.git = Some(git);
                    card.copies = Some(copies);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn edit_convention(
        &mut self,
        path: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(card) = self.card.as_mut() else {
            return;
        };
        let opened = std::fs::read_to_string(card.path.join(path)).ok();
        let text = opened.clone().unwrap_or_default();
        let editor = cx.new(|cx| TextareaState::new(window, cx).auto_grow(12, 32));
        editor.update(cx, |editor, cx| {
            editor.set_value(text, window, cx);
            editor.focus(window, cx);
        });
        card.editing = Some(Editing {
            path,
            editor,
            opened,
        });
        card.note = None;
        cx.notify();
    }

    pub(super) fn save_convention(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(card) = self.card.as_mut() else {
            return;
        };
        let Some(editing) = card.editing.as_mut() else {
            return;
        };
        let text = editing.editor.read(cx).value().to_string();
        let file = card.path.join(editing.path);
        card.note = Some(
            match project::save(&file, editing.opened.as_deref(), &text) {
                Ok(()) => {
                    editing.opened = Some(text);
                    card.conventions = project::conventions(&card.path);
                    t!("card.saved", file = editing.path).into()
                }
                Err(SaveError::ChangedElsewhere) => t!("card.changed_elsewhere").into(),
                Err(SaveError::Io(error)) => t!("card.save_failed", error = error).into(),
            },
        );
        let _ = window;
        cx.notify();
    }

    /// Reprend la convention telle qu'elle est sur le disque ; ce qui était
    /// tapé est abandonné.
    fn reload_convention(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self
            .card
            .as_ref()
            .and_then(|card| card.editing.as_ref())
            .map(|e| e.path)
        else {
            return;
        };
        self.edit_convention(path, window, cx);
    }

    pub(super) fn remove_copy(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        let Some(card) = self.card.as_mut() else {
            return;
        };
        card.confirm_remove = None;
        let store = Store::for_project(&Store::default_dir(), &card.path);
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_spawn(async move { store.remove_worktree(&root) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(card) = this.card.as_mut() {
                    card.note = Some(match done {
                        Ok(()) => t!("card.copy_removed").into(),
                        Err(error) => t!("card.save_failed", error = error).into(),
                    });
                }
                this.reload_card(cx);
            });
        })
        .detach();
    }

    pub(super) fn render_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(card) = self.card.as_ref() else {
            return div().into_any_element();
        };
        let theme = cx.theme();
        let (muted, border) = (theme.muted_foreground, theme.border);
        let note = |text: String| div().text_sm().text_color(muted).child(text);
        let section = |title: String| {
            div()
                .pt_6()
                .pb_2()
                .text_xs()
                .text_color(muted)
                .child(caps(&title))
        };
        let row = || h_flex().py_3().gap_4().border_b_1().border_color(border);
        let project = self
            .state
            .projects
            .iter()
            .find(|known| known.path == card.path);
        let sessions: Vec<&Session> = project
            .map(|p| p.sessions.iter().collect())
            .unwrap_or_default();
        let name = card
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Conventions : une ligne par fichier, et l'éditeur sous celle ouverte.
        let conventions = card.conventions.iter().enumerate().map(|(ix, convention)| {
            let path = convention.path;
            let open = card
                .editing
                .as_ref()
                .is_some_and(|editing| editing.path == path);
            let detail = match convention.file {
                Some((bytes, modified)) => t!(
                    "card.convention_detail",
                    size = human_size(bytes),
                    age = short_age(modified)
                )
                .to_string(),
                None => t!("card.convention_missing").to_string(),
            };
            let exists = convention.file.is_some();
            let project_path = card.path.clone();
            v_flex()
                .border_b_1()
                .border_color(border)
                .child(
                    h_flex()
                        .py_3()
                        .gap_4()
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(div().font_weight(FontWeight::BOLD).child(path))
                                .child(note(detail)),
                        )
                        .when(exists, |row| {
                            row.child(
                                Button::new(("convention-view", ix))
                                    .ghost()
                                    .small()
                                    .label(t!("card.view").to_string())
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.go_home(Some(project_path.clone()), window, cx);
                                        let home = this.home.clone();
                                        home.update(cx, |home, cx| {
                                            home.open_file(path.to_string(), cx)
                                        });
                                    })),
                            )
                        })
                        .when(!open, |row| {
                            row.child(
                                Button::new(("convention-edit", ix))
                                    .outline()
                                    .small()
                                    .label(if exists {
                                        t!("card.edit").to_string()
                                    } else {
                                        t!("card.create").to_string()
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.edit_convention(path, window, cx)
                                    })),
                            )
                        }),
                )
                .when_some(card.editing.as_ref().filter(|_| open), |column, editing| {
                    column.child(
                        v_flex()
                            .pb_3()
                            .gap_2()
                            .child(
                                div()
                                    .p_2()
                                    .rounded(px(4.))
                                    .border_1()
                                    .border_color(border)
                                    .font_family(crate::theme::READING_FONT)
                                    .child(Textarea::new(&editing.editor).appearance(false)),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Button::new("convention-save")
                                            .primary()
                                            .small()
                                            .label(t!("card.save").to_string())
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.save_convention(window, cx)
                                            })),
                                    )
                                    .child(
                                        Button::new("convention-reload")
                                            .ghost()
                                            .small()
                                            .label(t!("card.reload").to_string())
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.reload_convention(window, cx)
                                            })),
                                    )
                                    .child(
                                        Button::new("convention-close")
                                            .ghost()
                                            .small()
                                            .label(t!("card.close").to_string())
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                if let Some(card) = this.card.as_mut() {
                                                    card.editing = None;
                                                    card.note = None;
                                                }
                                                cx.notify();
                                            })),
                                    ),
                            ),
                    )
                })
        });

        let git = match &card.git {
            None => note(t!("card.reading").to_string()).into_any_element(),
            Some(None) => note(t!("card.no_git").to_string()).into_any_element(),
            Some(Some(git)) => v_flex()
                .child(
                    row()
                        .child(div().flex_1().child(t!("card.branch").to_string()))
                        .child(
                            git.branch
                                .clone()
                                .unwrap_or_else(|| t!("card.detached").to_string()),
                        ),
                )
                .child(
                    row()
                        .child(div().flex_1().child(t!("card.changes").to_string()))
                        .child(
                            t!(
                                "card.changes_count",
                                changed = git.changed,
                                untracked = git.untracked
                            )
                            .to_string(),
                        ),
                )
                .child(
                    row()
                        .child(div().flex_1().child(t!("card.last_commit").to_string()))
                        .child(
                            div().min_w_0().truncate().child(
                                git.last_commit
                                    .as_ref()
                                    .map(|(hash, subject, when)| {
                                        let age = t!("card.ago", age = short_age(*when));
                                        format!("{hash} · {subject} · {age}")
                                    })
                                    .unwrap_or_else(|| t!("card.no_commit").to_string()),
                            ),
                        ),
                )
                .into_any_element(),
        };

        let missions: Vec<&Session> = sessions
            .iter()
            .copied()
            .filter(|s| s.mission.is_some())
            .collect();
        let mission_rows = missions.iter().enumerate().map(|(ix, session)| {
            let mission = session.mission.as_ref().expect("filtré au-dessus");
            let (path, opened) = (card.path.clone(), (*session).clone());
            row()
                .id(("card-mission", ix))
                .cursor_pointer()
                .hover(|row| row.bg(theme.muted))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(session.title.clone()),
                )
                .child(note(mission_words(mission.status, session)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open(path.clone(), opened.clone(), window, cx)
                }))
        });

        let copies = match &card.copies {
            None => vec![note(t!("card.reading").to_string()).into_any_element()],
            Some(copies) if copies.is_empty() => {
                vec![note(t!("card.no_copies").to_string()).into_any_element()]
            }
            Some(copies) => copies
                .iter()
                .enumerate()
                .map(|(ix, copy)| {
                    // La mission qui se sert de cette copie, s'il y en a une.
                    let owner = sessions.iter().find(|session| {
                        session
                            .mission
                            .as_ref()
                            .and_then(|mission| mission.worktree.as_ref())
                            .is_some_and(|dir| dir.starts_with(&copy.root))
                    });
                    let in_use = owner
                        .and_then(|session| session.mission.as_ref())
                        .is_some_and(|mission| alive(mission.status));
                    let label = match owner {
                        Some(session) => session.title.clone(),
                        None => t!("card.copy_unowned").to_string(),
                    };
                    let id = copy
                        .root
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let confirming = card.confirm_remove.as_ref() == Some(&copy.root);
                    let root = copy.root.clone();
                    row()
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(div().truncate().child(label))
                                .child(note(format!("{id} · {}", human_size(copy.bytes)))),
                        )
                        .child(if in_use {
                            note(t!("card.copy_in_use").to_string()).into_any_element()
                        } else if confirming {
                            h_flex()
                                .gap_2()
                                .child(note(t!("card.copy_confirm").to_string()))
                                .child(
                                    Button::new(("copy-remove-go", ix))
                                        .primary()
                                        .small()
                                        .label(t!("card.remove").to_string())
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.remove_copy(root.clone(), cx)
                                        })),
                                )
                                .child(
                                    Button::new(("copy-remove-cancel", ix))
                                        .ghost()
                                        .small()
                                        .label(t!("card.close").to_string())
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(card) = this.card.as_mut() {
                                                card.confirm_remove = None;
                                            }
                                            cx.notify();
                                        })),
                                )
                                .into_any_element()
                        } else {
                            Button::new(("copy-remove", ix))
                                .outline()
                                .small()
                                .label(t!("card.remove").to_string())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(card) = this.card.as_mut() {
                                        card.confirm_remove = Some(root.clone());
                                    }
                                    cx.notify();
                                }))
                                .into_any_element()
                        })
                        .into_any_element()
                })
                .collect(),
        };

        div()
            .id("card")
            .size_full()
            .overflow_y_scroll()
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(760.))
                    .mx_auto()
                    .px_8()
                    .py_8()
                    .child(div().text_2xl().font_weight(FontWeight::BOLD).child(name))
                    .child(note(card.path.display().to_string()))
                    .children(card.note.clone().map(|text| div().pt_3().child(text)))
                    .child(section(t!("card.conventions").to_string()))
                    .children(conventions)
                    .child(section(t!("card.git").to_string()))
                    .child(git)
                    .child(section(t!("card.missions").to_string()))
                    .when(missions.is_empty(), |column| {
                        column.child(note(t!("card.no_missions").to_string()))
                    })
                    .children(mission_rows)
                    .child(section(t!("card.copies").to_string()))
                    .children(copies),
            )
            .into_any_element()
    }
}

/// Une taille lisible : « 840 o », « 12 Ko », « 3,4 Mo ».
fn human_size(bytes: u64) -> String {
    let units = [t!("unit.b"), t!("unit.kb"), t!("unit.mb"), t!("unit.gb")];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024. && unit < units.len() - 1 {
        size /= 1024.;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", units[0])
    } else {
        format!("{size:.1} {}", units[unit])
    }
}
