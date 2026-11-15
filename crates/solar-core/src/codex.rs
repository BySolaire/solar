//! Codex (OpenAI) piloté en natif : `codex exec --json`, un processus par tour,
//! repris d'un tour à l'autre par l'identifiant de sa session. Ses événements
//! sont traduits en ceux de Claude, pour que la vue reste la même.
//!
//! Ce format n'est pas un contrat : toute ligne inconnue est ignorée. Codex en
//! mode `exec` ne demande aucune permission ; c'est sa sandbox qui borne ce
//! qu'il peut faire, choisie à chaque tour d'après le mode de SOLAR.

use std::collections::{HashSet, VecDeque};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::claude::{Event, Image};

/// La demande d'approbation d'un plan, que SOLAR forme lui-même à la fin d'un
/// tour en lecture seule : Codex n'a pas d'outil pour proposer un plan.
pub const PLAN_REQUEST: &str = "codex-plan";
/// Le nom d'outil que la vue reconnaît comme une proposition de plan.
const PLAN_TOOL: &str = "ExitPlanMode";
const APPROVED: &str = "The plan is approved. Implement it now, as planned.";

type OnEvent = Arc<dyn Fn(Event) + Send + Sync>;

struct Turn {
    text: String,
    images: Vec<Image>,
}

struct Inner {
    cwd: PathBuf,
    thread: Option<String>,
    mode: String,
    model: Option<String>,
    effort: Option<String>,
    running: Option<Child>,
    interrupted: bool,
    queue: VecDeque<Turn>,
}

/// Une conversation avec Codex. La lâcher arrête le tour en cours.
pub struct Codex {
    inner: Arc<Mutex<Inner>>,
    on_event: OnEvent,
}

impl Codex {
    /// Prépare la conversation dans `cwd` ; aucun processus ne tourne avant le
    /// premier message. `codex` doit être installé.
    pub fn spawn(
        cwd: &Path,
        resume: Option<&str>,
        permission_mode: &str,
        on_event: impl Fn(Event) + Send + Sync + 'static,
    ) -> io::Result<Self> {
        Command::new("codex")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner {
                cwd: cwd.to_path_buf(),
                thread: resume.map(str::to_string),
                mode: permission_mode.to_string(),
                model: None,
                effort: None,
                running: None,
                interrupted: false,
                queue: VecDeque::new(),
            })),
            on_event: Arc::new(on_event),
        })
    }

    /// Envoie un message. Pendant un tour, il attend la fin de celui-ci.
    pub fn send(&mut self, text: &str, images: &[Image]) -> io::Result<()> {
        let turn = Turn {
            text: text.to_string(),
            images: images.to_vec(),
        };
        let mut inner = self.inner.lock().expect("verrou de Codex");
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
        let mut inner = self.inner.lock().expect("verrou de Codex");
        inner.queue.clear();
        let inner = &mut *inner;
        if let Some(child) = inner.running.as_mut() {
            inner.interrupted = true;
            child.kill()?;
        }
        Ok(())
    }

    pub fn set_model(&mut self, model: &str) {
        self.inner.lock().expect("verrou de Codex").model = Some(model.to_string());
    }

    pub fn set_effort(&mut self, effort: &str) {
        self.inner.lock().expect("verrou de Codex").effort = Some(effort.to_string());
    }

    pub fn set_permission_mode(&mut self, mode: &str) {
        self.inner.lock().expect("verrou de Codex").mode = mode.to_string();
    }
}

impl Drop for Codex {
    fn drop(&mut self) {
        let _ = self.interrupt();
    }
}

/// Les options de sandbox d'un mode de SOLAR. Un mode que Codex ne sait pas
/// tenir (« demander à chaque fois ») part en lecture seule, jamais plus large.
fn sandbox(mode: &str) -> &'static [&'static str] {
    match mode {
        "bypassPermissions" => &["--dangerously-bypass-approvals-and-sandbox"],
        "acceptEdits" => &["-c", "sandbox_mode=\"workspace-write\""],
        _ => &["-c", "sandbox_mode=\"read-only\""],
    }
}

