//! Les agents pilotés un processus par tour (Codex, Grok) : chaque message
//! lance le CLI, qui reprend la session du tour précédent, écrit ses
//! événements en JSON puis s'arrête. Ce module tient la conversation : file des
//! messages, interruption, et le plan à approuver à la fin d'un tour en
//! lecture seule. Chaque CLI ne fournit que sa commande et sa traduction.

use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde_json::json;

use crate::claude::{Event, Image};

/// La demande d'approbation d'un plan, que Solar forme lui-même à la fin d'un
/// tour en lecture seule : ces CLI ne peuvent pas la poser sans terminal.
pub const PLAN_REQUEST: &str = "solar-plan";
/// Le nom d'outil que la vue reconnaît comme une proposition de plan.
const PLAN_TOOL: &str = "ExitPlanMode";
const APPROVED: &str = "The plan is approved. Implement it now, as planned.";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cli {
    Codex,
    Grok,
}

impl Cli {
    fn program(self) -> &'static str {
        match self {
            Cli::Codex => "codex",
            Cli::Grok => "grok",
        }
    }

    fn command(self, settings: &Settings, turn: &Turn) -> io::Result<(Command, Option<String>)> {
        match self {
            Cli::Codex => crate::codex::command(settings, turn),
            Cli::Grok => crate::grok::command(settings, turn),
        }
    }

    fn parser(self) -> Box<dyn TurnParser + Send> {
        match self {
            Cli::Codex => Box::<crate::codex::Parser>::default(),
            Cli::Grok => Box::<crate::grok::Parser>::default(),
        }
    }
}

/// Traduit la sortie d'un tour, ligne par ligne, en événements de Claude.
pub trait TurnParser {
    fn line(&mut self, line: &str) -> Vec<Event>;
    /// Le tour est fini, réussi ou non.
    fn done(&self) -> bool;
    fn ok(&self) -> bool;
    /// Le dernier message de l'agent : sa réponse, ou son plan en lecture seule.
    fn last_message(&self) -> &str;
}

pub struct Turn {
    pub text: String,
    pub images: Vec<Image>,
}

/// Ce que le tour suivant reprend.
pub struct Settings {
    pub session: Option<String>,
    pub mode: String,
    pub model: Option<String>,
    pub effort: Option<String>,
}

type OnEvent = Arc<dyn Fn(Event) + Send + Sync>;

struct Inner {
    cli: Cli,
    cwd: PathBuf,
    settings: Settings,
    running: Option<Child>,
    interrupted: bool,
    queue: VecDeque<Turn>,
}

/// Une conversation avec un agent piloté par tours. La lâcher arrête le tour
/// en cours.
pub struct Turns {
    inner: Arc<Mutex<Inner>>,
    on_event: OnEvent,
}

impl Turns {
    /// Prépare la conversation dans `cwd` ; aucun processus ne tourne avant le
    /// premier message. Le CLI doit être installé.
    pub fn spawn(
        cli: Cli,
        cwd: &Path,
        resume: Option<&str>,
        permission_mode: &str,
        on_event: impl Fn(Event) + Send + Sync + 'static,
    ) -> io::Result<Self> {
        Command::new(cli.program())
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner {
                cli,
                cwd: cwd.to_path_buf(),
                settings: Settings {
                    session: resume.map(str::to_string),
                    mode: permission_mode.to_string(),
                    model: None,
                    effort: None,
                },
                running: None,
                interrupted: false,
                queue: VecDeque::new(),
            })),
            on_event: Arc::new(on_event),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("verrou de la conversation")
    }

    /// Envoie un message. Pendant un tour, il attend la fin de celui-ci.
    pub fn send(&mut self, text: &str, images: &[Image]) -> io::Result<()> {
        let turn = Turn {
            text: text.to_string(),
            images: images.to_vec(),
        };
        let mut inner = self.lock();
        if inner.running.is_some() {
            inner.queue.push_back(turn);
            return Ok(());
        }
        start(&self.inner, &self.on_event, &mut inner, turn)
    }

    /// Le plan proposé est approuvé : un nouveau tour l'exécute, dans le mode
    /// choisi juste avant.
    pub fn allow(&mut self, request_id: &str) -> io::Result<()> {
        if request_id == PLAN_REQUEST {
            self.send(APPROVED, &[])?;
        }
        Ok(())
    }

    pub fn interrupt(&mut self) -> io::Result<()> {
        let mut inner = self.lock();
        inner.queue.clear();
        let inner = &mut *inner;
        if let Some(child) = inner.running.as_mut() {
            inner.interrupted = true;
            child.kill()?;
        }
        Ok(())
    }

    pub fn set_model(&mut self, model: &str) {
        self.lock().settings.model = Some(model.to_string());
    }

    pub fn set_effort(&mut self, effort: &str) {
        self.lock().settings.effort = Some(effort.to_string());
    }

    pub fn set_permission_mode(&mut self, mode: &str) {
        self.lock().settings.mode = mode.to_string();
    }
}

impl Drop for Turns {
    fn drop(&mut self) {
        let _ = self.interrupt();
    }
}

fn start(
    shared: &Arc<Mutex<Inner>>,
    on_event: &OnEvent,
    inner: &mut Inner,
    turn: Turn,
) -> io::Result<()> {
    let (mut command, input) = inner.cli.command(&inner.settings, &turn)?;
    crate::checkpoint::pin_for_agent(
        &inner.cwd,
        &crate::checkpoint::Store::default_dir(),
        &mut command,
    )?;
    let mut child = command
        .current_dir(&inner.cwd)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        stdin.write_all(input.as_bytes())?;
    }
    let stdout = child.stdout.take().expect("stdout demandé en pipe");
    let mut stderr = child.stderr.take().expect("stderr demandé en pipe");
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let program = inner.cli.program();
    let mut parser = inner.cli.parser();
    inner.running = Some(child);
    inner.interrupted = false;

    let (shared, on_event) = (shared.clone(), on_event.clone());
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            for event in parser.line(&line) {
                if let Event::Ready { session_id, .. } = &event {
                    shared
                        .lock()
                        .expect("verrou de la conversation")
                        .settings
                        .session = Some(session_id.clone());
                }
                on_event(event);
            }
        }
        let stderr = errors.join().unwrap_or_default();
        let mut inner = shared.lock().expect("verrou de la conversation");
        let status = inner.running.take().and_then(|mut child| child.wait().ok());
        let interrupted = std::mem::take(&mut inner.interrupted);
        if !parser.done() {
            let error = (!interrupted).then(|| {
                let last = stderr.lines().rev().find(|line| !line.trim().is_empty());
                match (status, last) {
                    (_, Some(line)) => line.trim().to_string(),
                    (Some(status), None) => format!("{program} : {status}"),
                    (None, None) => format!("{program} s'est arrêté"),
                }
            });
            on_event(Event::TurnDone {
                error,
                context_tokens: 0,
                cost_usd: None,
            });
        }
        let plan = parser.last_message();
        if inner.settings.mode == "plan" && parser.ok() && !interrupted && !plan.is_empty() {
            on_event(Event::PermissionRequest {
                request_id: PLAN_REQUEST.into(),
                tool: PLAN_TOOL.into(),
                input: json!({"plan": plan}),
            });
        }
        if let Some(next) = inner.queue.pop_front()
            && let Err(error) = start(&shared, &on_event, &mut inner, next)
        {
            on_event(Event::TurnDone {
                error: Some(error.to_string()),
                context_tokens: 0,
                cost_usd: None,
            });
        }
    });
    Ok(())
}
