//! L'agent relecteur d'une mission : une seconde session Claude, au contexte
//! neuf, en mode plan (lecture seule imposée par le moteur). Elle reçoit le plan
//! approuvé et le diff exact examiné, et rend un verdict lié à cet état. Elle
//! n'écrit rien ; c'est l'utilisateur qui décide quoi en faire.

use rust_i18n::t;
use solar_core::checkpoint::Store;
use solar_core::claude::{Claude, Event};
use solar_core::state::AgentReview;

use super::SessionView;
use gpui_kit::*;

/// Au-delà, le diff envoyé au relecteur est coupé ; il peut lire les fichiers.
const PATCH_LIMIT: usize = 60_000;

/// La relecture en cours : son processus, l'état relu et ce qu'elle a dit.
pub(super) struct Reviewer {
    claude: Claude,
    tree: String,
    text: String,
}

/// Le verdict écrit par le relecteur sur une ligne `VERDICT: OK` ou
/// `VERDICT: PROBLÈMES` (ou `ISSUES`).
pub(super) fn verdict(text: &str) -> Option<bool> {
    text.lines().find_map(|line| {
        let line = line.trim().trim_matches('*').trim();
        let rest = line
            .get(..8)
            .filter(|head| head.eq_ignore_ascii_case("verdict:"))
            .map(|_| line[8..].trim().to_uppercase())?;
        Some(rest.starts_with("OK"))
    })
}

impl SessionView {
    /// Lance la relecture de l'état examiné dans la revue.
    pub(super) fn start_review(&mut self, cx: &mut Context<Self>) {
        let (Some(base), Some(tree)) = (
            self.mission.as_ref().and_then(|m| m.checkpoint.clone()),
            self.review.as_ref().map(|review| review.tree.clone()),
        ) else {
            return;
        };
        if self.reviewer.is_some() {
            return;
        }
        let plan = self
            .mission
            .as_ref()
            .and_then(|m| m.plans.last().cloned())
            .unwrap_or_default();
        let store = Store::for_project(&self.checkpoints, &self.project);
        let project = self.project.clone();
        self.review_note = None;
        self.reviewing = true;
        self.reviewer_failed = false;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let patch = cx
                .background_spawn(
                    async move { store.patch(&base, &tree).map(|patch| (patch, tree)) },
                )
                .await;
            let _ = this.update(cx, |this, cx| match patch {
                Ok((patch, tree)) => this.spawn_reviewer(&project, plan, patch, tree, cx),
                Err(error) => {
                    this.reviewing = false;
                    this.review_note = Some(t!("reviewer.failed", error = error).into());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn spawn_reviewer(
        &mut self,
        project: &std::path::Path,
        plan: String,
        mut patch: String,
        tree: String,
        cx: &mut Context<Self>,
    ) {
        if patch.len() > PATCH_LIMIT {
            let mut cut = PATCH_LIMIT;
            while !patch.is_char_boundary(cut) {
                cut -= 1;
            }
            patch.truncate(cut);
            patch.push_str(&t!("reviewer.cut"));
        }
        let prompt = t!("reviewer.prompt", plan = plan, patch = patch).to_string();
        let (tx, rx) = smol::channel::unbounded();
        let started = Claude::spawn(project, None, "plan", move |event| {
            let _ = tx.send_blocking(event);
        })
        .and_then(|mut claude| claude.send(&prompt, &[]).map(|_| claude));
        match started {
            Ok(claude) => {
                self.reviewer = Some(Reviewer {
                    claude,
                    tree,
                    text: String::new(),
                });
                cx.spawn(async move |this, cx| {
                    while let Ok(event) = rx.recv().await {
                        let done = this.update(cx, |this, cx| this.on_reviewer(event, cx));
                        if !matches!(done, Ok(false)) {
                            break;
                        }
                    }
                })
                .detach();
            }
            Err(error) => {
                self.reviewing = false;
                self.review_note = Some(t!("reviewer.failed", error = error).into());
            }
        }
        cx.notify();
    }

    /// Rend `true` quand la relecture est finie.
    fn on_reviewer(&mut self, event: Event, cx: &mut Context<Self>) -> bool {
        let Some(reviewer) = self.reviewer.as_mut() else {
            return true;
        };
        match event {
            Event::Text(chunk) => reviewer.text.push_str(&chunk),
            // En mode plan, Claude peut rendre sa relecture comme un plan à
            // approuver : on la prend, et on refuse pour qu'il s'arrête là.
            Event::PermissionRequest {
                request_id,
                tool,
                input,
            } => {
                if let Some(text) = input["plan"].as_str() {
                    reviewer.text = text.to_string();
                }
                let _ = reviewer.claude.deny(
                    &request_id,
                    &format!("{tool} is not allowed: this is a read-only review. Stop here."),
                );
            }
            Event::TurnDone { error, .. } => {
                let mut reviewer = self.reviewer.take().expect("vérifié au-dessus");
                if let Some(error) = error {
                    reviewer.text = format!("{}\n\n{error}", reviewer.text);
                }
                let text = reviewer.text.trim().to_string();
                if let Some(mission) = self.mission.as_mut() {
                    mission.reviews.push(AgentReview {
                        agent: "Claude".into(),
                        ok: verdict(&text),
                        tree: reviewer.tree,
                        text,
                        at: chrono::Local::now().to_rfc3339(),
                    });
                }
                self.reviewing = false;
                cx.notify();
                return true;
            }
            Event::Exited => {
                self.reviewer = None;
                self.reviewing = false;
                self.reviewer_failed = true;
                self.review_note = Some(t!("reviewer.exited").into());
                cx.notify();
                return true;
            }
            _ => {}
        }
        cx.notify();
        false
    }

    /// Arrête la relecture en cours ; rien n'est enregistré.
    pub(super) fn stop_review(&mut self, cx: &mut Context<Self>) {
        self.reviewer = None;
        self.reviewing = false;
        cx.notify();
    }

    /// Transmet les problèmes relevés à l'implémenteur, comme une correction.
    pub(super) fn send_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = self.mission.as_ref().and_then(|m| m.reviews.last()) else {
            return;
        };
        let message = t!("reviewer.forward", text = review.text).to_string();
        self.input
            .update(cx, |input, cx| input.set_value(message, window, cx));
        self.pane = super::workspace::Pane::Conversation;
        self.set_preview_visible(true, cx);
        self.send(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use core::prelude::v1::test;

    use super::verdict;

    #[test]
    fn the_verdict_line_is_read() {
        assert_eq!(verdict("Relu.\nVERDICT: OK\nRien à redire."), Some(true));
        assert_eq!(
            verdict("**Verdict : problèmes**"),
            None,
            "le format exact est attendu"
        );
        assert_eq!(
            verdict("**VERDICT: PROBLÈMES**\n- test manquant"),
            Some(false)
        );
        assert_eq!(verdict("verdict: issues found"), Some(false));
        assert_eq!(verdict("Pas de verdict ici."), None);
    }
}
