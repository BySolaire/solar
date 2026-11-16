//! La fiche d'un projet : conventions, état git, copies de travail.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use solar_core::checkpoint::{self, Store};
use solar_core::project::{SaveError, conventions, copies, git_summary, logo, save};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("solar-project-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::canonicalize(dir).unwrap()
}

fn git(dir: &Path, args: &[&str]) {
    Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
}

#[test]
fn conventions_show_the_usual_two_and_any_other_present() {
    let root = temp("conventions");
    fs::write(root.join("AGENTS.md"), "# Règles\n").unwrap();
    fs::write(root.join(".cursorrules"), "x").unwrap();
    let found: Vec<(&str, bool)> = conventions(&root)
        .into_iter()
        .map(|c| (c.path, c.file.is_some()))
        .collect();
    assert_eq!(
        found,
        [
            ("CLAUDE.md", false),
            ("AGENTS.md", true),
            (".cursorrules", true)
        ]
    );
}

#[test]
fn a_convention_changed_elsewhere_is_not_overwritten() {
    let root = temp("save");
    let file = root.join(".claude/CLAUDE.md");
    save(&file, None, "v1").unwrap();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "v1",
        "créé avec son dossier"
    );
    // Un agent écrit le fichier pendant qu'il est ouvert dans la fiche.
    fs::write(&file, "v2 par l'agent").unwrap();
    assert!(matches!(
        save(&file, Some("v1"), "v1 + moi"),
        Err(SaveError::ChangedElsewhere)
    ));
    assert_eq!(fs::read_to_string(&file).unwrap(), "v2 par l'agent");
    save(&file, Some("v2 par l'agent"), "v3").unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "v3");
}

#[test]
fn git_state_and_mission_copies_are_read() {
    let (repo, store_dir, other) = (temp("repo"), temp("store"), temp("other"));
    assert_eq!(git_summary(&repo), None, "pas encore un dépôt");
    git(&repo, &["init", "-q", "-b", "main"]);
    fs::write(repo.join("a.txt"), "1").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "Premier commit"]);
    fs::write(repo.join("a.txt"), "2").unwrap();
    fs::write(repo.join("b.txt"), "neuf").unwrap();
    let summary = git_summary(&repo).unwrap();
    assert_eq!(summary.branch.as_deref(), Some("main"));
    assert_eq!((summary.changed, summary.untracked), (1, 1));
    assert_eq!(summary.last_commit.unwrap().1, "Premier commit");
    let index = fs::metadata(repo.join(".git/index"))
        .unwrap()
        .modified()
        .unwrap();

    let store = Store::for_project(&store_dir, &repo);
    let (_, dir) = store.isolate("m1").unwrap().unwrap();
    // Une copie d'un autre dépôt, rangée au même endroit, n'est pas de ce projet.
    git(&other, &["init", "-q"]);
    fs::write(other.join("c.txt"), "c").unwrap();
    git(&other, &["add", "."]);
    git(&other, &["commit", "-qm", "c"]);
    Store::for_project(&store_dir, &other)
        .isolate("m2")
        .unwrap()
        .unwrap();

    let found = copies(&checkpoint::worktrees(&store_dir), &repo);
    assert_eq!(found.len(), 1);
    assert!(dir.starts_with(&found[0].root) && found[0].bytes > 0);
    assert_eq!(
        fs::metadata(repo.join(".git/index"))
            .unwrap()
            .modified()
            .unwrap(),
        index,
        "la lecture ne touche pas l'index de l'utilisateur"
    );
}

#[test]
fn the_project_logo_is_a_square_icon_app_icon_first() {
    let root = temp("logo");
    let svg = |w: u32, h: u32| {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}"/>"#)
    };
    assert_eq!(logo(&root), None, "pas de logo : le dossier reste");
    // Un mot-symbole, trop large pour une icône, n'est jamais choisi.
    fs::create_dir_all(root.join("assets/brand")).unwrap();
    fs::write(root.join("assets/brand/logo-wordmark.svg"), svg(400, 60)).unwrap();
    fs::write(root.join("assets/brand/horizon-logo.svg"), svg(400, 100)).unwrap();
    assert_eq!(logo(&root), None);
    fs::write(root.join("logo.svg"), svg(24, 24)).unwrap();
    assert_eq!(logo(&root), Some(root.join("logo.svg")));
    fs::create_dir_all(root.join("public")).unwrap();
    fs::write(root.join("public/favicon.svg"), svg(32, 32)).unwrap();
    assert_eq!(
        logo(&root),
        Some(root.join("public/favicon.svg")),
        "le favicon avant le logo"
    );
    fs::write(root.join("assets/brand/app-icon.svg"), svg(64, 64)).unwrap();
    assert_eq!(
        logo(&root),
        Some(root.join("assets/brand/app-icon.svg")),
        "l'icône d'application d'abord"
    );
}

#[test]
fn solar_finds_its_own_app_icon() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let found = logo(&root).expect("le kit d'identité de Solar");
    // Son icône d'application, en PNG de 64 pixels.
    let name = found.file_name().unwrap().to_string_lossy();
    assert!(
        name == "solar-app-icon-64.png" || name == "icon_32x32@2x.png",
        "{}",
        found.display()
    );
}
