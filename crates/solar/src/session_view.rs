//! Une conversation avec Claude : l'accueil tant qu'elle n'a pas commencé,
//! puis son fil et sa zone de saisie.

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::base::input::{IndentInline, Paste};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::component::shimmer::ShimmerText;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::text::{TextView, TextViewStyle};
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Selectable, Sizable, StyledExt, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rust_i18n::t;
use serde_json::Value;
use solar_core::agent::{Agent, Provider};
use solar_core::claude::{Event, Model, SlashCommand};
use solar_core::files;
use solar_core::state::{Mission, MissionStatus};

use crate::assets::CLAUDE_LOGO;
use crate::theme::{BLACK, CLAUDE_BRAND, READING_FONT, VEIL, caps};

mod home;
mod pickers;
mod preview;
mod review;
mod reviewer;
mod workspace;

/// Largeur de la colonne de l'accueil ; la fenêtre y cale aussi la planète.
pub const HOME_WIDTH: Pixels = px(720.);

/// Modes de permission de Claude Code proposés dans le sélecteur.
pub(crate) const MODES: [&str; 5] = ["bypassPermissions", "acceptEdits", "auto", "manual", "plan"];
pub(crate) use pickers::{Catalog, Catalogs, Favorites};

/// Un mode noté (réglages, mission) ramené à ceux d'aujourd'hui : l'ancien
/// « default » du CLI s'appelle désormais « manual ».
pub(crate) fn known_mode(mode: &str) -> Option<&'static str> {
    let mode = if mode == "default" { "manual" } else { mode };
    MODES.into_iter().find(|known| *known == mode)
}
/// Le mode où Claude prépare un plan sans rien écrire : le moteur le fait respecter.
const PLAN_MODE: &str = "plan";
/// L'outil par lequel Claude, en mode plan, propose son plan à approuver.
const PLAN_TOOL: &str = "ExitPlanMode";
/// Modes proposés au lancement d'une mission.
const LAUNCH_MODES: [&str; 4] = ["bypassPermissions", "acceptEdits", "auto", "manual"];
/// Taille maximale d'une image acceptée par l'API de Claude.
const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const MAX_SUGGESTIONS: usize = 8;

/// Une proposition du menu `/` ou `@` : ce qu'on montre, et le texte qu'elle met
/// dans la zone de saisie.
struct Suggestion {
    label: SharedString,
    detail: SharedString,
    insert: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Status {
    Idle,
    Working,
    /// Claude attend une réponse à une demande de permission.
    NeedsYou,
}

enum Item {
    User(SharedString),
    /// Réponse de Claude en Markdown, qui grandit pendant le streaming.
    Text(String),
    Thinking,
    Tool {
        id: String,
        name: String,
        input: Value,
        result: Option<(bool, String)>,
        /// Heure du résultat : notée par Claude dans l'historique, sinon à réception.
        finished: Option<chrono::DateTime<chrono::Local>>,
        open: bool,
    },
    Note {
        text: SharedString,
        error: bool,
    },
}

struct Permission {
    request_id: String,
    tool: String,
    input: Value,
}

/// Une mission demande une nouvelle tentative : la fenêtre ouvre une
/// conversation neuve qui la porte.
pub struct Relaunch {
    pub project: PathBuf,
    pub provider: Provider,
    pub title: SharedString,
    pub mission: Mission,
    pub brief: String,
}

impl EventEmitter<Relaunch> for SessionView {}

pub struct SessionView {
    /// Dossier de travail ; vide tant qu'aucun projet n'est choisi sur l'accueil.
    pub project: PathBuf,
    /// Chemin réel du projet, retiré des chemins affichés dans le fil.
    root: PathBuf,
    /// Connu une fois que le CLI a créé la session.
    pub session_id: Option<String>,
    pub title: SharedString,
    pub status: Status,
    items: Vec<Item>,
    scroller: Entity<MessageScrollerState>,
    input: Entity<TextareaState>,
    claude: Option<Agent>,
    /// L'agent qui tient la conversation : Claude ou Codex.
    pub provider: Provider,
    /// La branche git du projet, montrée sous le composer.
    branch: Option<String>,
    /// Le menu du modèle : ouvert, son panneau, l'onglet de fournisseur
    /// (`None` : les favoris) et sa recherche.
    model_menu_open: bool,
    model_panel: pickers::Panel,
    model_tab: Option<Provider>,
    model_search: Entity<gpui_kit::component::input::InputState>,
    mode: &'static str,
    permission: Option<Permission>,
    models: Vec<Model>,
    commands: Vec<SlashCommand>,
    /// Valeur du modèle choisi, ou celle que la session annonce.
    model: Option<String>,
    effort: Option<String>,
    images: Vec<Arc<Image>>,
    /// Fichiers du projet, listés à la première mention `@`.
    files: Option<Vec<String>>,
    /// Projets proposés sur l'accueil, tenus à jour par la fenêtre.
    /// Projets connus, proposés par le sélecteur de projet de l'accueil.
    projects: Vec<PathBuf>,
    /// Numéro du processus `claude` courant : les événements d'un processus
    /// remplacé (changement de projet) sont ignorés.
    spawned: u64,
    /// Échec de lancement du CLI avant le premier message, montré sur l'accueil.
    launch_error: Option<SharedString>,
    /// La mission que porte la conversation, sauvegardée par la fenêtre.
    pub mission: Option<Mission>,
    /// Sur l'accueil : le premier message ouvre une mission, planifiée d'abord.
    planning: bool,
    /// Mode proposé au lancement de la mission : celui d'avant la planification.
    launch_mode: &'static str,
    /// Vue affichée d'une mission : la conversation ou son plan.
    pane: workspace::Pane,
    /// Une interruption est demandée et pas encore confirmée par la fin du tour.
    stopping: bool,
    /// Rouverte après un arrêt de SOLAR au milieu d'un tour : rien n'est relancé.
    interrupted: bool,
    /// Arborescence de la vue Fichiers, lue à sa première ouverture.
    file_tree: Option<Entity<gpui_kit::component::tree::TreeState>>,
    files_listed: bool,
    /// Le fichier ouvert dans la vue Fichiers et son contenu.
    opened: Option<(String, files::Content)>,
    /// Dossier des instantanés de projet ; un dossier jetable dans les tests.
    checkpoints: PathBuf,
    /// L'instantané de lancement est en cours : un second clic ne relance rien.
    launching: bool,
    review: Option<review::Review>,
    review_busy: bool,
    /// « Restaurer » attend sa confirmation.
    confirm_restore: bool,
    /// Ce que la revue doit signaler : changement depuis l'examen, échec.
    review_note: Option<SharedString>,
    preview: preview::Preview,
    /// L'agent relecteur au travail, et son état affiché.
    reviewer: Option<reviewer::Reviewer>,
    reviewing: bool,
    /// L'agent choisi pour la dernière relecture.
    reviewer_provider: Provider,
    /// Le relecteur s'est arrêté sans verdict : il est bloqué, on le dit.
    reviewer_failed: bool,
    /// Le dernier tour de Claude a fini en erreur.
    turn_failed: bool,
    /// Pour une nouvelle tentative : ce que la session neuve reçoit au lancement
    /// (le plan approuvé, et ce qui n'allait pas avant).
    pub brief: Option<String>,
    /// Arrête le serveur de preview quand SOLAR quitte.
    _on_quit: Subscription,
}

impl SessionView {
    pub fn new(
        project: PathBuf,
        session_id: Option<String>,
        title: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(if session_id.is_some() { 1 } else { 3 }, 10)
                .submit_on_enter(true)
                .placeholder(if session_id.is_some() {
                    t!("composer.placeholder", agent = Provider::Claude.name())
                } else {
                    t!("home.placeholder")
                })
        });
        cx.subscribe_in(
            &input,
            window,
            |this, input, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false, .. } => this.send(window, cx),
                InputEvent::Change => {
                    if this.files.is_none() && input.read(cx).value().contains('@') {
                        this.files = Some(files::list(&this.workdir()));
                    }
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();

        let mut this = Self {
            root: std::fs::canonicalize(&project).unwrap_or_else(|_| project.clone()),
            project,
            session_id,
            title,
            status: Status::Idle,
            items: Vec::new(),
            scroller: cx.new(|cx| MessageScrollerState::new(0, cx)),
            input,
            claude: None,
            provider: Provider::Claude,
            branch: None,
            model_menu_open: false,
            model_panel: pickers::Panel::default(),
            model_tab: None,
            model_search: {
                let search = cx.new(|cx| {
                    gpui_kit::component::input::InputState::new(window, cx)
                        .placeholder(t!("models.search"))
                });
                cx.observe(&search, |_, _, cx| cx.notify()).detach();
                search
            },
            mode: MODES[0],
            permission: None,
            models: Vec::new(),
            commands: Vec::new(),
            model: None,
            effort: None,
            images: Vec::new(),
            files: None,
            projects: Vec::new(),
            spawned: 0,
            launch_error: None,
            mission: None,
            planning: false,
            launch_mode: MODES[0],
            pane: workspace::Pane::default(),
            stopping: false,
            interrupted: false,
            file_tree: None,
            files_listed: false,
            opened: None,
            checkpoints: solar_core::checkpoint::Store::default_dir(),
            launching: false,
            review: None,
            review_busy: false,
            confirm_restore: false,
            review_note: None,
            preview: preview::Preview::default(),
            reviewer: None,
            reviewing: false,
            reviewer_provider: Provider::Claude,
            reviewer_failed: false,
            turn_failed: false,
            brief: None,
            _on_quit: cx.on_app_quit(|this: &mut Self, _| {
                this.preview.server.take();
                async {}
            }),
        };
        this.load_history(cx);
        if !this.project.as_os_str().is_empty() {
            this.refresh_branch(cx);
        }
        this
    }

    // shortcut: l'historique est relu d'un bloc à l'ouverture ; le charger en
    // arrière-plan si de longues sessions ralentissent l'ouverture d'un onglet.
    fn load_history(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.session_id.clone() {
            for event in solar_core::agent::history(self.provider, &self.workdir(), &id) {
                self.on_event(event, cx);
            }
            self.status = Status::Idle;
            self.scroller.update(cx, |list, cx| list.scroll_to_end(cx));
        }
    }

