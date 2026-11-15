//! Lecture d'un vrai fichier de session Claude Code, réduit et nettoyé.

use solar_core::claude::Event;
use solar_core::history::parse;

#[test]
fn session_file_becomes_the_thread_without_meta_or_subagent_lines() {
    let (title, events) = parse(include_str!("fixtures/history.jsonl"));

    assert_eq!(title.as_deref(), Some("Fichier note.txt"));
    let [
        Event::User(asked),
        Event::ToolStart { name, .. },
        Event::ToolResult {
            is_error: false, ..
        },
        Event::Thinking,
        Event::Text(answer),
    ] = events.as_slice()
    else {
        panic!("fil inattendu : {events:#?}");
    };
    assert!(asked.starts_with("Crée le fichier note.txt"));
    assert_eq!(name, "Write");
    assert_eq!(answer, "fait.");
}
