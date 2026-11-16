//! Sessions enregistrées par Claude Code dans `~/.claude/projects`.
//!
//! Le format de ces fichiers est interne à Claude Code et change d'une version
//! à l'autre : toute ligne inconnue est ignorée.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use crate::claude::{Event, blocks};

/// Une session trouvée sur le disque, proposée à l'import.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub id: String,
    pub title: String,
    pub modified: SystemTime,
}

/// Dossier où Claude Code range les sessions d'un projet.
fn project_dir(project: &Path) -> PathBuf {
    let root = std::env::var_os("CLAUDE_CONFIG_DIR").map_or_else(
        || std::env::home_dir().unwrap_or_default().join(".claude"),
        PathBuf::from,
    );
    // Claude Code nomme le dossier d'après le chemin réel, chaque caractère
    // non alphanumérique remplacé par un tiret.
    let real = fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    let slug: String = real
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    root.join("projects").join(slug)
}

/// Le fil d'une session, tel qu'il s'est déroulé. Vide si le fichier manque.
pub fn load(project: &Path, session_id: &str) -> Vec<Event> {
    let path = project_dir(project).join(format!("{session_id}.jsonl"));
    fs::read_to_string(path).map_or_else(|_| vec![], |text| parse(&text).1)
}

/// Date de dernière modification d'une conversation, sans lire son fichier.
pub fn modified(project: &Path, session_id: &str) -> Option<SystemTime> {
    let path = project_dir(project).join(format!("{session_id}.jsonl"));
    path.metadata().ok()?.modified().ok()
}

/// Sessions du projet, la plus récente d'abord.
pub fn list(project: &Path) -> Vec<Found> {
    let Ok(entries) = fs::read_dir(project_dir(project)) else {
        return vec![];
    };
    let mut found: Vec<Found> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? != "jsonl" {
                return None;
            }
            let (title, events) = parse(&fs::read_to_string(&path).ok()?);
            // Sans message de l'utilisateur, ce n'est pas une conversation.
            let first = events.iter().find_map(|event| match event {
                Event::User(text) => Some(text.clone()),
                _ => None,
            })?;
            Some(Found {
                id: path.file_stem()?.to_string_lossy().to_string(),
                title: title.unwrap_or(first).chars().take(60).collect(),
                modified: path.metadata().ok()?.modified().ok()?,
            })
        })
        .collect();
    found.sort_by_key(|session| std::cmp::Reverse(session.modified));
    found
}

/// Titre donné par Claude Code (s'il existe) et fil de la conversation.
pub fn parse(text: &str) -> (Option<String>, Vec<Event>) {
    let mut title = None;
    let mut events = Vec::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match v["type"].as_str().unwrap_or_default() {
            "ai-title" => title = v["aiTitle"].as_str().map(str::to_string),
            // On écarte les sous-agents et les messages techniques du CLI.
            "user" | "assistant" if v["isSidechain"] != true && v["isMeta"] != true => {
                // Le résultat d'un outil garde l'heure de la ligne : celle des tests affichés.
                let stamp = v["timestamp"].as_str().map(str::to_string);
                events.extend(
                    blocks(&v["message"], true)
                        .into_iter()
                        .map(|event| match event {
                            Event::ToolResult {
                                id, is_error, text, ..
                            } => Event::ToolResult {
                                id,
                                is_error,
                                text,
                                at: stamp.clone(),
                            },
                            event => event,
                        }),
                );
                // La taille du contexte à ce message : l'indicateur la retrouve à
                // la réouverture.
                if v["type"] == "assistant"
                    && let Some(tokens) = crate::claude::context_tokens(&v["message"]["usage"])
                {
                    events.push(Event::Context {
                        tokens: Some(tokens),
                        window: None,
                    });
                }
            }
            _ => {}
        }
    }
    (title, events)
}