    /// Où Claude travaille : la copie de travail de la mission, sinon le projet.
    pub(crate) fn workdir(&self) -> PathBuf {
        self.mission
            .as_ref()
            .and_then(|mission| mission.worktree.clone())
            .unwrap_or_else(|| self.project.clone())
    }

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
        self.mission = mission;
    }

    /// Un plan attend une réponse du CLI en cours.
    fn plan_pending(&self) -> bool {
        self.permission
            .as_ref()
            .is_some_and(|permission| permission.tool == PLAN_TOOL)
    }

    /// Lance la mission dans le mode choisi. Le mode est envoyé avant l'accord :
    /// sinon le CLI reviendrait un instant au mode d'avant la planification.
    /// L'instantané du projet est pris d'abord, en arrière-plan : c'est la base
    /// de la revue. Claude ne reçoit l'accord qu'ensuite.
    fn launch_mission(&mut self, mode: &'static str, cx: &mut Context<Self>) {
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
        cx.spawn(async move |this, cx| {
            let base = cx
                .background_spawn(async move {
                    if fresh && let Some((base, dir)) = store.isolate(&id)? {
                        return Ok((base, Some(dir)));
                    }
                    store.checkpoint().map(|base| (base, None))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.launching = false;
                match base {
                    Ok((base, dir)) => {
                        // L'agent lancé d'avance tourne dans le projet : il repart
                        // dans la copie.
                        if dir.is_some() {
                            this.claude = None;
                        }
                        if let Some(mission) = this.mission.as_mut() {
                            mission.checkpoint = Some(base);
                            mission.worktree = dir;
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

    fn start_mission(&mut self, mode: &'static str, cx: &mut Context<Self>) {
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
        self.status = Status::Working;
        cx.notify();
    }

    /// Abandonne la mission avant son lancement : Claude, toujours en mode plan,
    /// apprend qu'il doit s'arrêter ; la conversation et ses plans restent.
    fn cancel_mission(&mut self, cx: &mut Context<Self>) {
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
    fn relaunch(&mut self, cx: &mut Context<Self>) {
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
        let next = Mission {
            status: MissionStatus::Running,
            plans: mission.plans.clone(),
            launched_mode: mission.launched_mode.clone(),
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

    /// La conversation a commencé : elle n'est plus un brouillon de l'accueil.
    pub fn started(&self) -> bool {
        !self.items.is_empty()
    }

    /// La vue passe au premier plan : on y place le curseur et on lance le CLI,
    /// pour connaître tout de suite ses modèles et ses commandes.
    // shortcut: un processus `claude` reste vivant par conversation déjà affichée ;
    // l'arrêter après un temps d'inactivité si la mémoire devient un sujet.
    pub fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
        self.launch(cx);
    }

    /// Lance le CLI dès qu'un projet est choisi.
    fn launch(&mut self, cx: &mut Context<Self>) {
        if self.project.as_os_str().is_empty() {
            return;
        }
        if let Err(error) = self.ensure_claude(cx).map(|_| ()) {
            self.fail(error, cx);
        }
    }

    /// Le CLI n'a pas pu démarrer ou recevoir le message. Sur l'accueil, l'erreur
    /// s'affiche sans commencer la conversation ni toucher au brouillon.
    fn fail(&mut self, error: std::io::Error, cx: &mut Context<Self>) {
        self.claude = None;
        let text = t!(
            "thread.failed",
            agent = self.provider.name(),
            error = error.to_string()
        );
        if self.started() {
            self.note(text, true, cx);
        } else {
            self.launch_error = Some(text.into());
            cx.notify();
        }
    }

    /// Change le projet d'une conversation pas encore commencée. Le brouillon
    /// (texte, images, réglages) est conservé.
    pub fn set_project(&mut self, project: PathBuf, cx: &mut Context<Self>) {
        if self.started() || project == self.project {
            return;
        }
        // Le CLI est lié à un dossier : celui de l'ancien projet est arrêté.
        self.claude = None;
        self.root = std::fs::canonicalize(&project).unwrap_or_else(|_| project.clone());
        self.project = project;
        self.files = None;
        self.launch_error = None;
        self.launch(cx);
        self.refresh_branch(cx);
        cx.notify();
    }

    /// Relit la branche git du projet, en arrière-plan.
    pub(crate) fn refresh_branch(&mut self, cx: &mut Context<Self>) {
        let project = self.project.clone();
        cx.spawn(async move |this, cx| {
            let branch = cx
                .background_spawn(async move { solar_core::project::branch(&project) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.branch = branch;
                cx.notify();
            });
        })
        .detach();
    }

    pub fn set_projects(&mut self, projects: Vec<PathBuf>, cx: &mut Context<Self>) {
        if self.projects != projects {
            self.projects = projects;
            cx.notify();
        }
    }

    /// Ouvre le sélecteur de dossier natif et prend le dossier choisi comme
    /// projet ; `then_send` envoie ensuite le brouillon qui attendait un projet.
    fn choose_folder(&mut self, then_send: bool, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                if let Some(path) = paths.into_iter().next() {
                    this.set_project(path, cx);
                    if then_send {
                        this.send(window, cx);
                    }
                }
            });
        })
        .detach();
    }

    /// Ouvre le sélecteur de fichiers natif pour joindre des images.
    fn pick_images(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picked.await {
                let _ = this.update(cx, |this, cx| this.attach_files(&paths, cx));
            }
        })
        .detach();
    }

    /// Remplace le brouillon et place le curseur à la fin.
    fn fill(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            let end = text.len();
            input.set_value(text, window, cx);
            input.set_selected_range(end..end, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        // Sans projet, on demande d'abord le dossier ; le brouillon attend.
        if self.project.as_os_str().is_empty() {
            self.choose_folder(true, window, cx);
            return;
        }
        // Un plan attend : le message est une correction. Claude reste en mode
        // plan et propose une nouvelle version.
        if self.plan_pending() {
            let plan = self.permission.take().expect("vérifié juste au-dessus");
            if let Some(claude) = self.claude.as_mut() {
                let _ = claude.correct_plan(&plan.request_id, &text);
            }
            if let Some(mission) = self.mission.as_mut() {
                mission.status = MissionStatus::Planning;
            }
            self.input
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.push(Item::User(text.into()), cx);
            self.status = Status::Working;
            cx.notify();
            return;
        }
        let plan_first = self.planning && !self.started();
        // Une mission naît : sa copie de travail est prête avant que Claude ne
        // démarre, pour qu'il y travaille dès le plan.
        if plan_first && self.mission.is_none() {
            if self.launching {
                return;
            }
            self.launching = true;
            cx.notify();
            let store =
                solar_core::checkpoint::Store::for_project(&self.checkpoints, &self.project);
            let id = copy_id();
            cx.spawn_in(window, async move |this, cx| {
                let copy = cx.background_spawn(async move { store.isolate(&id) }).await;
                let _ = this.update_in(cx, |this, window, cx| {
                    this.launching = false;
                    let mission = this.mission.insert(Mission::default());
                    match copy {
                        Ok(Some((base, dir))) => {
                            mission.checkpoint = Some(base);
                            mission.worktree = Some(dir);
                            // Le Claude lancé d'avance tourne dans le projet ; aucune
                            // conversation n'a commencé, il repart dans la copie.
                            this.claude = None;
                        }
                        Ok(None) => {}
                        // Sans copie, la mission travaille dans le projet, comme avant.
                        Err(error) => this.note(t!("mission.copy_failed", error = error), true, cx),
                    }
                    this.send(window, cx);
                });
            })
            .detach();
            return;
        }
        let images: Vec<_> = self
            .images
            .iter()
            .map(|image| (image.format.mime_type().to_string(), image.bytes.clone()))
            .collect();
        let sent = self.ensure_claude(cx).and_then(|claude| {
            if plan_first {
                claude.set_permission_mode(PLAN_MODE)?;
            }
            claude.send(&text, &images)
        });
        if let Err(error) = sent {
            self.fail(error, cx);
            return;
        }
        if plan_first {
            self.launch_mode = self.mode;
            self.mode = PLAN_MODE;
            self.planning = false;
            self.mission.get_or_insert_with(Mission::default);
        }
        if !self.started() {
            // Le brouillon de l'accueil devient une conversation ordinaire.
            self.input.update(cx, |input, cx| {
                input.set_placeholder(
                    t!("composer.placeholder", agent = self.provider.name()),
                    window,
                    cx,
                );
                input.set_auto_grow(1, 10, cx);
            });
        }
        if self.session_id.is_none() && self.items.is_empty() {
            self.title = text.chars().take(48).collect::<String>().into();
        }
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        let shown = if images.is_empty() {
            text
        } else {
            format!("{text}\n{}", t!("thread.images", count = images.len()))
        };
        self.images.clear();
        self.interrupted = false;
        self.turn_failed = false;
        if let Some(mission) = self.mission.as_mut().filter(|mission| {
            matches!(
                mission.status,
                MissionStatus::Accepted | MissionStatus::Restored
            )
        }) {
            mission.status = MissionStatus::Running;
        }
        self.push(Item::User(shown.into()), cx);
        self.scroller.update(cx, |list, cx| list.scroll_to_end(cx));
        self.status = Status::Working;
        cx.notify();
    }

    /// Lance le CLI au premier message, ou le relance s'il s'est arrêté.
    fn ensure_claude(&mut self, cx: &mut Context<Self>) -> std::io::Result<&mut Agent> {
        if self.claude.is_none() {
            self.spawned += 1;
            let generation = self.spawned;
            let (tx, rx) = smol::channel::unbounded();
            let workdir = self.workdir();
            if !workdir.is_dir() {
                return Err(std::io::Error::other(t!("mission.copy_gone").to_string()));
            }
            let mut claude = Agent::spawn(
                self.provider,
                &workdir,
                self.session_id.as_deref(),
                self.mode,
                move |event| {
                    let _ = tx.send_blocking(event);
                },
            )?;
            cx.spawn(async move |this, cx| {
                while let Ok(event) = rx.recv().await {
                    let alive = this.update(cx, |this, cx| {
                        if this.spawned == generation {
                            this.on_event(event, cx);
                        }
                    });
                    if alive.is_err() {
                        break;
                    }
                }
            })
            .detach();
            if let Some(model) = &self.model {
                claude.set_model(model)?;
            }
            if let Some(effort) = &self.effort {
                claude.set_effort(effort)?;
            }
            self.claude = Some(claude);
            self.launch_error = None;
        }
        Ok(self.claude.as_mut().expect("créé juste au-dessus"))
    }

    fn on_event(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Catalog { models, commands } => {
                // Les modèles de Claude servent aussi aux autres conversations.
                cx.default_global::<Catalogs>().0.insert(
                    Provider::Claude,
                    Catalog {
                        models: models.clone(),
                        default: None,
                    },
                );
                self.models = models;
                self.commands = commands;
            }
            Event::Settings { model, effort } => {
                self.model = self.model.take().or(model);
                self.effort = self.effort.take().or(effort);
            }
            Event::Ready {
                session_id, model, ..
            } => {
                self.session_id = Some(session_id);
                // Codex n'annonce pas son modèle : celui de sa configuration s'applique.
                if self.model.is_none() && !model.is_empty() {
                    let known = self.models.iter().find(|known| known.resolved == model);
                    self.model = Some(known.map_or(model, |known| known.value.clone()));
                }
            }
            Event::User(text) => self.push(Item::User(text.into()), cx),
            Event::Text(chunk) => {
                self.status = Status::Working;
                if let Some(Item::Text(text)) = self.items.last_mut() {
                    text.push_str(&chunk);
                    self.remeasure(self.items.len() - 1, cx);
                } else {
                    self.push(Item::Text(chunk), cx);
                }
            }
            Event::Thinking => {
                self.status = Status::Working;
                if !matches!(self.items.last(), Some(Item::Thinking)) {
                    self.push(Item::Thinking, cx);
                }
            }
            Event::ToolStart { id, name, input } => {
                self.status = Status::Working;
                self.push(
                    Item::Tool {
                        id,
                        name,
                        input,
                        result: None,
                        finished: None,
                        open: false,
                    },
                    cx,
                );
            }
            Event::ToolResult {
                id,
                is_error,
                text,
                at,
            } => {
                let at = at
                    .and_then(|at| chrono::DateTime::parse_from_rfc3339(&at).ok())
                    .map_or_else(chrono::Local::now, |at| at.with_timezone(&chrono::Local));
                let found =
                    self.items
                        .iter_mut()
                        .enumerate()
                        .rev()
                        .find_map(|(ix, item)| match item {
                            Item::Tool {
                                id: tool_id,
                                result,
                                finished,
                                ..
                            } if *tool_id == id => Some((ix, result, finished)),
                            _ => None,
                        });
                if let Some((ix, result, finished)) = found {
                    *result = Some((is_error, text));
                    *finished = Some(at);
                    self.remeasure(ix, cx);
                }
            }
            Event::PermissionRequest {
                request_id,
                tool,
                input,
            } => {
                if tool == PLAN_TOOL {
                    let plan = input["plan"].as_str().unwrap_or_default().to_string();
                    let mission = self.mission.get_or_insert_with(Mission::default);
                    if mission.plans.last() != Some(&plan) {
                        mission.plans.push(plan);
                    }
                    mission.status = MissionStatus::Ready;
                }
                self.permission = Some(Permission {
                    request_id,
                    tool,
                    input,
                });
                self.status = Status::NeedsYou;
            }
            Event::TurnDone {
                error,
                context_tokens,
                cost_usd,
            } => {
                self.status = Status::Idle;
                self.permission = None;
                self.stopping = false;
                self.turn_failed = error.is_some();
                match error {
                    Some(error) => self.note(error, true, cx),
                    None => self.note(
                        t!(
                            "thread.stats",
                            tokens = format!("{:.1}k", context_tokens as f64 / 1000.0),
                            cost = format!("{cost_usd:.2}")
                        ),
                        false,
                        cx,
                    ),
                }
            }
            Event::Exited => {
                self.claude = None;
                self.status = Status::Idle;
                self.permission = None;
                self.stopping = false;
            }
        }
        cx.notify();
    }

    fn answer_permission(&mut self, allow: bool, cx: &mut Context<Self>) {
        let (Some(permission), Some(claude)) = (self.permission.take(), self.claude.as_mut())
        else {
            return;
        };
        let _ = if allow {
            claude.allow(&permission.request_id, &permission.input)
        } else {
            claude.deny(&permission.request_id, "Denied by the user")
        };
        self.status = Status::Working;
        cx.notify();
    }

    /// Demande l'arrêt du tour en cours ; « Arrêt demandé » s'affiche jusqu'à la
    /// fin du tour, que le CLI confirme.
    fn interrupt(&mut self, cx: &mut Context<Self>) {
        if self.status == Status::Idle {
            return;
        }
        if let Some(claude) = self.claude.as_mut() {
            let _ = claude.interrupt();
            self.stopping = true;
            cx.notify();
        }
    }

    /// Choisit l'agent de la conversation, tant qu'elle n'a pas commencé, ou
    /// celui d'une conversation rouverte. Le CLI lancé d'avance pour l'autre
    /// fournisseur s'arrête.
    pub fn set_provider(
        &mut self,
        provider: Provider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if provider == self.provider {
            return;
        }
        self.provider = provider;
        self.claude = None;
        self.launch_error = None;
        self.models.clear();
        self.commands.clear();
        self.model = None;
        self.effort = None;
        // Codex ne sait pas demander avant d'agir : le mode le plus proche est
        // l'écriture dans le projet, bornée par sa sandbox.
        if !provider.supports(self.mode) {
            self.mode = MODES[1];
        }
        if self.session_id.is_some() || self.started() {
            self.input.update(cx, |input, cx| {
                input.set_placeholder(
                    t!("composer.placeholder", agent = provider.name()),
                    window,
                    cx,
                )
            });
        } else {
            self.launch(cx);
        }
        cx.notify();
    }

    /// Le mode de permission en cours.
    pub(crate) fn mode(&self) -> &'static str {
        self.mode
    }

    /// Les fichiers touchés par l'agent dans cette conversation, relatifs au
    /// dossier de travail, du plus récent au plus ancien.
    pub(crate) fn touched_files(&self) -> Vec<String> {
        let root = self.workdir();
        let mut seen = Vec::new();
        for item in self.items.iter().rev() {
            if let Item::Tool { input, .. } = item
                && let Some(file) = input["file_path"].as_str()
                && let Ok(inside) = std::path::Path::new(file).strip_prefix(&root)
            {
                let path = inside.to_string_lossy().to_string();
                if !seen.contains(&path) {
                    seen.push(path);
                }
            }
        }
        seen
    }

    pub(crate) fn set_mode(&mut self, mode: &'static str, cx: &mut Context<Self>) {
        self.mode = mode;
        if let Some(claude) = self.claude.as_mut() {
            let _ = claude.set_permission_mode(mode);
        }
        cx.notify();
    }

    /// Le modèle qu'utilisera la session : choisi, réglé dans Claude Code, ou à
    /// défaut celui que le CLI désigne par `default`.
    fn current_model(&self) -> Option<&Model> {
        let find = |value: &str| self.models.iter().find(|model| model.value == value);
        match self.model.as_deref() {
            // Les réglages peuvent ajouter une variante entre crochets : `opus[1m]`.
            Some(value) => find(value).or_else(|| find(value.split('[').next()?)),
            None => find("default"),
        }
    }

    /// Les modèles annoncés par Claude au lancement ; vide pour les autres agents.
    pub(crate) fn models(&self) -> &[Model] {
        &self.models
    }

    pub(crate) fn set_model(&mut self, value: String, cx: &mut Context<Self>) {
        if let Some(claude) = self.claude.as_mut() {
            let _ = claude.set_model(&value);
        }
        self.model = Some(value);
        // Un effort que le nouveau modèle ne propose pas est abandonné.
        let supported = self.current_model().map(|model| model.efforts.clone());
        if let (Some(effort), Some(supported)) = (&self.effort, supported)
            && !supported.contains(effort)
        {
            self.effort = None;
        }
        cx.notify();
    }

    pub(crate) fn set_effort(&mut self, effort: String, cx: &mut Context<Self>) {
        if let Some(claude) = self.claude.as_mut() {
            let _ = claude.set_effort(&effort);
        }
        self.effort = Some(effort);
        cx.notify();
    }

    fn attach(&mut self, image: Image, cx: &mut Context<Self>) {
        if image.bytes.len() > MAX_IMAGE_BYTES {
            self.note(t!("composer.image_too_big"), true, cx);
        } else {
            self.images.push(Arc::new(image));
            cx.notify();
        }
    }

    /// Colle l'image du presse-papiers, s'il y en a une.
    fn attach_from_clipboard(&mut self, cx: &mut Context<Self>) -> bool {
        let image = cx.read_from_clipboard().and_then(|item| {
            item.entries().iter().find_map(|entry| match entry {
                ClipboardEntry::Image(image) => Some(image.clone()),
                _ => None,
            })
        });
        image.map(|image| self.attach(image, cx)).is_some()
    }

    fn attach_files(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        for path in paths {
            let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
            let format = match extension.to_ascii_lowercase().as_str() {
                "png" => ImageFormat::Png,
                "jpg" | "jpeg" => ImageFormat::Jpeg,
                "webp" => ImageFormat::Webp,
                "gif" => ImageFormat::Gif,
                _ => continue,
            };
            if let Ok(bytes) = std::fs::read(path) {
                self.attach(Image::from_bytes(format, bytes), cx);
            }
        }
    }

    /// Propositions pour ce qui est en train d'être tapé : commandes si le
    /// message commence par `/`, fichiers si le dernier mot commence par `@`.
    fn suggestions(&self, cx: &App) -> Vec<Suggestion> {
        let value = self.input.read(cx).value();
        let word_start = value.rfind(char::is_whitespace).map_or(0, |ix| {
            ix + value[ix..].chars().next().map_or(1, char::len_utf8)
        });
        let word = &value[word_start..];

        if let Some(query) = value.strip_prefix('/')
            && !query.contains(char::is_whitespace)
        {
            let query = query.to_lowercase();
            self.commands
                .iter()
                .filter(|command| command.name.to_lowercase().contains(&query))
                .take(MAX_SUGGESTIONS)
                .map(|command| Suggestion {
                    label: format!("/{}", command.name).into(),
                    detail: command.description.clone().into(),
                    insert: format!("/{} ", command.name),
                })
                .collect()
        } else if let Some(query) = word.strip_prefix('@') {
            let query = query.to_lowercase();
            self.files
                .iter()
                .flatten()
                .filter(|file| file.to_lowercase().contains(&query))
                .take(MAX_SUGGESTIONS)
                .map(|file| Suggestion {
                    label: file.clone().into(),
                    detail: SharedString::default(),
                    insert: format!("{}@{file} ", &value[..word_start]),
                })
                .collect()
        } else {
            vec![]
        }
    }

    fn accept(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(suggestion) = self.suggestions(cx).into_iter().nth(ix) else {
            return false;
        };
        self.fill(suggestion.insert, window, cx);
        true
    }

    fn toggle(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(Item::Tool { open, .. }) = self.items.get_mut(ix) {
            *open = !*open;
            self.remeasure(ix, cx);
            cx.notify();
        }
    }

    fn note(&mut self, text: impl Into<SharedString>, error: bool, cx: &mut Context<Self>) {
        self.push(
            Item::Note {
                text: text.into(),
                error,
            },
            cx,
        );
        cx.notify();
    }

    fn push(&mut self, item: Item, cx: &mut Context<Self>) {
        self.items.push(item);
        self.scroller.update(cx, |list, cx| {
            list.append(1, cx);
        });
    }

    /// La liste est virtualisée : une ligne qui change de hauteur doit être remesurée.
    fn remeasure(&self, ix: usize, cx: &mut Context<Self>) {
        self.scroller.update(cx, |list, cx| {
            list.remeasure_items(ix..ix + 1, cx);
        });
    }

    /// Ce qu'un outil manipule, en une ligne : fichier, commande ou motif.
    fn tool_summary(&self, input: &Value) -> String {
        if let Some(file) = input["file_path"].as_str() {
            let root = self.mission.as_ref().and_then(|m| m.worktree.as_deref());
            return short_path(std::path::Path::new(file), root.unwrap_or(&self.root));
        }
        ["command", "pattern", "url", "description", "prompt"]
            .iter()
            .find_map(|key| input[key].as_str())
            .and_then(|value| value.lines().next())
            .unwrap_or_default()
            .to_string()
    }

    fn render_item(&self, ix: usize, view: &Entity<Self>, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let last = ix + 1 == self.items.len();
        let gap = match &self.items[ix] {
            Item::User(_) => px(14.),
            Item::Text(_) => px(8.),
            _ => px(3.),
        };
        let avatar = |content: AnyElement| {
            h_flex()
                .flex_shrink_0()
                .size_7()
                .justify_center()
                .rounded(theme.radius)
                .border_1()
                .border_color(theme.border)
                .bg(theme.secondary)
                .child(content)
        };
        let from_user = matches!(self.items[ix], Item::User(_));
        // Premier élément d'un tour de Claude : il porte son avatar et son nom.
        let opens_turn = !from_user && (ix == 0 || matches!(self.items[ix - 1], Item::User(_)));
        let row = match &self.items[ix] {
            Item::User(text) => h_flex()
                .justify_end()
                .child(
                    div()
                        .min_w_0()
                        .max_w(relative(0.85))
                        .px_3p5()
                        .py_2()
                        .rounded(theme.radius)
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.secondary)
                        .child(text.clone()),
                )
                .into_any_element(),
            Item::Text(text) => TextView::markdown(("text", ix), text.clone())
                .style(
                    TextViewStyle::default().code_block(
                        StyleRefinement::default()
                            .bg(transparent_black())
                            .border_1()
                            .border_color(theme.border),
                    ),
                )
                .selectable(true)
                .into_any_element(),
            Item::Thinking => {
                let label = t!("thread.thinking").to_string();
                if last && self.status == Status::Working {
                    ShimmerText::new(label)
                        .id(("thinking", ix))
                        .into_any_element()
                } else {
                    div()
                        .text_color(theme.muted_foreground)
                        .child(label.to_string())
                        .into_any_element()
                }
            }
            Item::Tool { name, input, .. } if name == PLAN_TOOL => v_flex()
                .p_4()
                .gap_2()
                .rounded(theme.radius)
                .border_1()
                .border_color(theme.input)
                .child(
                    h_flex()
                        .gap_2()
                        .text_xs()
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.muted_foreground)
                        .child(Icon::new(IconName::ListChecks).xsmall())
                        .child(caps(&t!("mission.plan"))),
                )
                .child(
                    TextView::markdown(
                        ("plan", ix),
                        input["plan"].as_str().unwrap_or_default().to_string(),
                    )
                    .selectable(true),
                )
                .into_any_element(),
            Item::Tool {
                name,
                input,
                result,
                open,
                ..
            } => {
                let failed = matches!(result, Some((true, _)));
                let view = view.clone();
                v_flex()
                    .gap_1p5()
                    .child(
                        h_flex()
                            .id(("tool", ix))
                            .gap_2()
                            .cursor_pointer()
                            .text_color(if failed {
                                theme.danger
                            } else {
                                theme.muted_foreground
                            })
                            .hover(|row| row.text_color(theme.foreground))
                            .child(if result.is_none() && self.status == Status::Working {
                                Spinner::new().xsmall().into_any_element()
                            } else if failed {
                                Icon::new(IconName::CircleAlert).xsmall().into_any_element()
                            } else {
                                Icon::new(tool_icon(name)).xsmall().into_any_element()
                            })
                            .child(div().font_medium().child(name.clone()))
                            .child(div().truncate().child(self.tool_summary(input)))
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| this.toggle(ix, cx));
                            }),
                    )
                    .when(*open, |column| {
                        let detail = match result {
                            Some((_, text)) => format!("{input:#}\n\n{text}"),
                            None => format!("{input:#}"),
                        };
                        column.child(
                            div()
                                .p_3()
                                .rounded(theme.radius)
                                .bg(theme.secondary)
                                .font_family(theme.mono_font_family.clone())
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                // shortcut: sortie coupée à 4000 caractères, sans défilement ;
                                // vue complète quand le panneau diff et terminal existera (étape 2).
                                .child(detail.chars().take(4000).collect::<String>()),
                        )
                    })
                    .into_any_element()
            }
            Item::Note { text, error } => div()
                .text_xs()
                .text_color(if *error {
                    theme.danger
                } else {
                    theme.muted_foreground
                })
                .child(text.clone())
                .into_any_element(),
        };
        h_flex()
            .w_full()
            .justify_center()
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(760.))
                    .px_6()
                    .py(gap)
                    .gap_2()
                    .when(opens_turn, |turn| {
                        turn.child(
                            h_flex()
                                .gap_3()
                                // L'avatar porte le logo du fournisseur qui répond.
                                .child(avatar(match self.provider {
                                    Provider::Claude => svg()
                                        .path(CLAUDE_LOGO)
                                        .size_4()
                                        .text_color(rgb(CLAUDE_BRAND))
                                        .into_any_element(),
                                    // shortcut: icône neutre tant que les logos officiels
                                    // d'OpenAI et de xAI ne sont pas récupérés (voir
                                    // assets/providers).
                                    Provider::Codex | Provider::Grok => Icon::new(IconName::Bot)
                                        .small()
                                        .text_color(cx.theme().foreground)
                                        .into_any_element(),
                                }))
                                .child(div().font_medium().child(caps(self.provider.name()))),
                        )
                    })
                    // Ce que dit et fait Claude s'aligne sous son nom.
                    .child(
                        div()
                            .font_family(READING_FONT)
                            .when(!from_user, |row| row.pl(px(40.)))
                            .child(row),
                    ),
            )
            .into_any_element()
    }

    /// Un bouton discret qui ouvre un menu d'options, l'option courante cochée.
    // Chaque sélecteur du composer a son identifiant, son icône, ses options et son effet.
    #[allow(clippy::too_many_arguments)]
    fn render_picker(
        &self,
        id: &'static str,
        icon: IconName,
        label: SharedString,
        options: Vec<(String, SharedString)>,
        current: Option<String>,
        pick: fn(&mut Self, String, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity();
        Button::new(id)
            .ghost()
            .small()
            .icon(icon)
            .label(label)
            .dropdown_menu(move |menu, _, _| {
                options.iter().fold(menu, |menu, (value, name)| {
                    let (view, value) = (view.clone(), value.clone());
                    menu.item(
                        PopupMenuItem::new(name.clone())
                            .checked(current.as_deref() == Some(value.as_str()))
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| pick(this, value.clone(), window, cx));
                            }),
                    )
                })
            })
            .on_open_change(self.page_under_menu(cx))
    }

    fn render_suggestions(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let suggestions = self.suggestions(cx);
        if suggestions.is_empty() {
            return None;
        }
        let theme = cx.theme();
        let rows = suggestions.into_iter().enumerate().map(|(ix, suggestion)| {
            h_flex()
                .id(("suggestion", ix))
                .px_2p5()
                .py_1p5()
                .gap_3()
                .rounded(theme.radius)
                .cursor_pointer()
                .hover(|row| row.bg(theme.secondary))
                // La première ligne est celle que Tab valide.
                .when(ix == 0, |row| row.bg(theme.secondary))
                .child(div().flex_shrink_0().font_medium().child(suggestion.label))
                .child(
                    div()
                        .truncate()
                        .text_color(theme.muted_foreground)
                        .child(suggestion.detail),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.accept(ix, window, cx);
                }))
        });
        Some(
            v_flex()
                .w_full()
                .max_w(px(760.))
                .p_1()
                .rounded(theme.radius)
                .border_1()
                .border_color(theme.border)
                .children(rows),
        )
    }

    fn render_composer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let working = self.status != Status::Idle;

        let can_send = !self.input.read(cx).value().trim().is_empty();

        let thumbnails = self.images.iter().enumerate().map(|(ix, image)| {
            div()
                .id(("image", ix))
                .cursor_pointer()
                .child(
                    img(image.clone())
                        .size_12()
                        .rounded(theme.radius)
                        .object_fit(ObjectFit::Cover),
                )
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(
                        t!("composer.remove_image").to_string(),
                    )
                    .build(window, cx)
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    if ix < this.images.len() {
                        this.images.remove(ix);
                        cx.notify();
                    }
                }))
        });

        let composer = v_flex()
            .id("composer")
            .w_full()
            .max_w(px(760.))
            .p_2()
            .gap_1()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            // Voile de lisibilité : le composer passe sur la planète de l'accueil.
            .bg(rgb(BLACK).opacity(VEIL))
            // Le champ de texte associe Tab et Coller à ses propres actions : on les
            // intercepte avant lui pour valider une proposition ou coller une image.
            .capture_action(cx.listener(|this, _: &IndentInline, window, cx| {
                if this.accept(0, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|this, _: &Paste, _, cx| {
                if this.attach_from_clipboard(cx) {
                    cx.stop_propagation();
                }
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.attach_files(paths.paths(), cx);
            }))
            .when(!self.images.is_empty(), |column| {
                column.child(
                    h_flex()
                        .gap_2()
                        .px_1()
                        .pt_1()
                        .flex_wrap()
                        .children(thumbnails),
                )
            })
            .child(
                div()
                    .font_family(READING_FONT)
                    .child(Textarea::new(&self.input).appearance(false)),
            )
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_1()
                            .flex_wrap()
                            .child(self.render_plus_menu(cx))
                            // Planifier actif : on le voit sans ouvrir le menu ; un clic
                            // le retire.
                            .when(self.planning && !self.started(), |row| {
                                row.child(
                                    Button::new("plan-first")
                                        .rounded(crate::theme::INNER_RADIUS)
                                        .ghost()
                                        .small()
                                        .selected(true)
                                        .icon(IconName::ListChecks)
                                        .label(t!("composer.plan").to_string())
                                        .tooltip(t!("composer.plan.off"))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.planning = false;
                                            cx.notify();
                                        })),
                                )
                            })
                            .child(self.render_model_menu(cx))
                            .child(self.render_mode_menu(cx)),
                    )
                    .child(if working {
                        Button::new("stop")
                            .rounded(crate::theme::INNER_RADIUS)
                            .small()
                            .icon(IconName::Square)
                            .tooltip(t!("composer.stop"))
                            .on_click(cx.listener(|this, _, _, cx| this.interrupt(cx)))
                            .into_any_element()
                    } else {
                        Button::new("send")
                            .rounded(crate::theme::INNER_RADIUS)
                            .map(|send| {
                                if can_send {
                                    send.primary()
                                } else {
                                    send.outline()
                                }
                            })
                            .small()
                            // Rien à envoyer : le bouton s'éteint.
                            .disabled(!can_send)
                            .icon(IconName::ArrowUp)
                            .tooltip(t!("composer.send"))
                            .on_click(cx.listener(|this, _, window, cx| this.send(window, cx)))
                            .into_any_element()
                    }),
            );
        // Sous le composer : le projet et sa branche.
        v_flex()
            .w_full()
            .max_w(px(760.))
            .gap_2()
            .child(composer)
            .child(self.render_under_composer(cx))
    }

    /// Le plan attend une décision : le corriger en écrivant dessous, ou lancer
    /// la mission dans le mode de permission choisi.
    fn render_plan_ready(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let waiting = self.plan_pending()
            || (self.permission.is_none()
                && self.status == Status::Idle
                && self
                    .mission
                    .as_ref()
                    .is_some_and(|mission| mission.status == MissionStatus::Ready));
        if !waiting {
            return None;
        }
        let theme = cx.theme();
        let (view, current, provider) = (cx.entity(), self.launch_mode, self.provider);
        Some(
            h_flex()
                .w_full()
                .max_w(px(760.))
                .px_3p5()
                .py_2p5()
                .gap_3()
                .rounded(theme.radius)
                .border_1()
                .border_color(theme.input)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(t!("mission.ready").to_string()),
                )
                .child(
                    Button::new("cancel-mission")
                        .ghost()
                        .small()
                        .label(t!("mission.cancel").to_string())
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_mission(cx))),
                )
                .child(
                    Button::new("launch-mission")
                        .primary()
                        .small()
                        .icon(IconName::Play)
                        .label(t!("mission.launch").to_string())
                        .dropdown_menu(move |menu, _, _| {
                            // Seuls les modes que l'agent tient vraiment.
                            LAUNCH_MODES
                                .iter()
                                .filter(|mode| provider.supports(mode))
                                .fold(menu, |menu, &mode| {
                                    let view = view.clone();
                                    menu.item(
                                        PopupMenuItem::new(t!(format!("mode.{mode}")))
                                            .checked(mode == current)
                                            .on_click(move |_, _, cx| {
                                                view.update(cx, |this, cx| {
                                                    this.launch_mission(mode, cx)
                                                })
                                            }),
                                    )
                                })
                        })
                        .on_open_change(self.page_under_menu(cx)),
                ),
        )
    }

    fn render_permission(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let permission = self.permission.as_ref().filter(|p| p.tool != PLAN_TOOL)?;
        let theme = cx.theme();
        Some(
            h_flex()
                .w_full()
                .max_w(px(760.))
                .px_3p5()
                .py_2p5()
                .gap_3()
                .rounded(theme.radius)
                .border_1()
                .border_color(theme.border)
                .bg(theme.secondary)
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(t!("permission.title", tool = permission.tool).to_string())
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .truncate()
                                .child(self.tool_summary(&permission.input)),
                        ),
                )
                .child(
                    Button::new("deny")
                        .small()
                        .label(t!("permission.deny").to_string())
                        .on_click(cx.listener(|this, _, _, cx| this.answer_permission(false, cx))),
                )
                .child(
                    Button::new("allow")
                        .primary()
                        .small()
                        .label(t!("permission.allow").to_string())
                        .on_click(cx.listener(|this, _, _, cx| this.answer_permission(true, cx))),
                ),
        )
    }
}

