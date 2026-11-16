//! Les bots : ce qui est rangé dans l'état, et la consigne envoyée à chaque
//! exécution.

use std::collections::BTreeMap;

use solar_core::agent::Provider;
use solar_core::bots::{Bot, Prompt, PromptError, Run, Scope, Trigger, prompt};
use solar_core::library::{Origin, parse};
use solar_core::state::State;

fn bot(prompt: Prompt) -> Bot {
    Bot {
        id: "b1".into(),
        name: "Revue des dépendances".into(),
        project: "/work/solar".into(),
        trigger: Trigger::Manual,
        prompt,
        provider: Provider::Codex,
        model: Some("gpt-5".into()),
        effort: None,
        mode: "acceptEdits".into(),
        scope: Scope::Mission,
        enabled: true,
        runs: Vec::new(),
    }
}

#[test]
fn an_old_state_has_no_bots_and_bots_survive_a_save() {
    // Un fichier d'avant les bots : ni bots, ni marque sur les sessions.
    let old = r#"{"projects":[{"path":"/p","sessions":[{"id":"s","title":"T"}]}],"open_tabs":[]}"#;
    let state: State = serde_json::from_str(old).unwrap();
    assert!(state.bots.is_empty());
    assert_eq!(state.projects[0].sessions[0].bot, None);
    // Sans bot, rien de neuf n'est écrit.
    assert!(!serde_json::to_string(&state).unwrap().contains("bot"));

    let mut state = state;
    let mut b = bot(Prompt::Text("Relis les dépendances.".into()));
    b.runs.push(Run {
        at: "2026-11-16T09:00:00+01:00".into(),
        session: Some("s".into()),
        problem: None,
    });
    state.bots.push(b);
    state.projects[0].sessions[0].bot = Some("b1".into());
    let text = serde_json::to_string(&state).unwrap();
    assert_eq!(serde_json::from_str::<State>(&text).unwrap(), state);
}

#[test]
fn the_prompt_is_the_text_or_the_filled_template() {
    let templates = [parse(
        "changelog",
        "Mets à jour le changelog pour {version}, depuis {depuis}.",
        Origin::Personal,
        None,
    )];
    assert_eq!(
        prompt(&bot(Prompt::Text("  Relis tout.\n".into())), &templates),
        Ok("Relis tout.".to_string())
    );
    assert_eq!(
        prompt(&bot(Prompt::Text("  ".into())), &templates),
        Err(PromptError::Empty)
    );

    let mut values = BTreeMap::new();
    values.insert("version".to_string(), "1.2".to_string());
    values.insert("depuis".to_string(), "lundi".to_string());
    let filled = Prompt::Template {
        name: "changelog".into(),
        values: values.clone(),
    };
    assert_eq!(
        prompt(&bot(filled), &templates),
        Ok("Mets à jour le changelog pour 1.2, depuis lundi.".to_string())
    );

    // Un trou vide : le bot n'envoie pas une consigne à moitié remplie.
    values.insert("depuis".to_string(), " ".to_string());
    let partial = Prompt::Template {
        name: "changelog".into(),
        values,
    };
    assert_eq!(
        prompt(&bot(partial), &templates),
        Err(PromptError::Unfilled(vec!["depuis".to_string()]))
    );

    // Le modèle a été supprimé ou renommé depuis.
    let gone = Prompt::Template {
        name: "disparu".into(),
        values: BTreeMap::new(),
    };
    assert_eq!(
        prompt(&bot(gone), &templates),
        Err(PromptError::MissingTemplate("disparu".to_string()))
    );
}

#[test]
fn a_bot_keeps_its_recent_runs_only() {
    let mut b = bot(Prompt::Text("x".into()));
    for i in 0..30 {
        b.record(Run {
            at: format!("{i}"),
            session: None,
            problem: None,
        });
    }
    assert_eq!(b.runs.len(), 20);
    assert_eq!(b.runs.first().unwrap().at, "10");
    assert_eq!(b.last_run().unwrap().at, "29");
}
