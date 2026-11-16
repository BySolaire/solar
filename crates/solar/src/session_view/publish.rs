//! Publier une mission sur GitHub : le bouton de la vue Modifications, l'état
//! du dépôt (compte, droits) et le dialogue en deux étapes. Voir
//! `docs/tickets/github/spec.md`.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Disableable, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rust_i18n::t;
use solar_core::github::{Gh, Reason, Repo};

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
            .on_click(cx.listener(|this, _, _, cx| {
                this.publishing = true;
                cx.notify();
            }));
        Some(
            match reason {
                Some(reason) => button.tooltip(reason),
                None => button,
            }
            .into_any_element(),
        )
    }

    /// Le dialogue de publication, au-dessus de la vue.
    pub(super) fn render_publish_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.publishing {
            return None;
        }
        let repo = self.github.as_ref().and_then(|found| found.as_ref().ok())?;
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let step = |n: &str, label: String, current: bool| {
            div()
                .font_weight(FontWeight::BOLD)
                .when(!current, |d| d.text_color(muted))
                .child(format!("{n}  {label}"))
        };
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
                        .border_color(theme.border)
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
                                        .gap_3()
                                        .child(step(
                                            "1",
                                            t!("publish.step.commit").to_string(),
                                            true,
                                        ))
                                        .child(div().text_color(muted).child("—"))
                                        .child(step("2", t!("publish.step.pr").to_string(), false)),
                                )
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .text_color(muted)
                                        .child(Icon::new(IconName::Github).small())
                                        .child(repo_text(repo)),
                                ),
                        )
                        .child(
                            h_flex()
                                .flex_shrink_0()
                                .justify_end()
                                .gap_2()
                                .px_6()
                                .py_4()
                                .border_t_1()
                                .border_color(theme.border)
                                .child(
                                    Button::new("publish-cancel")
                                        .ghost()
                                        .small()
                                        .label(t!("publish.cancel").to_string())
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.publishing = false;
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}
