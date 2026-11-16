//! GitHub par le CLI `gh` : le dépôt d'un projet, le compte qui publiera, et
//! (tickets suivants) commit, push, pull request et checks d'une mission.
//! Solar ne lit ni ne garde aucun jeton : `gh` tient l'authentification, et
//! tout passe en arguments séparés, sans shell.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::checkpoint::CopyGit;

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

/// `git` dans `dir`, en lecture : ni verrou d'index, ni hook, ni commande
/// de surveillance des fichiers.
fn read_git(dir: &Path, args: &[&str]) -> io::Result<String> {
    read_git_at(dir, None, args)
}

/// `read_git`, sur le dépôt épinglé d'une copie s'il est donné.
fn read_git_at(dir: &Path, copy: Option<&CopyGit>, args: &[&str]) -> io::Result<String> {
    let mut command = Command::new("git");
    if let Some(copy) = copy {
        copy.pin(&mut command);
    }
    let output = command
        .args(["--no-optional-locks", "-c", "core.hooksPath=/dev/null"])
        .args(["-c", "core.fsmonitor=false"])
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
    /// Une branche de ce nom existe déjà dans le dépôt.
    BranchExists,
    /// Les fichiers choisis ne changent rien : aucun commit.
    Nothing,
    /// Un hook ou git a refusé : leur sortie.
    Failed(String),
    /// Le `.git` de la copie n'est plus celui que Solar a posé : rien n'est
    /// lancé dedans.
    Altered,
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
    let Ok(Some(pin)) = CopyGit::of(copy) else {
        return Err(CommitError::Altered);
    };
    let read = |args: &[&str]| read_git_at(copy, Some(&pin), args);
    if read(&["check-ref-format", "--branch", branch]).is_err() {
        return Err(CommitError::BadBranch);
    }
    let current = read(&["branch", "--show-current"]).map_err(failed)?;
    if current != branch {
        if read(&[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .is_ok()
        {
            return Err(CommitError::BranchExists);
        }
        // `switch -c` garde les fichiers de la copie tels quels.
        read(&["switch", "--quiet", "-c", branch]).map_err(failed)?;
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
            pin.pin(&mut Command::new("git"))
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
        read(&["rev-parse", "HEAD"]).map_err(failed)
    })();
    let _ = std::fs::remove_file(&index);
    result
}

/// `name`, ou `name-2`, `name-3`… : le premier nom libre dans le dépôt.
pub fn free_branch(project: &Path, name: &str) -> String {
    (1..)
        .map(|n| match n {
            1 => name.to_string(),
            n => format!("{name}-{n}"),
        })
        .find(|candidate| branch_head(project, candidate).is_none())
        .unwrap_or_else(|| name.to_string())
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
    /// Le `.git` de la copie n'est plus celui que Solar a posé.
    Altered,
}

impl Gh {
    /// Pousse la branche de mission sur `origin`, avec le compte actif de `gh`
    /// (son aide d'identification, le temps de cette commande), jamais de force.
    pub fn push(&self, copy: &Path, branch: &str) -> Result<(), PushError> {
        let helper = format!(
            "credential.helper=!'{}' auth git-credential",
            self.program.display()
        );
        let Ok(Some(pin)) = CopyGit::of(copy) else {
            return Err(PushError::Altered);
        };
        let output = pin
            .pin(&mut Command::new("git"))
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

/// Une pull request ouverte par Solar.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PullRequest {
    pub number: u32,
    pub url: String,
    /// Ouverte, mergée ou fermée, à la dernière lecture.
    #[serde(default)]
    pub state: PrState,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrState {
    #[default]
    Open,
    Merged,
    Closed,
}

/// Ce que Solar demande à `gh pr create`.
#[derive(Clone, Debug)]
pub struct NewPr {
    pub head: String,
    pub base: String,
    pub title: String,
    pub body: String,
    pub draft: bool,
}

/// Un segment d'adresse : seuls lettres, chiffres et `-._~` passent tels quels.
fn encode(segment: &str) -> String {
    segment
        .bytes()
        .map(|byte| match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

impl Gh {
    /// La branche existe-t-elle sur GitHub ?
    pub fn branch_exists(&self, dir: &Path, repo: &Repo, branch: &str) -> bool {
        self.run(
            dir,
            &[
                "api",
                &format!("repos/{}/branches/{}", repo.name, encode(branch)),
                "--hostname",
                &repo.host,
                "--silent",
            ],
        )
        .is_ok()
    }

    /// Ouvre la pull request de la branche `head` (déjà poussée) vers `base` ;
    /// le corps passe par l'entrée standard.
    pub fn create_pr(&self, dir: &Path, repo: &Repo, pr: &NewPr) -> Result<PullRequest, String> {
        let target = format!("{}/{}", repo.host, repo.name);
        let mut args = vec![
            "pr",
            "create",
            "--repo",
            &target,
            "--head",
            &pr.head,
            "--base",
            &pr.base,
            "--title",
            &pr.title,
            "--body-file",
            "-",
        ];
        if pr.draft {
            args.push("--draft");
        }
        let body = &pr.body;
        // `gh` lance git dans la copie : sur son dépôt épinglé, jamais ailleurs.
        let pin = CopyGit::of(dir).map_err(|error| error.to_string())?;
        let mut command = Command::new(&self.program);
        if let Some(pin) = &pin {
            pin.pin(&mut command);
        }
        let mut child = command
            .args(&args)
            .current_dir(dir)
            .env("GH_PROMPT_DISABLED", "1")
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| error.to_string())?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin
                .write_all(body.as_bytes())
                .map_err(|error| error.to_string())?;
        }
        let output = child
            .wait_with_output()
            .map_err(|error| error.to_string())?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        let url = stdout
            .lines()
            .rev()
            .find(|line| line.contains("/pull/"))
            .map(str::trim)
            .ok_or_else(|| format!("gh pr create : adresse introuvable dans « {stdout} »"))?;
        let number = url
            .rsplit('/')
            .next()
            .and_then(|n| n.parse().ok())
            .ok_or_else(|| format!("gh pr create : numéro introuvable dans {url}"))?;
        Ok(PullRequest {
            number,
            url: url.to_string(),
            state: PrState::Open,
        })
    }
}

impl Gh {
    /// L'état de la PR `number` sur GitHub.
    pub fn pr_state(&self, dir: &Path, repo: &Repo, number: u32) -> Result<PrState, String> {
        let view = self
            .run(
                dir,
                &[
                    "pr",
                    "view",
                    &number.to_string(),
                    "--repo",
                    &format!("{}/{}", repo.host, repo.name),
                    "--json",
                    "state",
                ],
            )
            .map_err(failed_text)?;
        let view: Value =
            serde_json::from_str(&view).map_err(|error| format!("gh pr view : {error}"))?;
        Ok(match view["state"].as_str() {
            Some("MERGED") => PrState::Merged,
            Some("CLOSED") => PrState::Closed,
            _ => PrState::Open,
        })
    }

    /// Ferme la PR `number`, sans supprimer sa branche.
    pub fn close_pr(&self, dir: &Path, repo: &Repo, number: u32) -> Result<(), String> {
        self.run(
            dir,
            &[
                "pr",
                "close",
                &number.to_string(),
                "--repo",
                &format!("{}/{}", repo.host, repo.name),
            ],
        )
        .map(|_| ())
        .map_err(failed_text)
    }
}

fn failed_text(reason: Reason) -> String {
    match reason {
        Reason::Failed(error) => error,
        other => format!("{other:?}"),
    }
}

/// Retire les séquences de couleur ANSI (`ESC [ … m`).
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Où en est un check, d'après le `bucket` de `gh pr checks`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bucket {
    Pass,
    Fail,
    Pending,
    Skipping,
    Cancel,
}

/// Un check d'une pull request.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub name: String,
    pub workflow: String,
    pub bucket: Bucket,
    pub link: String,
    /// Sa durée, une fois fini.
    pub seconds: Option<i64>,
}

impl Check {
    pub fn from_json(value: &Value) -> Self {
        let text = |key: &str| value[key].as_str().unwrap_or_default().to_string();
        let time = |key: &str| {
            value[key]
                .as_str()
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                // GitHub note « 0001-01-01 » pour un check pas encore fini.
                .filter(|at| at.timestamp() > 0)
        };
        let bucket = match value["bucket"].as_str() {
            Some("pass") => Bucket::Pass,
            Some("fail") => Bucket::Fail,
            Some("skipping") => Bucket::Skipping,
            Some("cancel") => Bucket::Cancel,
            _ => Bucket::Pending,
        };
        Self {
            name: text("name"),
            workflow: text("workflow"),
            bucket,
            link: text("link"),
            seconds: time("startedAt")
                .zip(time("completedAt"))
                .map(|(start, end)| (end - start).num_seconds()),
        }
    }
}

/// Une issue ouverte, telle que la rend `gh issue list --json`.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub number: u64,
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub url: String,
    pub created_at: String,
}

