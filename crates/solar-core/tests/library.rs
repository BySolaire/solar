//! La bibliothèque : modèles de messages lus sur le disque, leurs trous et
//! leur remplissage. Dossiers jetables.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use solar_core::agent::Provider;
use solar_core::library::{Example, Origin, fill, holes, load, parse};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("solar-library-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_template_reads_its_header_and_body() {
    let text = "---\ntitle: Corriger un bug\ndescription: Avec un test qui le prouve\nagent: codex\nmission: true\n---\nCorrige {bug} dans {fichier}.\n";
    let template = parse("bug", text, Origin::Personal, None);
    assert_eq!(template.name, "bug");
    assert_eq!(template.title, "Corriger un bug");
    assert_eq!(template.description, "Avec un test qui le prouve");
    assert_eq!(template.agent, Some(Provider::Codex));
    assert!(template.mission);
    assert_eq!(template.body, "Corrige {bug} dans {fichier}.");
    assert_eq!(template.problem, None);

    // Sans en-tête : le nom sert de titre, tout est corps.
    let bare = parse("relire", "Relis {fichier}.", Origin::Personal, None);
    assert_eq!(
        (bare.title.as_str(), bare.body.as_str()),
        ("relire", "Relis {fichier}.")
    );
    assert!(!bare.mission && bare.agent.is_none());

    // Un en-tête cassé est signalé, le texte reste utilisable.
    let broken = parse(
        "x",
        "---\ntitle Sans deux-points\n---\nTexte",
        Origin::Personal,
        None,
    );
    assert!(broken.problem.is_some());
    assert_eq!(broken.body, "Texte");
    // Un agent inconnu aussi.
    assert!(
        parse("y", "---\nagent: gemini\n---\nT", Origin::Personal, None)
            .problem
            .is_some()
    );
}

#[test]
fn holes_are_found_once_in_order_and_filled() {
    let body =
        "Corrige {bug} dans {fichier}, puis relis {fichier}. Garde {{accolades}}. $ARGUMENTS";
    assert_eq!(holes(body), ["bug", "fichier", "arguments"]);
    let values: HashMap<String, String> = [
        ("bug".to_string(), "le crash".to_string()),
        ("fichier".to_string(), "login.rs".to_string()),
        ("arguments".to_string(), "vite".to_string()),
    ]
    .into();
    assert_eq!(
        fill(body, &values),
        "Corrige le crash dans login.rs, puis relis login.rs. Garde {accolades}. vite"
    );
    // Un trou sans valeur disparaît ; une accolade seule reste telle quelle.
    assert_eq!(fill("a {x} b { c }", &HashMap::new()), "a  b { c }");
    assert!(holes("{ pas un trou }").is_empty());
}

#[test]
fn personal_templates_override_examples_of_the_same_name() {
    let dir = temp("merge");
    fs::write(dir.join("bug.md"), "---\ntitle: Mon bug\n---\nÀ moi").unwrap();
    fs::write(dir.join("notes.txt"), "ignoré").unwrap();
    fs::write(dir.join("relire.md"), "Relis {fichier}").unwrap();
    let examples = [
        Example {
            name: "bug".into(),
            text: "---\ntitle: Exemple\n---\nExemple".into(),
        },
        Example {
            name: "tests".into(),
            text: "Écris des tests".into(),
        },
    ];
    let found = load(&dir, None, &examples);
    let summary: Vec<(&str, &str, Origin)> = found
        .iter()
        .map(|t| (t.name.as_str(), t.title.as_str(), t.origin))
        .collect();
    assert_eq!(
        summary,
        [
            ("bug", "Mon bug", Origin::Personal),
            ("relire", "relire", Origin::Personal),
            ("tests", "tests", Origin::Example),
        ]
    );
    assert_eq!(found[0].path.as_deref(), Some(dir.join("bug.md").as_path()));
    // Un dossier absent : seulement les exemples.
    assert_eq!(load(&dir.join("absent"), None, &examples).len(), 2);
}

use solar_core::library::{DeleteError, delete, free_name, valid_name};

#[test]
fn project_templates_win_over_personal_ones() {
    let personal = temp("personal");
    let project = temp("project");
    fs::write(personal.join("bug.md"), "Personnel").unwrap();
    fs::create_dir_all(project.join(".solar/library")).unwrap();
    fs::write(project.join(".solar/library/bug.md"), "Du projet").unwrap();
    let found = load(&personal, Some(&project), &[]);
    assert_eq!(found.len(), 1);
    assert_eq!(
        (found[0].origin, found[0].body.as_str()),
        (Origin::Project, "Du projet")
    );
}

#[test]
fn template_names_are_plain_commands() {
    assert!(valid_name("corriger-bug_2"));
    for bad in ["", "Avec espace", "../x", "é", "a/b", "-tiret"] {
        assert!(!valid_name(bad), "{bad}");
    }
    let dir = temp("names");
    assert_eq!(free_name(&dir, "bug"), "bug");
    fs::write(dir.join("bug.md"), "x").unwrap();
    fs::write(dir.join("bug-2.md"), "x").unwrap();
    assert_eq!(free_name(&dir, "bug"), "bug-3");
}

#[test]
fn a_template_changed_elsewhere_is_not_deleted() {
    let dir = temp("delete");
    let file = dir.join("bug.md");
    fs::write(&file, "v1").unwrap();
    fs::write(&file, "v2 par ailleurs").unwrap();
    assert!(matches!(
        delete(&file, "v1"),
        Err(DeleteError::ChangedElsewhere)
    ));
    assert!(file.exists());
    delete(&file, "v2 par ailleurs").unwrap();
    assert!(!file.exists());
}
