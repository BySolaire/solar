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
use gpui_kit::component::progress::ProgressCircle;
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
mod library;
mod mission;
pub(crate) use library::examples as library_examples;
mod pickers;
mod preview;
pub(crate) mod publish;
mod review;
mod reviewer;
mod verbs;
use verbs::working_star;

/// Le réglage « ligne du tour au-dessus du composer » ; à défaut, elle est dans le fil.
pub struct TurnLineAbove(pub bool);

impl Global for TurnLineAbove {}

/// Le réglage « mettre en file les messages envoyés pendant un tour » ; à
/// défaut, ils entrent dans le tour en cours (steer).
pub struct QueueWhileWorking(pub bool);

impl Global for QueueWhileWorking {}

fn queue_while_working(cx: &App) -> bool {
    cx.try_global::<QueueWhileWorking>()
        .is_some_and(|queue| queue.0)
}

fn line_above(cx: &App) -> bool {
    cx.try_global::<TurnLineAbove>()
        .is_some_and(|above| above.0)
}
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
    /// Un modèle de la bibliothèque : le choisir l'utilise.
    template: Option<String>,
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
    /// Une réflexion ; sa durée en secondes une fois finie, si elle a été suivie
    /// en direct (l'historique ne la donne pas).
    Thinking(Option<u64>),
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
    /// Le contexte de la conversation : tokens au dernier appel, et taille de la
    /// fenêtre du modèle quand l'agent la donne.
    context_tokens: Option<u64>,
    context_window: Option<u64>,
    /// Le tour en cours : son début, les tokens écrits exacts (fins d'appel) et
    /// les caractères reçus depuis, pour l'estimation en direct ; son verbe,
    /// tiré au hasard comme dans Claude Code.
    turn_started: Option<std::time::Instant>,
    turn_verb: &'static str,
    /// La réflexion en cours (son début), ou la durée de la dernière, en secondes.
    thinking_since: Option<std::time::Instant>,
    thought_for: Option<u64>,
    turn_tokens: u64,
    turn_chars: usize,
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
    /// La largeur de la vue au dernier rendu : sous `COMPACT`, les panneaux
    /// latéraux cèdent la place et les vues de la barre passent en icônes.
    width: Pixels,
    /// Une interruption est demandée et pas encore confirmée par la fin du tour.
    stopping: bool,
    /// Messages mis en file pendant un tour : ils partent ensemble à sa fin.
    queued: Vec<(String, Vec<Arc<Image>>)>,
    /// Le choix de la flèche d'envoi pour le prochain message seulement :
    /// `Some(true)` en file, `Some(false)` steer.
    send_queue: Option<bool>,
    /// Codex ou Grok arrêtent leur tour pour repartir avec un message (steer) :
    /// la fin du tour arrêté n'est pas celle du travail.
    restarting: bool,
    /// Rouverte après un arrêt de Solar au milieu d'un tour : rien n'est relancé.
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
    /// Le projet peut-il publier sur GitHub ? `None` tant que c'est lu.
    github: Option<Result<solar_core::github::Repo, solar_core::github::Reason>>,
    /// Le dialogue de publication ouvert.
    publish: Option<publish::PublishForm>,
    /// Les modèles de la bibliothèque, et le formulaire des trous ouvert.
    templates: Vec<solar_core::library::Template>,
    template_form: Option<library::TemplateForm>,
    /// Les checks de la PR, le journal de chaque check en échec (par lien), et
    /// la tâche qui les relit tant que certains sont en cours.
    checks: Option<Result<Vec<solar_core::github::Check>, String>>,
    check_logs: std::collections::HashMap<String, Result<String, String>>,
    checks_poll: Option<Task<()>>,
    /// À l'abandon de la copie, fermer aussi sa PR (décoché par défaut).
    close_pr_on_discard: bool,
    /// L'agent choisi pour la dernière relecture.
    reviewer_provider: Provider,
    /// Le relecteur s'est arrêté sans verdict : il est bloqué, on le dit.
    reviewer_failed: bool,
    /// Le dernier tour de Claude a fini en erreur.
    turn_failed: bool,
    /// Pour une nouvelle tentative : ce que la session neuve reçoit au lancement
    /// (le plan approuvé, et ce qui n'allait pas avant).
    pub brief: Option<String>,
    /// Le bot qui a ouvert la conversation, par son identifiant.
    pub bot: Option<String>,
    /// Exécution de bot en mission complète : le plan est lancé dès son
    /// arrivée, dans ce mode, à condition d'avoir sa copie de travail.
    auto_launch: Option<&'static str>,
    /// Le coût annoncé par l'agent pour sa session (cumulé), s'il le donne.
    pub(crate) cost: Option<f64>,
    /// Arrête le serveur de preview quand Solar quitte.
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
            context_tokens: None,
            context_window: None,
            turn_started: None,
            turn_verb: "Working",
            thinking_since: None,
            thought_for: None,
            turn_tokens: 0,
            turn_chars: 0,
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
            width: px(f32::MAX),
            stopping: false,
            queued: Vec::new(),
            send_queue: None,
            restarting: false,
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
            github: None,
            publish: None,
            templates: Vec::new(),
            template_form: None,
            checks: None,
            check_logs: std::collections::HashMap::new(),
            checks_poll: None,
            close_pr_on_discard: false,
            reviewer_provider: Provider::Claude,
            reviewer_failed: false,
            turn_failed: false,
            brief: None,
            bot: None,
            auto_launch: None,
            cost: None,
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
        self.load_templates(cx);
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
        // « /nom » d'un modèle (et pas d'une commande de l'agent) : le modèle
        // s'utilise au lieu de partir tel quel.
        if let Some(name) = text.strip_prefix('/')
            && self.templates.iter().any(|t| t.name == name)
            && !self.commands.iter().any(|c| c.name == name)
        {
            let name = name.to_string();
            self.use_template(&name, window, cx);
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
            self.begin_turn(cx);
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
            let project = self.project.clone();
            cx.spawn_in(window, async move |this, cx| {
                // L'état du projet juste avant la copie : la publication en aura besoin.
                let copy = cx
                    .background_spawn(async move {
                        let launch = solar_core::github::launch_state(&project).ok();
                        store
                            .isolate(&id)
                            .map(|copy| copy.map(|copy| (copy, launch)))
                    })
                    .await;
                let _ = this.update_in(cx, |this, window, cx| {
                    this.launching = false;
                    let mission = this.mission.insert(Mission::default());
                    match copy {
                        Ok(Some(((base, dir), launch))) => {
                            mission.checkpoint = Some(base);
                            mission.worktree = Some(dir);
                            mission.launch = launch;
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
        // Pendant un tour : en file, ou steer. Claude reçoit le message en cours
        // de route ; Codex et Grok, un processus par tour, s'arrêtent et repartent.
        let queue = self
            .send_queue
            .take()
            .unwrap_or_else(|| queue_while_working(cx));
        if self.status != Status::Idle {
            if queue {
                self.queued.push((text, std::mem::take(&mut self.images)));
                self.input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                cx.notify();
                return;
            }
            self.restart_for_steer();
        }
        // Hors tour, une file restée en attente (envoi échoué) part avec ce message.
        let (text, images) = if self.queued.is_empty() {
            (text, self.images.clone())
        } else {
            let mut texts: Vec<String> = Vec::new();
            let mut images = Vec::new();
            for (text, queued_images) in &self.queued {
                texts.push(text.clone());
                images.extend(queued_images.iter().cloned());
            }
            texts.push(text);
            images.extend(self.images.iter().cloned());
            (texts.join("\n\n"), images)
        };
        let started = self.started();
        if !self.deliver(text, images, plan_first, cx) {
            return;
        }
        self.queued.clear();
        self.images.clear();
        if !started {
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
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    /// Steer avec Codex ou Grok, un processus par tour : le tour s'arrête et
    /// repart avec le message. Claude le reçoit en cours de route.
    fn restart_for_steer(&mut self) {
        if self.provider != Provider::Claude
            && let Some(agent) = self.claude.as_mut()
        {
            let _ = agent.interrupt();
            self.restarting = true;
        }
    }

    /// Un message en file part tout de suite, dans le tour en cours (steer).
    fn steer_queued(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix >= self.queued.len() {
            return;
        }
        let (text, images) = self.queued.remove(ix);
        if self.status != Status::Idle {
            self.restart_for_steer();
        }
        // Sans agent pour le recevoir, il reste en file.
        if !self.deliver(text.clone(), images.clone(), false, cx) {
            self.queued.insert(ix, (text, images));
        }
        cx.notify();
    }

    /// Envoie un message à l'agent et l'ajoute au fil ; un tour commence s'il
    /// n'y en a pas. En cas d'échec, l'erreur est montrée et rien n'est ajouté.
    fn deliver(
        &mut self,
        text: String,
        images: Vec<Arc<Image>>,
        plan_first: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let payload: Vec<_> = images
            .iter()
            .map(|image| (image.format.mime_type().to_string(), image.bytes.clone()))
            .collect();
        let sent = self.ensure_claude(cx).and_then(|claude| {
            if plan_first {
                claude.set_permission_mode(PLAN_MODE)?;
            }
            claude.send(&text, &payload)
        });
        if let Err(error) = sent {
            self.restarting = false;
            self.fail(error, cx);
            return false;
        }
        if plan_first {
            self.launch_mode = self.mode;
            self.mode = PLAN_MODE;
            self.planning = false;
            self.mission.get_or_insert_with(Mission::default);
        }
        // Une exécution de bot garde le nom du bot pour titre.
        if self.session_id.is_none() && self.items.is_empty() && self.bot.is_none() {
            self.title = text.chars().take(48).collect::<String>().into();
        }
        let shown = if images.is_empty() {
            text
        } else {
            format!("{text}\n{}", t!("thread.images", count = images.len()))
        };
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
        self.begin_turn(cx);
        self.status = Status::Working;
        cx.notify();
        true
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
        let was = self.status;
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
                self.turn_chars += chunk.chars().count();
                self.status = Status::Working;
                self.end_thinking(cx);
                if let Some(Item::Text(text)) = self.items.last_mut() {
                    text.push_str(&chunk);
                    self.remeasure(self.items.len() - 1, cx);
                } else {
                    self.push(Item::Text(chunk), cx);
                }
            }
            Event::Thinking => {
                self.status = Status::Working;
                // Seule une réflexion vécue en direct a une durée, pas celle d'un historique relu.
                if self.thinking_since.is_none() && self.turn_started.is_some() {
                    self.thinking_since = Some(std::time::Instant::now());
                    self.thought_for = None;
                }
                if !matches!(self.items.last(), Some(Item::Thinking(_))) {
                    self.push(Item::Thinking(None), cx);
                }
            }
            Event::ToolStart { id, name, input } => {
                self.status = Status::Working;
                self.end_thinking(cx);
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
            // Codex et Grok : en lecture seule, chaque fin de tour arrive comme un
            // plan ; hors préparation d'une mission, c'est une simple réponse.
            Event::PermissionRequest { request_id, .. }
                if request_id == solar_core::turns::PLAN_REQUEST
                    && !self.mission.as_ref().is_some_and(|mission| {
                        matches!(
                            mission.status,
                            MissionStatus::Planning | MissionStatus::Ready
                        )
                    }) => {}
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
                let is_plan = tool == PLAN_TOOL;
                self.permission = Some(Permission {
                    request_id,
                    tool,
                    input,
                });
                self.status = Status::NeedsYou;
                if is_plan && let Some(mode) = self.auto_launch.take() {
                    // Sans copie de travail, la mission écrirait dans le projet :
                    // le plan attend la décision de l'utilisateur.
                    if self.mission.as_ref().is_some_and(|m| m.worktree.is_some()) {
                        self.launch_mission(mode, cx);
                    } else {
                        self.note(t!("bots.no_copy"), true, cx);
                    }
                }
            }
            Event::TurnDone {
                error,
                context_tokens,
                cost_usd,
            } if std::mem::take(&mut self.restarting) => {
                // Le tour arrêté pour un steer : le suivant part déjà, avec son
                // propre compte.
                let _ = (error, context_tokens, cost_usd);
                self.status = Status::Idle;
                self.begin_turn(cx);
                self.status = Status::Working;
            }
            Event::TurnDone {
                error,
                context_tokens,
                cost_usd,
            } => {
                // Claude annonce le coût cumulé de sa session : le plus grand vu.
                if let Some(cost) = cost_usd {
                    self.cost = Some(self.cost.map_or(cost, |known| known.max(cost)));
                }
                self.status = Status::Idle;
                self.permission = None;
                self.stopping = false;
                self.turn_started = None;
                self.turn_failed = error.is_some();
                match error {
                    Some(error) => self.note(error, true, cx),
                    None => {
                        // Le contexte du dernier appel ; le total du tour additionne les appels.
                        let tokens = crate::decimal(format!(
                            "{:.1}k",
                            self.context_tokens.unwrap_or(context_tokens) as f64 / 1000.0
                        ));
                        // Codex ne donne pas de coût : on n'en affiche pas.
                        let stats = match cost_usd {
                            Some(cost) => t!(
                                "thread.stats",
                                tokens = tokens,
                                cost = crate::decimal(format!("{cost:.2}"))
                            ),
                            None => t!("thread.context", tokens = tokens),
                        };
                        self.note(stats.to_string(), false, cx)
                    }
                }
                // Les messages mis en file partent ensemble, à la fin du tour.
                if !self.queued.is_empty() {
                    let queued = std::mem::take(&mut self.queued);
                    let text = queued
                        .iter()
                        .map(|(text, _)| text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n\n");
                    let images = queued
                        .iter()
                        .flat_map(|(_, images)| images.clone())
                        .collect();
                    // Sans agent pour les recevoir, ils restent en file : le prochain
                    // envoi les reprend.
                    if !self.deliver(text, images, false, cx) {
                        self.queued = queued;
                    }
                }
            }
            Event::Streamed { chars } => self.turn_chars += chars,
            Event::Output { tokens } => {
                self.turn_tokens += tokens;
                self.turn_chars = 0;
            }
            Event::Context { tokens, window } => {
                self.context_tokens = tokens.or(self.context_tokens);
                self.context_window = window.or(self.context_window);
            }
            Event::Exited => {
                self.claude = None;
                self.status = Status::Idle;
                self.permission = None;
                self.stopping = false;
            }
        }
        // La ligne du tour apparaît ou disparaît au bout du fil.
        if self.status != was {
            self.remeasure_last(cx);
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
        self.remeasure_last(cx);
        cx.notify();
    }

    /// Demande l'arrêt du tour en cours ; « Arrêt demandé » s'affiche jusqu'à la
    /// fin du tour, que le CLI confirme.
    fn interrupt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.status == Status::Idle {
            return;
        }
        // Arrêter rend les messages en file au champ, sans rien perdre.
        if !self.queued.is_empty() {
            let queued = std::mem::take(&mut self.queued);
            let draft = self.input.read(cx).value().to_string();
            let mut texts: Vec<String> = Vec::new();
            for (text, images) in queued {
                texts.push(text);
                self.images.extend(images);
            }
            if !draft.trim().is_empty() {
                texts.push(draft);
            }
            self.input.update(cx, |input, cx| {
                input.set_value(texts.join("\n\n"), window, cx)
            });
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

    /// Le modèle, l'effort et le mode de permission, pour les garder avec la
    /// conversation.
    pub(crate) fn choices(&self) -> (Option<String>, Option<String>, Option<String>) {
        (
            self.model.clone(),
            self.effort.clone(),
            Some(self.mode.to_string()),
        )
    }

    /// Reprend les choix gardés avec la conversation, avant le lancement de
    /// l'agent. Un mode que l'agent ne tient pas est ignoré.
    pub(crate) fn restore_choices(
        &mut self,
        model: Option<String>,
        effort: Option<String>,
        mode: Option<&str>,
    ) {
        if model.is_some() {
            self.model = model;
            self.effort = effort;
        }
        if let Some(mode) = mode
            .and_then(known_mode)
            .filter(|mode| self.provider.supports(mode))
        {
            self.mode = mode;
        }
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
            // Les modèles de la bibliothèque d'abord, puis les commandes de l'agent.
            let templates = self
                .templates
                .iter()
                .filter(|template| {
                    template.name.to_lowercase().contains(&query)
                        || template.title.to_lowercase().contains(&query)
                })
                .map(|template| Suggestion {
                    label: format!("/{}", template.name).into(),
                    detail: t!("library.suggestion", title = template.title)
                        .to_string()
                        .into(),
                    insert: String::new(),
                    template: Some(template.name.clone()),
                });
            let commands = self
                .commands
                .iter()
                .filter(|command| command.name.to_lowercase().contains(&query))
                .map(|command| Suggestion {
                    label: format!("/{}", command.name).into(),
                    detail: command.description.clone().into(),
                    insert: format!("/{} ", command.name),
                    template: None,
                });
            // Les commandes gardent au moins deux places quand il y en a.
            let commands: Vec<Suggestion> = commands.collect();
            let room = MAX_SUGGESTIONS - commands.len().min(2);
            templates
                .take(room)
                .chain(commands)
                .take(MAX_SUGGESTIONS)
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
                    template: None,
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
        match suggestion.template {
            Some(name) => self.use_template(&name, window, cx),
            None => self.fill(suggestion.insert, window, cx),
        }
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
        // L'ancien dernier élément perd la ligne du tour : sa hauteur change.
        self.remeasure_last(cx);
        self.items.push(item);
        self.scroller.update(cx, |list, cx| {
            list.append(1, cx);
        });
    }

    fn remeasure_last(&self, cx: &mut Context<Self>) {
        if let Some(last) = self.items.len().checked_sub(1) {
            self.remeasure(last, cx);
        }
    }

    /// La liste est virtualisée : une ligne qui change de hauteur doit être remesurée.
    fn remeasure(&self, ix: usize, cx: &mut Context<Self>) {
        self.scroller.update(cx, |list, cx| {
            list.remeasure_items(ix..ix + 1, cx);
        });
    }

    /// Ce qu'approuve une demande de permission, en entier : la commande (et sa
    /// description à part), le fichier, l'adresse ou le motif ; sinon toute
    /// l'entrée de l'outil.
    fn permission_detail(&self, input: &Value) -> (Option<String>, String) {
        if input["file_path"].is_string() {
            return (None, self.tool_summary(input));
        }
        let description = input["description"].as_str().map(str::to_string);
        if let Some(command) = input["command"].as_str() {
            return (description, command.to_string());
        }
        match ["url", "pattern", "prompt"]
            .iter()
            .find_map(|key| input[key].as_str())
        {
            Some(value) => (description, value.to_string()),
            None => (None, format!("{input:#}")),
        }
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
        // La ligne du tour se place au bout du fil, sauf si le réglage la veut en bas.
        let live = last && self.status == Status::Working && !line_above(cx);
        let mut line = if live {
            self.render_turn_line(cx)
        } else {
            None
        };
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
        let header = || {
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
                .child(div().font_medium().child(caps(self.provider.name())))
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
            Item::Thinking(_) if live => line.take().unwrap_or_else(|| div().into_any_element()),
            Item::Thinking(Some(secs)) => div()
                .text_color(theme.muted_foreground)
                .child(format!("Thought for {secs}s"))
                .into_any_element(),
            Item::Thinking(None) => {
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
                    .when(opens_turn, |turn| turn.child(header()))
                    // Ce que dit et fait Claude s'aligne sous son nom.
                    .child(
                        div()
                            .font_family(READING_FONT)
                            .when(!from_user, |row| row.pl(px(40.)))
                            .child(row),
                    )
                    // Le tour vient de partir : Claude n'a encore rien dit, son nom
                    // ouvre la ligne sous le message.
                    .when_some(line, |column, line| {
                        column
                            .when(from_user, |column| column.child(header()))
                            .child(div().pl(px(40.)).child(line))
                    }),
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
                    .child(
                        h_flex()
                            .gap_2()
                            .children(self.render_context(cx))
                            .when(working && can_send, |row| {
                                row.child(self.render_send_menu(cx))
                            })
                            .child(if working {
                                Button::new("stop")
                                    .rounded(crate::theme::INNER_RADIUS)
                                    .small()
                                    .icon(IconName::Square)
                                    .tooltip(t!("composer.stop"))
                                    .on_click(
                                        cx.listener(|this, _, window, cx| {
                                            this.interrupt(window, cx)
                                        }),
                                    )
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
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.send(window, cx)),
                                    )
                                    .into_any_element()
                            }),
                    ),
            );
        // Sous le composer : le projet et sa branche.
        v_flex()
            .w_full()
            .max_w(px(760.))
            .gap_2()
            .child(composer)
            .child(self.render_under_composer(cx))
    }

    /// Un tour commence : son chronomètre et son compte repartent de zéro, et la
    /// ligne de travail se redessine chaque seconde jusqu'à sa fin. Un message
    /// mis en file pendant un tour ne le recommence pas.
    fn begin_turn(&mut self, cx: &mut Context<Self>) {
        if self.status == Status::Working {
            return;
        }
        self.turn_started = Some(std::time::Instant::now());
        // Les nanosecondes de l'horloge suffisent à varier le verbe d'un tour à l'autre.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos() as usize);
        self.turn_verb = verbs::VERBS[nanos % verbs::VERBS.len()];
        self.thinking_since = None;
        self.thought_for = None;
        self.remeasure_last(cx);
        self.turn_tokens = 0;
        self.turn_chars = 0;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
                let going = this.update(cx, |this, cx| {
                    cx.notify();
                    this.turn_started.is_some()
                });
                if !matches!(going, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    /// La réflexion s'arrête quand l'agent écrit ou lance un outil.
    fn end_thinking(&mut self, cx: &mut Context<Self>) {
        if let Some(since) = self.thinking_since.take() {
            let secs = since.elapsed().as_secs().max(1);
            self.thought_for = Some(secs);
            if let Some(ix) = self
                .items
                .iter()
                .rposition(|item| matches!(item, Item::Thinking(_)))
            {
                self.items[ix] = Item::Thinking(Some(secs));
                self.remeasure(ix, cx);
            }
        }
    }

    /// « 12 s · ↓ 1,2k tokens · thinking » : le temps du tour, les tokens écrits,
    /// exacts à chaque fin d'appel et estimés entre deux (environ 4 caractères par
    /// token), et l'état de la réflexion avec les mots de Claude Code.
    /// Dans le fil, la durée d'une réflexion finie y est déjà écrite : `above`
    /// ne la répète qu'au-dessus du composer.
    fn turn_progress(&self, above: bool) -> Option<String> {
        let started = self.turn_started?;
        let secs = started.elapsed().as_secs();
        let time = if secs < 60 {
            t!("thread.seconds", n = secs).to_string()
        } else {
            t!(
                "thread.minutes",
                m = secs / 60,
                s = format!("{:02}", secs % 60)
            )
            .to_string()
        };
        let tokens = self.turn_tokens + (self.turn_chars / 4) as u64;
        let tokens = if tokens < 1000 {
            tokens.to_string()
        } else {
            crate::decimal(format!("{:.1}k", tokens as f64 / 1000.0))
        };
        let progress = t!("thread.progress", time = time, tokens = tokens).to_string();
        let thinking = match (self.thinking_since, self.thought_for) {
            (Some(since), _) => Some(
                match since.elapsed().as_secs() {
                    0..10 => "thinking",
                    10..20 => "still thinking",
                    20..30 => "thinking more",
                    30..45 => "thinking some more",
                    _ => "deep in thought",
                }
                .to_string(),
            ),
            (None, Some(secs)) if above => Some(format!("thought for {secs}s")),
            (None, _) => None,
        };
        Some(match thinking {
            Some(thinking) => format!("{progress} · {thinking}"),
            None => progress,
        })
    }

    /// L'indicateur du contexte, à côté de l'envoi : un anneau rempli à la part
    /// de la fenêtre utilisée quand sa taille est connue, sinon le seul nombre
    /// de tokens. Le détail est dans l'infobulle.
    fn render_context(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let tokens = self.context_tokens?;
        let thousands = |n: u64| format!("{:.0}k", n as f64 / 1000.0);
        let muted = cx.theme().muted_foreground;
        let (shown, tip) = match self.context_window.filter(|window| *window > 0) {
            Some(window) => {
                let percent = (tokens as f64 * 100.0 / window as f64).min(100.0) as f32;
                let tip = t!(
                    "composer.context",
                    used = thousands(tokens),
                    window = thousands(window),
                    percent = format!("{percent:.0}")
                )
                .to_string();
                (
                    ProgressCircle::new("context")
                        .value(percent)
                        .color(cx.theme().foreground)
                        .accessibility_label(tip.clone())
                        .into_any_element(),
                    tip,
                )
            }
            None => (
                div().text_xs().child(thousands(tokens)).into_any_element(),
                t!("composer.context.unknown", used = thousands(tokens)).to_string(),
            ),
        };
        Some(
            div()
                .id("context")
                .flex()
                .items_center()
                .text_color(muted)
                .child(shown)
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                }),
        )
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
                // Dans une vue étroite, les boutons passent sous le texte.
                .flex_wrap()
                .child(
                    div()
                        .flex_grow(1.)
                        .flex_basis(px(240.))
                        .min_w_0()
                        .child(t!("mission.ready").to_string()),
                )
                // Les deux boutons restent ensemble, à droite, quand ils passent
                // sous le texte.
                .child(
                    h_flex()
                        .ml_auto()
                        .gap_3()
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
                ),
        )
    }

    fn render_permission(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let permission = self.permission.as_ref().filter(|p| p.tool != PLAN_TOOL)?;
        let (description, detail) = self.permission_detail(&permission.input);
        let theme = cx.theme();
        Some(
            h_flex()
                .items_start()
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
                        .gap_1()
                        .child(t!("permission.title", tool = permission.tool).to_string())
                        .children(description.map(|description| {
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(description)
                        }))
                        // Tout ce qui sera approuvé, sans troncature : une
                        // commande longue défile.
                        .child(
                            div()
                                .id("permission-detail")
                                .max_h(px(160.))
                                .overflow_y_scroll()
                                .font_family(theme.mono_font_family.clone())
                                .text_xs()
                                .child(detail),
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
            // Vue étroite : la liste des fichiers modifiés disparaît (le diff les
            // titre), l'arborescence ou le fichier ouvert prend toute la place.
            workspace::Pane::Files => {
                let (viewer, tree) = self.render_files_view(cx);
                match (self.compact(), self.opened.is_some()) {
                    (false, _) => (viewer, Some(tree)),
                    (true, true) => (viewer, None),
                    (true, false) => (tree, None),
                }
            }
            workspace::Pane::Changes => {
                let (diff, list) = self.render_changes_view(cx);
                (diff, (!self.compact()).then_some(list))
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
                            .child(self.render_bottom(cx)),
                    )
                    .children(aside),
            )
    }

    /// Sous le contenu : travail en cours, suggestions, demandes et composer.
    /// Pendant tout le tour : l'étoile, le verbe (« Pondering… »), le temps, les
    /// tokens écrits et l'état de la réflexion.
    fn render_turn_line(&self, cx: &App) -> Option<AnyElement> {
        (self.status == Status::Working).then(|| {
            h_flex()
                .gap_2()
                .child(working_star(cx))
                .child(ShimmerText::new(format!("{}…", self.turn_verb)).id("working"))
                .children(self.turn_progress(line_above(cx)).map(|progress| {
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(progress)
                }))
                .into_any_element()
        })
    }

    fn render_bottom(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .items_center()
            .px_6()
            .pb_4()
            .gap_2()
            // La ligne du tour, si le réglage la veut au-dessus du composer.
            .when(line_above(cx), |column| {
                column.children(
                    self.render_turn_line(cx)
                        .map(|line| div().w_full().max_w(px(760.)).child(line)),
                )
            })
            .children(self.render_suggestions(cx))
            .children(self.render_plan_ready(cx))
            .children(self.render_permission(cx))
            .children(self.render_queued(cx))
            .children(self.render_template_form(cx))
            .child(self.render_composer(cx))
    }
}

/// En dessous de cette largeur, la vue passe en disposition compacte.
const COMPACT: Pixels = px(640.);

impl SessionView {
    fn compact(&self) -> bool {
        self.width < COMPACT
    }
}

impl Render for SessionView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // La largeur réelle, connue au rendu ; un changement redessine la vue.
        let view = cx.entity();
        let measure = canvas(
            move |bounds, _, cx| {
                view.update(cx, |this, cx| {
                    if this.width != bounds.size.width {
                        this.width = bounds.size.width;
                        cx.notify();
                    }
                })
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        v_flex()
            .relative()
            .size_full()
            .text_sm()
            .child(measure)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    // Échap ferme d'abord le dialogue de publication.
                    if this.publish.take().is_some() || this.template_form.take().is_some() {
                        cx.notify();
                    } else {
                        this.interrupt(window, cx);
                    }
                }
            }))
            .child(if self.started() || self.pane == workspace::Pane::Files {
                self.render_thread(cx).into_any_element()
            } else {
                self.render_home(window, cx).into_any_element()
            })
            .children(
                (self.pane == workspace::Pane::Changes)
                    .then(|| self.render_publish_dialog(cx))
                    .flatten(),
            )
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

#[cfg(all(test, unix))]
pub(crate) mod tests;