impl SessionView {
    /// Une conversation commencée : le fil, ou pour une mission la barre de
    /// mission et la vue choisie ; dessous, les demandes et le composer, joignables
    /// depuis toutes les vues.
    fn render_thread(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let waiting = self.status == Status::Working
            && !matches!(self.items.last(), Some(Item::Text(_) | Item::Thinking));
        // Hors mission, seule la vue Fichiers s'ajoute à la conversation.
        let pane = if self.mission.is_some() || self.pane == workspace::Pane::Files {
            self.pane
        } else {
            workspace::Pane::Conversation
        };

        // La vue Fichiers ajoute son arborescence à droite, sur toute la hauteur ;
        // le composer reste dans la colonne du contenu.
        let (content, aside) = match pane {
            workspace::Pane::Plan => (self.render_plan_view(cx).into_any_element(), None),
            workspace::Pane::Files => {
                let (viewer, tree) = self.render_files_view(cx);
                (viewer, Some(tree))
            }
            workspace::Pane::Changes => {
                let (diff, list) = self.render_changes_view(cx);
                (diff, Some(list))
            }
            workspace::Pane::Preview => (self.render_preview_view(cx).into_any_element(), None),
            workspace::Pane::Conversation => (
                MessageScroller::new("thread", self.scroller.clone(), move |ix, _, cx| {
                    view.read(cx).render_item(ix, &view, cx)
                })
                .with_jump_button_label(t!("thread.jump"))
                // Le fil règle lui-même l'espace entre ses lignes.
                .with_row_style(StyleRefinement::default().p_0())
                .flex_1()
                .min_h_0()
                .into_any_element(),
                None,
            ),
        };

        v_flex()
            .size_full()
            .children(self.render_mission_bar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(content)
                            .child(self.render_bottom(waiting, cx)),
                    )
                    .children(aside),
            )
    }

    /// Sous le contenu : travail en cours, suggestions, demandes et composer.
    fn render_bottom(&self, waiting: bool, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .items_center()
            .px_6()
            .pb_4()
            .gap_2()
            .when(waiting, |column| {
                column.child(
                    h_flex()
                        .w_full()
                        .max_w(px(760.))
                        .child(ShimmerText::new(t!("thread.working").to_string()).id("working")),
                )
            })
            .children(self.render_suggestions(cx))
            .children(self.render_plan_ready(cx))
            .children(self.render_permission(cx))
            .child(self.render_composer(cx))
    }
}

