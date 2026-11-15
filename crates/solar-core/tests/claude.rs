//! `parse_line` face à des lignes enregistrées lors d'une vraie session du CLI
//! (mode « demander », création d'un fichier), plus deux lignes invalides.

use serde_json::json;
use solar_core::claude::{Event, parse_line};

#[test]
fn real_session_becomes_events_and_unknown_lines_are_skipped() {
    let events: Vec<Event> = include_str!("fixtures/write_file.jsonl")
        .lines()
        .flat_map(parse_line)
        .collect();

    let input = json!({"file_path": "/work/note.txt", "content": "ok"});
    let [
        Event::Ready {
            session_id,
            model,
            permission_mode,
        },
        Event::ToolStart {
            id,
            name,
            input: started,
        },
        Event::PermissionRequest {
            request_id,
            tool,
            input: asked,
        },
        Event::ToolResult {
            id: result_id,
            is_error: false,
            text,
            at: None,
        },
        Event::Thinking,
        Event::Text(first),
        Event::Text(second),
        Event::TurnDone {
            error: None,
            context_tokens: 108_514,
            cost_usd,
        },
    ] = events.as_slice()
    else {
        panic!("suite d'événements inattendue : {events:#?}");
    };

    assert_eq!(session_id, "a4483f72-ef29-4b31-97b5-f025efb8edcf");
    assert_eq!(model, "claude-opus-5-5");
    assert_eq!(permission_mode, "default");
    assert_eq!((name.as_str(), tool.as_str()), ("Write", "Write"));
    assert_eq!((started, asked), (&input, &input));
    assert_eq!(id, result_id);
    assert_eq!(request_id, "316a49cc-17ff-4c37-bde7-0322d3b388fb");
    assert!(text.starts_with("File created successfully at: /work/note.txt"));
    assert_eq!(format!("{first}{second}"), "fait.");
    assert!((cost_usd - 0.3344648).abs() < 1e-9);
}

#[test]
fn interrupted_turn_is_not_reported_as_an_error() {
    let line =
        r#"{"type":"result","is_error":true,"terminal_reason":"aborted_tools","result":"x"}"#;
    let [Event::TurnDone { error: None, .. }] = parse_line(line).as_slice() else {
        panic!("une interruption ne doit pas produire d'erreur");
    };
}

#[test]
fn init_response_lists_models_and_commands() {
    let line = r#"{"type":"control_response","response":{"subtype":"success","request_id":"solar-init","response":{"commands":[{"name":"simplify","description":"Review the changed code","argumentHint":""}],"models":[{"value":"default","resolvedModel":"claude-fable-5-2","displayName":"Default (recommended)","description":"Fable 5.2","supportsEffort":true,"supportedEffortLevels":["low","medium","high"]},{"value":"haiku","resolvedModel":"claude-haiku-5-5","displayName":"Haiku 5.5"}]}}}"#;
    let events = parse_line(line);
    let [Event::Catalog { models, commands }] = events.as_slice() else {
        panic!("catalogue attendu");
    };
    assert_eq!(commands[0].name, "simplify");
    assert_eq!(models[0].value, "default");
    assert_eq!(models[0].resolved, "claude-fable-5-2");
    assert_eq!(models[0].description, "Fable 5.2");
    assert_eq!(models[0].efforts, ["low", "medium", "high"]);
    assert!(models[1].efforts.is_empty());
}

#[test]
fn settings_response_gives_the_configured_model_and_effort() {
    let line = r#"{"type":"control_response","response":{"subtype":"success","request_id":"solar-settings","response":{"effective":{"permissions":{"defaultMode":"dontAsk"},"model":"opus[1m]"}}}}"#;
    assert_eq!(
        parse_line(line),
        [Event::Settings {
            model: Some("opus[1m]".into()),
            effort: None,
        }]
    );
    let empty = r#"{"type":"control_response","response":{"subtype":"success","request_id":"solar-settings","response":{}}}"#;
    assert_eq!(
        parse_line(empty),
        [Event::Settings {
            model: None,
            effort: None,
        }]
    );
}
