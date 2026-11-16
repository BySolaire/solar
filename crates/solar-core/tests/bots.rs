//! Les bots : ce qui est rangé dans l'état, et la consigne envoyée à chaque
//! exécution.

use std::collections::BTreeMap;

use solar_core::agent::Provider;
use solar_core::bots::{
    Bot, Prompt, PromptError, Run, Scope, Trigger, new_issues, prompt, prompt_for,
};
use solar_core::github::Issue;
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
        checked: None,
        missed: 0,
        handled: Vec::new(),
        daily_cap: 3,
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
        issue: Some(12),
        cost: None,
        skipped: false,
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
            issue: None,
            cost: None,
            skipped: false,
        });
    }
    assert_eq!(b.runs.len(), 20);
    assert_eq!(b.runs.first().unwrap().at, "10");
    assert_eq!(b.last_run().unwrap().at, "29");
}

fn at(text: &str) -> chrono::NaiveDateTime {
    chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").unwrap()
}

#[test]
fn a_daily_schedule_runs_next_at_its_time_skipping_weekends_if_asked() {
    use solar_core::bots::next_due;
    let daily = Trigger::Daily {
        time: "09:00".into(),
        weekdays_only: false,
    };
    // Lundi 16 novembre 2026.
    assert_eq!(
        next_due(&daily, at("2026-11-16 08:59")),
        Some(at("2026-11-16 09:00"))
    );
    // À l'heure pile, c'est passé : demain.
    assert_eq!(
        next_due(&daily, at("2026-11-16 09:00")),
        Some(at("2026-11-17 09:00"))
    );
    let weekdays = Trigger::Daily {
        time: "09:00".into(),
        weekdays_only: true,
    };
    // Vendredi après l'heure : lundi.
    assert_eq!(
        next_due(&weekdays, at("2026-11-20 10:00")),
        Some(at("2026-11-23 09:00"))
    );
    // Une heure mal écrite ne déclenche rien ; le manuel non plus.
    let broken = Trigger::Daily {
        time: "25:00".into(),
        weekdays_only: false,
    };
    assert_eq!(next_due(&broken, at("2026-11-16 08:00")), None);
    assert_eq!(next_due(&Trigger::Manual, at("2026-11-16 08:00")), None);
}

#[test]
fn an_interval_runs_at_fixed_hours_from_midnight() {
    use solar_core::bots::next_due;
    let every = Trigger::Every { hours: 6 };
    assert_eq!(
        next_due(&every, at("2026-11-16 05:30")),
        Some(at("2026-11-16 06:00"))
    );
    assert_eq!(
        next_due(&every, at("2026-11-16 18:00")),
        Some(at("2026-11-17 00:00"))
    );
    // Cinq heures : 0, 5, 10, 15, 20, puis minuit.
    let five = Trigger::Every { hours: 5 };
    assert_eq!(
        next_due(&five, at("2026-11-16 20:10")),
        Some(at("2026-11-17 00:00"))
    );
    assert_eq!(
        next_due(&Trigger::Every { hours: 0 }, at("2026-11-16 20:10")),
        None
    );
}

#[test]
fn runs_due_in_a_span_are_counted_without_running_them_all() {
    use solar_core::bots::due_between;
    let daily = Trigger::Daily {
        time: "09:00".into(),
        weekdays_only: true,
    };
    // Fermé du vendredi 18 h au lundi 10 h : seul lundi 9 h compte.
    assert_eq!(
        due_between(&daily, at("2026-11-20 18:00"), at("2026-11-23 10:00")),
        1
    );
    // Fermé du jeudi 8 h au lundi 10 h : jeudi, vendredi, lundi.
    assert_eq!(
        due_between(&daily, at("2026-11-19 08:00"), at("2026-11-23 10:00")),
        3
    );
    assert_eq!(
        due_between(&daily, at("2026-11-23 10:00"), at("2026-11-23 10:05")),
        0
    );
    // Une longue absence est plafonnée, sans tout parcourir.
    let hourly = Trigger::Every { hours: 1 };
    assert_eq!(
        due_between(&hourly, at("2025-01-01 00:00"), at("2026-11-16 10:00")),
        100
    );
}

#[test]
fn a_scheduled_bot_keeps_its_trigger_and_what_it_missed() {
    let mut b = bot(Prompt::Text("x".into()));
    b.trigger = Trigger::Daily {
        time: "09:00".into(),
        weekdays_only: true,
    };
    b.checked = Some("2026-11-16T09:00:00+01:00".into());
    b.missed = 2;
    let text = serde_json::to_string(&b).unwrap();
    assert_eq!(serde_json::from_str::<Bot>(&text).unwrap(), b);
    // Un bot du ticket précédent, sans ces champs, se relit.
    let old = r#"{"id":"b","name":"n","project":"/p","trigger":{"kind":"manual"},"prompt":{"text":"x"},"mode":"acceptEdits","enabled":true}"#;
    let old: Bot = serde_json::from_str(old).unwrap();
    assert_eq!((old.checked, old.missed), (None, 0));
}

fn issue(number: u64, created: &str) -> Issue {
    Issue {
        number,
        title: format!("Bug {number}"),
        body: "Le bouton ne répond pas.".into(),
        url: format!("https://github.com/o/r/issues/{number}"),
        created_at: created.into(),
    }
}

