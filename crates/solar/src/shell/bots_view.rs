//! La vue Bots : chaque bot, son déclencheur, sa dernière exécution ; créer,
//! modifier, mettre en pause, supprimer, lancer maintenant. Une exécution est
//! une mission ordinaire, marquée du bot, qui attend ton examen.

use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rust_i18n::t;
use solar_core::agent::Provider;
use solar_core::bots::{self, Bot, Prompt, PromptError, Run, Scope, Trigger};
use solar_core::github::{Gh, Issue};
use solar_core::library::{self, Template};

use super::Shell;
use crate::session_view::{Catalogs, MODES, SessionView, known_mode};
use crate::theme::caps;

pub(super) struct BotsView {
    editing: Option<BotEditor>,
    /// Le bot dont la suppression attend confirmation.
    confirm_delete: Option<String>,
    pub(super) note: Option<String>,
}

struct BotEditor {
    /// `None` : un bot neuf.
    id: Option<String>,
    name: Entity<InputState>,
    project: PathBuf,
    provider: Provider,
    model: Option<String>,
    mode: &'static str,
    scope: Scope,
    /// Le modèle de la bibliothèque choisi ; `None` : un texte.
    template: Option<String>,
    values: Vec<(String, Entity<InputState>)>,
    text: Entity<TextareaState>,
    /// Les modèles du projet choisi et les personnels.
    templates: Vec<Template>,
    /// Le déclencheur : « manual », « daily », « weekdays » ou « every » ;
    /// l'heure d'un horaire quotidien, l'intervalle en heures.
    trigger: &'static str,
    time: Entity<InputState>,
    hours: u32,
    /// L'étiquette des issues, et « aussi les issues déjà ouvertes ».
    label: Entity<InputState>,
    include_existing: bool,
    error: Option<String>,
}

/// Tous les combien les issues d'un bot sont relues, SOLAR ouvert.
const ISSUE_POLL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// Les intervalles proposés, en heures.
const INTERVALS: [u32; 7] = [1, 2, 3, 4, 6, 8, 12];

/// Une date rangée (RFC 3339) en heure locale.
fn local(at: &str) -> Option<chrono::NaiveDateTime> {
    chrono::DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|at| at.with_timezone(&chrono::Local).naive_local())
}

/// « 16/11 à 09:00 », dans la langue de l'interface.
fn when(at: chrono::NaiveDateTime) -> String {
    at.format(&t!("bots.date_format")).to_string()
}

/// Le déclencheur en mots : « Chaque jour à 09:00 ».
fn trigger_words(trigger: &Trigger) -> String {
    match trigger {
        Trigger::Manual => t!("bots.trigger.manual").to_string(),
        Trigger::Daily {
            time,
            weekdays_only: false,
        } => t!("bots.trigger.daily_at", time = time).to_string(),
        Trigger::Daily {
            time,
            weekdays_only: true,
        } => t!("bots.trigger.weekdays_at", time = time).to_string(),
        Trigger::Every { hours } => t!("bots.trigger.every_n", hours = hours).to_string(),
        Trigger::Issues { label, .. } => t!("bots.trigger.issues_with", label = label).to_string(),
    }
}

/// Les modes proposés à un bot : tous sauf le plan, qui ne lancerait rien.
fn bot_modes(provider: Provider) -> Vec<&'static str> {
    MODES
        .into_iter()
        .filter(|mode| *mode != "plan" && provider.supports(mode))
        .collect()
}

fn now() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

/// Les modèles disponibles pour un bot du projet : ceux du projet, les
/// personnels, les exemples.
fn templates_for(project: &std::path::Path) -> Vec<Template> {
    library::load(
        &library::personal_dir(),
        Some(project),
        &crate::session_view::library_examples(),
    )
}

impl Shell {
    pub(super) fn open_bots(&mut self, cx: &mut Context<Self>) {
        self.importing = None;
        self.settings_open = false;
        self.card = None;
        self.library = None;
        self.take_focus = true;
        self.bots_view = Some(BotsView {
            editing: None,
            confirm_delete: None,
            note: None,
        });
        self.sync_previews(cx);
        cx.notify();
    }

