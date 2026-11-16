use solar_core::agent::Provider;
use solar_core::state::{Mission, MissionStatus, Project, Session, Settings, State};

fn temp_file(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("solar-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("nested/state.json")
}

#[test]
fn state_survives_a_save_and_load() {
    let path = temp_file("roundtrip");
    let state = State {
        projects: vec![Project {
            path: "/work/solar".into(),
            sessions: vec![Session {
                id: "abc".into(),
                title: "Première session".into(),
                mission: Some(Mission {
                    status: MissionStatus::Ready,
                    plans: vec!["## Plan\n1. Lire".into()],
                    launched_mode: None,
                    checkpoint: None,
                    accepted: None,
                    reviews: Vec::new(),
                    attempt: 2,
                    next_attempt: None,
                    worktree: Some("/home/solar/copie".into()),
                }),
                provider: Provider::Codex,
            }],
        }],
        open_tabs: vec!["abc".into()],
        settings: Settings {
            provider: Provider::Grok,
            mode: Some("acceptEdits".into()),
            claude_model: Some("opus".into()),
            claude_effort: None,
            language: Some("fr".into()),
            favorite_models: vec!["claude:opus".into()],
            glass: true,
            turn_line_above: true,
        },
    };

    assert_eq!(State::load(&path), State::default(), "fichier absent");
    state.save(&path).unwrap();
    assert_eq!(State::load(&path), state);
}

#[test]
fn corrupt_file_is_kept_aside_not_overwritten() {
    let path = temp_file("corrupt");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{ pas du json").unwrap();

    assert_eq!(State::load(&path), State::default());
    let kept = std::fs::read_to_string(path.with_extension("json.bad")).unwrap();
    assert_eq!(kept, "{ pas du json");
}

#[test]
fn a_file_from_before_missions_still_loads_and_is_backed_up() {
    let path = temp_file("before-missions");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let old = r#"{"projects":[{"path":"/work","sessions":[{"id":"abc","title":"Ancienne"}]}],"open_tabs":["abc"]}"#;
    std::fs::write(&path, old).unwrap();

    let mut state = State::load(&path);
    assert_eq!(state.projects[0].sessions[0].mission, None);
    // Une mission d'avant les tentatives est la tentative 1.
    let older: solar_core::state::Mission =
        serde_json::from_str(r#"{"status":"running"}"#).unwrap();
    assert_eq!(older.attempt, 1);
    state.projects[0].sessions[0].mission = Some(Mission::default());
    state.save(&path).unwrap();

    let backup = path.with_extension("json.bak");
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), old);
    assert_eq!(State::load(&path), state);
    // La copie n'est faite qu'une fois : elle garde l'état d'avant les missions.
    state.save(&path).unwrap();
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), old);
}
