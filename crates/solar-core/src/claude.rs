//! Claude Code piloté en natif : le CLI `claude` en mode stream-json.
//!
//! Ce protocole n'est pas documenté par Anthropic. Toute ligne inconnue est
//! ignorée pour qu'une mise à jour du CLI ne fasse pas planter l'app.

use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Value, json};

/// Un modèle proposé par le CLI.
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    /// Valeur attendue par le CLI (`default`, `opus`…).
    pub value: String,
    pub name: String,
    /// Précision du CLI ; pour `default`, le nom du modèle qu'il désigne.
    pub description: String,
    /// Identifiant complet, celui qu'annonce une session en cours.
    pub resolved: String,
    pub efforts: Vec<String>,
}

/// Une commande `/` ou un skill proposé par le CLI.
#[derive(Clone, Debug, PartialEq)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
}

/// Une image jointe à un message : type MIME et octets.
pub type Image = (String, Vec<u8>);

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Ce que le CLI propose, reçu juste après son lancement.
    Catalog {
        models: Vec<Model>,
        commands: Vec<SlashCommand>,
    },
    /// La session existe côté CLI ; `session_id` sert à la reprendre.
    Ready {
        session_id: String,
        model: String,
        permission_mode: String,
    },
    /// Message de l'utilisateur, relu dans l'historique d'une session.
    User(String),
    Text(String),
    /// Claude réfléchit. Le CLI ne transmet pas le texte de la réflexion.
    Thinking,
    ToolStart {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        id: String,
        is_error: bool,
        text: String,
        /// Horodatage ISO 8601 noté par Claude dans l'historique ; absent en direct.
        at: Option<String>,
    },
    PermissionRequest {
        request_id: String,
        tool: String,
        input: Value,
    },
    TurnDone {
        error: Option<String>,
        context_tokens: u64,
        cost_usd: f64,
    },
    /// Réglages de Claude Code en vigueur, lus au lancement : le modèle et l'effort
    /// qu'utilisera la session si l'on ne choisit rien.
    Settings {
        model: Option<String>,
        effort: Option<String>,
    },
    /// Un morceau écrit par le modèle hors du texte visible (réflexion, entrée
    /// d'un outil) : sa taille sert à estimer les tokens en direct.
    Streamed {
        chars: usize,
    },
    /// Le nombre exact de tokens écrits par un appel au modèle, à sa fin.
    Output {
        tokens: u64,
    },
    /// La taille du contexte de la conversation, en tokens, et celle de la
    /// fenêtre du modèle ; l'un ou l'autre peut arriver seul.
    Context {
        tokens: Option<u64>,
        window: Option<u64>,
    },
    Exited,
}

/// Une session Claude Code vivante. La lâcher arrête le processus.
pub struct Claude {
    stdin: ChildStdin,
    child: Child,
}