impl Render for SessionView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .text_sm()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.interrupt(cx);
                }
            }))
            .child(if self.started() || self.pane == workspace::Pane::Files {
                self.render_thread(cx).into_any_element()
            } else {
                self.render_home(window, cx).into_any_element()
            })
    }
}

/// Le nom du dossier de la copie de travail d'une mission.
fn copy_id() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S-%f").to_string()
}

/// L'icône d'un outil de Claude, d'après son nom.
fn tool_icon(name: &str) -> IconName {
    match name {
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => IconName::Pencil,
        "Read" => IconName::Eye,
        "Grep" | "Glob" => IconName::Search,
        "Bash" | "BashOutput" | "KillShell" => IconName::Play,
        "WebFetch" | "WebSearch" => IconName::Globe,
        "Task" | "Agent" => IconName::Bot,
        "TodoWrite" => IconName::ListChecks,
        _ => IconName::Wrench,
    }
}

/// Un fichier touché par un outil : relatif à la racine du projet, sinon ses deux
/// derniers éléments, pour ne pas étaler un long chemin absolu.
fn short_path(file: &std::path::Path, root: &std::path::Path) -> String {
    if let Ok(inside) = file.strip_prefix(root) {
        return inside.display().to_string();
    }
    let parts: Vec<_> = file.iter().collect();
    if parts.len() <= 3 {
        return file.display().to_string();
    }
    let tail: std::path::PathBuf = parts[parts.len() - 2..].iter().collect();
    format!("…/{}", tail.display())
}

