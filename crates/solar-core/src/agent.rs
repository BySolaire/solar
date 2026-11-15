//! Le fournisseur d'une conversation, et l'agent qui la tient : Claude, Codex
//! ou Grok, derrière les mêmes gestes.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::claude::{Claude, Event, Image};
use crate::turns::{Cli, Turns};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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

/// La conversation enregistrée par le fournisseur, relue en événements.
pub fn history(provider: Provider, workdir: &Path, session_id: &str) -> Vec<Event> {
    match provider {
        Provider::Claude => crate::history::load(workdir, session_id),
        Provider::Codex => crate::codex::history(session_id),
        Provider::Grok => crate::grok::history(session_id),
    }
}
