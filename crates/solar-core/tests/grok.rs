//! Le pilote Grok face à de vrais tours enregistrés (plan, puis approbation)
//! et au journal de la même session, chemins remplacés.

use solar_core::claude::Event;
use solar_core::grok::{Parser, parse_history};
use solar_core::turns::TurnParser;

fn run(fixture: &str) -> (Parser, Vec<Event>) {
    let mut parser = Parser::default();
    let events = fixture.lines().flat_map(|line| parser.line(line)).collect();
    (parser, events)
}

fn tools(events: &[Event]) -> Vec<(String, bool)> {
    // Chaque outil et son issue : (nom, en erreur).
    events
        .iter()
        .filter_map(|event| match event {
            Event::ToolStart { id, name, .. } => events.iter().find_map(|result| match result {
                Event::ToolResult {
                    id: done, is_error, ..
                } if done == id => Some((name.clone(), *is_error)),
                _ => None,
            }),
            _ => None,
        })
        .collect()
}

#[test]
fn a_plan_turn_ends_with_its_plan_and_hides_plan_mode_tools() {
    let (parser, events) = run(include_str!("fixtures/grok_plan.jsonl"));
    let names: Vec<String> = tools(&events).into_iter().map(|(name, _)| name).collect();
    assert_eq!(
        names,
        ["Glob", "Grep", "Read", "Edit"],
        "exit_plan_mode est masqué"
    );
    assert!(parser.done() && parser.ok());
    assert!(
        parser.last_message().contains("calc.py"),
        "{}",
        parser.last_message()
    );
    let [
        ..,
        Event::Ready { session_id, .. },
        Event::TurnDone {
            error: None,
            cost_usd,
            ..
        },
    ] = &events[..]
    else {
        panic!("fin de tour attendue : {:?}", events.last())
    };
    assert_eq!(session_id, "01a1e15b-7e4d-7020-a55b-d28d8af0d581");
    // Grok donne le coût du tour ; Codex non (voir tests/codex.rs).
    assert_eq!(*cost_usd, Some(0.09506));
}

#[test]
fn refused_tools_are_errors() {
    let (_, events) = run(include_str!("fixtures/grok_approve.jsonl"));
    assert_eq!(
        tools(&events),
        [
            ("Edit".into(), true),
            ("Edit".into(), false),
            ("Bash".into(), true),
            ("Bash".into(), true),
            ("Read".into(), false),
        ]
    );
    let refusals: Vec<&String> = events
        .iter()
        .filter_map(|event| match event {
            Event::ToolResult {
                is_error: true,
                text,
                ..
            } => Some(text),
            _ => None,
        })
        .collect();
    assert!(
        refusals[0].contains("not allowed in plan mode"),
        "{}",
        refusals[0]
    );
    assert!(refusals[1].contains("not pre-approved"), "{}", refusals[1]);
}

#[test]
fn the_saved_session_is_read_back_with_both_turns() {
    let events = parse_history(include_str!("fixtures/grok_updates.jsonl"));
    let users: Vec<&String> = events
        .iter()
        .filter_map(|event| match event {
            Event::User(text) => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(users.len(), 2);
    assert!(users[1].starts_with("The plan is approved"));
    assert_eq!(tools(&events).len(), 9, "{:?}", tools(&events));
    assert!(events.iter().any(
        |event| matches!(event, Event::ToolResult { at: Some(at), .. } if at.starts_with("2026-"))
    ));
}