impl Claude {
    /// Lance le CLI dans `cwd`. `on_event` est appelé depuis un thread de lecture.
    pub fn spawn(
        cwd: &Path,
        resume: Option<&str>,
        permission_mode: &str,
        on_event: impl Fn(Event) + Send + 'static,
    ) -> io::Result<Self> {
        let mut command = Command::new("claude");
        command
            .args([
                "-p",
                "--verbose",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--include-partial-messages",
                "--permission-prompt-tool",
                "stdio",
                "--permission-mode",
                permission_mode,
            ])
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        if let Some(id) = resume {
            command.args(["--resume", id]);
        }
        let mut child = command.spawn()?;
        let stdout = child.stdout.take().expect("stdout demandé en pipe");
        let stdin = child.stdin.take().expect("stdin demandé en pipe");

        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                parse_line(&line).into_iter().for_each(&on_event);
            }
            on_event(Event::Exited);
        });
        let mut this = Self { stdin, child };
        this.control("solar-init", json!({"subtype": "initialize"}))?;
        this.control("solar-settings", json!({"subtype": "get_settings"}))?;
        Ok(this)
    }

    /// Envoie un message. Pendant un tour, le CLI le met en file.
    pub fn send(&mut self, text: &str, images: &[Image]) -> io::Result<()> {
        let mut content: Vec<Value> = images
            .iter()
            .map(|(media_type, bytes)| {
                json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": media_type,
                        "data": BASE64.encode(bytes),
                    },
                })
            })
            .collect();
        content.push(json!({"type": "text", "text": text}));
        self.write(json!({
            "type": "user",
            "message": {"role": "user", "content": content},
        }))
    }

    pub fn allow(&mut self, request_id: &str, input: &Value) -> io::Result<()> {
        self.answer(
            request_id,
            json!({"behavior": "allow", "updatedInput": input}),
        )
    }

    /// Refuse, en disant à Claude pourquoi : pour un plan, ce sont les corrections.
    pub fn deny(&mut self, request_id: &str, message: &str) -> io::Result<()> {
        self.answer(request_id, json!({"behavior": "deny", "message": message}))
    }

    pub fn interrupt(&mut self) -> io::Result<()> {
        self.control("solar-interrupt", json!({"subtype": "interrupt"}))
    }

    pub fn set_model(&mut self, model: &str) -> io::Result<()> {
        self.control(
            "solar-model",
            json!({"subtype": "set_model", "model": model}),
        )
    }

    pub fn set_effort(&mut self, effort: &str) -> io::Result<()> {
        self.control(
            "solar-effort",
            json!({"subtype": "apply_flag_settings", "settings": {"effortLevel": effort}}),
        )
    }

    pub fn set_permission_mode(&mut self, mode: &str) -> io::Result<()> {
        self.control(
            "solar-mode",
            json!({"subtype": "set_permission_mode", "mode": mode}),
        )
    }

    fn control(&mut self, request_id: &str, request: Value) -> io::Result<()> {
        self.write(json!({
            "type": "control_request",
            "request_id": request_id,
            "request": request,
        }))
    }

    fn answer(&mut self, request_id: &str, response: Value) -> io::Result<()> {
        self.write(json!({
            "type": "control_response",
            "response": {"subtype": "success", "request_id": request_id, "response": response},
        }))
    }

    fn write(&mut self, frame: Value) -> io::Result<()> {
        writeln!(self.stdin, "{frame}")?;
        self.stdin.flush()
    }
}

impl Drop for Claude {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// L'entrée d'un appel au modèle, cache compris ; `None` sans décompte.
pub(crate) fn context_tokens(usage: &Value) -> Option<u64> {
    let tokens = |key: &str| usage[key].as_u64();
    let total = tokens("input_tokens")?
        + tokens("cache_creation_input_tokens").unwrap_or(0)
        + tokens("cache_read_input_tokens").unwrap_or(0);
    Some(total)
}

/// Traduit une ligne de sortie du CLI. Une ligne inconnue ne donne rien.
pub fn parse_line(line: &str) -> Vec<Event> {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return vec![];
    };
    // shortcut: les lignes des sous-agents sont ignorées ; les afficher sous leur
    // outil quand le fil saura imbriquer.
    if !v["parent_tool_use_id"].is_null() {
        return vec![];
    }
    let text = |value: &Value| value.as_str().unwrap_or_default().to_string();

