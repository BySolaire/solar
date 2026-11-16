//! La conduite des bots : lancer une exécution, ses garde-fous, l'horaire, la
//! lecture des issues, le suivi des exécutions et leurs notifications. La vue
//! (liste, éditeur) est dans `bots_view.rs`.

use gpui_kit::*;
use rust_i18n::t;
use solar_core::agent::Provider;
use solar_core::bots::{self, Decision, PromptError, Run, Scope, Trigger};
use solar_core::github::{Gh, Issue};
use solar_core::library::{self, Template};

use super::Shell;
use crate::session_view::{MODES, SessionView, Status, known_mode};

/// Tous les combien les issues d'un bot sont relues, Solar ouvert.
pub(super) const ISSUE_POLL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// Une date rangée (RFC 3339) en heure locale.
pub(super) fn local(at: &str) -> Option<chrono::NaiveDateTime> {
    chrono::DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|at| at.with_timezone(&chrono::Local).naive_local())
}

/// Les modes proposés à un bot : tous sauf le plan, qui ne lancerait rien.
/// Un bot d'issues n'a pas l'accès complet : le texte d'une issue vient de son
/// auteur, et un modèle d'issue peut poser l'étiquette tout seul.
pub(super) fn bot_modes(provider: Provider, issues: bool) -> Vec<&'static str> {
    MODES
        .into_iter()
        .filter(|mode| *mode != "plan" && provider.supports(mode))
        .filter(|mode| !(issues && *mode == "bypassPermissions"))
        .collect()
}

pub(super) fn now() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

/// Les modèles disponibles pour un bot du projet : ceux du projet, les
/// personnels, les exemples.
pub(super) fn templates_for(project: &std::path::Path) -> Vec<Template> {
    library::load(
        &library::personal_dir(),
        Some(project),
        &crate::session_view::library_examples(),
    )
}

pub(super) fn prompt_problem(error: &PromptError) -> String {
    match error {
        PromptError::Empty => t!("bots.prompt_empty").to_string(),
        PromptError::MissingTemplate(name) => t!("bots.template_gone", name = name).to_string(),
        PromptError::Unfilled(holes) => t!("bots.unfilled", holes = holes.join(", ")).to_string(),
    }
}

impl Shell {
    pub(super) fn save_state(&self) {
        if let Err(error) = self.state.save(&self.state_path) {
            eprintln!("solar: bots non sauvegardés : {error}");
        }
    }

    pub(super) fn toggle_bot(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(bot) = self.state.bots.iter_mut().find(|bot| bot.id == id) {
            bot.enabled = !bot.enabled;
            // Réactivé, il reprend à partir de maintenant : la pause ne se rattrape pas.
            if bot.enabled {
                bot.checked = Some(now());
                bot.missed = 0;
            }
        }
        self.save_state();
        cx.notify();
    }

    /// Supprime le bot ; ses conversations restent, sans lien vers lui.
    pub(super) fn delete_bot(&mut self, id: &str, cx: &mut Context<Self>) {
        self.state.bots.retain(|bot| bot.id != id);
        self.save_state();
        if let Some(view) = self.bots_view.as_mut() {
            view.confirm_delete = None;
            view.note = None;
        }
        cx.notify();
    }

    /// Lance une exécution : une conversation neuve, en fond, qui planifie la
    /// mission avec la consigne du bot. Un échec est noté dans ses exécutions.
    /// « Lancer maintenant » : une exécution ; pour un bot d'issues, la
    /// recherche d'une nouvelle issue, tout de suite.
    pub(super) fn run_bot(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = self.bots_view.as_mut() {
            view.note = None;
        }
        let issues = self
            .state
            .bots
            .iter()
            .any(|bot| bot.id == id && matches!(bot.trigger, Trigger::Issues { .. }));
        if issues {
            self.poll_issues(id, true, window, cx);
        } else {
            self.start_run(id, None, false, window, cx);
        }
    }