#[test]
fn only_new_unhandled_issues_are_taken_oldest_first() {
    let mut b = bot(Prompt::Text("x".into()));
    b.trigger = Trigger::Issues {
        label: "solar".into(),
        since: Some("2026-11-16T10:00:00Z".into()),
    };
    b.handled = vec![7];
    let found = vec![
        issue(9, "2026-11-16T12:00:00Z"),
        issue(7, "2026-11-16T11:00:00Z"),
        issue(3, "2026-11-01T09:00:00Z"),
        issue(8, "2026-11-16T11:30:00Z"),
    ];
    let numbers = |b: &Bot| {
        new_issues(b, &found)
            .iter()
            .map(|i| i.number)
            .collect::<Vec<_>>()
    };
    // L'ancienne (3) et la déjà traitée (7) restent de côté.
    assert_eq!(numbers(&b), [8, 9]);
    // « Aussi les issues déjà ouvertes » : pas de date de départ.
    b.trigger = Trigger::Issues {
        label: "solar".into(),
        since: None,
    };
    assert_eq!(numbers(&b), [3, 8, 9]);
}

#[test]
fn the_issue_fills_its_holes_or_follows_the_prompt() {
    let i = issue(12, "2026-11-16T12:00:00Z");
    // Un texte sans trou : l'issue suit la consigne.
    let text = prompt_for(&bot(Prompt::Text("Corrige ce bug.".into())), &[], Some(&i)).unwrap();
    assert!(text.starts_with("Corrige ce bug.\n\n"), "{text}");
    assert!(
        text.contains("#12 · Bug 12")
            && text.contains("issues/12")
            && text.contains("ne répond pas")
    );
    // Un modèle à trous d'issue : ils sont remplis, rien n'est ajouté.
    let templates = [parse(
        "issue",
        "Corrige #{issue_number} ({issue_title}) : {issue_body}",
        Origin::Personal,
        None,
    )];
    let filled = Prompt::Template {
        name: "issue".into(),
        values: BTreeMap::new(),
    };
    assert_eq!(
        prompt_for(&bot(filled.clone()), &templates, Some(&i)),
        Ok("Corrige #12 (Bug 12) : Le bouton ne répond pas.".to_string())
    );
    // Sans issue, ces trous restent vides : la consigne n'est pas complète.
    assert_eq!(
        prompt(&bot(filled), &templates),
        Err(PromptError::Unfilled(vec![
            "issue_number".into(),
            "issue_title".into(),
            "issue_body".into()
        ]))
    );
}

#[test]
fn issues_are_read_from_gh_as_json() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("solar-bots-gh-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("gh.log");
    let json = r#"[{"number":4,"title":"Crash","body":"Au démarrage","url":"https://github.com/o/r/issues/4","createdAt":"2026-11-16T12:00:00Z"}]"#;
    let script = format!(
        "#!/bin/sh\necho \"$*\" >> '{}'\nprintf '%s' '{json}'\n",
        log.display()
    );
    let gh = dir.join("gh");
    std::fs::write(&gh, script).unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    let found = solar_core::github::Gh::at(&gh)
        .issues(&dir, "bug urgent")
        .unwrap();
    assert_eq!(
        found,
        vec![Issue {
            number: 4,
            title: "Crash".into(),
            body: "Au démarrage".into(),
            url: "https://github.com/o/r/issues/4".into(),
            created_at: "2026-11-16T12:00:00Z".into(),
        }]
    );
    let args = std::fs::read_to_string(&log).unwrap();
    assert!(
        args.contains("issue list --label bug urgent --state open"),
        "{args}"
    );
}

#[test]
fn a_trigger_is_skipped_while_busy_or_once_the_daily_cap_is_reached() {
    use solar_core::bots::{Decision, decide};
    let mut b = bot(Prompt::Text("x".into()));
    assert_eq!(b.daily_cap, 3, "trois par jour par défaut");
    let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
    let run = |at: &str, session: Option<&str>, problem: Option<&str>| Run {
        at: at.into(),
        session: session.map(String::from),
        problem: problem.map(String::from),
        issue: None,
        cost: None,
        skipped: false,
    };
    assert_eq!(decide(&b, false, today), Decision::Go);
    assert_eq!(decide(&b, true, today), Decision::Busy);
    // Hier et les échecs ne comptent pas ; deux parties aujourd'hui, pas encore trois.
    b.runs = vec![
        run("2026-11-15T09:00:00+01:00", Some("a"), None),
        run("2026-11-16T09:00:00+01:00", Some("b"), None),
        run("2026-11-16T10:00:00+01:00", None, Some("modèle disparu")),
        run("2026-11-16T11:00:00+01:00", Some("c"), None),
    ];
    assert_eq!(decide(&b, false, today), Decision::Go);
    b.runs
        .push(run("2026-11-16T12:00:00+01:00", Some("d"), None));
    assert_eq!(decide(&b, false, today), Decision::Capped);
    b.daily_cap = 5;
    assert_eq!(decide(&b, false, today), Decision::Go);
}

#[test]
fn a_repeated_skip_is_noted_once() {
    let mut b = bot(Prompt::Text("x".into()));
    b.skip("2026-11-16T09:00:00+01:00".into(), "occupé".into());
    b.skip("2026-11-16T09:05:00+01:00".into(), "occupé".into());
    assert_eq!(b.runs.len(), 1);
    assert!(b.runs[0].skipped);
    b.skip("2026-11-16T09:10:00+01:00".into(), "plafond".into());
    assert_eq!(b.runs.len(), 2);
    // Une vraie exécution entre deux : le saut suivant est noté.
    b.record(Run {
        at: "2026-11-16T10:00:00+01:00".into(),
        session: Some("s".into()),
        problem: None,
        issue: None,
        cost: None,
        skipped: false,
    });
    b.skip("2026-11-16T11:00:00+01:00".into(), "plafond".into());
    assert_eq!(b.runs.len(), 4);
}
