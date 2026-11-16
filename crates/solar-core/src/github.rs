//! GitHub par le CLI `gh` : le dépôt d'un projet, le compte qui publiera, et
//! (tickets suivants) commit, push, pull request et checks d'une mission.
//! SOLAR ne lit ni ne garde aucun jeton : `gh` tient l'authentification, et
//! tout passe en arguments séparés, sans shell.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

/// Le dépôt GitHub où un projet peut publier.
#[derive(Clone, Debug, PartialEq)]
pub struct Repo {
    pub host: String,
    /// `propriétaire/dépôt`.
    pub name: String,
    pub default_branch: String,
    /// Le compte actif de `gh` pour cet hôte : celui qui publiera.
    pub account: String,
    pub url: String,
}

/// Pourquoi un projet ne peut pas publier.
#[derive(Clone, Debug, PartialEq)]
pub enum Reason {
    /// `gh` n'est pas installé.
    NoGh,
    /// Le dépôt n'a pas de remote `origin`.
    NoOrigin,
    /// `origin` est sur un hôte que `gh` ne connaît pas.
    NotGitHub(String),
    /// `gh` n'a pas de compte connecté pour cet hôte.
    LoggedOut(String),
    Fork,
    /// Le compte (nommé) ne peut que lire ce dépôt.
    ReadOnly(String),
    /// Une commande a échoué : son message.
    Failed(String),
}

/// Le CLI `gh` ; un autre programme pour les essais.
#[derive(Clone, Debug)]
pub struct Gh {
    program: PathBuf,
}

impl Default for Gh {
    fn default() -> Self {
        Self::at("gh")
    }
}

impl Gh {
    pub fn at(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
        }
    }

    /// Lance `gh` dans `dir` ; sa sortie, ou son message d'erreur.
    fn run(&self, dir: &Path, args: &[&str]) -> Result<String, Reason> {
        let output = Command::new(&self.program)
            .args(args)
            .current_dir(dir)
            .stdin(Stdio::null())
            // Jamais de question ni de couleur : `gh` parle à un programme.
            .env("GH_PROMPT_DISABLED", "1")
            .env("NO_COLOR", "1")
            .output()
            .map_err(|error| match error.kind() {
                io::ErrorKind::NotFound => Reason::NoGh,
                _ => Reason::Failed(error.to_string()),
            })?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            Err(Reason::Failed(error.trim().to_string()))
        }
    }

    /// Le compte actif pour `host`, sans jamais demander de jeton.
    fn account(&self, dir: &Path, host: &str) -> Result<Option<String>, Reason> {
        let status = self.run(dir, &["auth", "status", "--json", "hosts"])?;
        let status: Value = serde_json::from_str(&status).unwrap_or_default();
        Ok(status["hosts"][host].as_array().and_then(|accounts| {
            accounts
                .iter()
                .find(|account| account["active"] == true && account["state"] == "success")
                .and_then(|account| account["login"].as_str())
                .map(str::to_string)
        }))
    }

    /// Ce projet peut-il publier sur GitHub, et avec quel compte ? Lance
    /// `git` et `gh` : à appeler en arrière-plan.
    pub fn availability(&self, project: &Path) -> Result<Repo, Reason> {
        let origin = Command::new("git")
            .args(["--no-optional-locks", "remote", "get-url", "origin"])
            .current_dir(project)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .ok_or(Reason::NoOrigin)?;
        let origin = String::from_utf8_lossy(&origin.stdout);
        let (host, name) = remote_repo(origin.trim())
            .ok_or_else(|| Reason::NotGitHub(origin.trim().to_string()))?;
        // `gh` sait-il quelque chose de cet hôte ? Sinon, ce n'est pas GitHub
        // pour lui (GitLab…), sauf github.com, où il faut simplement se connecter.
        let account = match self.account(project, &host)? {
            Some(account) => account,
            None if host == "github.com" => return Err(Reason::LoggedOut(host)),
            None => return Err(Reason::NotGitHub(host)),
        };
        let view = self.run(
            project,
            &[
                "repo",
                "view",
                &format!("{host}/{name}"),
                "--json",
                "nameWithOwner,defaultBranchRef,viewerPermission,isFork,url",
            ],
        )?;
        let view: Value = serde_json::from_str(&view)
            .map_err(|error| Reason::Failed(format!("gh repo view : {error}")))?;
        if view["isFork"] == true {
            return Err(Reason::Fork);
        }
        if !matches!(
            view["viewerPermission"].as_str(),
            Some("ADMIN" | "MAINTAIN" | "WRITE")
        ) {
            return Err(Reason::ReadOnly(account));
        }
        let text = |value: &Value| value.as_str().unwrap_or_default().to_string();
        Ok(Repo {
            host,
            name: text(&view["nameWithOwner"]),
            default_branch: text(&view["defaultBranchRef"]["name"]),
            account,
            url: text(&view["url"]),
        })
    }
}

/// L'hôte et le `propriétaire/dépôt` d'une adresse de remote : HTTPS, SSH
/// (`git@hôte:…` ou `ssh://…`). `None` pour un chemin local ou une adresse
/// sans dépôt.
pub fn remote_repo(url: &str) -> Option<(String, String)> {
    let rest = if let Some((_, rest)) = url.split_once("://") {
        rest
    } else if let Some((user_host, path)) = url.split_once(':') {
        // `git@github.com:propriétaire/dépôt.git`
        if user_host.contains('/') {
            return None;
        }
        return split(user_host, path);
    } else {
        return None;
    };
    let (host, path) = rest.split_once('/')?;
    split(host, path)
}

fn split(user_host: &str, path: &str) -> Option<(String, String)> {
    let host = user_host.rsplit('@').next()?;
    let host = host.split(':').next()?; // port éventuel
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = path.split('/');
    let (owner, repo) = (parts.next()?, parts.next()?);
    // Aucune partie vide ni commençant par un tiret : l'argument donné à `gh`
    // ne doit jamais passer pour une option.
    let fine = |part: &str| !part.is_empty() && !part.starts_with('-');
    (fine(host) && fine(owner) && fine(repo) && parts.next().is_none())
        .then(|| (host.to_string(), format!("{owner}/{repo}")))
}
