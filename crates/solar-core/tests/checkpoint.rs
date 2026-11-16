use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use solar_core::checkpoint::{ChangeKind, RestoreError, Store};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("solar-checkpoint-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(root: &Path, file: &str, text: &str) {
    let path = root.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn read(root: &Path, file: &str) -> Option<String> {
    fs::read_to_string(root.join(file)).ok()
}

fn git(project: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(project)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_mission_is_seen_exactly_and_restored_without_touching_anything_else() {
    let (project, store_dir) = (temp("project"), temp("store"));
    write(&project, "src/lib.rs", "pub fn a() {}\n");
    write(&project, "notes.txt", "à garder\n");
    write(&project, "old.txt", "ancien\n");
    write(&project, "node_modules/pkg/index.js", "énorme\n");
    let store = Store::for_project(&store_dir, &project);

    let base = store.checkpoint().unwrap();
    // La mission : une modification, un ajout, une suppression.
    write(&project, "src/lib.rs", "pub fn a() {}\npub fn b() {}\n");
    write(&project, "src/new.rs", "// nouveau\n");
    fs::remove_file(project.join("old.txt")).unwrap();
    let reviewed = store.snapshot().unwrap();

    let changes = store.changes(&base, &reviewed).unwrap();
    let seen: Vec<(&str, ChangeKind)> = changes.iter().map(|c| (c.path.as_str(), c.kind)).collect();
    assert_eq!(
        seen,
        [
            ("old.txt", ChangeKind::Deleted),
            ("src/lib.rs", ChangeKind::Modified),
            ("src/new.rs", ChangeKind::Added),
        ]
    );
    assert_eq!(changes[1].lines, Some((1, 0)));
    let patch = store.patch(&base, &reviewed).unwrap();
    assert!(patch.contains("+pub fn b() {}"), "{patch}");
    assert!(
        !patch.contains("node_modules"),
        "les dépendances ne sont pas copiées"
    );

    // Après l'examen, l'utilisateur touche un autre fichier : il est préservé.
    write(&project, "notes.txt", "modifié par moi\n");
    let paths: Vec<String> = changes.iter().map(|c| c.path.clone()).collect();
    store.put(&base, &reviewed, &paths).unwrap();
    assert_eq!(
        read(&project, "src/lib.rs").as_deref(),
        Some("pub fn a() {}\n")
    );
    assert_eq!(read(&project, "src/new.rs"), None);
    assert_eq!(read(&project, "old.txt").as_deref(), Some("ancien\n"));
    assert_eq!(
        read(&project, "notes.txt").as_deref(),
        Some("modifié par moi\n")
    );
}

#[test]
fn restore_refuses_a_file_changed_since_review() {
    let (project, store_dir) = (temp("moved"), temp("moved-store"));
    write(&project, "a.txt", "1\n");
    let store = Store::for_project(&store_dir, &project);
    let base = store.checkpoint().unwrap();
    write(&project, "a.txt", "2\n");
    let reviewed = store.snapshot().unwrap();
    write(&project, "a.txt", "3, après l'examen\n");

    match store.put(&base, &reviewed, &["a.txt".into()]) {
        Err(RestoreError::ChangedSinceReview(paths)) => assert_eq!(paths, ["a.txt"]),
        other => panic!("attendu un refus, obtenu {other:?}"),
    }
    assert_eq!(
        read(&project, "a.txt").as_deref(),
        Some("3, après l'examen\n")
    );
}

#[test]
fn the_users_own_repository_is_never_touched() {
    let (project, store_dir) = (temp("repo"), temp("repo-store"));
    git(&project, &["init", "-q"]);
    write(&project, "tracked.txt", "v1\n");
    git(&project, &["add", "tracked.txt"]);
    write(&project, "tracked.txt", "v2, non indexé\n");
    write(&project, "untracked.txt", "neuf\n");
    let status = git(&project, &["status", "--porcelain"]);
    let index = fs::read(project.join(".git/index")).unwrap();

    let store = Store::for_project(&store_dir, &project);
    let base = store.checkpoint().unwrap();
    write(&project, "untracked.txt", "changé\n");
    let reviewed = store.snapshot().unwrap();
    store
        .put(&base, &reviewed, &["untracked.txt".into()])
        .unwrap();

    assert_eq!(git(&project, &["status", "--porcelain"]), status);
    assert_eq!(fs::read(project.join(".git/index")).unwrap(), index);
    assert_eq!(
        read(&project, "tracked.txt").as_deref(),
        Some("v2, non indexé\n")
    );
    let patch = store.patch(&base, &reviewed).unwrap();
    assert!(!patch.contains(".git/"), "le dossier .git n'est pas copié");
}

#[test]
fn a_mission_copy_starts_from_the_project_and_comes_back_only_without_conflict() {
    let (repo, store_dir) = (temp("wt-repo"), temp("wt-store"));
    git(&repo, &["init", "-q"]);
    write(&repo, "app/a.txt", "v1\n");
    write(&repo, "app/gone.txt", "bientôt supprimé\n");
    git(&repo, &["add", "."]);
    git(
        &repo,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "un",
        ],
    );
    let project = repo.join("app");
    // Ce qui n'est pas commité part aussi dans la copie.
    write(&project, "a.txt", "v2, non commité\n");
    fs::remove_file(project.join("gone.txt")).unwrap();
    write(&project, "new.txt", "neuf\n");
    let branches = git(&repo, &["branch", "--list"]);

    let store = Store::for_project(&store_dir, &project);
    let (base, dir) = store.isolate("m1").unwrap().expect("dépôt git");
    assert!(!dir.starts_with(fs::canonicalize(&repo).unwrap()));
    assert!(dir.to_string_lossy().ends_with("/app"), "{}", dir.display());
    assert_eq!(read(&dir, "a.txt").as_deref(), Some("v2, non commité\n"));
    assert_eq!(read(&dir, "gone.txt"), None);
    assert_eq!(read(&dir, "new.txt").as_deref(), Some("neuf\n"));

    // La mission travaille dans sa copie ; le projet ne bouge pas.
    write(&dir, "a.txt", "v3, par la mission\n");
    write(&dir, "tool.sh", "#!/bin/sh\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir.join("tool.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    }
    let copy = store.in_worktree(&dir);
    let reviewed = copy.snapshot().unwrap();
    let paths: Vec<String> = copy
        .changes(&base, &reviewed)
        .unwrap()
        .into_iter()
        .map(|change| change.path)
        .collect();
    assert_eq!(paths, ["a.txt", "tool.sh"]);
    assert_eq!(
        read(&project, "a.txt").as_deref(),
        Some("v2, non commité\n")
    );

    // Une seconde mission en parallèle touche le même fichier que l'utilisateur.
    let (base2, dir2) = store.isolate("m2").unwrap().unwrap();
    write(&dir2, "new.txt", "par la mission 2\n");
    let reviewed2 = store.in_worktree(&dir2).snapshot().unwrap();

    store.put(&reviewed, &base, &paths).unwrap();
    assert_eq!(
        read(&project, "a.txt").as_deref(),
        Some("v3, par la mission\n")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(project.join("tool.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "le script reste exécutable");
    }

    write(&project, "new.txt", "changé par l'utilisateur\n");
    match store.put(&reviewed2, &base2, &["new.txt".into()]) {
        Err(RestoreError::ChangedSinceReview(files)) => assert_eq!(files, ["new.txt"]),
        other => panic!("conflit attendu, eu {other:?}"),
    }
    assert_eq!(
        read(&project, "new.txt").as_deref(),
        Some("changé par l'utilisateur\n")
    );

    store.remove_worktree(&dir).unwrap();
    store.remove_worktree(&dir2).unwrap();
    store.remove_worktree(&dir2).unwrap();
    assert!(!dir.exists() && !dir2.exists());
    assert_eq!(git(&repo, &["worktree", "list"]).lines().count(), 1);
    assert_eq!(
        git(&repo, &["branch", "--list"]),
        branches,
        "aucune branche créée"
    );
}

#[test]
fn a_folder_without_git_gets_no_copy() {
    let (project, store_dir) = (temp("plain"), temp("plain-store"));
    write(&project, "a.txt", "v1\n");
    let store = Store::for_project(&store_dir, &project);
    assert!(store.isolate("m").unwrap().is_none());
}

#[test]
fn applying_a_copy_never_writes_through_a_link_of_the_project() {
    // Le projet suit un lien qui sort de lui ; dans sa copie, l'agent le
    // remplace par un fichier. Appliquer remplace le lien, jamais sa cible.
    let (root, store_dir) = (temp("link-out"), temp("link-out-store"));
    let project = root.join("project");
    write(&project, "a.txt", "a\n");
    write(&root, "outside.txt", "ORIGINAL");
    std::os::unix::fs::symlink("../outside.txt", project.join("config")).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["add", "-A"]);
    git(
        &project,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "i",
        ],
    );
    let store = Store::for_project(&store_dir, &project);
    let (base, copy) = store.isolate("m").unwrap().unwrap();

    fs::remove_file(copy.join("config")).unwrap();
    write(&copy, "config", "PWNED");
    let reviewed = store.in_worktree(&copy).snapshot().unwrap();
    let paths: Vec<String> = store
        .changes(&base, &reviewed)
        .unwrap()
        .into_iter()
        .map(|change| change.path)
        .collect();
    store.put(&reviewed, &base, &paths).unwrap();

    assert_eq!(read(&root, "outside.txt").as_deref(), Some("ORIGINAL"));
    let config = project.join("config");
    assert!(
        !fs::symlink_metadata(&config)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(read(&project, "config").as_deref(), Some("PWNED"));
}

#[test]
fn a_link_made_in_the_copy_comes_back_as_a_link() {
    let (project, store_dir) = (temp("link-back"), temp("link-back-store"));
    write(&project, "target.txt", "cible\n");
    git(&project, &["init", "-q"]);
    git(&project, &["add", "-A"]);
    git(
        &project,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "i",
        ],
    );
    let store = Store::for_project(&store_dir, &project);
    let (base, copy) = store.isolate("m").unwrap().unwrap();

    std::os::unix::fs::symlink("target.txt", copy.join("link")).unwrap();
    let reviewed = store.in_worktree(&copy).snapshot().unwrap();
    store.put(&reviewed, &base, &["link".to_string()]).unwrap();

    let link = project.join("link");
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_link(&link).unwrap(), Path::new("target.txt"));
}

