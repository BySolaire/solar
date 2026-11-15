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
    store.restore(&base, &reviewed, &paths).unwrap();
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

    match store.restore(&base, &reviewed, &["a.txt".into()]) {
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
        .restore(&base, &reviewed, &["untracked.txt".into()])
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