    /// Ouvre l'éditeur, sur un bot existant ou neuf.
    pub(super) fn edit_bot(
        &mut self,
        id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.bots_view.is_none() {
            self.open_bots(cx);
        }
        let existing = id
            .as_ref()
            .and_then(|id| self.state.bots.iter().find(|bot| bot.id == *id))
            .cloned();
        let project = existing
            .as_ref()
            .map(|bot| bot.project.clone())
            .or_else(|| {
                Some(self.current_view().read(cx).project.clone())
                    .filter(|p| !p.as_os_str().is_empty())
            })
            .or_else(|| self.state.projects.first().map(|p| p.path.clone()));
        let Some(project) = project else {
            if let Some(view) = self.bots_view.as_mut() {
                view.note = Some(t!("bots.no_project").to_string());
            }
            cx.notify();
            return;
        };
        let name = cx.new(|cx| InputState::new(window, cx));
        let text = cx.new(|cx| TextareaState::new(window, cx).auto_grow(4, 16));
        let (mut template, mut values) = (None, Default::default());
        match existing.as_ref().map(|bot| &bot.prompt) {
            Some(Prompt::Text(body)) => {
                text.update(cx, |text, cx| text.set_value(body.clone(), window, cx))
            }
            Some(Prompt::Template { name, values: v }) => {
                template = Some(name.clone());
                values = v.clone();
            }
            None => {}
        }
        name.update(cx, |input, cx| {
            if let Some(bot) = &existing {
                input.set_value(bot.name.clone(), window, cx);
            }
            input.focus(window, cx);
        });
        let provider = existing
            .as_ref()
            .map_or(self.state.settings.provider, |bot| bot.provider);
        let mode = existing
            .as_ref()
            .and_then(|bot| known_mode(&bot.mode))
            .filter(|mode| bot_modes(provider).contains(mode))
            .unwrap_or_else(|| {
                if provider.supports("acceptEdits") {
                    "acceptEdits"
                } else {
                    bot_modes(provider)
                        .first()
                        .copied()
                        .unwrap_or("acceptEdits")
                }
            });
        let (trigger, time, hours) = match existing.as_ref().map(|bot| &bot.trigger) {
            Some(Trigger::Daily {
                time,
                weekdays_only,
            }) => (
                if *weekdays_only { "weekdays" } else { "daily" },
                time.clone(),
                6,
            ),
            Some(Trigger::Every { hours }) => ("every", "09:00".to_string(), *hours),
            Some(Trigger::Issues { .. }) => ("issues", "09:00".to_string(), 6),
            _ => ("manual", "09:00".to_string(), 6),
        };
        let (label, include_existing) = match existing.as_ref().map(|bot| &bot.trigger) {
            Some(Trigger::Issues { label, since }) => (label.clone(), since.is_none()),
            _ => ("solar".to_string(), false),
        };
        let label_input = cx.new(|cx| InputState::new(window, cx));
        label_input.update(cx, |input, cx| input.set_value(label, window, cx));
        let time_input = cx.new(|cx| InputState::new(window, cx));
        time_input.update(cx, |input, cx| input.set_value(time, window, cx));
        let mut editor = BotEditor {
            id,
            name,
            project: project.clone(),
            provider,
            model: existing.as_ref().and_then(|bot| bot.model.clone()),
            mode,
            scope: existing.as_ref().map_or(Scope::Plan, |bot| bot.scope),
            template,
            values: Vec::new(),
            text,
            templates: templates_for(&project),
            trigger,
            time: time_input,
            hours,
            label: label_input,
            include_existing,
            error: None,
        };
        editor.values = Self::hole_inputs(&editor, &values, window, cx);
        if let Some(view) = self.bots_view.as_mut() {
            view.editing = Some(editor);
            view.note = None;
            view.confirm_delete = None;
        }
        cx.notify();
    }