    match v["type"].as_str().unwrap_or_default() {
        "system" if v["subtype"] == "init" => vec![Event::Ready {
            session_id: text(&v["session_id"]),
            model: text(&v["model"]),
            permission_mode: text(&v["permissionMode"]),
        }],
        "stream_event" => {
            let event = &v["event"];
            let delta = &event["delta"];
            if event["content_block"]["type"] == "thinking" {
                vec![Event::Thinking]
            } else if delta["type"] == "text_delta" {
                vec![Event::Text(text(&delta["text"]))]
            } else if let Some(part) = delta["thinking"]
                .as_str()
                .or(delta["partial_json"].as_str())
            {
                vec![Event::Streamed {
                    chars: part.chars().count(),
                }]
            } else if event["type"] == "message_delta"
                && let Some(tokens) = event["usage"]["output_tokens"].as_u64()
            {
                vec![Event::Output { tokens }]
            } else {
                vec![]
            }
        }
        // Le texte est déjà arrivé par morceaux : on ne garde ici que les outils.
        "assistant" | "user" => {
            let mut events = blocks(&v["message"], false);
            // Le dernier appel du modèle dit ce que contient le contexte : son
            // entrée, cache compris. (Le total de fin de tour additionne les appels.)
            if v["type"] == "assistant"
                && let Some(tokens) = context_tokens(&v["message"]["usage"])
            {
                events.push(Event::Context {
                    tokens: Some(tokens),
                    window: None,
                });
            }
            events
        }
        "control_response" if v["response"]["request_id"] == "solar-settings" => {
            let effective = &v["response"]["response"]["effective"];
            let value = |key: &str| effective[key].as_str().filter(|value| !value.is_empty());
            vec![Event::Settings {
                model: value("model").map(str::to_string),
                effort: value("effortLevel").map(str::to_string),
            }]
        }
        "control_response" if v["response"]["request_id"] == "solar-init" => {
            let body = &v["response"]["response"];
            let list = |key: &str| body[key].as_array().cloned().unwrap_or_default();
            vec![Event::Catalog {
                models: list("models")
                    .iter()
                    .map(|model| Model {
                        value: text(&model["value"]),
                        name: text(&model["displayName"]),
                        description: text(&model["description"]),
                        resolved: text(&model["resolvedModel"]),
                        efforts: model["supportedEffortLevels"]
                            .as_array()
                            .map(|levels| levels.iter().map(text).collect())
                            .unwrap_or_default(),
                    })
                    .collect(),
                commands: list("commands")
                    .iter()
                    .map(|command| SlashCommand {
                        name: text(&command["name"]),
                        description: text(&command["description"]),
                    })
                    .collect(),
            }]
        }
        "control_request" if v["request"]["subtype"] == "can_use_tool" => {
            vec![Event::PermissionRequest {
                request_id: text(&v["request_id"]),
                tool: text(&v["request"]["tool_name"]),
                input: v["request"]["input"].clone(),
            }]
        }
        "result" => {
            let usage = &v["usage"];
            let tokens = |key: &str| usage[key].as_u64().unwrap_or(0);
            vec![Event::TurnDone {
                // Un tour interrompu par l'utilisateur n'est pas une erreur.
                error: (v["is_error"] == true
                    && !text(&v["terminal_reason"]).starts_with("aborted"))
                .then(|| text(&v["result"])),
                context_tokens: tokens("input_tokens")
                    + tokens("cache_creation_input_tokens")
                    + tokens("cache_read_input_tokens"),
                cost_usd: v["total_cost_usd"].as_f64().unwrap_or(0.0),
            }]
            .into_iter()
            .chain(
                v["modelUsage"]
                    .as_object()
                    .and_then(|models| models.values().find_map(|m| m["contextWindow"].as_u64()))
                    .map(|window| Event::Context {
                        tokens: None,
                        window: Some(window),
                    }),
            )
            .collect()
        }
        _ => vec![],
    }
}

/// Contenu d'un message complet. `with_text` ajoute les textes et les
/// réflexions : l'historique en a besoin, le direct les reçoit déjà par morceaux.
pub(crate) fn blocks(message: &Value, with_text: bool) -> Vec<Event> {
    let text = |value: &Value| value.as_str().unwrap_or_default().to_string();
    let from_user = message["role"] == "user";
    let spoken = |words: String| {
        if from_user {
            Event::User(words)
        } else {
            Event::Text(words)
        }
    };
    let content = match &message["content"] {
        Value::Array(content) => content,
        Value::String(words) if with_text => return vec![spoken(words.clone())],
        _ => return vec![],
    };
    content
        .iter()
        .filter_map(|block| match block["type"].as_str()? {
            "text" if with_text => Some(spoken(text(&block["text"]))),
            "thinking" if with_text => Some(Event::Thinking),
            "tool_use" => Some(Event::ToolStart {
                id: text(&block["id"]),
                name: text(&block["name"]),
                input: block["input"].clone(),
            }),
            "tool_result" => Some(Event::ToolResult {
                id: text(&block["tool_use_id"]),
                is_error: block["is_error"] == true,
                // Le résultat est soit une chaîne, soit une liste de blocs de texte.
                text: match &block["content"] {
                    Value::Array(parts) => parts
                        .iter()
                        .filter_map(|part| part["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    other => text(other),
                },
                at: None,
            }),
            _ => None,
        })
        .collect()
}
