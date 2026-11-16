//! Publier une mission sur GitHub : le bouton de la vue Modifications, l'état
//! du dépôt (compte, droits) et le dialogue en deux étapes. Voir
//! `docs/tickets/github/spec.md`.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme, Disableable, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rust_i18n::t;
use solar_core::github::{self, CommitError, Gh, PushError, Reason, Repo};
use solar_core::state::Publication;

use super::{SessionView, Status};

/// Pourquoi un projet ne peut pas publier, en clair.
pub(crate) fn reason_text(reason: &Reason) -> String {
    match reason {
        Reason::NoGh => t!("github.reason.no_gh"),
        Reason::NoOrigin => t!("github.reason.no_origin"),
        Reason::NotGitHub(host) => t!("github.reason.not_github", host = host),
        Reason::LoggedOut(host) => t!("github.reason.logged_out", host = host),
        Reason::Fork => t!("github.reason.fork"),
        Reason::ReadOnly(account) => t!("github.reason.read_only", account = account),
        Reason::Failed(error) => t!("github.reason.failed", error = error),
    }
    .to_string()
}

/// Le dépôt et le compte qui publieront : « BySolaire/app · compte BySolaire ».
pub(crate) fn repo_text(repo: &Repo) -> String {
    t!("github.repo", repo = repo.name, account = repo.account).to_string()
}

