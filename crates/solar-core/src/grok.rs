//! Grok (xAI) piloté en natif : `grok -p --output-format streaming-json`, un
//! processus par tour (voir `turns`), repris par l'identifiant de sa session.
//! Ses événements, proches de ceux du protocole ACP, sont traduits en ceux de
//! Claude.
//!
//! Ce format n'est pas un contrat : toute ligne inconnue est ignorée. Sans
//! terminal, Grok refuse tout outil qui n'est pas autorisé d'avance : le mode
//! de SOLAR choisit, à chaque tour, ce qui l'est.

use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::process::Command;

use serde_json::{Value, json};

use crate::claude::Event;
use crate::turns::{Settings, Turn, TurnParser};

/// Les outils d'écriture de fichiers, autorisés en « accepter les
/// modifications » ; les commandes restent refusées.
const EDIT_TOOLS: [&str; 2] = ["search_replace", "write"];
/// Les outils du mode plan, qui ne montrent rien d'utile.
const PLAN_TOOLS: [&str; 2] = ["enter_plan_mode", "exit_plan_mode"];

/// Les options de permission d'un mode de SOLAR. Un mode que Grok ne sait pas
/// tenir sans terminal (« demander ») part en mode plan, jamais plus large.
fn permissions(mode: &str) -> Vec<&'static str> {
    match mode {
        "bypassPermissions" => vec!["--permission-mode", "bypassPermissions"],
        "acceptEdits" => {
            let mut args = vec!["--permission-mode", "acceptEdits"];
            for tool in EDIT_TOOLS {
                args.extend(["--allow", tool]);
            }
            args
        }
        "auto" => vec!["--permission-mode", "auto"],
        _ => vec!["--permission-mode", "plan"],
    }
}

/// Ce que la ligne de commande peut porter d'images, encodées : macOS borne
/// arguments et environnement ensemble à 1 Mio.
const MAX_IMAGES: usize = 768 * 1024;

/// Le message et ses images en blocs de contenu ACP, la forme que lit
/// `grok --prompt-json` (éprouvé avec grok 1.0.77 le 16 novembre 2026 : une
/// image PNG décrite correctement). Grok ne lit pas ce JSON sur l'entrée
/// standard : trop lourdes, les images sont refusées avec un message clair.
fn prompt_blocks(turn: &Turn) -> io::Result<String> {
    use base64::Engine as _;
    let images: Vec<(&str, String)> = turn
        .images
        .iter()
        .map(|(mime, bytes)| {
            (
                mime.as_str(),
                base64::engine::general_purpose::STANDARD.encode(bytes),
            )
        })
        .collect();
    if images.iter().map(|(_, data)| data.len()).sum::<usize>() > MAX_IMAGES {
        return Err(io::Error::other(
            "Grok : images trop lourdes pour un message (environ 550 Ko en tout)",
        ));
    }
    let blocks: Vec<Value> = std::iter::once(json!({"type": "text", "text": turn.text}))
        .chain(
            images
                .into_iter()
                .map(|(mime, data)| json!({"type": "image", "mimeType": mime, "data": data})),
        )
        .collect();
    Ok(Value::Array(blocks).to_string())
}

/// La commande d'un tour ; le message passe en argument.
pub(crate) fn command(settings: &Settings, turn: &Turn) -> io::Result<(Command, Option<String>)> {
    let mut command = Command::new("grok");
    if turn.images.is_empty() {
        // `--single=` : un message qui commence par un tiret reste un message.
        command.arg(format!("--single={}", turn.text));
    } else {
        command.args(["--prompt-json", &prompt_blocks(turn)?]);
    }
    command
        .args(["--output-format", "streaming-json"])
        .args(permissions(&settings.mode));
    if let Some(id) = &settings.session {
        command.args(["--resume", id]);
    }
    if let Some(model) = &settings.model {
        command.args(["--model", model]);
    }
    if let Some(effort) = &settings.effort {
        command.args(["--reasoning-effort", effort]);
    }
    Ok((command, None))
}

