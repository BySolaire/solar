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
    // Ni chemin détourné vers une autre adresse de l'API, ni caractère exotique.
    assert_eq!(parsed("https://github.com/../app"), None);
    assert_eq!(parsed("https://github.com/owner/a%2Fb"), None);
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

// ---- Commit et push d'une mission (ticket 07) ----

use solar_core::checkpoint::Store;
use solar_core::github::{
    CommitError, PushError, branch_name, commit_mission, free_branch, launch_state,
};

#[test]
fn a_mission_branch_is_named_after_its_title_and_attempt() {
    assert_eq!(
        branch_name("Connexion par e-mail", 1),
        "solar/connexion-par-e-mail"
    );
    assert_eq!(
        branch_name("Ajoute l’accueil !", 2),
        "solar/ajoute-l-accueil-2"
    );
    assert_eq!(branch_name("   ", 1), "solar/mission");
    let long = branch_name(&"très long titre ".repeat(10), 1);
    assert!(
        long.len() <= "solar/".len() + 48 && !long.ends_with('-'),
        "{long}"
    );
}

fn git_in(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Un dépôt nu servant de GitHub, un projet qui y pousse `main`, une
/// modification non commitée de l'utilisateur, puis la copie d'une mission.
struct Fixture {
    root: PathBuf,
    project: PathBuf,
    copy: PathBuf,
    start: String,
}

fn fixture(name: &str) -> Fixture {
    let root = temp(name);
    let origin = root.join("origin.git");
    git_in(
        &root,
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "main",
            origin.to_str().unwrap(),
        ],
    );
    let project = root.join("app");
    fs::create_dir_all(&project).unwrap();
    git_in(&project, &["init", "-q", "-b", "main"]);
    // Les commits de Solar prennent l'identité de l'utilisateur.
    git_in(&project, &["config", "user.name", "Utilisateur"]);
    git_in(&project, &["config", "user.email", "u@example.com"]);
    for (file, text) in [("a.txt", "a\n"), ("b.txt", "b\n"), ("gone.txt", "g\n")] {
        fs::write(project.join(file), text).unwrap();
    }
    git_in(&project, &["add", "."]);
    git_in(&project, &["commit", "-qm", "init"]);
    git_in(
        &project,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    git_in(&project, &["push", "-q", "origin", "main"]);
    let start = git_in(&project, &["rev-parse", "HEAD"]);
    // Le travail de l'utilisateur, non commité, avant le lancement.
    fs::write(project.join("a.txt"), "a + utilisateur\n").unwrap();
    let launch = launch_state(&project).unwrap();
    assert_eq!(launch.start, start);
    assert_eq!(launch.branch.as_deref(), Some("main"));
    assert_eq!(launch.dirty, ["a.txt"]);
    let store = Store::for_project(&root.join("checkpoints"), &project);
    let (_, copy) = store.isolate("m1").unwrap().unwrap();
    // La mission touche le fichier de l'utilisateur, en modifie un, en crée un
    // et en supprime un.
    fs::write(copy.join("a.txt"), "a + utilisateur + mission\n").unwrap();
    fs::write(copy.join("b.txt"), "b + mission\n").unwrap();
    fs::write(copy.join("c.txt"), "c\n").unwrap();
    fs::remove_file(copy.join("gone.txt")).unwrap();
    Fixture {
        root,
        project,
        copy,
        start,
    }
}

fn files(list: &[&str]) -> Vec<String> {
    list.iter().map(|file| file.to_string()).collect()
}

#[test]
fn the_commit_holds_only_the_chosen_files_on_the_start_commit() {
    let f = fixture("commit");
    let user_status = git_in(&f.project, &["status", "--porcelain"]);
    let user_branches = git_in(&f.project, &["branch", "--show-current"]);

    let sha = commit_mission(
        &f.copy,
        "solar/essai",
        &files(&["b.txt", "c.txt", "gone.txt"]),
        "Mission : essai",
    )
    .unwrap();

    assert_eq!(git_in(&f.copy, &["rev-parse", "HEAD^"]), f.start);
    assert_eq!(
        git_in(&f.copy, &["show", "--name-status", "--format=", &sha]),
        "M\tb.txt\nA\tc.txt\nD\tgone.txt"
    );
    assert_eq!(git_in(&f.copy, &["show", &format!("{sha}:a.txt")]), "a");
    assert_eq!(
        git_in(&f.copy, &["log", "-1", "--format=%s|%an", &sha]),
        "Mission : essai|Utilisateur"
    );
    // Le projet de l'utilisateur ne bouge pas : branche, index, fichiers.
    assert_eq!(
        git_in(&f.project, &["branch", "--show-current"]),
        user_branches
    );
    assert_eq!(git_in(&f.project, &["status", "--porcelain"]), user_status);
    assert_eq!(
        fs::read_to_string(f.project.join("a.txt")).unwrap(),
        "a + utilisateur\n"
    );

    // Une correction : un second commit sur la même branche.
    fs::write(f.copy.join("b.txt"), "b + mission, corrigé\n").unwrap();
    let second = commit_mission(&f.copy, "solar/essai", &files(&["b.txt"]), "Correction").unwrap();
    assert_eq!(git_in(&f.copy, &["rev-parse", &format!("{second}^")]), sha);
    // Rien de neuf : rien n'est commité.
    assert!(matches!(
        commit_mission(&f.copy, "solar/essai", &files(&["b.txt"]), "Rien"),
        Err(CommitError::Nothing)
    ));
}

#[test]
fn a_failing_hook_stops_the_commit_and_says_why() {
    let f = fixture("hook");
    let hook = f.project.join(".git/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\necho 'lint KO : b.txt' >&2\nexit 1\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let head = git_in(&f.copy, &["rev-parse", "HEAD"]);
    match commit_mission(&f.copy, "solar/essai", &files(&["b.txt"]), "Essai") {
        Err(CommitError::Failed(output)) => assert!(output.contains("lint KO"), "{output}"),
        other => panic!("le hook aurait dû bloquer : {other:?}"),
    }
    assert_eq!(git_in(&f.copy, &["rev-parse", "HEAD"]), head);
}

#[test]
fn the_branch_is_pushed_and_never_forced() {
    let f = fixture("push");
    let gh = Gh::at(f.root.join("gh-absent"));
    let sha = commit_mission(&f.copy, "solar/essai", &files(&["b.txt"]), "Essai").unwrap();
    gh.push(&f.copy, "solar/essai").unwrap();
    let origin = f.root.join("origin.git");
    assert_eq!(
        git_in(&origin, &["rev-parse", "refs/heads/solar/essai"]),
        sha
    );

    // Quelqu'un pousse sur la même branche depuis ailleurs.
    let other = f.root.join("other");
    git_in(
        &f.root,
        &[
            "clone",
            "-q",
            "-b",
            "solar/essai",
            origin.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    fs::write(other.join("d.txt"), "d\n").unwrap();
    git_in(&other, &["add", "."]);
    git_in(&other, &["commit", "-qm", "ailleurs"]);
    git_in(&other, &["push", "-q", "origin", "solar/essai"]);
    let theirs = git_in(&other, &["rev-parse", "HEAD"]);

    fs::write(f.copy.join("b.txt"), "b encore\n").unwrap();
    commit_mission(&f.copy, "solar/essai", &files(&["b.txt"]), "Encore").unwrap();
    assert!(matches!(
        gh.push(&f.copy, "solar/essai"),
        Err(PushError::Moved)
    ));
    assert_eq!(
        git_in(&origin, &["rev-parse", "refs/heads/solar/essai"]),
        theirs,
        "rien n'est écrasé"
    );
}

#[test]
fn a_branch_already_in_the_repository_is_neither_reused_nor_proposed() {
    let f = fixture("taken");
    git_in(&f.project, &["branch", "solar/essai"]);
    assert!(matches!(
        commit_mission(&f.copy, "solar/essai", &files(&["b.txt"]), "Essai"),
        Err(CommitError::BranchExists)
    ));
    assert_eq!(free_branch(&f.project, "solar/essai"), "solar/essai-2");
    assert_eq!(free_branch(&f.project, "solar/libre"), "solar/libre");
}

#[test]
fn a_branch_name_git_refuses_is_refused_before_anything() {
    let f = fixture("bad-branch");
    assert!(matches!(
        commit_mission(&f.copy, "solar/a..b", &files(&["b.txt"]), "Essai"),
        Err(CommitError::BadBranch)
    ));
}

// ---- Pull request (ticket 08) ----

use solar_core::github::{NewPr, PullRequest};

fn sample_repo() -> Repo {
    Repo {
        host: "github.com".into(),
        name: "BySolaire/app".into(),
        default_branch: "main".into(),
        account: "BySolaire".into(),
        url: "https://github.com/BySolaire/app".into(),
    }
}

/// Un faux `gh` pour les PR : `pr create` note ses arguments et son entrée
/// standard puis rend une adresse ; `api` réussit pour la branche `develop`.
fn fake_gh_pr(dir: &Path) -> Gh {
    let log = dir.join("gh.log");
    let script = format!(
        "#!/bin/sh\necho \"ARGS $*\" >> '{log}'\ncase \"$1 $2\" in\n  'pr create') echo 'STDIN' >> '{log}'; cat >> '{log}'; echo 'Creating pull request…' >&2; echo 'https://github.com/BySolaire/app/pull/12' ;;\n  'api repos/BySolaire/app/branches/develop') echo '{{}}' ;;\n  'api '*) echo 'gh: Branch not found (HTTP 404)' >&2; exit 1 ;;\n  *) exit 2 ;;\nesac\n",
        log = log.display()
    );
    let path = dir.join("gh");
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    Gh::at(path)
}

#[test]
fn a_pull_request_is_opened_with_its_texts_and_read_back() {
    let dir = temp("pr");
    let gh = fake_gh_pr(&dir);
    let pr = gh
        .create_pr(
            &dir,
            &sample_repo(),
            &NewPr {
                head: "solar/essai".into(),
                base: "main".into(),
                title: "Connexion par e-mail".into(),
                body: "## Plan\n1. Lire".into(),
                draft: true,
            },
        )
        .unwrap();
    assert_eq!(
        pr,
        PullRequest {
            number: 12,
            url: "https://github.com/BySolaire/app/pull/12".into(),
            state: PrState::Open,
        }
    );
    let log = fs::read_to_string(dir.join("gh.log")).unwrap();
    assert!(
        log.contains("ARGS pr create --repo github.com/BySolaire/app --head solar/essai --base main --title Connexion par e-mail --body-file - --draft"),
        "{log}"
    );
    assert!(
        log.contains("STDIN\n## Plan\n1. Lire"),
        "le corps passe par l'entrée standard : {log}"
    );
}

#[test]
fn the_base_branch_is_checked_on_github() {
    let dir = temp("branch");
    let gh = fake_gh_pr(&dir);
    assert!(gh.branch_exists(&dir, &sample_repo(), "develop"));
    assert!(!gh.branch_exists(&dir, &sample_repo(), "locale"));
}

// ---- Checks (ticket 09) ----

use solar_core::github::{Bucket, Check};

/// Un faux `gh` pour les checks : `pr checks` rend `checks` avec le code
/// `code`, `run view --job 42 --log-failed` rend un long journal.
fn fake_gh_checks(dir: &Path, checks: &str, code: i32) -> Gh {
    let log = dir.join("gh.log");
    // Le format de `gh run view --log-failed` : job, étape, horodatage, ligne.
    let journal: String = (1..=30)
        .map(|n| format!("tests\\tRun cargo test\\t2026-11-16T10:01:{n:02}.0000000Z ligne {n}\\n"))
        .collect();
    // Comme dans un vrai journal : marque d'ordre, couleurs, marqueurs de groupe.
    let journal = format!(
        "tests\\tRun\\t\\xef\\xbb\\xbf2026-11-16T10:00:00.4883141Z ##[group]Run \\033[36;1m./check.sh\\033[0m\\ntests\\tRun\\t2026-11-16T10:00:00.4883141Z ##[endgroup]\\n{journal}"
    );
    let script = format!(
        "#!/bin/sh\necho \"ARGS $*\" >> '{log}'\ncase \"$1 $2\" in\n  'pr checks') printf '%s' '{checks}'; exit {code} ;;\n  'run view') printf '{journal}' ;;\n  *) exit 2 ;;\nesac\n",
        log = log.display()
    );
    let path = dir.join("gh");
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    Gh::at(path)
}

const CHECKS: &str = r#"[{"name":"build","state":"SUCCESS","bucket":"pass","link":"https://github.com/BySolaire/app/actions/runs/7/job/41","workflow":"CI","startedAt":"2026-11-16T10:00:00Z","completedAt":"2026-11-16T10:02:10Z"},{"name":"tests","state":"FAILURE","bucket":"fail","link":"https://github.com/BySolaire/app/actions/runs/7/job/42","workflow":"CI","startedAt":"2026-11-16T10:00:00Z","completedAt":"2026-11-16T10:01:42Z"},{"name":"lint","state":"IN_PROGRESS","bucket":"pending","link":"https://github.com/BySolaire/app/actions/runs/7/job/43","workflow":"CI","startedAt":"2026-11-16T10:00:00Z","completedAt":"0001-01-01T00:00:00Z"}]"#;

#[test]
fn checks_are_read_even_while_some_are_pending() {
    let dir = temp("checks");
    // Code 8 : des checks sont encore en cours ; ce n'est pas une erreur.
    let gh = fake_gh_checks(&dir, CHECKS, 8);
    let checks = gh.checks(&dir, &sample_repo(), 12).unwrap();
    let summary: Vec<(&str, Bucket, Option<i64>)> = checks
        .iter()
        .map(|check| (check.name.as_str(), check.bucket, check.seconds))
        .collect();
    assert_eq!(
        summary,
        [
            ("build", Bucket::Pass, Some(130)),
            ("tests", Bucket::Fail, Some(102)),
            ("lint", Bucket::Pending, None),
        ]
    );
    let log = fs::read_to_string(dir.join("gh.log")).unwrap();
    assert!(
        log.contains("pr checks 12 --repo github.com/BySolaire/app --json"),
        "{log}"
    );
}

#[test]
fn a_failed_check_gives_the_end_of_its_log() {
    let dir = temp("failed-log");
    let gh = fake_gh_checks(&dir, CHECKS, 1);
    let checks: Vec<Check> = serde_json::from_str::<Vec<serde_json::Value>>(CHECKS)
        .unwrap()
        .into_iter()
        .map(|value| Check::from_json(&value))
        .collect();
    let whole = gh
        .failed_log(&dir, &sample_repo(), &checks[1], 100)
        .unwrap();
    assert!(
        whole.starts_with("Run ./check.sh\nligne 1\n"),
        "nettoyé : {whole}"
    );
    let log = gh.failed_log(&dir, &sample_repo(), &checks[1], 20).unwrap();
    assert_eq!(log.lines().count(), 20);
    assert!(
        log.starts_with("ligne 11") && log.ends_with("ligne 30"),
        "{log}"
    );
    let calls = fs::read_to_string(dir.join("gh.log")).unwrap();
    assert!(
        calls.contains("run view --job 42 --log-failed --repo github.com/BySolaire/app"),
        "{calls}"
    );
    // Un check sans job GitHub Actions n'a pas de journal à lire.
    let mut other = checks[1].clone();
    other.link = "https://ci.example.com/build/9".into();
    assert!(gh.failed_log(&dir, &sample_repo(), &other, 20).is_err());
}

#[test]
fn a_pull_request_without_checks_has_an_empty_list() {
    let dir = temp("no-checks");
    let path = dir.join("gh");
    fs::write(
        &path,
        "#!/bin/sh\necho \"no checks reported on the 'solar/essai' branch\" >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        Gh::at(path).checks(&dir, &sample_repo(), 12).unwrap(),
        Vec::new()
    );
}

// ---- Fin de vie (ticket 10) ----

use solar_core::github::PrState;

fn fake_gh_state(dir: &Path, state: &str) -> Gh {
    let log = dir.join("gh.log");
    let script = format!(
        "#!/bin/sh\necho \"ARGS $*\" >> '{log}'\ncase \"$1 $2\" in\n  'pr view') printf '%s' '{{\"state\":\"{state}\"}}' ;;\n  'pr close') echo '✓ Closed pull request #12' ;;\n  *) exit 2 ;;\nesac\n",
        log = log.display()
    );
    let path = dir.join("gh");
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    Gh::at(path)
}

#[test]
fn the_state_of_a_pull_request_is_read() {
    for (json, state) in [
        ("OPEN", PrState::Open),
        ("MERGED", PrState::Merged),
        ("CLOSED", PrState::Closed),
    ] {
        let dir = temp(&format!("state-{json}"));
        let gh = fake_gh_state(&dir, json);
        assert_eq!(gh.pr_state(&dir, &sample_repo(), 12).unwrap(), state);
    }
}

#[test]
fn closing_a_pull_request_never_deletes_its_branch() {
    let dir = temp("close");
    let gh = fake_gh_state(&dir, "OPEN");
    gh.close_pr(&dir, &sample_repo(), 12).unwrap();
    let calls = fs::read_to_string(dir.join("gh.log")).unwrap();
    assert!(
        calls.contains("pr close 12 --repo github.com/BySolaire/app"),
        "{calls}"
    );
    assert!(!calls.contains("--delete-branch"), "{calls}");
}

/// L'agent réécrit, avec de simples fichiers texte, le `.git` de sa copie
/// vers un dossier à lui dont la configuration lance une commande.
fn alter_copy(f: &Fixture) -> PathBuf {
    let marker = f.root.join("FSMON");
    let head = git_in(&f.project, &["rev-parse", "HEAD"]);
    fs::write(f.copy.join(".git"), "gitdir: ./x\n").unwrap();
    fs::create_dir_all(f.copy.join("x/refs/heads")).unwrap();
    fs::create_dir_all(f.copy.join("x/objects/info")).unwrap();
    fs::write(f.copy.join("x/HEAD"), format!("{head}\n")).unwrap();
    fs::write(
        f.copy.join("x/objects/info/alternates"),
        format!("{}\n", f.project.join(".git/objects").display()),
    )
    .unwrap();
    fs::write(
        f.copy.join("x/config"),
        format!(
            "[core]\n\trepositoryformatversion = 0\n\tbare = false\n\tfsmonitor = touch {}\n\thooksPath = h\n",
            marker.display()
        ),
    )
    .unwrap();
    marker
}

#[test]
fn an_altered_copy_runs_nothing_and_publishes_nothing() {
    let f = fixture("altered");
    let marker = alter_copy(&f);

    let listed = solar_core::files::list(&f.copy);
    assert!(
        !marker.exists(),
        "la liste des fichiers a lancé la commande"
    );
    assert!(listed.contains(&"b.txt".to_string()));

    assert!(matches!(
        commit_mission(&f.copy, "solar/essai", &files(&["b.txt"]), "Essai"),
        Err(CommitError::Altered)
    ));
    assert!(!marker.exists(), "le commit a lancé la commande");
    let gh = Gh::at("/nonexistent/gh");
    assert!(matches!(
        gh.push(&f.copy, "solar/essai"),
        Err(PushError::Altered)
    ));
    assert!(!marker.exists());
}

#[test]
fn a_copy_whose_git_is_a_folder_is_refused() {
    // L'agent remplace le fichier `.git` par un dossier : ce n'est plus la
    // copie que Solar a faite.
    let f = fixture("git-folder");
    fs::remove_file(f.copy.join(".git")).unwrap();
    fs::create_dir_all(f.copy.join(".git")).unwrap();
    assert!(matches!(
        commit_mission(&f.copy, "solar/essai", &files(&["b.txt"]), "Essai"),
        Err(CommitError::Altered)
    ));
}
