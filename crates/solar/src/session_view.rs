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
use solar_core::claude::{Claude, Event, Model, SlashCommand};
use solar_core::state::{Mission, MissionStatus};
use solar_core::{files, history};

use crate::assets::CLAUDE_LOGO;
use crate::theme::{BLACK, CLAUDE_BRAND, READING_FONT, VEIL, caps};

mod home;
mod workspace;

/// Largeur de la colonne de l'accueil ; la fenêtre y cale aussi la planète.
pub const HOME_WIDTH: Pixels = px(720.);

/// Modes de permission de Claude Code proposés dans le sélecteur.
const MODES: [&str; 4] = ["bypassPermissions", "acceptEdits", "default", "plan"];
/// Le mode où Claude prépare un plan sans rien écrire : le moteur le fait respecter.
const PLAN_MODE: &str = "plan";
/// L'outil par lequel Claude, en mode plan, propose son plan à approuver.
const PLAN_TOOL: &str = "ExitPlanMode";
/// Modes proposés au lancement d'une mission.
const LAUNCH_MODES: [&str; 3] = ["bypassPermissions", "acceptEdits", "default"];
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
    claude: Option<Claude>,
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
                    t!("composer.placeholder")
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
                        this.files = Some(files::list(&this.project));
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
        };
        // shortcut: l'historique est relu d'un bloc à l'ouverture ; le charger en
        // arrière-plan si de longues sessions ralentissent l'ouverture d'un onglet.
        if let Some(id) = this.session_id.clone() {
            for event in history::load(&this.project, &id) {
                this.on_event(event, cx);
            }
            this.status = Status::Idle;
            this.scroller.update(cx, |list, cx| list.scroll_to_end(cx));
        }
        this
    }

    /// Reprend la mission d'une conversation rouverte : tant qu'elle n'est pas
    /// lancée, Claude reste en mode plan, sans rien écrire.
    pub fn restore_mission(&mut self, mission: Option<Mission>) {
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
    fn launch_mission(&mut self, mode: &'static str, cx: &mut Context<Self>) {
        let pending = self.permission.take().filter(|p| p.tool == PLAN_TOOL);
        self.mode = mode;
        self.launch_mode = mode;
        let approved = t!("mission.approved").to_string();
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
        self.note(t!("mission.cancelled"), false, cx);
        cx.notify();
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
        let text = t!("thread.failed", error = error.to_string());
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
        cx.notify();
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
                let _ = claude.deny(&plan.request_id, &text);
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
            self.mission = Some(Mission::default());
        }
        if !self.started() {
            // Le brouillon de l'accueil devient une conversation ordinaire.
            self.input.update(cx, |input, cx| {
                input.set_placeholder(t!("composer.placeholder"), window, cx);
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
        self.push(Item::User(shown.into()), cx);
        self.scroller.update(cx, |list, cx| list.scroll_to_end(cx));
        self.status = Status::Working;
        cx.notify();
    }

    /// Lance le CLI au premier message, ou le relance s'il s'est arrêté.
    fn ensure_claude(&mut self, cx: &mut Context<Self>) -> std::io::Result<&mut Claude> {
        if self.claude.is_none() {
            self.spawned += 1;
            let generation = self.spawned;
            let (tx, rx) = smol::channel::unbounded();
            let mut claude = Claude::spawn(
                &self.project,
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
                if self.model.is_none() {
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
                        open: false,
                    },
                    cx,
                );
            }
            Event::ToolResult { id, is_error, text } => {
                let found =
                    self.items
                        .iter_mut()
                        .enumerate()
                        .rev()
                        .find_map(|(ix, item)| match item {
                            Item::Tool {
                                id: tool_id,
                                result,
                                ..
                            } if *tool_id == id => Some((ix, result)),
                            _ => None,
                        });
                if let Some((ix, result)) = found {
                    *result = Some((is_error, text));
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

    fn set_mode(&mut self, mode: &'static str, cx: &mut Context<Self>) {
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

    fn set_model(&mut self, value: String, cx: &mut Context<Self>) {
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

    fn set_effort(&mut self, effort: String, cx: &mut Context<Self>) {
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
            return short_path(std::path::Path::new(file), &self.root);
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
                                .child(avatar(
                                    svg()
                                        .path(CLAUDE_LOGO)
                                        .size_4()
                                        .text_color(rgb(CLAUDE_BRAND))
                                        .into_any_element(),
                                ))
                                .child(div().font_medium().child(caps(&t!("thread.claude")))),
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
    }

    /// Les réglages de la conversation en un menu : permissions, puis effort si le
    /// modèle en propose.
    fn render_settings(&self, efforts: Vec<String>, cx: &mut Context<Self>) -> impl IntoElement {
        let (view, mode, effort) = (cx.entity(), self.mode, self.effort.clone());
        Button::new("settings")
            .ghost()
            .small()
            .icon(IconName::SlidersHorizontal)
            .tooltip(t!("composer.settings"))
            .dropdown_menu(move |menu, _, _| {
                let menu =
                    MODES
                        .iter()
                        .fold(menu.label(t!("composer.permissions")), |menu, &value| {
                            let view = view.clone();
                            menu.item(
                                PopupMenuItem::new(t!(format!("mode.{value}")))
                                    .checked(mode == value)
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| this.set_mode(value, cx))
                                    }),
                            )
                        });
                if efforts.is_empty() {
                    return menu;
                }
                let menu = menu.separator().label(t!("composer.effort"));
                efforts.iter().fold(menu, |menu, value| {
                    let (view, value) = (view.clone(), value.clone());
                    menu.item(
                        PopupMenuItem::new(value.clone())
                            .checked(effort.as_deref() == Some(value.as_str()))
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| this.set_effort(value.clone(), cx))
                            }),
                    )
                })
            })
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
        let muted = theme.muted_foreground;
        let working = self.status != Status::Idle;

        let models = self
            .models
            .iter()
            .map(|model| (model.value.clone(), model.name.clone().into()))
            .collect::<Vec<_>>();
        let model_label = self
            .current_model()
            .map(|model| {
                // `default` s'appelle « Default (recommended) » : on montre le modèle qu'il désigne.
                if model.value == "default" && !model.description.is_empty() {
                    model.description.clone()
                } else {
                    model.name.clone()
                }
            })
            .or_else(|| self.model.clone())
            .unwrap_or_else(|| t!("composer.model").to_string());
        let model_value = self.current_model().map(|model| model.value.clone());
        // Les niveaux d'effort dépendent du modèle : sans modèle connu, pas de sélecteur.
        let efforts = self
            .current_model()
            .map(|model| model.efforts.clone())
            .unwrap_or_default();
        let can_send = !self.input.read(cx).value().trim().is_empty();

        // Le projet se choisit tant que la conversation n'a pas commencé.
        let project_label = self
            .project
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| t!("home.project.none").to_string());
        let projects = self
            .projects
            .iter()
            .map(|path| {
                let name = path.file_name().unwrap_or(path.as_os_str());
                (
                    path.to_string_lossy().to_string(),
                    name.to_string_lossy().to_string().into(),
                )
            })
            .chain([(String::new(), t!("home.project.choose").into())])
            .collect::<Vec<_>>();

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

        v_flex()
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
                            .child(
                                Button::new("attach")
                                    .ghost()
                                    .small()
                                    .icon(IconName::Plus)
                                    .tooltip(t!("composer.attach"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.pick_images(window, cx)
                                    })),
                            )
                            .when(!self.started(), |row| {
                                row.child(self.render_picker(
                                    "project",
                                    IconName::Folder,
                                    project_label.into(),
                                    projects,
                                    Some(self.project.to_string_lossy().to_string()),
                                    |this, path, window, cx| {
                                        // La dernière entrée, sans chemin, ouvre le sélecteur de dossier.
                                        if path.is_empty() {
                                            this.choose_folder(false, window, cx);
                                        } else {
                                            this.set_project(path.into(), cx);
                                        }
                                    },
                                    cx,
                                ))
                                // Planifier d'abord : Claude prépare un plan sans rien écrire.
                                .child(
                                    Button::new("plan-first")
                                        .ghost()
                                        .small()
                                        .icon(IconName::ListChecks)
                                        .label(t!("composer.plan").to_string())
                                        .selected(self.planning)
                                        .tooltip(t!("composer.plan.tooltip"))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.planning = !this.planning;
                                            cx.notify();
                                        })),
                                )
                            })
                            .when(!models.is_empty(), |row| {
                                row.child(self.render_picker(
                                    "model",
                                    IconName::Box,
                                    model_label.into(),
                                    models,
                                    model_value,
                                    |this, model, _, cx| this.set_model(model, cx),
                                    cx,
                                ))
                            })
                            .child(self.render_settings(efforts, cx))
                            // Accès complet : Claude écrit et exécute sans demander. Le
                            // réglage est rangé dans le menu, le signal reste en vue.
                            .when(self.mode == MODES[0], |row| {
                                row.child(
                                    div()
                                        .id("full-access")
                                        .px_1()
                                        .text_color(muted)
                                        .child(Icon::new(IconName::ShieldAlert).small())
                                        .tooltip(|window, cx| {
                                            gpui_kit::component::tooltip::Tooltip::new(
                                                t!("mode.bypassPermissions").to_string(),
                                            )
                                            .build(window, cx)
                                        }),
                                )
                            }),
                    )
                    .child(if working {
                        Button::new("stop")
                            .small()
                            .icon(IconName::Square)
                            .tooltip(t!("composer.stop"))
                            .on_click(cx.listener(|this, _, _, cx| this.interrupt(cx)))
                            .into_any_element()
                    } else {
                        Button::new("send")
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
        let (view, current) = (cx.entity(), self.launch_mode);
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
                            LAUNCH_MODES.iter().fold(menu, |menu, &mode| {
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
                        }),
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
        let pane = if self.mission.is_some() {
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
            .child(if self.started() {
                self.render_thread(cx).into_any_element()
            } else {
                self.render_home(window, cx).into_any_element()
            })
    }
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
    fn planned_mission(
        name: &str,
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
            "#!/bin/sh\nwhile read -r line; do\n  printf '%s\\n' \"$line\" >> '{log}'\n  case \"$line\" in\n    \
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

        cx.executor().allow_parking();
        cx.update(gpui_kit::init);
        let (window, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| SessionView::new(dir.clone(), None, "Test".into(), window, cx))
            })
            .unwrap()
        });
        cx.update_window(window, |_, window, cx| {
            view.update(cx, |view, cx| {
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
        let (window, view, log, path) = planned_mission("plan-test", cx);

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
        let (window, view, log, path) = planned_mission("cancel-test", cx);
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
            view.update(cx, |view, _| {
                view.items.push(Item::Tool {
                    id: "t".into(),
                    name: "Bash".into(),
                    input: Value::Null,
                    result: None,
                    open: false,
                });
                view.restore_mission(mission(MissionStatus::Running));
                assert!(view.interrupted);
                assert!(view.claude.is_none(), "rien n'est relancé");
                // Un plan prêt attend une décision : il n'est pas interrompu.
                view.restore_mission(mission(MissionStatus::Ready));
                assert!(!view.interrupted);
                // Un tour fini non plus.
                view.items.push(Item::Text("fait".into()));
                view.restore_mission(mission(MissionStatus::Running));
                assert!(!view.interrupted);
            })
        });
    }

    #[gpui_kit::test]
    async fn files_view_lists_the_project_and_shows_the_chosen_file(cx: &mut TestAppContext) {
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
