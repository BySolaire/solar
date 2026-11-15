use solar_core::state::{Project, Session, State};

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
            }],
        }],
        open_tabs: vec!["abc".into()],
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