/// Traduit la sortie de `grok -p --output-format streaming-json`.
#[derive(Default)]
pub struct Parser {
    hidden: HashSet<String>,
    finished: HashSet<String>,
    /// Le texte écrit depuis le dernier outil : la réponse, ou le plan.
    current: String,
    done: bool,
    ok: bool,
}

impl Parser {
    pub fn line(&mut self, line: &str) -> Vec<Event> {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        match value["type"].as_str().unwrap_or_default() {
            "text" => {
                let chunk = value["data"].as_str().unwrap_or_default();
                self.current.push_str(chunk);
                vec![Event::Text(chunk.to_string())]
            }
            "thought" => vec![
                Event::Thinking,
                Event::Streamed {
                    chars: value["data"].as_str().unwrap_or_default().chars().count(),
                },
            ],
            // Une ligne par appel au modèle : la taille du contexte à cet instant,
            // et ce qu'il a écrit.
            "usage" => {
                let usage = &value["usage"];
                crate::claude::context_tokens(usage)
                    .map(|tokens| Event::Context {
                        tokens: Some(tokens),
                        window: None,
                    })
                    .into_iter()
                    .chain(
                        usage["output_tokens"]
                            .as_u64()
                            .map(|tokens| Event::Output { tokens }),
                    )
                    .collect()
            }
            "tool_call" => {
                let name = value["toolName"].as_str().or(value["title"].as_str());
                self.tool_call(&value, name.unwrap_or_default())
            }
            "tool_call_update" => self.tool_update(&value),
            "end" => {
                if std::mem::replace(&mut self.done, true) {
                    return Vec::new();
                }
                let reason = value["stopReason"].as_str().unwrap_or("end_turn");
                self.ok = matches!(reason, "end_turn" | "cancelled");
                let usage = &value["usage"];
                let tokens = usage["input_tokens"].as_u64().unwrap_or_default()
                    + usage["cache_read_input_tokens"]
                        .as_u64()
                        .unwrap_or_default();
                vec![
                    Event::Ready {
                        session_id: value["sessionId"].as_str().unwrap_or_default().into(),
                        model: String::new(),
                        permission_mode: String::new(),
                    },
                    Event::TurnDone {
                        error: (!self.ok).then(|| format!("Grok : {reason}")),
                        context_tokens: tokens,
                        cost_usd: value["total_cost_usd"].as_f64(),
                    },
                ]
            }
            "error" => {
                if std::mem::replace(&mut self.done, true) {
                    return Vec::new();
                }
                let message = value["message"].as_str().or(value["data"].as_str());
                vec![Event::TurnDone {
                    error: Some(message.unwrap_or("Grok : erreur").to_string()),
                    context_tokens: 0,
                    cost_usd: None,
                }]
            }
            _ => Vec::new(),
        }
    }

    fn tool_call(&mut self, value: &Value, name: &str) -> Vec<Event> {
        let id = value["toolCallId"].as_str().unwrap_or_default().to_string();
        self.current.clear();
        if PLAN_TOOLS.contains(&name) {
            self.hidden.insert(id);
            return Vec::new();
        }
        let input = &value["rawInput"];
        let (name, input) = match name {
            "read_file" => ("Read", json!({"file_path": input["target_file"]})),
            "run_terminal_command" => ("Bash", json!({"command": input["command"]})),
            "search_replace" => ("Edit", json!({"file_path": input["file_path"]})),
            "write" => ("Write", json!({"file_path": input["file_path"]})),
            "grep" => ("Grep", json!({"pattern": input["pattern"]})),
            "list_dir" => ("Glob", json!({"pattern": input["target_directory"]})),
            "web_search" => ("WebSearch", json!({"description": input["query"]})),
            "web_fetch" => ("WebFetch", json!({"url": input["url"]})),
            "todo_write" => ("TodoWrite", input.clone()),
            other => (other, input.clone()),
        };
        vec![Event::ToolStart {
            id,
            name: name.into(),
            input,
        }]
    }