    /// Un champ par trou du modèle choisi, prérempli.
    fn hole_inputs(
        editor: &BotEditor,
        values: &std::collections::BTreeMap<String, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<(String, Entity<InputState>)> {
        let Some(template) = editor
            .template
            .as_ref()
            .and_then(|name| editor.templates.iter().find(|t| t.name == *name))
        else {
            return Vec::new();
        };
        library::holes(&template.body)
            .into_iter()
            // Les trous d'issue se remplissent à chaque exécution.
            .filter(|hole| editor.trigger != "issues" || !hole.starts_with("issue_"))
            .map(|hole| {
                let input = cx.new(|cx| InputState::new(window, cx));
                if let Some(value) = values.get(&hole) {
                    input.update(cx, |input, cx| input.set_value(value.clone(), window, cx));
                }
                (hole, input)
            })
            .collect()
    }

    fn editor_values(editor: &BotEditor, cx: &App) -> std::collections::BTreeMap<String, String> {
        editor
            .values
            .iter()
            .map(|(hole, input)| (hole.clone(), input.read(cx).value().to_string()))
            .collect()
    }

    fn change_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut BotEditor),
    ) {
        let Some(editor) = self.bots_view.as_mut().and_then(|v| v.editing.as_mut()) else {
            return;
        };
        let (before_template, before_project, before_trigger) = (
            editor.template.clone(),
            editor.project.clone(),
            editor.trigger,
        );
        let values = Self::editor_values(editor, cx);
        change(editor);
        if editor.project != before_project {
            editor.templates = templates_for(&editor.project);
            if editor
                .template
                .as_ref()
                .is_some_and(|name| !editor.templates.iter().any(|t| t.name == *name))
            {
                editor.template = None;
            }
        }
        if editor.template != before_template
            || editor.project != before_project
            || editor.trigger != before_trigger
        {
            let editor = self
                .bots_view
                .as_ref()
                .and_then(|v| v.editing.as_ref())
                .expect("vérifié au-dessus");
            let inputs = Self::hole_inputs(editor, &values, window, cx);
            if let Some(editor) = self.bots_view.as_mut().and_then(|v| v.editing.as_mut()) {
                editor.values = inputs;
            }
        }
        if let Some(editor) = self.bots_view.as_mut().and_then(|v| v.editing.as_mut()) {
            // Le mode et le modèle suivent l'agent.
            if !bot_modes(editor.provider).contains(&editor.mode) {
                editor.mode = bot_modes(editor.provider)
                    .first()
                    .copied()
                    .unwrap_or("acceptEdits");
            }
            editor.error = None;
        }
        cx.notify();
    }

    pub(super) fn save_bot(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.bots_view.as_mut().and_then(|v| v.editing.as_mut()) else {
            return;
        };
        let name = editor.name.read(cx).value().trim().to_string();
        if name.is_empty() {
            editor.error = Some(t!("bots.name_missing").to_string());
            cx.notify();
            return;
        }
        let prompt = match &editor.template {
            Some(template) => Prompt::Template {
                name: template.clone(),
                values: Self::editor_values(editor, cx),
            },
            None => Prompt::Text(editor.text.read(cx).value().to_string()),
        };
        let time = editor.time.read(cx).value().trim().to_string();
        let trigger = match editor.trigger {
            "daily" | "weekdays" => {
                if chrono::NaiveTime::parse_from_str(&time, "%H:%M").is_err() {
                    editor.error = Some(t!("bots.bad_time").to_string());
                    cx.notify();
                    return;
                }
                Trigger::Daily {
                    time,
                    weekdays_only: editor.trigger == "weekdays",
                }
            }
            "every" => Trigger::Every {
                hours: editor.hours,
            },
            "issues" => {
                let label = editor.label.read(cx).value().trim().to_string();
                if label.is_empty() {
                    editor.error = Some(t!("bots.label_missing").to_string());
                    cx.notify();
                    return;
                }
                // Les issues d'avant le bot restent de côté, sauf demande ; une
                // étiquette inchangée garde sa date de départ.
                let kept = editor
                    .id
                    .as_ref()
                    .and_then(|id| self.state.bots.iter().find(|bot| bot.id == *id))
                    .and_then(|bot| match &bot.trigger {
                        Trigger::Issues { label: old, since } if *old == label => since.clone(),
                        _ => None,
                    });
                Trigger::Issues {
                    label,
                    since: (!editor.include_existing).then(|| kept.unwrap_or_else(now)),
                }
            }
            _ => Trigger::Manual,
        };
        let existing = editor
            .id
            .as_ref()
            .and_then(|id| self.state.bots.iter().find(|bot| bot.id == *id));
        // Un horaire neuf ou changé part de maintenant : rien d'avant ne compte.
        let same_trigger = existing.is_some_and(|bot| bot.trigger == trigger);
        let bot = Bot {
            id: editor
                .id
                .clone()
                .unwrap_or_else(|| chrono::Local::now().format("%Y%m%d-%H%M%S-%f").to_string()),
            name,
            project: editor.project.clone(),
            trigger,
            prompt,
            provider: editor.provider,
            model: editor.model.clone(),
            effort: existing.and_then(|bot| bot.effort.clone()),
            mode: editor.mode.to_string(),
            scope: editor.scope,
            enabled: existing.is_none_or(|bot| bot.enabled),
            runs: existing.map(|bot| bot.runs.clone()).unwrap_or_default(),
            checked: if same_trigger {
                existing.and_then(|bot| bot.checked.clone())
            } else {
                Some(now())
            },
            missed: if same_trigger {
                existing.map_or(0, |bot| bot.missed)
            } else {
                0
            },
            handled: existing.map(|bot| bot.handled.clone()).unwrap_or_default(),
        };
        // Un bot d'issues est vérifié avec une issue d'exemple.
        let sample = Issue {
            number: 1,
            title: "-".into(),
            body: "-".into(),
            url: "-".into(),
            created_at: String::new(),
        };
        let issue = matches!(bot.trigger, Trigger::Issues { .. }).then_some(&sample);
        if let Err(error) = bots::prompt_for(&bot, &editor.templates, issue) {
            editor.error = Some(prompt_problem(&error));
            cx.notify();
            return;
        }
        match self.state.bots.iter_mut().find(|known| known.id == bot.id) {
            Some(known) => *known = bot.clone(),
            None => self.state.bots.push(bot.clone()),
        }
        self.save_state();
        if let Some(view) = self.bots_view.as_mut() {
            view.editing = None;
            view.note = Some(t!("bots.saved", name = bot.name).to_string());
        }
        cx.notify();
    }

    fn save_state(&self) {
        if let Err(error) = self.state.save(&self.state_path) {
            eprintln!("solar: bots non sauvegardés : {error}");
        }
    }

    fn toggle_bot(&mut self, id: &str, cx: &mut Context<Self>) {
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
    fn delete_bot(&mut self, id: &str, cx: &mut Context<Self>) {
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
    fn start_run(
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
        let text = bots::prompt_for(&bot, &templates_for(&bot.project), issue.as_ref());
        let mode = known_mode(&bot.mode)
            .filter(|mode| bot_modes(bot.provider).contains(mode))
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
    fn poll_issues(
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

    fn clear_missed(&mut self, id: &str) {
        if let Some(bot) = self.state.bots.iter_mut().find(|bot| bot.id == id) {
            bot.missed = 0;
        }
    }

    /// Au lancement : les échéances passées SOLAR fermé sont comptées, pas
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

    /// Chaque minute, SOLAR ouvert : un bot dont une échéance est passée part
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

    fn record_run(
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
            });
        }
        self.save_state();
        cx.notify();
    }

    /// Ouvre la conversation d'une exécution.
    fn open_run(&mut self, session: &str, window: &mut Window, cx: &mut Context<Self>) {
        let found = self.state.projects.iter().find_map(|project| {
            let known = project.sessions.iter().find(|s| s.id == session)?;
            Some((project.path.clone(), known.clone()))
        });
        if let Some((project, session)) = found {
            self.open(project, session, window, cx);
        }
    }

    pub(super) fn render_bots(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(view) = self.bots_view.as_ref() else {
            return div().into_any_element();
        };
        let muted = cx.theme().muted_foreground;
        let section = |text: String| {
            div()
                .pt_6()
                .pb_2()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(muted)
                .child(caps(&text))
        };

        let mut body = v_flex()
            .w_full()
            .max_w(px(760.))
            .mx_auto()
            .px_8()
            .py_8()
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::BOLD)
                    .child(t!("bots.title").to_string()),
            )
            .child(
                div()
                    .pt_2()
                    .text_color(muted)
                    .child(t!("bots.intro").to_string()),
            );

        if let Some(editor) = view.editing.as_ref() {
            body = body.child(self.render_bot_editor(editor, cx));
        } else {
            body = body.child(
                h_flex().pt_4().child(
                    Button::new("bot-new")
                        .outline()
                        .small()
                        .icon(IconName::Plus)
                        .label(t!("bots.new").to_string())
                        .on_click(
                            cx.listener(|this, _, window, cx| this.edit_bot(None, window, cx)),
                        ),
                ),
            );
        }
        if let Some(note) = view.note.clone() {
            body = body.child(div().pt_3().text_sm().text_color(muted).child(note));
        }

        if !self.state.bots.is_empty() {
            body = body.child(section(t!("bots.list").to_string()));
        }
        let confirm = view.confirm_delete.clone();
        for (ix, bot) in self.state.bots.iter().enumerate() {
            body =
                body.child(self.render_bot_row(ix, bot, confirm.as_deref() == Some(&bot.id), cx));
        }

        div()
            .id("bots")
            .size_full()
            .overflow_y_scroll()
            .child(body)
            .into_any_element()
    }

    fn render_bot_row(
        &self,
        ix: usize,
        bot: &Bot,
        confirming: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let (muted, border) = (theme.muted_foreground, theme.border);
        let project = bot
            .project
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let trigger = trigger_words(&bot.trigger);
        let scope = match bot.scope {
            Scope::Plan => t!("bots.scope.plan").to_string(),
            Scope::Mission => t!("bots.scope.mission").to_string(),
        };
        let mut detail = vec![trigger, project, bot.provider.name().to_string(), scope];
        if !bot.enabled {
            detail.insert(0, t!("bots.paused").to_string());
        }
        let last = bot.last_run().map(|run| {
            let when = local(&run.at).map_or_else(|| run.at.clone(), when);
            let when = match run.issue {
                Some(n) => format!("{when} (#{n})"),
                None => when,
            };
            match &run.problem {
                Some(problem) => t!("bots.last_failed", when = when, problem = problem).to_string(),
                None => t!("bots.last_run", when = when).to_string(),
            }
        });
        let last_session = bot.last_run().and_then(|run| run.session.clone());
        // La prochaine échéance, pour un bot actif à horaire.
        let next = bot
            .enabled
            .then(|| bots::next_due(&bot.trigger, chrono::Local::now().naive_local()))
            .flatten()
            .map(|due| t!("bots.next", when = when(due)).to_string());
        let missed = bot.missed;
        let unreadable = self
            .issue_errors
            .get(&bot.id)
            .map(|error| t!("bots.issues_unreadable", error = error).to_string());
        let (id_run, id_edit, id_toggle, id_delete) = (
            bot.id.clone(),
            bot.id.clone(),
            bot.id.clone(),
            bot.id.clone(),
        );
        v_flex()
            .py_3()
            .gap_2()
            .border_b_1()
            .border_color(border)
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(Icon::new(IconName::Bot).small().text_color(muted))
                                    .child(
                                        div()
                                            .font_weight(FontWeight::BOLD)
                                            .truncate()
                                            .child(bot.name.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted)
                                    .truncate()
                                    .child(detail.join(" · ")),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .text_sm()
                                    .text_color(muted)
                                    .child(
                                        std::iter::once(
                                            last.unwrap_or_else(|| {
                                                t!("bots.never_run").to_string()
                                            }),
                                        )
                                        .chain(next)
                                        .collect::<Vec<_>>()
                                        .join(" · "),
                                    )
                                    .when_some(last_session, |row, session| {
                                        row.child(
                                            Button::new(("bot-open-run", ix))
                                                .ghost()
                                                .xsmall()
                                                .label(t!("bots.open_run").to_string())
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.open_run(&session, window, cx)
                                                    },
                                                )),
                                        )
                                    }),
                            ),
                    )
                    .child(
                        Button::new(("bot-run", ix))
                            .outline()
                            .small()
                            .label(t!("bots.run_now").to_string())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.run_bot(&id_run, window, cx)
                            })),
                    )
                    // Le reste derrière « ··· », pour garder la ligne lisible.
                    .child({
                        let (shell, enabled) = (cx.entity(), bot.enabled);
                        Button::new(("bot-menu", ix))
                            .ghost()
                            .small()
                            .icon(IconName::Ellipsis)
                            .tooltip(t!("bots.menu"))
                            .dropdown_menu(move |menu, _, _| {
                                let (s1, s2, s3) = (shell.clone(), shell.clone(), shell.clone());
                                let (toggle, edit, delete) =
                                    (id_toggle.clone(), id_edit.clone(), id_delete.clone());
                                menu.item(
                                    PopupMenuItem::new(if enabled {
                                        t!("bots.pause").to_string()
                                    } else {
                                        t!("bots.resume").to_string()
                                    })
                                    .on_click(
                                        move |_, _, cx| {
                                            s1.update(cx, |this, cx| this.toggle_bot(&toggle, cx))
                                        },
                                    ),
                                )
                                .item(PopupMenuItem::new(t!("library.edit").to_string()).on_click(
                                    move |_, window, cx| {
                                        s2.update(cx, |this, cx| {
                                            this.edit_bot(Some(edit.clone()), window, cx)
                                        })
                                    },
                                ))
                                .item(
                                    PopupMenuItem::new(t!("library.delete").to_string()).on_click(
                                        move |_, _, cx| {
                                            s3.update(cx, |this, cx| {
                                                if let Some(view) = this.bots_view.as_mut() {
                                                    view.confirm_delete = Some(delete.clone());
                                                }
                                                cx.notify();
                                            })
                                        },
                                    ),
                                )
                            })
                    }),
            )
            .children(unreadable.map(|text| div().text_sm().child(text)))
            // Échéances passées SOLAR fermé : rien n'est parti seul.
            .when(missed > 0, |column| {
                let (run, skip) = (bot.id.clone(), bot.id.clone());
                column.child(
                    h_flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            div()
                                .w_full()
                                .child(t!("bots.missed", count = missed).to_string()),
                        )
                        .child(
                            Button::new(("bot-missed-run", ix))
                                .primary()
                                .small()
                                .label(t!("bots.run_once").to_string())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.clear_missed(&run);
                                    this.run_bot(&run, window, cx)
                                })),
                        )
                        .child(
                            Button::new(("bot-missed-skip", ix))
                                .ghost()
                                .small()
                                .label(t!("bots.skip_missed").to_string())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.clear_missed(&skip);
                                    this.save_state();
                                    cx.notify();
                                })),
                        ),
                )
            })
            .when(confirming, |column| {
                let id = bot.id.clone();
                column.child(
                    h_flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            div()
                                .w_full()
                                .child(t!("bots.confirm_delete", name = bot.name).to_string()),
                        )
                        .child(
                            Button::new(("bot-delete-cancel", ix))
                                .ghost()
                                .small()
                                .label(t!("library.cancel").to_string())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(view) = this.bots_view.as_mut() {
                                        view.confirm_delete = None;
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new(("bot-delete-go", ix))
                                .primary()
                                .small()
                                .label(t!("library.delete").to_string())
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.delete_bot(&id, cx)),
                                ),
                        ),
                )
            })
    }

    /// Un menu de choix de l'éditeur.
    fn bot_choice(
        &self,
        id: &'static str,
        options: Vec<(String, String)>,
        current: &str,
        pick: impl Fn(&mut BotEditor, String) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let label = options
            .iter()
            .find(|(value, _)| value == current)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| current.to_string());
        let (shell, current, pick) = (cx.entity(), current.to_string(), Rc::new(pick));
        Button::new(id)
            .outline()
            .small()
            .label(label)
            .dropdown_menu(move |menu, _, _| {
                options.iter().fold(menu, |menu, (value, name)| {
                    let (shell, value, pick) = (shell.clone(), value.clone(), pick.clone());
                    menu.item(
                        PopupMenuItem::new(name.clone())
                            .checked(*value == current)
                            .on_click(move |_, window, cx| {
                                let (value, pick) = (value.clone(), pick.clone());
                                shell.update(cx, |shell, cx| {
                                    shell.change_editor(window, cx, |editor| pick(editor, value))
                                })
                            }),
                    )
                })
            })
    }

    fn render_bot_editor(&self, editor: &BotEditor, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (muted, border, radius) = (theme.muted_foreground, theme.border, theme.radius);
        let label = |text: String| {
            div()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(muted)
                .child(caps(&text))
        };
        let field =
            |name: String, control: AnyElement| v_flex().gap_1().child(label(name)).child(control);

        let projects: Vec<(String, String)> = self
            .state
            .projects
            .iter()
            .map(|p| {
                (
                    p.path.to_string_lossy().to_string(),
                    p.path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                )
            })
            .collect();
        let agents: Vec<(String, String)> = Provider::ALL
            .into_iter()
            .map(|p| (p.program().to_string(), p.name().to_string()))
            .collect();
        let models: Vec<(String, String)> = std::iter::once((
            String::new(),
            t!("bots.model_default", agent = editor.provider.name()).to_string(),
        ))
        .chain(
            cx.try_global::<Catalogs>()
                .and_then(|catalogs| catalogs.0.get(&editor.provider))
                .map(|catalog| {
                    catalog
                        .models
                        .iter()
                        .map(|m| (m.value.clone(), m.name.clone()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        )
        .collect();
        let modes: Vec<(String, String)> = bot_modes(editor.provider)
            .into_iter()
            .map(|mode| (mode.to_string(), t!(format!("mode.{mode}")).to_string()))
            .collect();
        let triggers: Vec<(String, String)> = [
            ("manual", t!("bots.trigger.manual")),
            ("daily", t!("bots.trigger.daily")),
            ("weekdays", t!("bots.trigger.weekdays")),
            ("every", t!("bots.trigger.every")),
            ("issues", t!("bots.trigger.issues")),
        ]
        .into_iter()
        .map(|(value, label)| (value.to_string(), label.to_string()))
        .collect();
        let hours: Vec<(String, String)> = INTERVALS
            .into_iter()
            .map(|n| {
                (
                    n.to_string(),
                    t!("bots.trigger.every_n", hours = n).to_string(),
                )
            })
            .collect();
        let scopes = vec![
            ("plan".to_string(), t!("bots.scope.plan").to_string()),
            ("mission".to_string(), t!("bots.scope.mission").to_string()),
        ];
        let prompts: Vec<(String, String)> =
            std::iter::once((String::new(), t!("bots.prompt_text").to_string()))
                .chain(
                    editor
                        .templates
                        .iter()
                        .map(|t| (t.name.clone(), format!("{} · /{}", t.title, t.name))),
                )
                .collect();

        let choices = h_flex()
            .flex_wrap()
            .gap_4()
            .child(field(
                t!("bots.project").to_string(),
                self.bot_choice(
                    "bot-project",
                    projects,
                    &editor.project.to_string_lossy(),
                    |editor, value| editor.project = PathBuf::from(value),
                    cx,
                )
                .into_any_element(),
            ))
            .child(field(
                t!("bots.agent").to_string(),
                self.bot_choice(
                    "bot-agent",
                    agents,
                    editor.provider.program(),
                    |editor, value| {
                        let provider = Provider::ALL
                            .into_iter()
                            .find(|p| p.program() == value)
                            .unwrap_or_default();
                        if provider != editor.provider {
                            editor.provider = provider;
                            editor.model = None;
                        }
                    },
                    cx,
                )
                .into_any_element(),
            ))
            .child(field(
                t!("bots.model").to_string(),
                self.bot_choice(
                    "bot-model",
                    models,
                    editor.model.as_deref().unwrap_or_default(),
                    |editor, value| editor.model = (!value.is_empty()).then_some(value),
                    cx,
                )
                .into_any_element(),
            ))
            .child(field(
                t!("bots.mode").to_string(),
                self.bot_choice(
                    "bot-mode",
                    modes,
                    editor.mode,
                    |editor, value| {
                        if let Some(mode) = known_mode(&value) {
                            editor.mode = mode;
                        }
                    },
                    cx,
                )
                .into_any_element(),
            ))
            .child(field(
                t!("bots.scope").to_string(),
                self.bot_choice(
                    "bot-scope",
                    scopes,
                    match editor.scope {
                        Scope::Plan => "plan",
                        Scope::Mission => "mission",
                    },
                    |editor, value| {
                        editor.scope = if value == "mission" {
                            Scope::Mission
                        } else {
                            Scope::Plan
                        }
                    },
                    cx,
                )
                .into_any_element(),
            ));

        let scope_note = match (editor.scope, editor.mode) {
            (Scope::Mission, "bypassPermissions") => Some(t!("bots.full_access_warning")),
            (Scope::Mission, _) => Some(t!("bots.scope.mission_note")),
            (Scope::Plan, _) => Some(t!("bots.scope.plan_note")),
        };

        let prompt = v_flex()
            .gap_2()
            .child(field(
                t!("bots.prompt").to_string(),
                h_flex()
                    .child(self.bot_choice(
                        "bot-prompt",
                        prompts,
                        editor.template.as_deref().unwrap_or_default(),
                        |editor, value| editor.template = (!value.is_empty()).then_some(value),
                        cx,
                    ))
                    .into_any_element(),
            ))
            .when(editor.template.is_none(), |column| {
                column.child(
                    div()
                        .p_2()
                        .rounded(radius)
                        .border_1()
                        .border_color(border)
                        .font_family(crate::theme::READING_FONT)
                        .child(Textarea::new(&editor.text).appearance(false)),
                )
            })
            .children(editor.values.iter().map(|(hole, input)| {
                field(hole.clone(), Input::new(input).small().into_any_element())
            }));

        v_flex()
            .pt_6()
            .gap_4()
            .child(field(
                t!("bots.name").to_string(),
                Input::new(&editor.name).small().into_any_element(),
            ))
            .child(field(
                t!("bots.trigger").to_string(),
                h_flex()
                    .gap_2()
                    .child(self.bot_choice(
                        "bot-trigger",
                        triggers,
                        editor.trigger,
                        |editor, value| {
                            editor.trigger = match value.as_str() {
                                "daily" => "daily",
                                "weekdays" => "weekdays",
                                "every" => "every",
                                "issues" => "issues",
                                _ => "manual",
                            }
                        },
                        cx,
                    ))
                    .when(matches!(editor.trigger, "daily" | "weekdays"), |row| {
                        row.child(div().w(px(88.)).child(Input::new(&editor.time).small()))
                    })
                    .when(editor.trigger == "issues", |row| {
                        let include = editor.include_existing;
                        row.child(div().w(px(160.)).child(Input::new(&editor.label).small()))
                            .child(
                                Switch::new("bot-include-existing")
                                    .checked(include)
                                    .label(t!("bots.include_existing").to_string())
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.change_editor(window, cx, |editor| {
                                            editor.include_existing = !include
                                        })
                                    })),
                            )
                    })
                    .when(editor.trigger == "every", |row| {
                        row.child(self.bot_choice(
                            "bot-hours",
                            hours,
                            &editor.hours.to_string(),
                            |editor, value| editor.hours = value.parse().unwrap_or(6),
                            cx,
                        ))
                    })
                    .into_any_element(),
            ))
            .when(editor.trigger == "issues", |column| {
                column.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(t!("bots.issue_hint").to_string()),
                )
            })
            .child(prompt)
            .child(choices)
            .children(
                scope_note.map(|note| div().text_sm().text_color(muted).child(note.to_string())),
            )
            .children(
                editor
                    .error
                    .clone()
                    .map(|error| div().text_sm().child(error)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("bot-save")
                            .primary()
                            .small()
                            .label(t!("library.save").to_string())
                            .on_click(cx.listener(|this, _, _, cx| this.save_bot(cx))),
                    )
                    .child(
                        Button::new("bot-cancel")
                            .ghost()
                            .small()
                            .label(t!("library.cancel").to_string())
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(view) = this.bots_view.as_mut() {
                                    view.editing = None;
                                    view.note = None;
                                }
                                cx.notify();
                            })),
                    ),
            )
    }
}

fn prompt_problem(error: &PromptError) -> String {
    match error {
        PromptError::Empty => t!("bots.prompt_empty").to_string(),
        PromptError::MissingTemplate(name) => t!("bots.template_gone", name = name).to_string(),
        PromptError::Unfilled(holes) => t!("bots.unfilled", holes = holes.join(", ")).to_string(),
    }
}