#[test]
fn nothing_is_written_under_a_linked_folder_of_the_project() {
    // Un dossier du projet qui est un lien vers l'extérieur : rien n'est
    // écrit à travers lui, l'écriture est refusée.
    let (root, store_dir) = (temp("dir-link"), temp("dir-link-store"));
    let project = root.join("project");
    write(&project, "a.txt", "a\n");
    write(&root, "outside/keep.txt", "keep");
    git(&project, &["init", "-q"]);
    git(&project, &["add", "-A"]);
    git(
        &project,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "i",
        ],
    );
    let store = Store::for_project(&store_dir, &project);
    let (base, copy) = store.isolate("m").unwrap().unwrap();
    write(&copy, "sub/evil.txt", "PWNED");
    let reviewed = store.in_worktree(&copy).snapshot().unwrap();
    // Entre la revue et l'application, `sub` devient un lien dans le projet.
    std::os::unix::fs::symlink("../outside", project.join("sub")).unwrap();

    assert!(
        store
            .put(&reviewed, &base, &["sub/evil.txt".to_string()])
            .is_err()
    );
    assert!(!root.join("outside/evil.txt").exists());
}

#[test]
fn removing_a_copy_never_deletes_what_its_git_points_to() {
    // L'agent réécrit le `.git` de sa copie pour désigner un dossier
    // extérieur : seul le dossier de la copie est retiré.
    let (root, store_dir) = (temp("rm-out"), temp("rm-out-store"));
    let project = root.join("project");
    write(&project, "app/a.txt", "a\n");
    write(&root, "victim/keep.txt", "keep");
    git(&project, &["init", "-q"]);
    git(&project, &["add", "-A"]);
    git(
        &project,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "i",
        ],
    );
    let store = Store::for_project(&store_dir, &project.join("app"));
    let (_, copy) = store.isolate("m").unwrap().unwrap();
    let top = copy.parent().unwrap().to_path_buf();
    fs::remove_file(top.join(".git")).unwrap();
    write(&top, ".git/HEAD", "ref: refs/heads/x\n");
    fs::create_dir_all(top.join(".git/objects")).unwrap();
    fs::create_dir_all(top.join(".git/refs")).unwrap();
    let victim = fs::canonicalize(root.join("victim")).unwrap();
    write(
        &top,
        ".git/config",
        &format!(
            "[core]\n\trepositoryformatversion = 0\n\tbare = false\n\tworktree = {}\n",
            victim.display()
        ),
    );

    store.remove_worktree(&copy).unwrap();

    assert_eq!(read(&root, "victim/keep.txt").as_deref(), Some("keep"));
    assert!(!top.exists());
}