    fn tool_update(&mut self, value: &Value) -> Vec<Event> {
        let id = value["toolCallId"].as_str().unwrap_or_default().to_string();
        let status = value["status"].as_str().unwrap_or_default();
        if !matches!(status, "completed" | "failed")
            || self.hidden.contains(&id)
            || !self.finished.insert(id.clone())
        {
            return Vec::new();
        }
        let text = value["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|part| part["content"]["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        vec![Event::ToolResult {
            id,
            is_error: status == "failed",
            text,
            at: None,
        }]
    }
}

impl TurnParser for Parser {
    fn line(&mut self, line: &str) -> Vec<Event> {
        Parser::line(self, line)
    }
    fn done(&self) -> bool {
        self.done
    }
    fn ok(&self) -> bool {
        self.ok
    }
    fn last_message(&self) -> &str {
        self.current.trim()
    }
}

/// La conversation enregistrée par Grok pour `session_id` (son journal
/// `updates.jsonl`), relue en événements, ou rien si Grok ne l'a pas.
pub fn history(session_id: &str) -> Vec<Event> {
    let home = std::env::var_os("GROK_HOME").map_or_else(
        || std::env::home_dir().unwrap_or_default().join(".grok"),
        PathBuf::from,
    );
    std::fs::read_dir(home.join("sessions"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|project| project.path().join(session_id).join("updates.jsonl"))
        .find(|file| file.is_file())
        .and_then(|file| std::fs::read_to_string(file).ok())
        .map(|text| parse_history(&text))
        .unwrap_or_default()
}

/// Traduit le journal d'une session : les mêmes mises à jour qu'en direct,
/// sous la forme du protocole ACP.
pub fn parse_history(text: &str) -> Vec<Event> {
    let mut parser = Parser::default();
    let mut events: Vec<Event> = Vec::new();
    for value in text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
    {
        let update = &value["params"]["update"];
        let chunk = update["content"]["text"].as_str().unwrap_or_default();
        match update["sessionUpdate"].as_str().unwrap_or_default() {
            "user_message_chunk" => match events.last_mut() {
                Some(Event::User(text)) => text.push_str(chunk),
                _ => events.push(Event::User(chunk.to_string())),
            },
            "agent_message_chunk" => events.push(Event::Text(chunk.to_string())),
            "tool_call" => {
                let name = update["title"].as_str().unwrap_or_default();
                events.extend(parser.tool_call(update, name));
            }
            "tool_call_update" => {
                for mut event in parser.tool_update(update) {
                    if let Event::ToolResult { at, .. } = &mut event {
                        *at = value["timestamp"]
                            .as_i64()
                            .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0))
                            .map(|at| at.to_rfc3339());
                    }
                    events.push(event);
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

    fn turn(images: Vec<crate::claude::Image>) -> Turn {
        Turn {
            text: "Quelle couleur ?".into(),
            images,
        }
    }

    fn settings() -> Settings {
        Settings {
            session: None,
            mode: "plan".into(),
            model: None,
            effort: None,
        }
    }

    #[test]
    fn images_go_with_the_text_as_acp_blocks() {
        let (command, stdin) = command(
            &settings(),
            &turn(vec![("image/png".into(), vec![1, 2, 3])]),
        )
        .unwrap();
        assert_eq!(stdin, None);
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect();
        let at = args.iter().position(|arg| arg == "--prompt-json").unwrap();
        let blocks: Value = serde_json::from_str(&args[at + 1]).unwrap();
        assert_eq!(
            blocks,
            serde_json::json!([
                {"type": "text", "text": "Quelle couleur ?"},
                {"type": "image", "mimeType": "image/png", "data": "AQID"}
            ])
        );
        assert!(!args.iter().any(|arg| arg.starts_with("--single")));
    }

    #[test]
    fn images_too_heavy_for_the_command_line_are_refused() {
        let heavy = vec![("image/png".into(), vec![0; 1024 * 1024])];
        assert!(command(&settings(), &turn(heavy)).is_err());
    }

    #[test]
    fn modes_allow_only_what_they_say() {
        assert_eq!(permissions("manual"), permissions("plan"));
        let accept = permissions("acceptEdits").join(" ");
        assert!(accept.contains("--allow search_replace") && !accept.contains("run_terminal"));
    }
}
