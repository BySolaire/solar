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

/// L'état du projet au lancement d'une mission : le commit de départ, la
/// branche de l'utilisateur et les fichiers qu'il avait modifiés sans commit
/// (chemins relatifs au projet). La publication s'en sert pour ne pas emporter
/// ses changements sans le lui dire.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Launch {
    pub start: String,
    pub branch: Option<String>,
    pub dirty: Vec<String>,
}

/// `git` dans `dir`, en lecture : ni verrou d'index ni hook.
fn read_git(dir: &Path, args: &[&str]) -> io::Result<String> {
    let output = Command::new("git")
        .args(["--no-optional-locks", "-c", "core.hooksPath=/dev/null"])
        .args(args)
        .current_dir(dir)
        .output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

pub fn launch_state(project: &Path) -> io::Result<Launch> {
    let start = read_git(project, &["rev-parse", "HEAD"])?;
    let branch = read_git(project, &["branch", "--show-current"])
        .ok()
        .filter(|branch| !branch.is_empty());
    let changed = read_git(
        project,
        &["diff", "--name-only", "--relative", "HEAD", "--", "."],
    )?;
    let untracked = read_git(
        project,
        &["ls-files", "--others", "--exclude-standard", "--", "."],
    )?;
    let mut dirty: Vec<String> = changed
        .lines()
        .chain(untracked.lines())
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    dirty.sort();
    dirty.dedup();
    Ok(Launch {
        start,
        branch,
        dirty,
    })
}

#[derive(Debug)]
pub enum CommitError {
    /// Git refuse ce nom de branche.
    BadBranch,
    /// Les fichiers choisis ne changent rien : aucun commit.
    Nothing,
    /// Un hook ou git a refusé : leur sortie.
    Failed(String),
}

/// Le commit d'une mission, dans sa copie (ADR 0001) : la branche de mission
/// (créée la première fois depuis le commit de départ), un index temporaire
/// qui ne reçoit que `files`, puis `git commit`, hooks de l'utilisateur
/// compris. Le projet, sa branche et son index ne sont pas touchés. Rend le
/// commit créé.
pub fn commit_mission(
    copy: &Path,
    branch: &str,
    files: &[String],
    message: &str,
) -> Result<String, CommitError> {
    let failed = |error: io::Error| CommitError::Failed(error.to_string());
    if read_git(copy, &["check-ref-format", "--branch", branch]).is_err() {
        return Err(CommitError::BadBranch);
    }
    let current = read_git(copy, &["branch", "--show-current"]).map_err(failed)?;
    if current != branch {
        // `switch -c` garde les fichiers de la copie tels quels.
        read_git(copy, &["switch", "--quiet", "-c", branch]).map_err(failed)?;
    }
    let index = std::env::temp_dir().join(format!(
        "solar-publish-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let result = (|| {
        let git = |args: &[&str]| -> io::Result<std::process::Output> {
            Command::new("git")
                .args(args)
                .current_dir(copy)
                .env("GIT_INDEX_FILE", &index)
                .stdin(Stdio::null())
                .output()
        };
        let ok = |output: std::process::Output| {
            if output.status.success() {
                Ok(())
            } else {
                Err(CommitError::Failed(
                    format!(
                        "{}\n{}",
                        String::from_utf8_lossy(&output.stderr).trim(),
                        String::from_utf8_lossy(&output.stdout).trim()
                    )
                    .trim()
                    .to_string(),
                ))
            }
        };
        ok(git(&["read-tree", "HEAD"]).map_err(failed)?)?;
        let mut add = vec!["add", "-A", "--"];
        add.extend(files.iter().map(String::as_str));
        ok(git(&add).map_err(failed)?)?;
        if git(&["diff", "--cached", "--quiet"])
            .map_err(failed)?
            .status
            .success()
        {
            return Err(CommitError::Nothing);
        }
        ok(git(&["commit", "--quiet", "-m", message]).map_err(failed)?)?;
        read_git(copy, &["rev-parse", "HEAD"]).map_err(failed)
    })();
    let _ = std::fs::remove_file(&index);
    result
}

/// Le dernier commit de la branche de mission dans la copie, s'il existe.
pub fn branch_head(copy: &Path, branch: &str) -> Option<String> {
    read_git(
        copy,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .ok()
}

#[derive(Debug)]
pub enum PushError {
    /// La branche a bougé sur GitHub : rien n'est écrasé.
    Moved,
    Failed(String),
}

impl Gh {
    /// Pousse la branche de mission sur `origin`, avec le compte actif de `gh`
    /// (son aide d'identification, le temps de cette commande), jamais de force.
    pub fn push(&self, copy: &Path, branch: &str) -> Result<(), PushError> {
        let helper = format!(
            "credential.helper=!'{}' auth git-credential",
            self.program.display()
        );
        let output = Command::new("git")
            .args(["-c", "credential.helper=", "-c", &helper])
            .args(["push", "--porcelain", "origin"])
            .arg(format!("refs/heads/{branch}:refs/heads/{branch}"))
            .current_dir(copy)
            .stdin(Stdio::null())
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .map_err(|error| PushError::Failed(error.to_string()))?;
        if output.status.success() {
            return Ok(());
        }
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if text.contains("[rejected]")
            || text.contains("non-fast-forward")
            || text.contains("fetch first")
        {
            Err(PushError::Moved)
        } else {
            Err(PushError::Failed(text.trim().to_string()))
        }
    }
}

/// La branche de mission : `solar/<titre>` en minuscules sans accents, puis
/// `-<n>` pour la tentative n.
pub fn branch_name(title: &str, attempt: u32) -> String {
    let mut slug = String::new();
    for c in crate::search::normalize(title).chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.truncate(48);
    let slug = slug.trim_end_matches('-');
    let slug = if slug.is_empty() { "mission" } else { slug };
    match attempt {
        0 | 1 => format!("solar/{slug}"),
        n => format!("solar/{slug}-{n}"),
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