    /// Lance une exécution : une conversation neuve, en fond, qui planifie la
    /// mission avec la consigne du bot (et l'issue). Un échec est noté ; venu
    /// d'un déclencheur, il met aussi le bot en pause, pour ne pas se répéter.
    pub(super) fn start_run(
        &mut self,
        id: &str,
        issue: Option<Issue>,
        triggered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(bot) = self.state.bots.iter().find(|bot| bot.id == id).cloned() else {
            return;
        };
        let at = now();
        // Garde-fous des déclencheurs : une exécution à la fois, un plafond par
        // jour. « Lancer maintenant » reste ton choix.
        if triggered {
            let reason = match bots::decide(
                &bot,
                self.bot_busy(id, cx),
                chrono::Local::now().date_naive(),
            ) {
                Decision::Go => None,
                Decision::Busy => Some(t!("bots.skipped_busy").to_string()),
                Decision::Capped => Some(t!("bots.skipped_cap", count = bot.daily_cap).to_string()),
            };
            if let Some(reason) = reason {
                if let Some(bot) = self.state.bots.iter_mut().find(|b| b.id == id) {
                    bot.skip(at, reason);
                }
                self.save_state();
                cx.notify();
                return;
            }
        }
        let text = bots::prompt_for(&bot, &templates_for(&bot.project), issue.as_ref());
        let mode = known_mode(&bot.mode)
            .filter(|mode| {
                bot_modes(bot.provider, matches!(bot.trigger, Trigger::Issues { .. }))
                    .contains(mode)
            })
            .ok_or_else(|| t!("bots.bad_mode", mode = bot.mode).to_string());
        let problem = match (&text, &mode) {
            (Err(error), _) => Some(prompt_problem(error)),
            (_, Err(problem)) => Some(problem.clone()),
            _ if !bot.project.is_dir() => Some(t!("bots.project_gone").to_string()),
            _ => None,
        };
        let number = issue.as_ref().map(|issue| issue.number);
        if let Some(problem) = problem {
            if triggered && let Some(bot) = self.state.bots.iter_mut().find(|b| b.id == id) {
                bot.enabled = false;
            }
            let problem = if triggered {
                t!("bots.paused_after", problem = problem).to_string()
            } else {
                problem
            };
            self.record_run(&bot.id, at, Some(problem), number, cx);
            return;
        }
        let (text, mode) = (text.unwrap_or_default(), mode.unwrap_or("acceptEdits"));
        let title = match number {
            Some(n) => format!("{} · #{n}", bot.name),
            None => bot.name.clone(),
        };
        let tab = cx.new(|cx| {
            let mut tab = SessionView::new(bot.project.clone(), None, title.into(), window, cx);
            tab.set_provider(bot.provider, window, cx);
            if let Some(model) = &bot.model {
                tab.set_model(model.clone(), cx);
            }
            if let Some(effort) = &bot.effort {
                tab.set_effort(effort.clone(), cx);
            }
            tab
        });
        cx.observe(&tab, |this, _, cx| this.sync(cx)).detach();
        cx.subscribe_in(&tab, window, Self::relaunch).detach();
        tab.update(cx, |tab, cx| {
            tab.run_for_bot(
                bot.id.clone(),
                text,
                mode,
                bot.scope == Scope::Mission,
                window,
                cx,
            )
        });
        // Une issue ne donne jamais deux missions.
        if let Some(n) = number
            && let Some(bot) = self.state.bots.iter_mut().find(|b| b.id == id)
            && !bot.handled.contains(&n)
        {
            bot.handled.push(n);
        }
        // En fond : l'exécution n'interrompt pas ce que tu regardes ; elle
        // apparaît dans la barre latérale dès que l'agent a créé sa conversation.
        self.background.push(tab.clone());
        self.bot_runs.push((bot.id.clone(), at.clone(), tab));
        self.record_run(&bot.id, at, None, number, cx);
    }