/// Ancienneté lisible : minutes, heures ou jours.
pub(crate) fn age(since: std::time::SystemTime) -> String {
    let minutes = since.elapsed().map_or(0, |elapsed| elapsed.as_secs() / 60);
    match minutes {
        0..60 => t!("import.minutes", count = minutes).to_string(),
        60..1440 => t!("import.hours", count = minutes / 60).to_string(),
        _ => t!("import.days", count = minutes / 1440).to_string(),
    }
}

/// La session pilotée de bout en bout dans une fenêtre invisible, face à un faux
/// `claude` qui rejoue une vraie session enregistrée (création d'un fichier, en
/// mode « demander »).
#[cfg(all(test, unix))]
pub(crate) mod tests {
    // `use super::*` amène le `test` de gpui_kit, que sa propre macro rappelle
    // sans fin : on rend explicite celui de Rust.
    use core::prelude::v1::test;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    use gpui_kit::test::{TestAppContextExt, TestWindowExt};

    use super::*;

    /// Les tests qui posent un faux `claude` en tête de PATH passent un par un.
    pub(crate) static PATH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn tool_paths_are_short() {
        let root = std::path::Path::new("/work/app");
        let short = |file: &str| short_path(std::path::Path::new(file), root);
        assert_eq!(short("/work/app/src/main.rs"), "src/main.rs");
        assert_eq!(short("/tmp/a/b/hello.md"), "…/b/hello.md");
        assert_eq!(short("/etc/hosts"), "/etc/hosts");
    }

    // Le verrou est tenu pendant tout le test : il change PATH.
    #[allow(clippy::await_holding_lock)]
    #[gpui_kit::test]
    async fn draft_survives_a_failed_launch_and_a_project_change(cx: &mut TestAppContext) {
        let _path = PATH_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!("solar-draft-test-{}", std::process::id()));
        let (first, second) = (dir.join("a"), dir.join("b"));
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        // Aucun `claude` sur ce PATH : chaque lancement échoue.
        let path = std::env::var("PATH").unwrap_or_default();
        unsafe { std::env::set_var("PATH", dir.display().to_string()) };

