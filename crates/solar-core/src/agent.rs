//! Le fournisseur d'une conversation, et l'agent qui la tient : Claude, Codex
//! ou Grok, derrière les mêmes gestes.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::claude::{Claude, Event, Image};
use crate::turns::{Cli, Turns};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Claude,
    Codex,
    Grok,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Claude, Provider::Codex, Provider::Grok];

    pub fn name(self) -> &'static str {
        match self {
            Provider::Claude => "Claude",
            Provider::Codex => "Codex",
            Provider::Grok => "Grok",
        }
    }

    /// Le programme à trouver sur le PATH.
    pub fn program(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::Codex => "codex",
            Provider::Grok => "grok",
        }
    }

    /// Les modes de permission que ce fournisseur tient vraiment. Codex et Grok,
    /// sans terminal, ne savent pas demander avant d'agir.
    pub fn supports(self, mode: &str) -> bool {
        self == Provider::Claude || mode != "default"
    }

    /// Le CLI est-il installé ? Un fournisseur absent est proposé grisé.
    pub fn installed(self) -> bool {
        std::env::var_os("PATH").is_some_and(|path| {
            std::env::split_paths(&path).any(|dir| dir.join(self.program()).is_file())
        })
    }

    pub fn is_default(&self) -> bool {
        *self == Provider::Claude
    }

    /// La commande qui connecte le CLI, à lancer par l'utilisateur.
    pub fn login_command(self) -> &'static str {
        match self {
            Provider::Claude => "claude auth login",
            Provider::Codex => "codex login",
            Provider::Grok => "grok login",
        }
    }

    /// L'état réel du CLI : version, connexion et modèle par défaut. Lance ses
    /// commandes d'état (aucune n'est facturée) : à appeler en arrière-plan.
    pub fn health(self) -> Health {
        let Some(version) = run(self.program(), &["--version"]) else {
            return Health::default();
        };
        let version = version.lines().next().map(|line| line.trim().to_string());
        let (signed_in, model) = match self {
            Provider::Claude => (
                run("claude", &["auth", "status"]).map(|out| claude_signed_in(&out)),
                read_config("CLAUDE_CONFIG_DIR", ".claude", "settings.json")
                    .and_then(|text| claude_model(&text)),
            ),
            Provider::Codex => (
                run("codex", &["login", "status"]).map(|out| out.contains("Logged in")),
                read_config("CODEX_HOME", ".codex", "config.toml")
                    .and_then(|text| codex_model(&text)),
            ),
            Provider::Grok => match run("grok", &["models"]) {
                Some(out) => (Some(out.contains("logged in")), grok_model(&out)),
                None => (None, None),
            },
        };
        Health {
            version,
            signed_in,
            model,
        }
    }
}

/// Ce que l'on sait d'un CLI. Tout vide : il n'est pas installé.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Health {
    pub version: Option<String>,
    /// `None` : l'état de connexion n'a pas pu être lu.
    pub signed_in: Option<bool>,
    pub model: Option<String>,
}

/// La sortie d'une commande qui a réussi, sortie d'erreur comprise.
fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr)
    })
}

fn read_config(env: &str, home: &str, file: &str) -> Option<String> {
    let dir = std::env::var_os(env).map_or_else(
        || std::env::home_dir().unwrap_or_default().join(home),
        std::path::PathBuf::from,
    );
    std::fs::read_to_string(dir.join(file)).ok()
}

// Seul `loggedIn` est lu : cette sortie contient aussi l'adresse du compte.
fn claude_signed_in(out: &str) -> bool {
    let start = out.find('{').unwrap_or(0);
    serde_json::from_str::<Value>(&out[start..]).is_ok_and(|status| status["loggedIn"] == true)
}

fn claude_model(settings: &str) -> Option<String> {
    let settings: Value = serde_json::from_str(settings).ok()?;
    settings["model"].as_str().map(str::to_string)
}

/// `model = "…"` au premier niveau de `config.toml`, avant toute section.
fn codex_model(config: &str) -> Option<String> {
    config
        .lines()
        .take_while(|line| !line.trim_start().starts_with('['))
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == "model").then(|| value.trim().trim_matches('"').to_string())
        })
}

fn grok_model(out: &str) -> Option<String> {
    out.lines()
        .find_map(|line| line.trim().strip_prefix("Default model:"))
        .map(|model| model.trim().to_string())
}

pub enum Agent {
    Claude(Claude),
    /// Codex ou Grok, un processus par tour.
    Turns(Turns),
}