fn start(
    shared: &Arc<Mutex<Inner>>,
    on_event: &OnEvent,
    inner: &mut Inner,
    turn: Turn,
) -> io::Result<()> {
    let mut command = Command::new("codex");
    command.arg("exec");
    if let Some(id) = &inner.thread {
        command.args(["resume", id]);
    }
    command
        .args(["--json", "--skip-git-repo-check"])
        .args(sandbox(&inner.mode));
    if let Some(model) = &inner.model {
        command.args(["-m", model]);
    }
    if let Some(effort) = &inner.effort {
        command.args(["-c", &format!("model_reasoning_effort=\"{effort}\"")]);
    }
    // shortcut: les images passent par des fichiers temporaires jamais effacés ;
    // les ranger dans un dossier nettoyé si elles s'accumulent.
    for (ix, (mime, bytes)) in turn.images.iter().enumerate() {
        let ext = mime.rsplit('/').next().unwrap_or("png");
        let file = std::env::temp_dir().join(format!(
            "solar-codex-{}-{}-{ix}.{ext}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::write(&file, bytes)?;
        command.arg("-i").arg(file);
    }
    let mut child = command
        .arg("-")
        .current_dir(&inner.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("stdin demandé en pipe");
    stdin.write_all(turn.text.as_bytes())?;
    drop(stdin);
    let stdout = child.stdout.take().expect("stdout demandé en pipe");
    let mut stderr = child.stderr.take().expect("stderr demandé en pipe");
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    inner.running = Some(child);
    inner.interrupted = false;

    let (shared, on_event) = (shared.clone(), on_event.clone());
    std::thread::spawn(move || {
        let mut parser = Parser::default();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            for event in parser.line(&line) {
                if let Event::Ready { session_id, .. } = &event {
                    shared.lock().expect("verrou de Codex").thread = Some(session_id.clone());
                }
                on_event(event);
            }
        }
        let stderr = errors.join().unwrap_or_default();
        let mut inner = shared.lock().expect("verrou de Codex");
        let status = inner.running.take().and_then(|mut child| child.wait().ok());
        let interrupted = std::mem::take(&mut inner.interrupted);
        if !parser.done {
            let error = (!interrupted).then(|| {
                let last = stderr.lines().rev().find(|line| !line.trim().is_empty());
                match (status, last) {
                    (_, Some(line)) => line.trim().to_string(),
                    (Some(status), None) => format!("codex : {status}"),
                    (None, None) => "codex s'est arrêté".to_string(),
                }
            });
            on_event(Event::TurnDone {
                error,
                context_tokens: 0,
                cost_usd: 0.0,
            });
        }
        if inner.mode == "plan" && parser.ok && !interrupted && !parser.last_message.is_empty() {
            on_event(Event::PermissionRequest {
                request_id: PLAN_REQUEST.into(),
                tool: PLAN_TOOL.into(),
                input: json!({"plan": parser.last_message}),
            });
        }
        if let Some(next) = inner.queue.pop_front()
            && let Err(error) = start(&shared, &on_event, &mut inner, next)
        {
            on_event(Event::TurnDone {
                error: Some(error.to_string()),
                context_tokens: 0,
                cost_usd: 0.0,
            });
        }
    });
    Ok(())
}

/// La commande lancée, sans l'enveloppe du shell (`/bin/zsh -lc '…'`).
fn shown_command(command: &str) -> String {
    let inner = ["/bin/zsh -lc ", "/bin/bash -lc ", "bash -lc ", "sh -c "]
        .iter()
        .find_map(|shell| command.strip_prefix(shell))
        .unwrap_or(command);
    for quote in ['\'', '"'] {
        if let Some(bare) = inner
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return bare.to_string();
        }
    }
    inner.to_string()
}

fn file_tool(kind: &str) -> &'static str {
    match kind {
        "add" => "Write",
        "delete" => "Delete",
        _ => "Edit",
    }
}