impl Gh {
    /// Les issues ouvertes du dépôt du projet qui portent l'étiquette `label`
    /// (les 100 plus récentes).
    pub fn issues(&self, dir: &Path, label: &str) -> Result<Vec<Issue>, String> {
        let text = self
            .run(
                dir,
                &[
                    "issue",
                    "list",
                    "--label",
                    label,
                    "--state",
                    "open",
                    "--limit",
                    "100",
                    "--json",
                    "number,title,body,url,createdAt",
                ],
            )
            .map_err(|reason| match reason {
                Reason::NoGh => "gh".to_string(),
                Reason::Failed(error) => error,
                other => format!("{other:?}"),
            })?;
        serde_json::from_str(&text).map_err(|error| format!("gh issue list : {error}"))
    }

    /// Les checks de la PR `number`. `gh` sort avec le code 8 tant que certains
    /// sont en cours : ce n'est pas une erreur. Sans check, une liste vide.
    pub fn checks(&self, dir: &Path, repo: &Repo, number: u32) -> Result<Vec<Check>, String> {
        let output = Command::new(&self.program)
            .args(["pr", "checks", &number.to_string(), "--repo"])
            .arg(format!("{}/{}", repo.host, repo.name))
            .args([
                "--json",
                "name,state,bucket,link,workflow,startedAt,completedAt",
            ])
            .current_dir(dir)
            .stdin(Stdio::null())
            .env("GH_PROMPT_DISABLED", "1")
            .env("NO_COLOR", "1")
            .output()
            .map_err(|error| error.to_string())?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no checks reported") {
            return Ok(Vec::new());
        }
        if !output.status.success() && output.status.code() != Some(8) {
            return Err(stderr.trim().to_string());
        }
        let checks: Vec<Value> = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("gh pr checks : {error}"))?;
        Ok(checks.iter().map(Check::from_json).collect())
    }

    /// Les `lines` dernières lignes du journal des étapes en échec d'un check
    /// GitHub Actions (l'identifiant du job est dans son lien).
    pub fn failed_log(
        &self,
        dir: &Path,
        repo: &Repo,
        check: &Check,
        lines: usize,
    ) -> Result<String, String> {
        let job = check
            .link
            .split("/job/")
            .nth(1)
            .map(|rest| rest.split(['/', '?', '#']).next().unwrap_or_default())
            .filter(|job| !job.is_empty() && job.bytes().all(|b| b.is_ascii_digit()))
            .ok_or_else(|| format!("pas de journal GitHub Actions pour {}", check.name))?;
        let log = self
            .run(
                dir,
                &[
                    "run",
                    "view",
                    "--job",
                    job,
                    "--log-failed",
                    "--repo",
                    &format!("{}/{}", repo.host, repo.name),
                ],
            )
            .map_err(|reason| match reason {
                Reason::Failed(error) => error,
                other => format!("{other:?}"),
            })?;
        // « job<TAB>étape<TAB>horodatage ligne » : seule la ligne compte, sans
        // les codes de couleur ni les marqueurs de groupe de GitHub Actions.
        let all: Vec<String> = log
            .lines()
            .map(|line| {
                let line = line.splitn(3, '\t').last().unwrap_or(line);
                let line = line.trim_start_matches('\u{feff}');
                let line = match line.split_once(' ') {
                    Some((stamp, rest)) if chrono::DateTime::parse_from_rfc3339(stamp).is_ok() => {
                        rest
                    }
                    _ => line,
                };
                let line = strip_ansi(line);
                ["##[group]", "##[error]", "##[warning]"]
                    .iter()
                    .fold(line, |line, marker| line.replace(marker, ""))
            })
            .filter(|line| !line.starts_with("##[endgroup]"))
            .collect();
        Ok(all[all.len().saturating_sub(lines)..].join("\n"))
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
    let fine = |part: &str| {
        !part.is_empty()
            && !part.starts_with('-')
            && !part.starts_with('.')
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    (fine(host) && fine(owner) && fine(repo) && parts.next().is_none())
        .then(|| (host.to_string(), format!("{owner}/{repo}")))
}
