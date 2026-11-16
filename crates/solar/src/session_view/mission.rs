//! Le cycle de vie d'une mission dans sa conversation : reprise à la
//! réouverture, lancement dans sa copie, annulation, nouvelle tentative, et
//! les exécutions de bot qui la lancent seules.

use super::*;

impl SessionView {
    /// Reprend la mission d'une conversation rouverte : tant qu'elle n'est pas
    /// lancée, Claude reste en mode plan, sans rien écrire.
    pub fn restore_mission(&mut self, mission: Option<Mission>, cx: &mut Context<Self>) {
        // L'historique n'a pas été trouvé dans le projet : il est rangé ailleurs,
        // dans la copie de la mission ou chez un autre fournisseur.
        if self.items.is_empty() {
            self.mission = mission.clone();
            self.load_history(cx);
        }
        // Un tour inachevé dans l'historique : SOLAR a été fermé pendant que
        // Claude travaillait. On le dit, sans rien relancer.
        let unfinished = matches!(
            self.items.last(),
            Some(Item::User(_) | Item::Tool { result: None, .. })
        );
        self.interrupted = unfinished
            && mission.as_ref().is_some_and(|mission| {
                matches!(
                    mission.status,
                    MissionStatus::Planning | MissionStatus::Running
                )
            });
        if mission.as_ref().is_some_and(|mission| {
            matches!(
                mission.status,
                MissionStatus::Planning | MissionStatus::Ready
            )
        }) {
            self.mode = PLAN_MODE;
        }
        // Une mission en cours reprend dans le mode choisi à son lancement, pas
        // dans le mode par défaut des nouvelles conversations.
        if let Some(mode) = mission
            .as_ref()
            .filter(|mission| mission.status == MissionStatus::Running)
            .and_then(|mission| mission.launched_mode.as_deref())
            .and_then(known_mode)
        {
            self.mode = mode;
        }
        // Une PR ouverte : son état et ses checks sont relus dès la réouverture.
        let published = mission
            .as_ref()
            .is_some_and(|m| m.publication.as_ref().is_some_and(|p| p.pr.is_some()));
        self.mission = mission;
        if published {
            self.load_github(cx);
        }
    }

    /// Un plan attend une réponse du CLI en cours.
    pub(super) fn plan_pending(&self) -> bool {
        self.permission
            .as_ref()
            .is_some_and(|permission| permission.tool == PLAN_TOOL)
    }

    /// Lance la mission dans le mode choisi. Le mode est envoyé avant l'accord :
    /// sinon le CLI reviendrait un instant au mode d'avant la planification.
    /// L'instantané du projet est pris d'abord, en arrière-plan : c'est la base
    /// de la revue. Claude ne reçoit l'accord qu'ensuite.
    pub(super) fn launch_mission(&mut self, mode: &'static str, cx: &mut Context<Self>) {
        if self.launching {
            return;
        }
        // La copie de travail prise au plan est la base : rien à reprendre.
        if self
            .mission
            .as_ref()
            .is_some_and(|m| m.worktree.is_some() && m.checkpoint.is_some())
        {
            self.start_mission(mode, cx);
            return;
        }
        self.launching = true;
        cx.notify();
        // Une tentative neuve, sans conversation, part dans sa copie ; une mission
        // planifiée dans le projet y reste, sa conversation y est rangée.
        let fresh = self.session_id.is_none() && !self.started();
        let store = solar_core::checkpoint::Store::for_project(&self.checkpoints, &self.project);
        let id = copy_id();
        let project = self.project.clone();
        cx.spawn(async move |this, cx| {
            let base = cx
                .background_spawn(async move {
                    let launch = solar_core::github::launch_state(&project).ok();
                    if fresh && let Some((base, dir)) = store.isolate(&id)? {
                        return Ok((base, Some(dir), launch));
                    }
                    store.checkpoint().map(|base| (base, None, None))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.launching = false;
                match base {
                    Ok((base, dir, launch)) => {
                        // L'agent lancé d'avance tourne dans le projet : il repart
                        // dans la copie.
                        if dir.is_some() {
                            this.claude = None;
                        }
                        if let Some(mission) = this.mission.as_mut() {
                            mission.checkpoint = Some(base);
                            mission.worktree = dir;
                            mission.launch = launch;
                        }
                    }
                    // Sans instantané, la mission part quand même ; la revue le dira.
                    Err(error) => this.note(t!("review.no_checkpoint", error = error), true, cx),
                }
                this.start_mission(mode, cx);
            });
        })
        .detach();
    }