/// Traduit la sortie de `codex exec --json`, ligne par ligne.
#[derive(Default)]
pub struct Parser {
    started: HashSet<String>,
    /// Le dernier message de Codex : sa réponse, ou son plan en lecture seule.
    pub last_message: String,
    /// Le tour est fini (réussi ou non).
    pub done: bool,
    /// Le tour a réussi.
    pub ok: bool,
}

impl Parser {
    pub fn line(&mut self, line: &str) -> Vec<Event> {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        let item = &value["item"];
        let id = item["id"].as_str().unwrap_or_default().to_string();
        match value["type"].as_str().unwrap_or_default() {
            "thread.started" => vec![Event::Ready {
                session_id: value["thread_id"].as_str().unwrap_or_default().into(),
                model: String::new(),
                permission_mode: String::new(),
            }],
            "item.started" => {
                self.started.insert(id.clone());
                tool_starts(&id, item)
            }
            "item.completed" => match item["type"].as_str().unwrap_or_default() {
                "agent_message" => {
                    let text = item["text"].as_str().unwrap_or_default().trim().to_string();
                    self.last_message = text.clone();
                    vec![Event::Text(format!("{text}\n\n"))]
                }
                "reasoning" => vec![Event::Thinking],
                _ => {
                    let mut events = if self.started.remove(&id) {
                        Vec::new()
                    } else {
                        tool_starts(&id, item)
                    };
                    events.extend(tool_results(&id, item));
                    events
                }
            },
            "turn.completed" => self.finish(None, value["usage"]["input_tokens"].as_u64()),
            "turn.failed" => {
                let message = value["error"]["message"].as_str().unwrap_or("turn failed");
                self.finish(Some(message.to_string()), None)
            }
            "error" => {
                let message = value["message"].as_str().unwrap_or("error");
                self.finish(Some(message.to_string()), None)
            }
            _ => Vec::new(),
        }
    }

    fn finish(&mut self, error: Option<String>, tokens: Option<u64>) -> Vec<Event> {
        if std::mem::replace(&mut self.done, true) {
            return Vec::new();
        }
        self.ok = error.is_none();
        vec![Event::TurnDone {
            error,
            context_tokens: tokens.unwrap_or_default(),
            cost_usd: 0.0,
        }]
    }
}

/// Les débuts d'outil d'un élément ; une modification de plusieurs fichiers
/// donne un outil par fichier.
fn tool_starts(id: &str, item: &Value) -> Vec<Event> {
    let start = |id: String, name: &str, input: Value| Event::ToolStart {
        id,
        name: name.into(),
        input,
    };
    match item["type"].as_str().unwrap_or_default() {
        "command_execution" => {
            let command = shown_command(item["command"].as_str().unwrap_or_default());
            vec![start(id.into(), "Bash", json!({"command": command}))]
        }
        "file_change" => changes(item)
            .enumerate()
            .map(|(ix, (path, kind))| {
                start(
                    format!("{id}:{ix}"),
                    file_tool(kind),
                    json!({"file_path": path}),
                )
            })
            .collect(),
        "web_search" => vec![start(
            id.into(),
            "WebSearch",
            json!({"description": item["query"]}),
        )],
        "mcp_tool_call" => {
            let name = format!(
                "mcp__{}__{}",
                item["server"].as_str().unwrap_or_default(),
                item["tool"].as_str().unwrap_or_default()
            );
            vec![start(id.into(), &name, item["arguments"].clone())]
        }
        _ => Vec::new(),
    }
}

fn tool_results(id: &str, item: &Value) -> Vec<Event> {
    let failed = item["status"].as_str() == Some("failed");
    let result = |id: String, is_error: bool, text: String| Event::ToolResult {
        id,
        is_error,
        text,
        at: None,
    };
    match item["type"].as_str().unwrap_or_default() {
        "command_execution" => vec![result(
            id.into(),
            failed || item["exit_code"].as_i64().is_some_and(|code| code != 0),
            item["aggregated_output"]
                .as_str()
                .unwrap_or_default()
                .into(),
        )],
        "file_change" => changes(item)
            .enumerate()
            .map(|(ix, _)| result(format!("{id}:{ix}"), failed, String::new()))
            .collect(),
        "web_search" | "mcp_tool_call" => {
            let text = item["result"].to_string();
            vec![result(id.into(), failed, text)]
        }
        _ => Vec::new(),
    }
}