        cx.update(gpui_kit::init);
        let (window, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| SessionView::new(first.clone(), None, "Test".into(), window, cx))
            })
            .unwrap()
        });
        cx.update_window(window, |_, window, cx| {
            view.update(cx, |view, cx| view.activate(window, cx));
            window.render_frame(cx);
            window.input("Mon brouillon", cx);
            view.update(cx, |view, cx| view.set_project(second.clone(), cx));
            window.render_frame(cx);
        })
        .unwrap();
        unsafe { std::env::set_var("PATH", path) };

        cx.update(|cx| {
            let view = view.read(cx);
            assert!(
                view.launch_error.is_some(),
                "le lancement aurait dû échouer"
            );
            assert_eq!(view.project, second);
            assert!(!view.started());
            assert_eq!(view.input.read(cx).value().as_ref(), "Mon brouillon");
        });
    }

    /// Pose en tête de PATH un faux CLI qui note tout ce qu'il reçoit et, à
    /// chaque message, propose un plan ; ouvre une vue qui planifie une mission.
    /// Rend la fenêtre, la vue, le journal et le PATH d'origine.
    /// `repo` : le projet est un dépôt git, dans un sous-dossier, et la mission
    /// travaille dans sa copie.
    fn planned_mission(
        name: &str,
        repo: bool,
        cx: &mut TestAppContext,
    ) -> (
        AnyWindowHandle,
        Entity<SessionView>,
        std::path::PathBuf,
        String,
    ) {
        let dir = std::env::temp_dir().join(format!("solar-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("stdin.log");
        let _ = std::fs::remove_file(&log);
        let plan = r#"{"plan":"1. Lire le code\n2. Ajouter la page"}"#;
        let script = format!(
            "#!/bin/sh\nprintf 'cwd %s\\n' \"$PWD\" >> '{log}'\nwhile read -r line; do\n  printf '%s\\n' \"$line\" >> '{log}'\n  case \"$line\" in\n    \
             *'\"type\":\"user\"'*)\n      \
             printf '%s\\n' '{{\"type\":\"assistant\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"tool_use\",\"id\":\"p1\",\"name\":\"ExitPlanMode\",\"input\":{plan}}}]}}}}'\n      \
             printf '%s\\n' '{{\"type\":\"control_request\",\"request_id\":\"perm-plan\",\"request\":{{\"subtype\":\"can_use_tool\",\"tool_name\":\"ExitPlanMode\",\"input\":{plan}}}}}'\n      ;;\n  \
             esac\ndone\n",
            log = log.display()
        );
        std::fs::write(dir.join("claude"), script).unwrap();
        std::fs::set_permissions(dir.join("claude"), PermissionsExt::from_mode(0o755)).unwrap();
        let path = std::env::var("PATH").unwrap_or_default();
        unsafe { std::env::set_var("PATH", format!("{}:{path}", dir.display())) };
        let project = if repo {
            let repo = dir.join("repo");
            let _ = std::fs::remove_dir_all(&repo);
            std::fs::create_dir_all(repo.join("app")).unwrap();
            std::fs::write(repo.join("app/a.txt"), "v1\n").unwrap();
            for args in [
                &["init", "-q"][..],
                &["add", "."],
                &[
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@t",
                    "commit",
                    "-qm",
                    "un",
                ],
            ] {
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(&repo)
                    .output()
                    .unwrap();
            }
            repo.join("app")
        } else {
            dir.clone()
        };

        cx.executor().allow_parking();
        cx.update(gpui_kit::init);
        let (window, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| SessionView::new(project, None, "Test".into(), window, cx))
            })
            .unwrap()
        });
        cx.update_window(window, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.checkpoints = dir.join("checkpoints");
                view.activate(window, cx);
                view.planning = true;
            });
            window.render_frame(cx);
            window.input("Ajoute une page de connexion", cx);
            window.press("enter", cx);
        })
        .unwrap();
        (window, view, log, path)
    }

    // Le verrou est tenu pendant tout le test : il change PATH.
    #[allow(clippy::await_holding_lock)]
    #[gpui_kit::test]
    async fn mission_is_planned_read_only_then_launched_in_the_chosen_mode(
        cx: &mut TestAppContext,
    ) {
        let _path = PATH_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (window, view, log, path) = planned_mission("plan-test", false, cx);

        let real_time = || std::thread::sleep(Duration::from_millis(5));
        cx.wait_for(window, Duration::from_secs(5), |_, cx| {
            real_time();
            view.read(cx).plan_pending()
        })
        .await;
        cx.update(|cx| {
            let view = view.read(cx);
            assert_eq!(view.mode, PLAN_MODE);
            let mission = view.mission.as_ref().expect("une mission est ouverte");
            assert_eq!(mission.status, MissionStatus::Ready);
            assert_eq!(mission.plans, ["1. Lire le code\n2. Ajouter la page"]);
        });

        cx.update(|cx| view.update(cx, |view, cx| view.launch_mission("acceptEdits", cx)));
        let read_log = || std::fs::read_to_string(&log).unwrap_or_default();
        cx.wait_for(window, Duration::from_secs(5), |_, _| {
            real_time();
            read_log().contains("\"behavior\":\"allow\"")
        })
        .await;
        unsafe { std::env::set_var("PATH", path) };

        let lines: Vec<String> = read_log().lines().map(String::from).collect();
        let at = |needle: &str| {
            lines
                .iter()
                .position(|line| line.contains(needle))
                .unwrap_or_else(|| panic!("{needle} absent de : {lines:#?}"))
        };
        // Le mode plan précède le premier message : rien n'est écrit pendant la préparation.
        assert!(at("\"mode\":\"plan\"") < at("\"type\":\"user\""));
        // Le mode choisi précède l'accord : le CLI ne repasse pas en accès complet.
        assert!(at("\"mode\":\"acceptEdits\"") < at("\"behavior\":\"allow\""));
        cx.update(|cx| {
            let mission = view.read(cx).mission.clone().unwrap();
            assert_eq!(mission.status, MissionStatus::Running);
            assert_eq!(mission.launched_mode.as_deref(), Some("acceptEdits"));
        });
    }

    // Le verrou est tenu pendant tout le test : il change PATH.
    #[allow(clippy::await_holding_lock)]
    #[gpui_kit::test]
    async fn cancelled_mission_is_never_approved(cx: &mut TestAppContext) {
        let _path = PATH_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (window, view, log, path) = planned_mission("cancel-test", false, cx);
        let real_time = || std::thread::sleep(Duration::from_millis(5));
        cx.wait_for(window, Duration::from_secs(5), |_, cx| {
            real_time();
            view.read(cx).plan_pending()
        })
        .await;

        cx.update(|cx| view.update(cx, |view, cx| view.cancel_mission(cx)));
        let read_log = || std::fs::read_to_string(&log).unwrap_or_default();
        cx.wait_for(window, Duration::from_secs(5), |_, _| {
            real_time();
            read_log().contains("cancelled this mission")
        })
        .await;
        unsafe { std::env::set_var("PATH", path) };

        let sent = read_log();
        assert!(sent.contains("\"behavior\":\"deny\""), "{sent}");
        assert!(!sent.contains("\"behavior\":\"allow\""), "{sent}");
        cx.update(|cx| {
            let view = view.read(cx);
            assert_eq!(view.mode, PLAN_MODE, "la mission annulée n'a rien autorisé");
            assert_eq!(
                view.mission.as_ref().unwrap().status,
                MissionStatus::Cancelled
            );
        });
    }

    // Le verrou est tenu pendant tout le test : il change PATH.
    #[allow(clippy::await_holding_lock)]
    #[gpui_kit::test]
    async fn a_relaunch_starts_a_fresh_attempt_with_the_plan_and_the_issues(
        cx: &mut TestAppContext,
    ) {
        let _path = PATH_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (window, view, log, path) = planned_mission("relaunch-test", false, cx);
        let real_time = || std::thread::sleep(Duration::from_millis(5));
        cx.wait_for(window, Duration::from_secs(5), |_, cx| {
            real_time();
            view.read(cx).plan_pending()
        })
        .await;

        let relaunched = std::rc::Rc::new(std::cell::RefCell::new(None));
        cx.update(|cx| {
            let relaunched = relaunched.clone();
            cx.subscribe(&view, move |_, event: &Relaunch, _| {
                *relaunched.borrow_mut() = Some((
                    event.project.clone(),
                    event.mission.clone(),
                    event.brief.clone(),
                ));
            })
            .detach();
            view.update(cx, |view, cx| {
                view.cancel_mission(cx);
                let mission = view.mission.as_mut().unwrap();
                mission.launched_mode = Some("acceptEdits".into());
                mission.reviews.push(solar_core::state::AgentReview {
                    agent: "Claude".into(),
                    tree: "t".into(),
                    ok: Some(false),
                    text: "VERDICT: ISSUES\n- test manquant".into(),
                    at: String::new(),
                });
                view.relaunch(cx);
            });
        });
        let (project, mission, brief) = relaunched.borrow_mut().take().expect("relance émise");
        cx.update(|cx| {
            assert_eq!(
                view.read(cx).mission.as_ref().unwrap().next_attempt,
                Some(2)
            );
        });
        assert_eq!(mission.attempt, 2);
        assert!(mission.reviews.is_empty() && mission.checkpoint.is_none());
        assert!(brief.contains("Lire le code") && brief.contains("test manquant"));

        // La fenêtre ouvre la tentative dans une conversation neuve.
        std::fs::remove_file(&log).unwrap();
        let (_, next) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut next = SessionView::new(project, None, "T2".into(), window, cx);
                    next.checkpoints = view.read(cx).checkpoints.clone();
                    next.mission = Some(mission);
                    next.brief = Some(brief);
                    next
                })
            })
            .unwrap()
        });
        cx.update(|cx| next.update(cx, |next, cx| next.start_attempt(cx)));
        let read_log = || std::fs::read_to_string(&log).unwrap_or_default();
        cx.wait_for(window, Duration::from_secs(5), |_, _| {
            real_time();
            read_log().contains("test manquant")
        })
        .await;
        unsafe { std::env::set_var("PATH", path) };

        let sent = read_log();
        assert!(sent.contains("\"mode\":\"acceptEdits\""), "{sent}");
        cx.update(|cx| {
            let next = next.read(cx);
            let mission = next.mission.as_ref().unwrap();
            // Le faux CLI répond à tout par un plan : le statut qui suit ne dit rien ici.
            assert_eq!(mission.launched_mode.as_deref(), Some("acceptEdits"));
            assert!(mission.checkpoint.is_some(), "un instantané pour la revue");
            assert!(
                next.session_id.is_none(),
                "session neuve, lancée sans --resume"
            );
        });
    }

    // Le verrou est tenu pendant tout le test : il change PATH.
    #[allow(clippy::await_holding_lock)]
    #[gpui_kit::test]
    async fn a_mission_works_in_its_copy_and_is_applied_on_acceptance(cx: &mut TestAppContext) {
        let _path = PATH_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (window, view, log, path) = planned_mission("copy-test", true, cx);
        let real_time = || std::thread::sleep(Duration::from_millis(5));
        cx.wait_for(window, Duration::from_secs(10), |_, cx| {
            real_time();
            view.read(cx).plan_pending()
        })
        .await;
        let (project, copy) = cx.update(|cx| {
            let view = view.read(cx);
            (view.project.clone(), view.workdir())
        });
        assert_ne!(copy, project, "la mission a sa copie");
        let read_log = || std::fs::read_to_string(&log).unwrap_or_default();
        assert!(
            read_log().contains(&format!("cwd {}", copy.display())),
            "Claude planifie dans la copie : {}",
            read_log()
        );

        cx.update(|cx| view.update(cx, |view, cx| view.launch_mission("acceptEdits", cx)));
        cx.wait_for(window, Duration::from_secs(5), |_, _| {
            real_time();
            read_log().contains("\"behavior\":\"allow\"")
        })
        .await;
        // L'agent écrit dans sa copie ; l'utilisateur, dans son projet, ailleurs.
        std::fs::write(copy.join("a.txt"), "v2\n").unwrap();
        std::fs::write(project.join("mine.txt"), "à moi\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(project.join("a.txt")).unwrap(),
            "v1\n"
        );

        cx.update(|cx| view.update(cx, |view, cx| view.refresh_review(cx)));
        cx.wait_for(window, Duration::from_secs(5), |_, cx| {
            real_time();
            view.read(cx)
                .review
                .as_ref()
                .is_some_and(|r| r.changes.len() == 1)
        })
        .await;
        cx.update(|cx| view.update(cx, |view, cx| view.accept_result(cx)));
        cx.wait_for(window, Duration::from_secs(5), |_, cx| {
            real_time();
            view.read(cx).mission.as_ref().unwrap().status == MissionStatus::Accepted
        })
        .await;
        unsafe { std::env::set_var("PATH", path) };

        assert_eq!(
            std::fs::read_to_string(project.join("a.txt")).unwrap(),
            "v2\n"
        );
        assert_eq!(
            std::fs::read_to_string(project.join("mine.txt")).unwrap(),
            "à moi\n"
        );
        assert!(!copy.exists(), "la copie est retirée");
        cx.update(|cx| assert!(view.read(cx).claude.is_none()));
    }

    // Le verrou est tenu pendant tout le test : il change PATH.
    #[allow(clippy::await_holding_lock)]
    #[gpui_kit::test]
    async fn a_codex_mission_plans_read_only_then_resumes_with_write_access(
        cx: &mut TestAppContext,
    ) {
        let _path = PATH_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!("solar-codex-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("codex.log");
        // Un faux `codex exec --json` : il note ses arguments et le message, puis
        // répond par un plan en un tour.
        let reply = [
            r#"{"type":"thread.started","thread_id":"th-1"}"#,
            r#"{"type":"turn.started"}"#,
            r#"{"type":"item.completed","item":{"id":"i1","type":"agent_message","text":"1. Lire\n2. Écrire"}}"#,
            r#"{"type":"turn.completed","usage":{"input_tokens":10}}"#,
        ]
        .join("\n");
        let script = format!(
            "#!/bin/sh\n[ \"$1\" = --version ] && exit 0\nprintf 'ARGS %s\\n' \"$*\" >> '{log}'\ncat >> '{log}'\necho >> '{log}'\ncat <<'EOF'\n{reply}\nEOF\n",
            log = log.display()
        );
        std::fs::write(dir.join("codex"), script).unwrap();
        std::fs::set_permissions(dir.join("codex"), PermissionsExt::from_mode(0o755)).unwrap();
        let path = std::env::var("PATH").unwrap_or_default();
        unsafe { std::env::set_var("PATH", format!("{}:{path}", dir.display())) };

        cx.executor().allow_parking();
        cx.update(gpui_kit::init);
        let project = dir.join("app");
        std::fs::create_dir_all(&project).unwrap();
        let (window, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| SessionView::new(project, None, "Test".into(), window, cx))
            })
            .unwrap()
        });
        cx.update_window(window, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.checkpoints = dir.join("checkpoints");
                view.set_provider(Provider::Codex, window, cx);
                view.activate(window, cx);
                view.planning = true;
            });
            window.render_frame(cx);
            window.input("Ajoute une page", cx);
            window.press("enter", cx);
        })
        .unwrap();
        let real_time = || std::thread::sleep(Duration::from_millis(5));
        cx.wait_for(window, Duration::from_secs(5), |_, cx| {
            real_time();
            view.read(cx).plan_pending()
        })
        .await;
        let read_log = || std::fs::read_to_string(&log).unwrap_or_default();
        let first = read_log();
        assert!(first.contains("sandbox_mode=\"read-only\""), "{first}");
        assert!(!first.contains("resume"), "{first}");
        assert!(first.contains("Ajoute une page"), "{first}");
        cx.update(|cx| {
            let view = view.read(cx);
            assert_eq!(view.mission.as_ref().unwrap().plans, ["1. Lire\n2. Écrire"]);
            assert_eq!(view.session_id.as_deref(), Some("th-1"));
        });

        cx.update(|cx| view.update(cx, |view, cx| view.launch_mission("acceptEdits", cx)));
        cx.wait_for(window, Duration::from_secs(5), |_, _| {
            real_time();
            read_log().contains("resume th-1")
        })
        .await;
        unsafe { std::env::set_var("PATH", path) };
        let second = read_log()[first.len()..].to_string();
        assert!(
            second.contains("sandbox_mode=\"workspace-write\""),
            "{second}"
        );
        assert!(second.contains("The plan is approved"), "{second}");
        cx.update(|cx| {
            let mission = view.read(cx).mission.clone().unwrap();
            assert_eq!(mission.launched_mode.as_deref(), Some("acceptEdits"));
        });
    }

    #[gpui_kit::test]
    fn an_unfinished_turn_reopens_as_interrupted_without_relaunch(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (_, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| SessionView::new("/nowhere".into(), None, "Test".into(), window, cx))
            })
            .unwrap()
        });
        let mission = |status| {
            Some(Mission {
                status,
                ..Mission::default()
            })
        };
        cx.update(|cx| {
            view.update(cx, |view, cx| {
                view.items.push(Item::Tool {
                    id: "t".into(),
                    name: "Bash".into(),
                    input: Value::Null,
                    result: None,
                    finished: None,
                    open: false,
                });
                view.restore_mission(mission(MissionStatus::Running), cx);
                assert!(view.interrupted);
                assert!(view.claude.is_none(), "rien n'est relancé");
                // Un plan prêt attend une décision : il n'est pas interrompu.
                view.restore_mission(mission(MissionStatus::Ready), cx);
                assert!(!view.interrupted);
                // Un tour fini non plus.
                view.items.push(Item::Text("fait".into()));
                view.restore_mission(mission(MissionStatus::Running), cx);
                assert!(!view.interrupted);
            })
        });
    }

    // `git` doit rester sur le PATH : un autre test le vide un instant.
    #[allow(clippy::await_holding_lock)]
    #[gpui_kit::test]
    async fn files_view_lists_the_project_and_shows_the_chosen_file(cx: &mut TestAppContext) {
        let _path = PATH_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = std::env::temp_dir().join(format!("solar-files-view-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("README.md"), "# Bonjour").unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();

        cx.executor().allow_parking();
        cx.update(gpui_kit::init);
        let (window, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| SessionView::new(root.clone(), None, "Test".into(), window, cx))
            })
            .unwrap()
        });
        cx.update(|cx| {
            view.update(cx, |view, cx| {
                view.mission = Some(Mission::default());
                view.items.push(Item::User("x".into()));
                view.open_files(cx);
            })
        });
        let real_time = || std::thread::sleep(Duration::from_millis(5));
        cx.wait_for(window, Duration::from_secs(5), |_, cx| {
            real_time();
            view.read(cx).files_listed
        })
        .await;

        let tree = cx.update(|cx| view.read(cx).file_tree.clone().unwrap());
        cx.update(|cx| {
            tree.update(cx, |tree, cx| {
                let ix = tree.index_of(&"README.md".into()).expect("README.md listé");
                tree.set_selected_index(Some(ix), cx);
            })
        });
        cx.wait_for(window, Duration::from_secs(5), |_, cx| {
            real_time();
            matches!(
                &view.read(cx).opened,
                Some((_, files::Content::Text { text, .. })) if text == "# Bonjour"
            )
        })
        .await;
        // La vue se dessine avec le fichier ouvert.
        cx.update_window(window, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }

    // `git` doit rester sur le PATH : un autre test le vide un instant.
    #[allow(clippy::await_holding_lock)]
    #[gpui_kit::test]
    async fn review_shows_exact_changes_refuses_stale_decisions_and_restores(
        cx: &mut TestAppContext,
    ) {
        let _path = PATH_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = std::env::temp_dir().join(format!("solar-review-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("lib.rs"), "fn a() {}\n").unwrap();
        std::fs::write(root.join("mine.txt"), "à moi\n").unwrap();
        let store_dir = root
            .join("../solar-review-store")
            .join(std::process::id().to_string());
        let base = solar_core::checkpoint::Store::for_project(&store_dir, &root)
            .checkpoint()
            .unwrap();
        // La mission : une modification et un ajout.
        std::fs::write(root.join("lib.rs"), "fn a() {}\nfn b() {}\n").unwrap();
        std::fs::write(root.join("new.rs"), "// neuf\n").unwrap();

        cx.executor().allow_parking();
        cx.update(gpui_kit::init);
        let (window, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| SessionView::new(root.clone(), None, "Test".into(), window, cx))
            })
            .unwrap()
        });
        cx.update(|cx| {
            view.update(cx, |view, cx| {
                view.checkpoints = store_dir.clone();
                view.items.push(Item::User("x".into()));
                view.mission = Some(Mission {
                    status: MissionStatus::Running,
                    checkpoint: Some(base),
                    ..Mission::default()
                });
                view.open_changes(cx);
            })
        });
        let real_time = || std::thread::sleep(Duration::from_millis(5));
        cx.wait_for(window, Duration::from_secs(5), |_, cx| {
            real_time();
            let view = view.read(cx);
            view.review.is_some() && !view.review_busy
        })
        .await;
        cx.update(|cx| {
            let paths: Vec<_> = view
                .read(cx)
                .review
                .as_ref()
                .unwrap()
                .changes
                .iter()
                .map(|c| c.path.clone())
                .collect();
            assert_eq!(paths, ["lib.rs", "new.rs"]);
        });
        cx.update_window(window, |_, window, cx| window.render_frame(cx))
            .unwrap();

        // Un fichier change après l'affichage : l'acceptation est refusée.
        std::fs::write(root.join("lib.rs"), "fn a() {}\nfn b() {}\nfn c() {}\n").unwrap();
        cx.update(|cx| view.update(cx, |view, cx| view.accept_result(cx)));
        cx.wait_for(window, Duration::from_secs(5), |_, cx| {
            real_time();
            let view = view.read(cx);
            view.review_note.is_some() && !view.review_busy
        })
        .await;
        cx.update(|cx| {
            assert_eq!(
                view.read(cx).mission.as_ref().unwrap().status,
                MissionStatus::Running
            );
        });

        // Examiné de nouveau, puis accepté : une décision, rien d'autre.
        cx.update(|cx| view.update(cx, |view, cx| view.accept_result(cx)));
        cx.wait_for(window, Duration::from_secs(5), |_, cx| {
            real_time();
            let view = view.read(cx);
            view.mission.as_ref().unwrap().status == MissionStatus::Accepted
        })
        .await;
        cx.update(|cx| assert!(view.read(cx).mission.as_ref().unwrap().accepted.is_some()));

        // Restaurer remet les fichiers de la mission, et eux seuls.
        std::fs::write(root.join("mine.txt"), "toujours à moi\n").unwrap();
        cx.update(|cx| {
            view.update(cx, |view, cx| {
                view.mission.as_mut().unwrap().status = MissionStatus::Running;
                view.restore_result(cx);
            })
        });
        cx.wait_for(window, Duration::from_secs(5), |_, cx| {
            real_time();
            let view = view.read(cx);
            view.mission.as_ref().unwrap().status == MissionStatus::Restored
        })
        .await;
        assert_eq!(
            std::fs::read_to_string(root.join("lib.rs")).unwrap(),
            "fn a() {}\n"
        );
        assert!(!root.join("new.rs").exists());
        assert_eq!(
            std::fs::read_to_string(root.join("mine.txt")).unwrap(),
            "toujours à moi\n"
        );
    }

    // Le verrou est tenu pendant tout le test : il change PATH.
    #[allow(clippy::await_holding_lock)]
    #[gpui_kit::test]
    async fn a_reviewer_reads_the_exact_diff_read_only_and_its_verdict_is_kept(
        cx: &mut TestAppContext,
    ) {
        let _path = PATH_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = std::env::temp_dir().join(format!("solar-reviewer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (project, bin) = (root.join("project"), root.join("bin"));
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(project.join("lib.rs"), "fn a() {}\n").unwrap();
        let store_dir = root.join("checkpoints");
        let base = solar_core::checkpoint::Store::for_project(&store_dir, &project)
            .checkpoint()
            .unwrap();
        std::fs::write(project.join("lib.rs"), "fn a() {}\nfn b() {}\n").unwrap();
        // Le faux relecteur note ses arguments et ce qu'il reçoit, puis rend un verdict.
        let log = root.join("reviewer.log");
        let script = format!(
            "#!/bin/sh\necho \"args $*\" >> '{log}'\nwhile read -r line; do\n  printf '%s\\n' \"$line\" >> '{log}'\n  case \"$line\" in\n    \
             *'\"type\":\"user\"'*)\n      \
             printf '%s\\n' '{{\"type\":\"stream_event\",\"event\":{{\"type\":\"content_block_delta\",\"delta\":{{\"type\":\"text_delta\",\"text\":\"VERDICT: PROBLÈMES\\\\n- lib.rs : b() sans test\"}}}}}}'\n      \
             printf '%s\\n' '{{\"type\":\"result\",\"is_error\":false,\"usage\":{{}}}}'\n      ;;\n  \
             esac\ndone\n",
            log = log.display()
        );
        std::fs::write(bin.join("claude"), script).unwrap();
        std::fs::set_permissions(bin.join("claude"), PermissionsExt::from_mode(0o755)).unwrap();
        let path = std::env::var("PATH").unwrap_or_default();
        unsafe { std::env::set_var("PATH", format!("{}:{path}", bin.display())) };

        cx.executor().allow_parking();
        cx.update(gpui_kit::init);
        let (window, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| SessionView::new(project.clone(), None, "Test".into(), window, cx))
            })
            .unwrap()
        });
        cx.update(|cx| {
            view.update(cx, |view, cx| {
                view.checkpoints = store_dir.clone();
                view.items.push(Item::User("x".into()));
                view.mission = Some(Mission {
                    status: MissionStatus::Running,
                    checkpoint: Some(base),
                    plans: vec!["Ajouter b()".into()],
                    ..Mission::default()
                });
                view.open_changes(cx);
            })
        });
        let real_time = || std::thread::sleep(Duration::from_millis(5));
        cx.wait_for(window, Duration::from_secs(5), |_, cx| {
            real_time();
            view.read(cx).review.is_some()
        })
        .await;
        cx.update(|cx| view.update(cx, |view, cx| view.start_review(Provider::Claude, cx)));
        cx.wait_for(window, Duration::from_secs(5), |_, cx| {
            real_time();
            view.read(cx)
                .mission
                .as_ref()
                .is_some_and(|m| !m.reviews.is_empty())
        })
        .await;
        unsafe { std::env::set_var("PATH", path) };

        cx.update(|cx| {
            let view = view.read(cx);
            let done = view.mission.as_ref().unwrap().reviews.last().unwrap();
            assert_eq!(done.ok, Some(false));
            assert_eq!(
                done.tree,
                view.review.as_ref().unwrap().tree,
                "lié à l'état relu"
            );
            assert!(done.text.contains("b() sans test"), "{}", done.text);
            assert!(!view.reviewing && view.reviewer.is_none());
        });
        let sent = std::fs::read_to_string(&log).unwrap();
        assert!(
            sent.contains("--permission-mode plan"),
            "lecture seule : {sent}"
        );
        assert!(
            sent.contains("Ajouter b()") && sent.contains("+fn b() {}"),
            "{sent}"
        );
        cx.update_window(window, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }

    const FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../solar-core/tests/fixtures/write_file.jsonl"
    );

    // Le verrou est tenu pendant tout le test, attentes comprises : c'est son rôle.
    #[allow(clippy::await_holding_lock)]
    #[gpui_kit::test]
    async fn message_is_sent_streamed_and_permission_answered(cx: &mut TestAppContext) {
        let _path = PATH_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!("solar-ui-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let answer = dir.join("answer.json");
        // Le faux CLI s'arrête sur la demande de permission (6e ligne) et note la réponse.
        let script = format!(
            "#!/bin/sh\nread -r init\nread -r settings\nread -r first\nhead -n 6 '{FIXTURE}'\nread -r answer\n\
             printf '%s' \"$answer\" > '{}'\ntail -n +7 '{FIXTURE}'\nread -r rest\n",
            answer.display()
        );
        std::fs::write(dir.join("claude"), script).unwrap();
        std::fs::set_permissions(dir.join("claude"), PermissionsExt::from_mode(0o755)).unwrap();
        let path = std::env::var("PATH").unwrap_or_default();
        // Seul test de ce binaire à toucher à PATH.
        unsafe { std::env::set_var("PATH", format!("{}:{path}", dir.display())) };

        // Le faux CLI réveille l'app depuis un vrai thread : le test n'est pas
        // déterministe, on l'autorise.
        cx.executor().allow_parking();
        cx.update(gpui_kit::init);
        let (window, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| SessionView::new(dir.clone(), None, "Test".into(), window, cx))
            })
            .unwrap()
        });

        cx.update_window(window, |_, window, cx| {
            view.update(cx, |view, cx| view.activate(window, cx));
            window.render_frame(cx);
            window.input("Crée note.txt", cx);
            window.press("enter", cx);
        })
        .unwrap();

        // Le faux CLI tourne en temps réel alors que l'horloge du test est simulée.
        let real_time = || std::thread::sleep(Duration::from_millis(5));
        cx.wait_for(window, Duration::from_secs(5), |_, cx| {
            real_time();
            view.read(cx).permission.is_some()
        })
        .await;
        cx.update(|cx| {
            let view = view.read(cx);
            assert!(view.status == Status::NeedsYou);
            assert_eq!(view.title.as_ref(), "Crée note.txt");
            assert!(matches!(view.items[0], Item::User(_)));
            assert!(matches!(&view.items[1], Item::Tool { name, .. } if name == "Write"));
        });

        cx.update_window(window, |_, window, cx| window.click("allow", cx))
            .unwrap();
        cx.wait_for(window, Duration::from_secs(5), |_, cx| {
            real_time();
            matches!(
                view.read(cx).items.last(),
                Some(Item::Note { error: false, .. })
            )
        })
        .await;

        cx.update(|cx| {
            let view = view.read(cx);
            assert!(view.status == Status::Idle);
            assert_eq!(
                view.session_id.as_deref(),
                Some("a4483f72-ef29-4b31-97b5-f025efb8edcf")
            );
            assert!(matches!(
                &view.items[1],
                Item::Tool {
                    result: Some((false, _)),
                    ..
                }
            ));
            assert!(
                view.items
                    .iter()
                    .any(|item| matches!(item, Item::Text(text) if text == "fait."))
            );
        });
        let sent = std::fs::read_to_string(&answer).unwrap();
        assert!(
            sent.contains(r#""behavior":"allow""#),
            "réponse envoyée : {sent}"
        );
        assert!(sent.contains("316a49cc-17ff-4c37-bde7-0322d3b388fb"));
    }

    #[gpui_kit::test]
    fn slash_and_at_menus_complete_what_is_typed(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (window, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut view =
                        SessionView::new("/nulle-part".into(), None, "Test".into(), window, cx);
                    view.commands = ["simplify", "code-review"]
                        .map(|name| SlashCommand {
                            name: name.into(),
                            description: String::new(),
                        })
                        .to_vec();
                    view.files = Some(vec!["src/main.rs".into(), "README.md".into()]);
                    view
                })
            })
            .unwrap()
        });
        let labels = |view: &Entity<SessionView>, cx: &App| -> Vec<String> {
            let suggestions = view.read(cx).suggestions(cx);
            suggestions.iter().map(|s| s.label.to_string()).collect()
        };

        cx.update_window(window, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.input.update(cx, |input, cx| input.focus(window, cx))
            });
            window.render_frame(cx);

            window.input("/rev", cx);
            assert_eq!(labels(&view, cx), ["/code-review"]);
            window.press("tab", cx);
            assert_eq!(
                view.read(cx).input.read(cx).value().as_ref(),
                "/code-review "
            );
            // Une commande suivie d'un espace n'est plus complétée.
            assert!(labels(&view, cx).is_empty());

            window.input("lis @main", cx);
            assert_eq!(labels(&view, cx), ["src/main.rs"]);
            window.press("tab", cx);
            assert_eq!(
                view.read(cx).input.read(cx).value().as_ref(),
                "/code-review lis @src/main.rs "
            );
        })
        .unwrap();
    }
}
