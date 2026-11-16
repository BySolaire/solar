//! GitHub par `gh` : un projet peut-il publier ? Dépôts git jetables et faux
//! `gh` qui rejoue des réponses enregistrées.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use solar_core::github::{Gh, Reason, Repo, remote_repo};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("solar-github-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::canonicalize(dir).unwrap()
}

/// Un dépôt avec un commit et, si donné, un remote `origin`.
fn repo(name: &str, origin: Option<&str>) -> PathBuf {
    let dir = temp(name).join("app");
    fs::create_dir_all(&dir).unwrap();
    let git = |args: &[&str]| {
        Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap()
    };
    git(&["init", "-q", "-b", "main"]);
    fs::write(dir.join("a.txt"), "a\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "init"]);
    if let Some(url) = origin {
        git(&["remote", "add", "origin", url]);
    }
    dir
}

const AUTH: &str = r#"{"hosts":{"github.com":[{"state":"success","active":true,"host":"github.com","login":"BySolaire"},{"state":"success","active":false,"host":"github.com","login":"Autre"}]}}"#;

/// Un faux `gh` : `auth status` rend `auth`, `repo view` rend `repo` (ou échoue
/// si vide) ; chaque appel est noté dans `gh.log`.
fn fake_gh(dir: &Path, auth: &str, repo: &str) -> Gh {
    let log = dir.join("gh.log");
    let script = format!(
        "#!/bin/sh\necho \"$*\" >> '{log}'\ncase \"$1 $2\" in\n  'auth status') printf '%s' '{auth}' ;;\n  'repo view') [ -n '{repo}' ] || {{ echo 'GraphQL: Could not resolve to a Repository' >&2; exit 1; }}; printf '%s' '{repo}' ;;\n  *) exit 2 ;;\nesac\n",
        log = log.display()
    );
    let path = dir.join("gh");
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    Gh::at(path)
}

fn repo_json(permission: &str, fork: bool) -> String {
    format!(
        r#"{{"nameWithOwner":"BySolaire/app","defaultBranchRef":{{"name":"main"}},"viewerPermission":"{permission}","isFork":{fork},"url":"https://github.com/BySolaire/app"}}"#
    )
}

#[test]
fn origin_urls_name_their_host_and_repository() {
    let parsed = |url: &str| remote_repo(url);
    let github = Some(("github.com".to_string(), "BySolaire/app".to_string()));
    assert_eq!(parsed("https://github.com/BySolaire/app.git"), github);
    assert_eq!(parsed("https://github.com/BySolaire/app"), github);
    assert_eq!(parsed("git@github.com:BySolaire/app.git"), github);
    assert_eq!(parsed("ssh://git@github.com/BySolaire/app.git"), github);
    assert_eq!(
        parsed("https://ghe.example.com/team/app.git"),
        Some(("ghe.example.com".to_string(), "team/app".to_string()))
    );
    assert_eq!(parsed("/srv/git/app.git"), None, "un chemin local");
    assert_eq!(parsed("https://github.com/BySolaire"), None, "sans dépôt");
    // Rien qui puisse passer pour une option de `gh`.
    assert_eq!(parsed("https://-x.example/owner/app"), None);
    assert_eq!(parsed("git@github.com:-owner/app.git"), None);
}

#[test]
fn a_pushable_github_repository_is_ready_with_the_active_account() {
    let project = repo("ready", Some("https://github.com/BySolaire/app.git"));
    let gh = fake_gh(project.parent().unwrap(), AUTH, &repo_json("WRITE", false));
    assert_eq!(
        gh.availability(&project),
        Ok(Repo {
            host: "github.com".into(),
            name: "BySolaire/app".into(),
            default_branch: "main".into(),
            account: "BySolaire".into(),
            url: "https://github.com/BySolaire/app".into(),
        })
    );
    let log = fs::read_to_string(project.parent().unwrap().join("gh.log")).unwrap();
    assert!(!log.contains("--show-token"), "aucun jeton lu : {log}");
    assert!(log.contains("repo view github.com/BySolaire/app"), "{log}");
}

#[test]
fn each_reason_not_to_publish_is_told() {
    // Sans `gh` sur le PATH.
    let project = repo("no-gh", Some("https://github.com/BySolaire/app.git"));
    assert_eq!(
        Gh::at(project.join("absent-gh")).availability(&project),
        Err(Reason::NoGh)
    );
    // Sans remote `origin`.
    let project = repo("no-origin", None);
    let gh = fake_gh(project.parent().unwrap(), AUTH, &repo_json("WRITE", false));
    assert_eq!(gh.availability(&project), Err(Reason::NoOrigin));
    // Un remote qui n'est pas sur GitHub (hôte inconnu de `gh`).
    let project = repo("gitlab", Some("https://gitlab.com/team/app.git"));
    let gh = fake_gh(project.parent().unwrap(), AUTH, &repo_json("WRITE", false));
    assert_eq!(
        gh.availability(&project),
        Err(Reason::NotGitHub("gitlab.com".into()))
    );
    // `gh` connecté à aucun compte pour github.com.
    let project = repo("logged-out", Some("git@github.com:BySolaire/app.git"));
    let gh = fake_gh(
        project.parent().unwrap(),
        r#"{"hosts":{}}"#,
        &repo_json("WRITE", false),
    );
    assert_eq!(
        gh.availability(&project),
        Err(Reason::LoggedOut("github.com".into()))
    );
    // Un fork, puis un dépôt où le compte ne peut que lire.
    let project = repo("fork", Some("https://github.com/BySolaire/app.git"));
    let gh = fake_gh(project.parent().unwrap(), AUTH, &repo_json("WRITE", true));
    assert_eq!(gh.availability(&project), Err(Reason::Fork));
    let project = repo("read-only", Some("https://github.com/BySolaire/app.git"));
    let gh = fake_gh(project.parent().unwrap(), AUTH, &repo_json("READ", false));
    assert_eq!(
        gh.availability(&project),
        Err(Reason::ReadOnly("BySolaire".into()))
    );
    // Un dépôt introuvable avec ce compte : l'erreur de `gh` est rendue telle quelle.
    let project = repo("missing", Some("https://github.com/BySolaire/app.git"));
    let gh = fake_gh(project.parent().unwrap(), AUTH, "");
    assert!(matches!(
        gh.availability(&project),
        Err(Reason::Failed(message)) if message.contains("Could not resolve")
    ));
}