    pub(super) fn start_mission(&mut self, mode: &'static str, cx: &mut Context<Self>) {
        let pending = self.permission.take().filter(|p| p.tool == PLAN_TOOL);
        self.mode = mode;
        self.launch_mode = mode;
        let approved = self
            .brief
            .take()
            .unwrap_or_else(|| t!("mission.approved").to_string());
        let sent = self.ensure_claude(cx).and_then(|claude| {
            claude.set_permission_mode(mode)?;
            match &pending {
                Some(plan) => claude.allow(&plan.request_id, &plan.input),
                // Après un redémarrage, la demande du CLI est perdue : on le dit en clair.
                None => claude.send(&approved, &[]),
            }
        });
        if let Err(error) = sent {
            self.fail(error, cx);
            return;
        }
        if pending.is_none() {
            self.push(Item::User(approved.into()), cx);
        }
        self.interrupted = false;
        if let Some(mission) = self.mission.as_mut() {
            mission.status = MissionStatus::Running;
            mission.launched_mode = Some(mode.to_string());
        }
        self.begin_turn(cx);
        self.status = Status::Working;
        cx.notify();
    }

    /// Abandonne la mission avant son lancement : Claude, toujours en mode plan,
    /// apprend qu'il doit s'arrêter ; la conversation et ses plans restent.
    pub(super) fn cancel_mission(&mut self, cx: &mut Context<Self>) {
        let pending = self.permission.take().filter(|p| p.tool == PLAN_TOOL);
        if let (Some(plan), Some(claude)) = (&pending, self.claude.as_mut()) {
            let _ = claude.deny(
                &plan.request_id,
                "The user cancelled this mission. Stop here and do not change anything.",
            );
        }
        if let Some(mission) = self.mission.as_mut() {
            mission.status = MissionStatus::Cancelled;
        }
        // Une mission annulée n'écrira plus : sa copie part. Une relance en refait une.
        if let Some(dir) = self.mission.as_ref().and_then(|m| m.worktree.clone()) {
            self.leave_copy();
            let project =
                solar_core::checkpoint::Store::for_project(&self.checkpoints, &self.project);
            cx.background_spawn(async move {
                let _ = project.remove_worktree(&dir);
            })
            .detach();
        }
        self.note(t!("mission.cancelled"), false, cx);
        cx.notify();
    }

    /// Ouvre la tentative suivante : même plan, session neuve. Les problèmes
    /// relevés par le relecteur, s'il y en a, lui sont donnés.
    pub(super) fn relaunch(&mut self, cx: &mut Context<Self>) {
        let Some(mission) = self.mission.as_mut() else {
            return;
        };
        let attempt = mission.attempt + 1;
        mission.next_attempt = Some(attempt);
        let plan = mission.plans.last().cloned().unwrap_or_default();
        let issues = mission
            .reviews
            .last()
            .filter(|review| review.ok == Some(false))
            .map(|review| t!("mission.attempt.issues", text = review.text).to_string())
            .unwrap_or_default();
        // La PR de cette tentative, ou celle qu'elle remplaçait déjà.
        let replaces = mission
            .publication
            .as_ref()
            .and_then(|publication| publication.pr.as_ref())
            .map(|pr| pr.number)
            .or(mission.replaces);
        let next = Mission {
            status: MissionStatus::Running,
            plans: mission.plans.clone(),
            launched_mode: mission.launched_mode.clone(),
            replaces,
            attempt,
            ..Mission::default()
        };
        let brief = t!(
            "mission.attempt.brief",
            n = attempt,
            plan = plan,
            issues = issues
        )
        .to_string();
        let title = t!("mission.attempt.title", title = self.title, n = attempt).to_string();
        cx.emit(Relaunch {
            project: self.project.clone(),
            provider: self.provider,
            title: title.into(),
            mission: next,
            brief,
        });
        cx.notify();
    }

    /// Démarre une tentative reçue de la fenêtre : instantané, puis lancement
    /// dans le mode choisi pour la mission.
    pub fn start_attempt(&mut self, cx: &mut Context<Self>) {
        let mode = self
            .mission
            .as_ref()
            .and_then(|mission| mission.launched_mode.as_deref())
            .and_then(known_mode)
            .unwrap_or(MODES[0]);
        self.mode = mode;
        self.launch_mission(mode, cx);
    }

    /// Une exécution de bot : une mission planifiée sur sa consigne, comme si
    /// elle était envoyée de l'accueil avec Planifier. En mission complète, le
    /// plan part dès son arrivée dans le mode du bot.
    pub(crate) fn run_for_bot(
        &mut self,
        bot: String,
        text: String,
        mode: &'static str,
        whole: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.bot = Some(bot);
        self.set_mode(mode, cx);
        self.planning = true;
        self.auto_launch = whole.then_some(mode);
        self.input
            .update(cx, |input, cx| input.set_value(text, window, cx));
        self.send(window, cx);
    }

    /// L'agent n'a pas pu démarrer et aucune conversation n'existe : son message.
    pub(crate) fn launch_failure(&self) -> Option<SharedString> {
        let idle = self.session_id.is_none() && !self.launching && self.status == Status::Idle;
        self.launch_error.clone().filter(|_| idle)
    }
}