fn changes(item: &Value) -> impl Iterator<Item = (&str, &str)> {
    item["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|change| {
            (
                change["path"].as_str().unwrap_or_default(),
                change["kind"].as_str().unwrap_or_default(),
            )
        })
}

/// La conversation enregistrée par Codex pour `session_id`, relue en
/// événements, ou rien si Codex ne l'a pas (ou plus).
pub fn history(session_id: &str) -> Vec<Event> {
    let home = std::env::var_os("CODEX_HOME").map_or_else(
        || std::env::home_dir().unwrap_or_default().join(".codex"),
        PathBuf::from,
    );
    let suffix = format!("-{session_id}.jsonl");
    ["sessions", "archived_sessions"]
        .iter()
        .find_map(|dir| find(&home.join(dir), &suffix))
        .and_then(|file| std::fs::read_to_string(file).ok())
        .map(|text| parse_history(&text))
        .unwrap_or_default()
}

fn find(dir: &Path, suffix: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        if path.is_dir() {
            find(&path, suffix)
        } else {
            path.to_string_lossy().ends_with(suffix).then_some(path)
        }
    })
}

/// Traduit un fichier de session de Codex : ses éléments terminés, dans l'ordre.
pub fn parse_history(text: &str) -> Vec<Event> {
    let mut events = Vec::new();
    for value in text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    {
        let payload = &value["payload"];
        if payload["type"] != "item_completed" {
            continue;
        }
        let item = &payload["item"];
        let id = item["id"].as_str().unwrap_or_default().to_string();
        let at = value["timestamp"].as_str().map(str::to_string);
        let texts = |key: &str| -> String {
            item["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|part| part[key].as_str())
                .collect()
        };
        match item["type"].as_str().unwrap_or_default() {
            "UserMessage" => events.push(Event::User(texts("text"))),
            "AgentMessage" => events.push(Event::Text(format!("{}\n\n", texts("text").trim()))),
            "CommandExecution" => {
                let command = item["command"]
                    .as_array()
                    .and_then(|parts| parts.last())
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                events.push(Event::ToolStart {
                    id: id.clone(),
                    name: "Bash".into(),
                    input: json!({"command": command}),
                });
                events.push(Event::ToolResult {
                    id,
                    is_error: item["exit_code"].as_i64().is_some_and(|code| code != 0),
                    text: item["aggregated_output"]
                        .as_str()
                        .unwrap_or_default()
                        .into(),
                    at,
                });
            }
            "FileChange" => {
                // Triés : l'ordre des clés JSON dépend des options de serde_json.
                let mut files: Vec<_> = item["changes"].as_object().into_iter().flatten().collect();
                files.sort_by_key(|(path, _)| path.as_str());
                for (ix, (path, change)) in files.into_iter().enumerate() {
                    let id = format!("{id}:{ix}");
                    events.push(Event::ToolStart {
                        id: id.clone(),
                        name: file_tool(change["type"].as_str().unwrap_or_default()).into(),
                        input: json!({"file_path": path}),
                    });
                    events.push(Event::ToolResult {
                        id,
                        is_error: item["status"] == "failed",
                        text: String::new(),
                        at: at.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shell_wrapper_is_not_shown() {
        assert_eq!(shown_command("/bin/zsh -lc ls"), "ls");
        assert_eq!(shown_command("/bin/zsh -lc 'cat calc.py'"), "cat calc.py");
        assert_eq!(shown_command("cargo test"), "cargo test");
    }

    #[test]
    fn an_unknown_mode_is_read_only() {
        assert_eq!(sandbox("default"), sandbox("plan"));
        assert!(sandbox("plan").contains(&"sandbox_mode=\"read-only\""));
    }
}
