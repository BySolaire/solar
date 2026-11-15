//! Le pilote Codex face à de vrais tours enregistrés (`codex exec --json`, puis
//! son fichier de session), chemins remplacés.

use serde_json::json;
use solar_core::claude::Event;
use solar_core::codex::{Parser, parse_history};

#[test]
fn a_real_turn_becomes_the_same_events_as_claude() {
    let mut parser = Parser::default();
    let events: Vec<Event> = include_str!("fixtures/codex_exec.jsonl")
        .lines()
        .flat_map(|line| parser.line(line))
        .collect();

    assert!(matches!(&events[0], Event::Ready { session_id, .. }
        if session_id == "01a1e144-6e16-7f33-9166-ab952f996dd7"));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::Text(t) if t.contains("ignoring"))),
        "les avertissements de configuration ne sont pas du texte"
    );
    let tools: Vec<(&str, &str)> = events
        .iter()
        .filter_map(|event| match event {
            Event::ToolStart { name, input, .. } => Some((
                name.as_str(),
                input["command"]
                    .as_str()
                    .or(input["file_path"].as_str())
                    .unwrap_or_default(),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(tools.len(), 4, "{tools:?}");
    assert_eq!(tools[1], ("Edit", "/work/app/calc.py"));
    assert_eq!(tools[2], ("Write", "/work/app/test_calc.py"));
    assert_eq!(tools[3], ("Bash", "python3 -B -m unittest test_calc.py"));
    // La commande qui échoue (assertion fausse) est une erreur, les autres non.
    let results: Vec<bool> = events
        .iter()
        .filter_map(|event| match event {
            Event::ToolResult { is_error, .. } => Some(*is_error),
            _ => None,
        })
        .collect();
    assert_eq!(results, [true, false, false, false]);
    assert!(matches!(
        events.last(),
        Some(Event::TurnDone {
            error: None,
            context_tokens: 191596,
            ..
        })
    ));
    assert!(parser.done && parser.ok);
    assert!(
        parser
            .last_message
            .starts_with("`calc.py` utilise désormais")
    );
}

#[test]
fn a_failed_turn_is_an_error_reported_once() {
    let mut parser = Parser::default();
    let events: Vec<Event> = [
        r#"{"type":"error","message":"stream disconnected"}"#,
        r#"{"type":"turn.failed","error":{"message":"stream disconnected"}}"#,
    ]
    .iter()
    .flat_map(|line| parser.line(line))
    .collect();
    assert_eq!(
        events,
        [Event::TurnDone {
            error: Some("stream disconnected".into()),
            context_tokens: 0,
            cost_usd: 0.0
        }]
    );
    assert!(!parser.ok);
}

#[test]
fn a_saved_session_is_read_back_in_order() {
    let events = parse_history(include_str!("fixtures/codex_rollout.jsonl"));
    let kinds: Vec<String> = events
        .iter()
        .map(|event| match event {
            Event::User(_) => "user".into(),
            Event::Text(_) => "text".into(),
            Event::ToolStart { name, .. } => name.clone(),
            Event::ToolResult { .. } => "result".into(),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "user", "text", "Bash", "result", "Bash", "result", "text", "user", "text", "text",
            "Bash", "result", "Edit", "result", "Write", "result"
        ]
    );
    assert_eq!(
        events[0],
        Event::User("Lis calc.py et dis en une phrase s'il y a un bogue. Lance aussi 'ls'.".into())
    );
    assert!(
        matches!(&events[2], Event::ToolStart { input, .. } if *input == json!({"command": "ls"}))
    );
    assert!(matches!(
        &events[3],
        Event::ToolResult {
            at: Some(_),
            is_error: false,
            ..
        }
    ));
}
