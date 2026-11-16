//! Publier une mission sur GitHub : le bouton de la vue Modifications, l'état
//! du dépôt (compte, droits) et le dialogue en deux étapes. Voir
//! `docs/tickets/github/spec.md`.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::{ActiveTheme, Disableable, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rust_i18n::t;
use solar_core::github::{
    self, Bucket, Check, CommitError, Gh, NewPr, PrState, PushError, Reason, Repo,
};
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

/// Ouvre une adresse venue de GitHub dans le navigateur, si c'est bien une
/// page web : un lien de check ne doit lancer aucune autre application.
fn open_web(url: &str, cx: &mut App) {
    if url.starts_with("https://") || url.starts_with("http://") {
        cx.open_url(url);
    }
}

/// « PR #12 ouverte », « mergée » ou « fermée ».
pub(crate) fn pr_state_text(pr: &github::PullRequest) -> String {
    match pr.state {
        PrState::Open => t!("publish.state.open", n = pr.number),
        PrState::Merged => t!("publish.state.merged", n = pr.number),
        PrState::Closed => t!("publish.state.closed", n = pr.number),
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
                this.watch_checks(cx);
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
        // Un nom déjà pris dans le dépôt reçoit un suffixe : `-2`, `-3`…
        let label = self.mission_label();
        let branch_text = published.map_or_else(
            || github::free_branch(&self.project, &github::branch_name(&label, mission.attempt)),
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
        // Après la première publication, un commit de plus est une correction.
        let title = if published.is_some() {
            t!("publish.message_fix", label = label).to_string()
        } else {
            label
        };
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
        // Poussée sans PR : on reprend à l'étape 2.
        let pushed = published
            .filter(|publication| publication.pr.is_none() && publication.pushed.is_some())
            .map(|publication| {
                t!(
                    "publish.pushed",
                    branch = publication.branch,
                    count = publication.commits
                )
                .to_string()
            });
        let pr_step = pushed.is_some();
        self.publish = Some(PublishForm {
            branch,
            message,
            files,
            busy: false,
            error: None,
            pushed,
            pr: None,
        });
        if pr_step {
            self.start_pr_step(window, cx);
        }
        cx.notify();
    }

    /// Le nom de la mission pour GitHub : le titre du plan approuvé (sans
    /// « Plan : »), sinon celui de la conversation, coupé au dernier mot entier
    /// s'il a été tronqué.
    pub(super) fn mission_label(&self) -> String {
        let heading = self
            .mission
            .as_ref()
            .and_then(|m| m.plans.last())
            .and_then(|plan| plan.lines().find(|line| line.trim_start().starts_with('#')))
            .map(|line| line.trim_start_matches('#').trim())
            .map(|line| {
                // « Plan : ajouter X » devient « ajouter X ».
                match line.get(..4).zip(line.get(4..)) {
                    Some((head, rest))
                        if head.eq_ignore_ascii_case("plan")
                            && rest.trim_start().starts_with(':') =>
                    {
                        rest.trim_start().trim_start_matches(':').trim()
                    }
                    _ => line,
                }
            })
            // Un titre qui ne dit que « Plan » ne nomme rien.
            .filter(|line| !line.is_empty() && !line.eq_ignore_ascii_case("plan"));
        let label = match heading {
            Some(heading) => heading.to_string(),
            None => {
                let title = self.title.trim();
                // Un titre coupé au milieu d'un mot perd ce mot.
                match title.rsplit_once(' ') {
                    Some((head, _)) if title.chars().count() >= 40 => head.to_string(),
                    _ => title.to_string(),
                }
            }
        };
        let mut chars = label.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => label,
        }
    }

    /// Le brouillon de la description : plan approuvé, validation, relecture,
    /// tentative remplacée, agent. Aucune signature publicitaire.
    pub(super) fn pr_body(&self) -> String {
        let Some(mission) = self.mission.as_ref() else {
            return String::new();
        };
        let mut body = String::new();
        if let Some(plan) = mission.plans.last() {
            // Le chemin local de la copie n'a rien à faire sur GitHub.
            let plan = match &mission.worktree {
                Some(copy) => {
                    let copy = copy.display().to_string();
                    plan.replace(&format!("{copy}/"), "").replace(&copy, ".")
                }
                None => plan.clone(),
            };
            body.push_str(plan.trim());
            body.push_str("\n\n");
        }
        body.push_str(&t!("publish.body.tests", tests = self.tests_text()));
        body.push('\n');
        if let Some(review) = mission.reviews.last() {
            let verdict = match review.ok {
                Some(true) => t!("publish.body.review_ok"),
                Some(false) => t!("publish.body.review_issues"),
                None => t!("publish.body.review_unclear"),
            };
            body.push_str(&t!(
                "publish.body.review",
                agent = review.agent,
                verdict = verdict
            ));
            body.push('\n');
        }
        if let Some(number) = mission.replaces {
            body.push_str(&t!("publish.body.replaces", n = number));
            body.push('\n');
        }
        body.push('\n');
        body.push_str(&t!("publish.body.agent", agent = self.provider.name()));
        body.push('\n');
        body
    }

    /// L'étape 2 : titre, description, branche visée et brouillon, préremplis.
    /// La branche de l'utilisateur au lancement est visée si elle existe sur
    /// GitHub, sinon la branche par défaut.
    fn start_pr_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(repo) = self
            .github
            .as_ref()
            .and_then(|found| found.as_ref().ok())
            .cloned()
        else {
            return;
        };
        let title_text = self.mission_label();
        let body_text = self.pr_body();
        let launch_branch = self
            .mission
            .as_ref()
            .and_then(|mission| mission.launch.as_ref())
            .and_then(|launch| launch.branch.clone());
        let title = cx.new(|cx| {
            let mut input = InputState::new(window, cx);
            input.set_value(title_text, window, cx);
            input
        });
        let body = cx.new(|cx| {
            let mut input = TextareaState::new(window, cx).auto_grow(6, 14);
            input.set_value(body_text, window, cx);
            input
        });
        let default = repo.default_branch.clone();
        let base = cx.new(|cx| {
            let mut input = InputState::new(window, cx);
            input.set_value(default.clone(), window, cx);
            input
        });
        if let Some(form) = self.publish.as_mut() {
            form.pr = Some(PrForm {
                title,
                body,
                base: base.clone(),
                draft: false,
            });
        }
        // La branche de lancement, si GitHub la connaît.
        if let Some(branch) = launch_branch.filter(|branch| *branch != default) {
            let project = self.project.clone();
            cx.spawn_in(window, async move |_, cx| {
                let exists = cx
                    .background_spawn({
                        let branch = branch.clone();
                        async move { Gh::default().branch_exists(&project, &repo, &branch) }
                    })
                    .await;
                if exists {
                    let _ =
                        base.update_in(cx, |input, window, cx| input.set_value(branch, window, cx));
                }
            })
            .detach();
        }
    }

    /// « Ouvrir la PR » : `gh pr create` en arrière-plan ; la PR est notée dans
    /// la publication et le dialogue se ferme.
    pub(super) fn open_pr(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self
            .github
            .as_ref()
            .and_then(|found| found.as_ref().ok())
            .cloned()
        else {
            return;
        };
        let (Some(form), Some(mission)) = (self.publish.as_mut(), self.mission.as_ref()) else {
            return;
        };
        let (Some(pr), Some(publication), Some(copy)) = (
            form.pr.as_ref(),
            mission.publication.as_ref(),
            mission.worktree.clone(),
        ) else {
            return;
        };
        let title = pr.title.read(cx).value().trim().to_string();
        let body = pr.body.read(cx).value().to_string();
        let base = pr.base.read(cx).value().trim().to_string();
        let (head, draft) = (publication.branch.clone(), pr.draft);
        if title.is_empty() || base.is_empty() {
            form.error = Some(t!("publish.error.pr_empty").to_string());
            cx.notify();
            return;
        }
        form.busy = true;
        form.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let opened = cx
                .background_spawn({
                    let base = base.clone();
                    async move {
                        let pr = NewPr {
                            head,
                            base,
                            title,
                            body,
                            draft,
                        };
                        Gh::default().create_pr(&copy, &repo, &pr)
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match opened {
                    Ok(pr) => {
                        if let Some(publication) =
                            this.mission.as_mut().and_then(|m| m.publication.as_mut())
                        {
                            publication.pr = Some(pr);
                            publication.base = Some(base);
                        }
                        this.publish = None;
                        this.check_logs.clear();
                        this.watch_checks(cx);
                    }
                    Err(error) => {
                        if let Some(form) = this.publish.as_mut() {
                            form.busy = false;
                            form.error = Some(t!("publish.error.pr", output = error).to_string());
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// « Commit et push » : le commit dans la copie, puis le push, en
    /// arrière-plan ; la publication est notée dans la mission.
    pub(super) fn commit_and_push(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        cx.spawn_in(window, async move |this, cx| {
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
                            CommitError::BranchExists => {
                                t!("publish.error.branch_exists", branch = branch).to_string()
                            }
                            CommitError::Nothing => t!("publish.error.nothing").to_string(),
                            CommitError::Failed(output) => {
                                t!("publish.error.commit", output = output).to_string()
                            }
                            CommitError::Altered => t!("publish.error.altered").to_string(),
                        })?;
                        Gh::default()
                            .push(&copy, &branch)
                            .map_err(|error| match error {
                                PushError::Moved => t!("publish.error.moved").to_string(),
                                PushError::Failed(output) => {
                                    t!("publish.error.push", output = output).to_string()
                                }
                                PushError::Altered => t!("publish.error.altered").to_string(),
                            })?;
                        Ok::<_, String>(sha)
                    }
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
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
                            pr: None,
                            base: None,
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
                let mut pr_step = false;
                if let Some(form) = this.publish.as_mut() {
                    form.busy = false;
                    match outcome {
                        Ok(done) => {
                            form.pushed = Some(done);
                            // Sans PR encore : l'étape 2 suit.
                            pr_step = this
                                .mission
                                .as_ref()
                                .and_then(|m| m.publication.as_ref())
                                .is_some_and(|p| p.pr.is_none());
                        }
                        Err(error) => form.error = Some(error),
                    }
                }
                if pr_step {
                    this.start_pr_step(window, cx);
                } else if this
                    .mission
                    .as_ref()
                    .and_then(|m| m.publication.as_ref())
                    .is_some_and(|p| p.pr.is_some())
                {
                    // Un nouveau commit sur la PR : de nouveaux checks.
                    this.check_logs.clear();
                    this.watch_checks(cx);
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

    /// Lit les checks de la PR, puis les relit toutes les 30 s tant que certains
    /// sont en cours ; le journal de chaque check en échec est lu une fois. La
    /// tâche s'arrête quand tout est fini, ou avec la vue.
    pub(super) fn watch_checks(&mut self, cx: &mut Context<Self>) {
        let number = self
            .mission
            .as_ref()
            .and_then(|m| m.publication.as_ref())
            .and_then(|p| p.pr.as_ref())
            .map(|pr| pr.number);
        let repo = self
            .github
            .as_ref()
            .and_then(|found| found.as_ref().ok())
            .cloned();
        let (Some(number), Some(repo)) = (number, repo) else {
            return;
        };
        let project = self.project.clone();
        self.checks_poll = Some(cx.spawn(async move |this, cx| {
            let mut waited = 0;
            loop {
                let (found, state) = cx
                    .background_spawn({
                        let (project, repo) = (project.clone(), repo.clone());
                        async move {
                            let gh = Gh::default();
                            (
                                gh.checks(&project, &repo, number),
                                gh.pr_state(&project, &repo, number).ok(),
                            )
                        }
                    })
                    .await;
                // Une PR mergée ou fermée n'a plus de checks à attendre.
                let pending = state.is_none_or(|state| state == PrState::Open)
                    && found
                        .as_ref()
                        .is_ok_and(|checks| checks.iter().any(|c| c.bucket == Bucket::Pending));
                let none_yet = found.as_ref().is_ok_and(|checks| checks.is_empty());
                let Ok(missing) = this.update(cx, |this, cx| {
                    if let (Some(state), Some(pr)) = (
                        state,
                        this.mission
                            .as_mut()
                            .and_then(|m| m.publication.as_mut())
                            .and_then(|p| p.pr.as_mut()),
                    ) {
                        pr.state = state;
                    }
                    let missing: Vec<Check> = found
                        .as_ref()
                        .map(|checks| {
                            checks
                                .iter()
                                .filter(|c| c.bucket == Bucket::Fail)
                                .filter(|c| !this.check_logs.contains_key(&c.link))
                                .cloned()
                                .collect()
                        })
                        .unwrap_or_default();
                    this.checks = Some(found);
                    cx.notify();
                    missing
                }) else {
                    break;
                };
                for check in missing {
                    let log = cx
                        .background_spawn({
                            let (project, repo, check) =
                                (project.clone(), repo.clone(), check.clone());
                            async move { Gh::default().failed_log(&project, &repo, &check, 20) }
                        })
                        .await;
                    if this
                        .update(cx, |this, cx| {
                            this.check_logs.insert(check.link.clone(), log);
                            cx.notify();
                        })
                        .is_err()
                    {
                        return;
                    }
                }
                // Juste après un push, GitHub n'a pas encore créé les checks :
                // on les attend quelques minutes avant de conclure qu'il n'y en a pas.
                if none_yet && waited < 6 {
                    waited += 1;
                } else if !pending {
                    break;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(30))
                    .await;
            }
        }));
    }

    /// « 1 réussi, 1 en échec, 1 en cours » ; `None` avant la première lecture.
    pub(super) fn checks_summary(&self) -> Option<String> {
        let checks = self.checks.as_ref()?.as_ref().ok()?;
        if checks.is_empty() {
            return Some(t!("publish.checks.none").to_string());
        }
        let count = |bucket| checks.iter().filter(|c| c.bucket == bucket).count();
        let parts: Vec<String> = [
            (Bucket::Pass, "publish.checks.pass"),
            (Bucket::Fail, "publish.checks.fail"),
            (Bucket::Pending, "publish.checks.pending"),
            (Bucket::Skipping, "publish.checks.skipping"),
            (Bucket::Cancel, "publish.checks.cancel"),
        ]
        .into_iter()
        .filter(|(bucket, _)| count(*bucket) > 0)
        .map(|(bucket, key)| t!(key, count = count(bucket)).to_string())
        .collect();
        Some(parts.join(", "))
    }

    /// Envoie un check en échec et la fin de son journal à l'agent, comme une
    /// correction ; on la publie ensuite par un nouveau « Commit et push ».
    pub(super) fn send_check_failure(
        &mut self,
        link: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(check) = self
            .checks
            .as_ref()
            .and_then(|found| found.as_ref().ok())
            .and_then(|checks| checks.iter().find(|c| c.link == link))
        else {
            return;
        };
        let log = match self.check_logs.get(&link) {
            Some(Ok(log)) => log.clone(),
            _ => String::new(),
        };
        let message = t!(
            "publish.checks.forward",
            name = check.name,
            link = check.link,
            log = log
        )
        .to_string();
        self.input
            .update(cx, |input, cx| input.set_value(message, window, cx));
        self.pane = super::workspace::Pane::Conversation;
        self.set_preview_visible(true, cx);
        self.send(window, cx);
    }

    fn render_checks(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = cx.theme();
        let (muted, success, danger, border) = (
            theme.muted_foreground,
            theme.success,
            theme.danger,
            theme.border,
        );
        let label = |text: String| {
            div()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(muted)
                .child(crate::theme::caps(&text))
        };
        let refresh = Button::new("checks-refresh")
            .ghost()
            .xsmall()
            .icon(IconName::RefreshCw)
            .tooltip(t!("publish.checks.refresh"))
            .on_click(cx.listener(|this, _, _, cx| this.watch_checks(cx)));
        let found = match self.checks.as_ref()? {
            Ok(found) => found,
            Err(error) => {
                return Some(
                    h_flex()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_color(danger)
                                .child(t!("publish.checks.error", error = error).to_string()),
                        )
                        .child(refresh)
                        .into_any_element(),
                );
            }
        };
        let summary = self.checks_summary().unwrap_or_default();
        let rows = found.iter().enumerate().map(|(ix, check)| {
            let (icon, color) = match check.bucket {
                Bucket::Pass => (IconName::Check, success),
                Bucket::Fail => (IconName::X, danger),
                Bucket::Pending => (IconName::Loader, muted),
                Bucket::Skipping | Bucket::Cancel => (IconName::Minus, muted),
            };
            let detail = match (check.bucket, check.seconds) {
                (Bucket::Pending, _) => t!("publish.checks.running").to_string(),
                (_, Some(seconds)) => t!(
                    "publish.checks.duration",
                    m = seconds / 60,
                    s = format!("{:02}", seconds % 60)
                )
                .to_string(),
                _ => String::new(),
            };
            let link = check.link.clone();
            v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .gap_2()
                        .child(Icon::new(icon).xsmall().text_color(color))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(check.name.clone()),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(muted)
                                .child(detail),
                        ),
                )
                .when(check.bucket == Bucket::Fail, |column| {
                    let log = match self.check_logs.get(&check.link) {
                        None => t!("publish.checks.reading_log").to_string(),
                        Some(Ok(log)) if !log.trim().is_empty() => log.clone(),
                        Some(Ok(_)) => t!("publish.checks.empty_log").to_string(),
                        Some(Err(error)) => error.clone(),
                    };
                    let (open, send) = (link.clone(), link.clone());
                    column
                        .child(
                            div()
                                .id(("check-log", ix))
                                .ml(px(22.))
                                .max_h(px(200.))
                                .overflow_y_scroll()
                                .p_3()
                                .rounded(px(8.))
                                .border_1()
                                .border_color(border)
                                .text_xs()
                                .font_family(cx.theme().mono_font_family.clone())
                                .whitespace_normal()
                                .child(log),
                        )
                        .child(
                            h_flex()
                                .ml(px(22.))
                                .flex_wrap()
                                .gap_2()
                                .child(
                                    Button::new(("send-check", ix))
                                        .primary()
                                        .small()
                                        .icon(IconName::Send)
                                        .label(t!("publish.checks.send").to_string())
                                        .disabled(self.status != Status::Idle)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.send_check_failure(send.clone(), window, cx)
                                        })),
                                )
                                .child(
                                    Button::new(("open-check", ix))
                                        .ghost()
                                        .small()
                                        .icon(IconName::ExternalLink)
                                        .label(t!("publish.checks.full_log").to_string())
                                        .on_click(move |_, _, cx| open_web(&open, cx)),
                                ),
                        )
                })
        });
        Some(
            v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .gap_2()
                        .child(div().flex_1().child(label(
                            t!("publish.checks.title", summary = summary).to_string(),
                        )))
                        .child(refresh),
                )
                .children(rows)
                .into_any_element(),
        )
    }

    /// Retire la copie d'une mission dont la PR est mergée ou fermée ; la
    /// mission et sa publication restent, le projet n'est pas touché.
    pub(super) fn remove_published_copy(&mut self, cx: &mut Context<Self>) {
        let Some(dir) = self.mission.as_ref().and_then(|m| m.worktree.clone()) else {
            return;
        };
        let store = solar_core::checkpoint::Store::for_project(&self.checkpoints, &self.project);
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_spawn(async move { store.remove_worktree(&dir) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match done {
                    Ok(()) => {
                        this.leave_copy();
                        this.review_note = Some(t!("publish.copy_removed").into());
                    }
                    Err(error) => {
                        this.review_note = Some(t!("review.failed", error = error).into());
                    }
                }
                this.refresh_review(cx);
            });
        })
        .detach();
    }

    /// À l'abandon d'une copie dont la PR est ouverte : le rappel, et la case
    /// « Fermer aussi la PR », décochée par défaut.
    pub(super) fn render_close_pr_choice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pr = self
            .mission
            .as_ref()
            .filter(|m| m.worktree.is_some())
            .and_then(|m| m.publication.as_ref())
            .and_then(|p| p.pr.as_ref())
            .filter(|pr| pr.state == PrState::Open)?;
        Some(
            v_flex()
                .w_full()
                .gap_1()
                .child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("publish.discard.pr_stays", n = pr.number).to_string()),
                )
                .child(
                    Checkbox::new("close-pr-on-discard")
                        .label(t!("publish.discard.close_pr", n = pr.number).to_string())
                        .checked(self.close_pr_on_discard)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.close_pr_on_discard = *checked;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    /// Ferme la PR de la mission (`gh pr close`, la branche reste).
    pub(super) fn close_pr(&mut self, cx: &mut Context<Self>) {
        let number = self
            .mission
            .as_ref()
            .and_then(|m| m.publication.as_ref())
            .and_then(|p| p.pr.as_ref())
            .map(|pr| pr.number);
        let repo = self
            .github
            .as_ref()
            .and_then(|found| found.as_ref().ok())
            .cloned();
        let (Some(number), Some(repo)) = (number, repo) else {
            return;
        };
        let project = self.project.clone();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_spawn(async move { Gh::default().close_pr(&project, &repo, number) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match done {
                    Ok(()) => {
                        if let Some(pr) = this
                            .mission
                            .as_mut()
                            .and_then(|m| m.publication.as_mut())
                            .and_then(|p| p.pr.as_mut())
                        {
                            pr.state = PrState::Closed;
                        }
                    }
                    Err(error) => {
                        this.review_note = Some(t!("publish.error.close", error = error).into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// En tête de la revue : la PR ouverte (numéro, branche → base, lien), ou
    /// la branche poussée qui attend sa PR.
    pub(super) fn render_pr_status(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let publication = self.mission.as_ref()?.publication.as_ref()?;
        let muted = cx.theme().muted_foreground;
        let row = h_flex().gap_2().min_w_0();
        Some(match &publication.pr {
            Some(pr) => {
                let url = pr.url.clone();
                let row =
                    row.child(Icon::new(IconName::GitPullRequest).small())
                        .child(
                            div()
                                .flex_shrink_0()
                                .font_weight(FontWeight::BOLD)
                                .child(pr_state_text(pr)),
                        )
                        .child(div().flex_1().min_w_0().truncate().text_color(muted).child(
                            format!(
                                "{} → {}",
                                publication.branch,
                                publication.base.clone().unwrap_or_default()
                            ),
                        ))
                        .child(
                            Button::new("open-pr-on-github")
                                .ghost()
                                .small()
                                .icon(IconName::ExternalLink)
                                .label(t!("publish.on_github").to_string())
                                .on_click(move |_, _, cx| open_web(&url, cx)),
                        );
                // Mergée ou fermée : la copie peut partir, sur décision.
                let copy_left = pr.state != PrState::Open
                    && self
                        .mission
                        .as_ref()
                        .and_then(|m| m.worktree.as_ref())
                        .is_some_and(|dir| dir.is_dir());
                v_flex()
                    .gap_3()
                    .child(row)
                    .when(copy_left, |column| {
                        column.child(
                            h_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_color(muted)
                                        .child(t!("publish.copy_left").to_string()),
                                )
                                .child(
                                    Button::new("remove-published-copy")
                                        .ghost()
                                        .small()
                                        .icon(IconName::Trash)
                                        .label(t!("publish.remove_copy").to_string())
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.remove_published_copy(cx)
                                        })),
                                ),
                        )
                    })
                    .children(self.render_checks(cx))
                    .into_any_element()
            }
            None => row
                .child(Icon::new(IconName::GitBranch).small())
                .child(
                    div().flex_1().min_w_0().truncate().text_color(muted).child(
                        t!(
                            "publish.pushed",
                            branch = publication.branch,
                            count = publication.commits
                        )
                        .to_string(),
                    ),
                )
                .child(
                    Button::new("resume-pr")
                        .ghost()
                        .small()
                        .icon(IconName::GitPullRequest)
                        .label(t!("publish.open_pr").to_string())
                        .disabled(self.github.as_ref().is_none_or(|found| found.is_err()))
                        .on_click(cx.listener(|this, _, window, cx| this.open_publish(window, cx))),
                )
                .into_any_element(),
        })
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
            .map(|body| match (&form.pushed, &form.pr) {
                (Some(done), Some(pr)) => body
                    .child(div().text_color(success).child(done.clone()))
                    .child(
                        v_flex()
                            .gap_1()
                            .child(label(t!("publish.pr_title").to_string()))
                            .child(Input::new(&pr.title).disabled(form.busy)),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(label(t!("publish.pr_body").to_string()))
                            .child(
                                div()
                                    .rounded(px(8.))
                                    .border_1()
                                    .border_color(border)
                                    .p_2()
                                    .child(Textarea::new(&pr.body).appearance(false)),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(label(t!("publish.pr_base").to_string()))
                            .child(Input::new(&pr.base).disabled(form.busy)),
                    )
                    .child(
                        Checkbox::new("publish-draft")
                            .label(t!("publish.draft").to_string())
                            .checked(pr.draft)
                            .disabled(form.busy)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                if let Some(pr) =
                                    this.publish.as_mut().and_then(|form| form.pr.as_mut())
                                {
                                    pr.draft = *checked;
                                    cx.notify();
                                }
                            })),
                    ),
                (Some(done), None) => body.child(div().text_color(success).child(done.clone())),
                (None, _) => body
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
        let go = match (pushed, form.pr.is_some()) {
            (false, _) => Some(
                Button::new("publish-go")
                    .primary()
                    .small()
                    .icon(IconName::ArrowUp)
                    .label(t!("publish.commit_push").to_string())
                    .loading(form.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.commit_and_push(window, cx))),
            ),
            (true, true) => Some(
                Button::new("publish-pr")
                    .primary()
                    .small()
                    .icon(IconName::GitPullRequest)
                    .label(t!("publish.open_pr").to_string())
                    .loading(form.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.open_pr(cx))),
            ),
            (true, false) => None,
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
    /// L'étape 2, quand la branche n'a pas encore de PR.
    pr: Option<PrForm>,
}

impl PublishForm {
    /// L'étape 2 est ouverte.
    #[cfg(test)]
    pub(super) fn has_pr_step(&self) -> bool {
        self.pr.is_some()
    }
}

struct PrForm {
    title: Entity<InputState>,
    body: Entity<TextareaState>,
    base: Entity<InputState>,
    draft: bool,
}

struct PublishFile {
    path: String,
    lines: String,
    /// Modifié par l'utilisateur avant le lancement : décoché par défaut.
    dirty: bool,
    checked: bool,
}