    /// Lit les issues du bot par `gh`, en arrière-plan, et confie la plus
    /// ancienne nouvelle issue à une mission : une par lecture, pour qu'un
    /// arriéré ne parte pas d'un coup.
    pub(super) fn poll_issues(
        &mut self,
        id: &str,
        by_hand: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(bot) = self.state.bots.iter().find(|bot| bot.id == id) else {
            return;
        };
        let Trigger::Issues { label, .. } = &bot.trigger else {
            return;
        };
        let (id, label, project) = (bot.id.clone(), label.clone(), bot.project.clone());
        self.issue_polls
            .insert(id.clone(), std::time::Instant::now());
        cx.spawn_in(window, async move |this, cx| {
            let found = cx
                .background_spawn(async move { Gh::default().issues(&project, &label) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let issue = match found {
                    Ok(found) => {
                        this.issue_errors.remove(&id);
                        let Some(bot) = this.state.bots.iter().find(|bot| bot.id == id) else {
                            return;
                        };
                        bots::new_issues(bot, &found)
                            .first()
                            .map(|issue| (*issue).clone())
                    }
                    Err(error) => {
                        this.issue_errors.insert(id.clone(), error);
                        None
                    }
                };
                match issue {
                    Some(issue) => this.start_run(&id, Some(issue), !by_hand, window, cx),
                    None if by_hand && !this.issue_errors.contains_key(&id) => {
                        if let Some(view) = this.bots_view.as_mut() {
                            view.note = Some(t!("bots.no_new_issue").to_string());
                        }
                    }
                    None => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Une exécution du bot est-elle encore ouverte : elle prépare son plan,
    /// l'attend, travaille, ou attend ta revue ?
    pub(super) fn bot_busy(&self, id: &str, cx: &App) -> bool {
        let starting = self
            .bot_runs
            .iter()
            .any(|(bot, _, view)| bot == id && view.read(cx).session_id.is_none());
        starting
            || self
                .state
                .projects
                .iter()
                .flat_map(|p| &p.sessions)
                .any(|session| {
                    session.bot.as_deref() == Some(id)
                        && session
                            .mission
                            .as_ref()
                            .is_some_and(|mission| mission.status.is_open())
                })
    }

    /// Un bot demande ton attention : une exécution a fini ou attend, des
    /// échéances ont été manquées, ou ses issues sont illisibles.
    pub(super) fn bots_need_you(&self) -> bool {
        !self.bot_attention.is_empty()
            || !self.issue_errors.is_empty()
            || self.state.bots.iter().any(|bot| bot.missed > 0)
    }

    /// Suit les exécutions de bot : quand l'une finit son tour ou attend une
    /// réponse, une notification part (dans l'app empaquetée) et l'entrée Bots
    /// porte un point jusqu'à ce que tu l'ouvres. Note aussi leur coût.
    pub(super) fn watch_bot_runs(&mut self, cx: &mut Context<Self>) {
        let shown = self
            .active
            .and_then(|ix| self.tabs.get(ix))
            .and_then(|tab| tab.read(cx).session_id.clone());
        let mut news = Vec::new();
        for view in self.tabs.iter().chain(&self.background) {
            let view = view.read(cx);
            let (Some(bot), Some(id)) = (&view.bot, &view.session_id) else {
                continue;
            };
            let before = self.bot_status.insert(id.clone(), view.status);
            if before == Some(Status::Working) && view.status != Status::Working {
                news.push((id.clone(), view.title.to_string(), view.status, bot.clone()));
            }
            if let Some(cost) = view.cost
                && let Some(run) = self
                    .state
                    .bots
                    .iter_mut()
                    .filter(|b| b.id == *bot)
                    .flat_map(|b| b.runs.iter_mut())
                    .find(|run| run.session.as_deref() == Some(id))
            {
                run.cost = Some(cost);
            }
        }
        for (id, title, status, _) in news {
            if shown.as_deref() == Some(&id) {
                continue;
            }
            let body = match status {
                Status::NeedsYou => t!("bots.notify.needs_you"),
                _ => t!("bots.notify.done"),
            };
            cx.show_system_notification(SystemNotification {
                tag: id.clone().into(),
                title: title.into(),
                body: body.into(),
                actions: Vec::new(),
            });
            self.bot_attention.insert(id);
        }
        if let Some(shown) = shown {
            self.bot_attention.remove(&shown);
        }
    }

    pub(super) fn clear_missed(&mut self, id: &str) {
        if let Some(bot) = self.state.bots.iter_mut().find(|bot| bot.id == id) {
            bot.missed = 0;
        }
    }

    /// Au lancement : les échéances passées Solar fermé sont comptées, pas
    /// lancées ; la vue Bots propose « Lancer une fois ».
    pub(super) fn catch_up_bots(&mut self) {
        let now_text = now();
        let Some(now) = local(&now_text) else { return };
        let mut changed = false;
        for bot in &mut self.state.bots {
            if matches!(bot.trigger, Trigger::Manual) {
                continue;
            }
            let since = bot.checked.as_deref().and_then(local).unwrap_or(now);
            if bot.enabled {
                bot.missed = bot
                    .missed
                    .saturating_add(bots::due_between(&bot.trigger, since, now));
            }
            bot.checked = Some(now_text.clone());
            changed = true;
        }
        if changed {
            self.save_state();
        }
    }

    /// Chaque minute, Solar ouvert : un bot dont une échéance est passée part
    /// une fois, même s'il en a passé plusieurs (veille de l'ordinateur).
    pub(super) fn tick_bots(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now_text = now();
        let Some(now) = local(&now_text) else { return };
        let mut due = Vec::new();
        let mut polls = Vec::new();
        for bot in &mut self.state.bots {
            if matches!(bot.trigger, Trigger::Manual) || !bot.enabled {
                continue;
            }
            // Les issues : relues toutes les 5 minutes.
            if matches!(bot.trigger, Trigger::Issues { .. }) {
                let polled = self.issue_polls.get(&bot.id);
                if polled.is_none_or(|at| at.elapsed() >= ISSUE_POLL) {
                    polls.push(bot.id.clone());
                }
                continue;
            }
            let since = bot.checked.as_deref().and_then(local).unwrap_or(now);
            if bots::due_between(&bot.trigger, since, now) > 0 {
                due.push(bot.id.clone());
            }
            bot.checked = Some(now_text.clone());
        }
        for id in due {
            self.start_run(&id, None, true, window, cx);
        }
        for id in polls {
            self.poll_issues(&id, false, window, cx);
        }
    }

    pub(super) fn record_run(
        &mut self,
        id: &str,
        at: String,
        problem: Option<String>,
        issue: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        if let Some(bot) = self.state.bots.iter_mut().find(|bot| bot.id == id) {
            bot.record(Run {
                at,
                session: None,
                problem,
                issue,
                cost: None,
                skipped: false,
            });
        }
        self.save_state();
        cx.notify();
    }

    /// Ouvre la conversation d'une exécution.
    pub(super) fn open_run(&mut self, session: &str, window: &mut Window, cx: &mut Context<Self>) {
        let found = self.state.projects.iter().find_map(|project| {
            let known = project.sessions.iter().find(|s| s.id == session)?;
            Some((project.path.clone(), known.clone()))
        });
        if let Some((project, session)) = found {
            self.open(project, session, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn an_issue_bot_never_gets_full_access() {
        for provider in Provider::ALL {
            assert!(!bot_modes(provider, true).contains(&"bypassPermissions"));
            assert!(!bot_modes(provider, false).contains(&"plan"));
        }
        assert!(bot_modes(Provider::Claude, false).contains(&"bypassPermissions"));
    }
}