impl Agent {
    pub fn spawn(
        provider: Provider,
        cwd: &Path,
        resume: Option<&str>,
        permission_mode: &str,
        on_event: impl Fn(Event) + Send + Sync + 'static,
    ) -> io::Result<Self> {
        Ok(match provider {
            Provider::Claude => {
                Agent::Claude(Claude::spawn(cwd, resume, permission_mode, on_event)?)
            }
            Provider::Codex => Agent::Turns(Turns::spawn(
                Cli::Codex,
                cwd,
                resume,
                permission_mode,
                on_event,
            )?),
            Provider::Grok => Agent::Turns(Turns::spawn(
                Cli::Grok,
                cwd,
                resume,
                permission_mode,
                on_event,
            )?),
        })
    }

    pub fn send(&mut self, text: &str, images: &[Image]) -> io::Result<()> {
        match self {
            Agent::Claude(claude) => claude.send(text, images),
            Agent::Turns(codex) => codex.send(text, images),
        }
    }

    pub fn allow(&mut self, request_id: &str, input: &Value) -> io::Result<()> {
        match self {
            Agent::Claude(claude) => claude.allow(request_id, input),
            Agent::Turns(codex) => codex.allow(request_id),
        }
    }

    /// Refuse une demande en cours. Codex et Grok n'en font jamais : rien à refuser.
    pub fn deny(&mut self, request_id: &str, message: &str) -> io::Result<()> {
        match self {
            Agent::Claude(claude) => claude.deny(request_id, message),
            Agent::Turns(_) => Ok(()),
        }
    }

    /// Corrige un plan proposé : Claude le reprend dans sa demande, les autres
    /// au tour suivant, toujours en lecture seule.
    pub fn correct_plan(&mut self, request_id: &str, text: &str) -> io::Result<()> {
        match self {
            Agent::Claude(claude) => claude.deny(request_id, text),
            Agent::Turns(codex) => codex.send(text, &[]),
        }
    }

    pub fn interrupt(&mut self) -> io::Result<()> {
        match self {
            Agent::Claude(claude) => claude.interrupt(),
            Agent::Turns(codex) => codex.interrupt(),
        }
    }

    pub fn set_model(&mut self, model: &str) -> io::Result<()> {
        match self {
            Agent::Claude(claude) => claude.set_model(model),
            Agent::Turns(codex) => {
                codex.set_model(model);
                Ok(())
            }
        }
    }

    pub fn set_effort(&mut self, effort: &str) -> io::Result<()> {
        match self {
            Agent::Claude(claude) => claude.set_effort(effort),
            Agent::Turns(codex) => {
                codex.set_effort(effort);
                Ok(())
            }
        }
    }

    pub fn set_permission_mode(&mut self, mode: &str) -> io::Result<()> {
        match self {
            Agent::Claude(claude) => claude.set_permission_mode(mode),
            Agent::Turns(codex) => {
                codex.set_permission_mode(mode);
                Ok(())
            }
        }
    }
}

/// Le texte d'une conversation enregistrée, messages et réponses, pour la
/// recherche.
pub fn transcript(provider: Provider, workdir: &Path, session_id: &str) -> String {
    history(provider, workdir, session_id)
        .into_iter()
        .filter_map(|event| match event {
            Event::User(text) | Event::Text(text) => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// La conversation enregistrée par le fournisseur, relue en événements.
pub fn history(provider: Provider, workdir: &Path, session_id: &str) -> Vec<Event> {
    match provider {
        Provider::Claude => crate::history::load(workdir, session_id),
        Provider::Codex => crate::codex::history(session_id),
        Provider::Grok => crate::grok::history(session_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_outputs_are_read() {
        assert!(claude_signed_in(
            "{\n  \"loggedIn\": true,\n  \"email\": \"x\"\n}"
        ));
        assert!(!claude_signed_in("{\"loggedIn\": false}"));
        assert_eq!(claude_model(r#"{"model":"opus"}"#).as_deref(), Some("opus"));
        let toml = "model = \"gpt-6\"\n[profiles.fast]\nmodel = \"mini\"\n";
        assert_eq!(codex_model(toml).as_deref(), Some("gpt-6"));
        assert_eq!(
            codex_model("[x]\nmodel = \"y\"\n"),
            None,
            "hors section seulement"
        );
        let grok = "You are logged in with grok.com.\n\nDefault model: grok-4.7\n";
        assert_eq!(grok_model(grok).as_deref(), Some("grok-4.7"));
    }
}