#[test]
fn a_folder_outside_the_copies_is_never_removed() {
    let (root, store_dir) = (temp("rm-elsewhere"), temp("rm-elsewhere-store"));
    let project = root.join("project");
    write(&project, "a.txt", "a\n");
    git(&project, &["init", "-q"]);
    git(&project, &["add", "-A"]);
    git(
        &project,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "i",
        ],
    );
    let store = Store::for_project(&store_dir, &project);
    store.isolate("m").unwrap().unwrap();
    // Un chemin de copie altéré dans l'état : le projet lui-même.
    assert!(store.remove_worktree(&project).is_err());
    assert_eq!(read(&project, "a.txt").as_deref(), Some("a\n"));
}

#[test]
fn an_agent_in_a_copy_gets_its_pinned_repository_and_never_an_altered_one() {
    use solar_core::checkpoint::pin_for_agent;
    let (project, store_dir) = (temp("agent-pin"), temp("agent-pin-store"));
    write(&project, "a.txt", "a\n");
    git(&project, &["init", "-q"]);
    git(&project, &["add", "-A"]);
    git(
        &project,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "i",
        ],
    );
    let store = Store::for_project(&store_dir, &project);
    let (_, copy) = store.isolate("m").unwrap().unwrap();
    let env = |command: &Command, key: &str| {
        command
            .get_envs()
            .find(|(name, _)| *name == key)
            .and_then(|(_, value)| value.map(PathBuf::from))
    };

    // Le projet lui-même : rien d'épinglé.
    let mut plain = Command::new("true");
    pin_for_agent(&project, &store_dir, &mut plain).unwrap();
    assert_eq!(env(&plain, "GIT_DIR"), None);

    // Une copie intacte : son dossier d'administration, hors de la copie.
    let mut pinned = Command::new("true");
    pin_for_agent(&copy, &store_dir, &mut pinned).unwrap();
    let git_dir = env(&pinned, "GIT_DIR").unwrap();
    assert!(!git_dir.starts_with(&copy));
    assert_eq!(
        env(&pinned, "GIT_WORK_TREE"),
        Some(fs::canonicalize(&copy).unwrap())
    );

    // Une copie dont le `.git` est devenu un dossier : refusée.
    fs::remove_file(copy.join(".git")).unwrap();
    fs::create_dir_all(copy.join(".git")).unwrap();
    assert!(pin_for_agent(&copy, &store_dir, &mut Command::new("true")).is_err());
}