impl SessionView {
    /// Relit, en arrière-plan, si le projet de la mission peut publier.
    pub(super) fn load_github(&mut self, cx: &mut Context<Self>) {
        if self.mission.as_ref().is_none_or(|m| m.worktree.is_none()) {
            return;
        }
        let project = self.project.clone();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn(async move { Gh::default().availability(&project) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.github = Some(found);
                cx.notify();
            });
        })
        .detach();
    }

    /// « Publier sur GitHub », à côté des décisions ; grisé avec la raison
    /// quand la publication n'est pas possible.
    pub(super) fn render_publish_button(
        &self,
        changes: usize,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mission = self.mission.as_ref()?;
        let reason = match (&mission.worktree, &self.github) {
            (None, _) => Some(t!("github.reason.no_copy").to_string()),
            (_, None) => Some(t!("github.reading").to_string()),
            (_, Some(Err(reason))) => Some(reason_text(reason)),
            _ if changes == 0 => Some(t!("github.reason.nothing").to_string()),
            _ if self.status != Status::Idle => Some(t!("github.reason.working").to_string()),
            _ => None,
        };
        let button = Button::new("publish")
            .ghost()
            .small()
            .icon(IconName::GitPullRequest)
            .label(t!("publish.open").to_string())
            .disabled(reason.is_some())
            .on_click(cx.listener(|this, _, window, cx| this.open_publish(window, cx)));
        Some(
            match reason {
                Some(reason) => button.tooltip(reason),
                None => button,
            }
            .into_any_element(),
        )
    }

    /// Ouvre le dialogue sur l'étape 1 : branche, fichiers de la mission et
    /// message, préremplis.
    pub(super) fn open_publish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(mission), Some(review)) = (self.mission.as_ref(), self.review.as_ref()) else {
            return;
        };
        let published = mission.publication.as_ref();
        let branch_text = published.map_or_else(
            || github::branch_name(&self.title, mission.attempt),
            |publication| publication.branch.clone(),
        );
        // Les fichiers que l'utilisateur avait modifiés avant le lancement ne
        // partent que s'il les coche.
        let dirty = mission
            .launch
            .as_ref()
            .map(|launch| launch.dirty.clone())
            .unwrap_or_default();
        let files = review
            .changes
            .iter()
            .map(|change| {
                let touched = dirty.contains(&change.path);
                PublishFile {
                    path: change.path.clone(),
                    lines: change
                        .lines
                        .map(|(added, removed)| format!("+{added} −{removed}"))
                        .unwrap_or_else(|| t!("review.binary").to_string()),
                    dirty: touched,
                    checked: !touched,
                }
            })
            .collect();
        let title = self.title.to_string();
        let branch = cx.new(|cx| {
            let mut input = InputState::new(window, cx);
            input.set_value(branch_text, window, cx);
            input
        });
        let message = cx.new(|cx| {
            let mut input = InputState::new(window, cx);
            input.set_value(title, window, cx);
            input
        });
        self.publish = Some(PublishForm {
            branch,
            message,
            files,
            busy: false,
            error: None,
            pushed: None,
        });
        cx.notify();
    }

    /// « Commit et push » : le commit dans la copie, puis le push, en
    /// arrière-plan ; la publication est notée dans la mission.
    fn commit_and_push(&mut self, cx: &mut Context<Self>) {
        let (Some(form), Some(mission)) = (self.publish.as_mut(), self.mission.as_ref()) else {
            return;
        };
        let Some(copy) = mission.worktree.clone() else {
            return;
        };
        let branch = form.branch.read(cx).value().trim().to_string();
        let message = form.message.read(cx).value().trim().to_string();
        let last_pushed = mission.publication.as_ref().and_then(|p| p.pushed.clone());
        let start = mission.launch.as_ref().map(|launch| launch.start.clone());
        let files: Vec<String> = form
            .files
            .iter()
            .filter(|file| file.checked)
            .map(|file| file.path.clone())
            .collect();
        if files.is_empty() || message.is_empty() {
            form.error = Some(t!("publish.error.empty").to_string());
            cx.notify();
            return;
        }
        form.busy = true;
        form.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_spawn({
                    let branch = branch.clone();
                    async move {
                        let sha = match github::commit_mission(&copy, &branch, &files, &message) {
                            // Un commit déjà fait dont le push avait échoué : on le repousse.
                            Err(CommitError::Nothing)
                                if github::branch_head(&copy, &branch).is_some_and(|head| {
                                    Some(&head) != last_pushed.as_ref()
                                        && Some(&head) != start.as_ref()
                                }) =>
                            {
                                Ok(github::branch_head(&copy, &branch).unwrap_or_default())
                            }
                            other => other,
                        }
                        .map_err(|error| match error {
                            CommitError::BadBranch => t!("publish.error.branch").to_string(),
                            CommitError::Nothing => t!("publish.error.nothing").to_string(),
                            CommitError::Failed(output) => {
                                t!("publish.error.commit", output = output).to_string()
                            }
                        })?;
                        Gh::default()
                            .push(&copy, &branch)
                            .map_err(|error| match error {
                                PushError::Moved => t!("publish.error.moved").to_string(),
                                PushError::Failed(output) => {
                                    t!("publish.error.push", output = output).to_string()
                                }
                            })?;
                        Ok::<_, String>(sha)
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                // Le push est fait même si le dialogue a été fermé entre-temps :
                // la mission le note dans tous les cas.
                let outcome = match done {
                    Ok(sha) => {
                        let Some(mission) = this.mission.as_mut() else {
                            return;
                        };
                        let publication = mission.publication.get_or_insert(Publication {
                            branch: branch.clone(),
                            commits: 0,
                            pushed: None,
                        });
                        publication.commits += 1;
                        publication.pushed = Some(sha);
                        Ok(t!(
                            "publish.pushed",
                            branch = branch,
                            count = publication.commits
                        )
                        .to_string())
                    }
                    Err(error) => Err(error),
                };
                if let Some(form) = this.publish.as_mut() {
                    form.busy = false;
                    match outcome {
                        Ok(done) => form.pushed = Some(done),
                        Err(error) => form.error = Some(error),
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn render_publish_files(&self, form: &PublishForm, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .gap_2()
            .children(form.files.iter().enumerate().map(|(ix, file)| {
                v_flex()
                    .gap_0p5()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Checkbox::new(("publish-file", ix))
                                    .checked(file.checked)
                                    .disabled(form.busy)
                                    .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                        if let Some(file) = this
                                            .publish
                                            .as_mut()
                                            .and_then(|form| form.files.get_mut(ix))
                                        {
                                            file.checked = *checked;
                                            cx.notify();
                                        }
                                    })),
                            )
                            .child(div().flex_1().min_w_0().truncate().child(file.path.clone()))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(file.lines.clone()),
                            ),
                    )
                    .when(file.dirty, |row| {
                        row.child(
                            h_flex()
                                .items_start()
                                .pl(px(24.))
                                .gap_1p5()
                                .text_xs()
                                .child(Icon::new(IconName::TriangleAlert).xsmall())
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .whitespace_normal()
                                        .child(t!("publish.dirty").to_string()),
                                ),
                        )
                    })
            }))
    }

    /// Le dialogue de publication, au-dessus de la vue.
    pub(super) fn render_publish_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let form = self.publish.as_ref()?;
        let repo = self.github.as_ref().and_then(|found| found.as_ref().ok())?;
        let theme = cx.theme();
        let (muted, border, danger, success) = (
            theme.muted_foreground,
            theme.border,
            theme.danger,
            theme.success,
        );
        let label = |text: String| {
            div()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(muted)
                .child(crate::theme::caps(&text))
        };
        let step = |n: &str, text: String, current: bool| {
            div()
                .font_weight(FontWeight::BOLD)
                .when(!current, |d| d.text_color(muted))
                .child(format!("{n}  {text}"))
        };
        let pushed = form.pushed.is_some();
        let base = self
            .mission
            .as_ref()
            .and_then(|mission| mission.launch.as_ref())
            .and_then(|launch| launch.branch.clone())
            .unwrap_or_else(|| repo.default_branch.clone());
        let body = v_flex()
            .gap_5()
            .children(form.error.clone().map(|error| {
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .gap_1p5()
                            .text_color(danger)
                            .child(Icon::new(IconName::CircleAlert).xsmall())
                            .child(t!("publish.failed").to_string()),
                    )
                    .child(
                        div()
                            .id("publish-error")
                            .max_h(px(160.))
                            .overflow_y_scroll()
                            .p_3()
                            .rounded(px(8.))
                            .border_1()
                            .border_color(border)
                            .text_xs()
                            .font_family(cx.theme().mono_font_family.clone())
                            .whitespace_normal()
                            .child(error),
                    )
            }))
            .map(|body| match &form.pushed {
                Some(done) => body.child(div().text_color(success).child(done.clone())),
                None => body
                    .child(
                        v_flex()
                            .gap_1()
                            .child(label(t!("publish.branch").to_string()))
                            .child(
                                Input::new(&form.branch).disabled(
                                    form.busy
                                        || self
                                            .mission
                                            .as_ref()
                                            .is_some_and(|m| m.publication.is_some()),
                                ),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_2()
                            .child(label(t!("publish.files").to_string()))
                            .child(self.render_publish_files(form, cx)),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(label(t!("publish.message").to_string()))
                            .child(Input::new(&form.message).disabled(form.busy)),
                    )
                    .child(div().text_xs().text_color(muted).whitespace_normal().child(
                        t!("publish.terms", base = base, account = repo.account).to_string(),
                    )),
            });
        let go = (!pushed).then(|| {
            Button::new("publish-go")
                .primary()
                .small()
                .icon(IconName::ArrowUp)
                .label(t!("publish.commit_push").to_string())
                .loading(form.busy)
                .on_click(cx.listener(|this, _, _, cx| this.commit_and_push(cx)))
        });
        Some(
            div()
                .id("publish-backdrop")
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .items_center()
                .p_4()
                .bg(rgb(crate::theme::BLACK).opacity(0.5))
                .occlude()
                .child(
                    v_flex()
                        .id("publish-dialog")
                        .w(px(560.))
                        .max_w_full()
                        .max_h_full()
                        .rounded(crate::theme::RADIUS)
                        .border_1()
                        .border_color(border)
                        .bg(rgb(crate::theme::BLACK))
                        .child(
                            v_flex()
                                .id("publish-body")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .p_6()
                                .gap_5()
                                .child(
                                    div()
                                        .text_lg()
                                        .font_weight(FontWeight::BOLD)
                                        .child(t!("publish.title").to_string()),
                                )
                                .child(
                                    h_flex()
                                        .flex_wrap()
                                        .gap_3()
                                        .child(step(
                                            "1",
                                            t!("publish.step.commit").to_string(),
                                            !pushed,
                                        ))
                                        .child(div().text_color(muted).child("—"))
                                        .child(step(
                                            "2",
                                            t!("publish.step.pr").to_string(),
                                            pushed,
                                        )),
                                )
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .text_color(muted)
                                        .child(Icon::new(IconName::Github).small())
                                        .child(repo_text(repo)),
                                )
                                .child(body),
                        )
                        .child(
                            h_flex()
                                .flex_shrink_0()
                                .justify_end()
                                .gap_2()
                                .px_6()
                                .py_4()
                                .border_t_1()
                                .border_color(border)
                                .children(go)
                                .child(
                                    Button::new("publish-cancel")
                                        .ghost()
                                        .small()
                                        .disabled(form.busy)
                                        .label(
                                            if pushed {
                                                t!("publish.close")
                                            } else {
                                                t!("publish.cancel")
                                            }
                                            .to_string(),
                                        )
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.publish = None;
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}

/// Le dialogue de publication ouvert : ses champs et son état.
pub(super) struct PublishForm {
    branch: Entity<InputState>,
    message: Entity<InputState>,
    files: Vec<PublishFile>,
    busy: bool,
    error: Option<String>,
    /// « Poussé sur … » : l'étape 1 est faite.
    pushed: Option<String>,
}

struct PublishFile {
    path: String,
    lines: String,
    /// Modifié par l'utilisateur avant le lancement : décoché par défaut.
    dirty: bool,
    checked: bool,
}
