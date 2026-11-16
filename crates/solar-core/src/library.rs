//! La bibliothèque : des modèles de messages réutilisables, un fichier Markdown
//! chacun, avec un en-tête facultatif (`title`, `description`, `agent`,
//! `mission`) et des trous `{nom}` à remplir au moment de s'en servir.
//! Personnels (`~/.solar/library/`), propres à un projet
//! (`<projet>/.solar/library/`) ou exemples intégrés.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::agent::Provider;

/// D'où vient un modèle ; à nom égal, le projet l'emporte sur le personnel,
/// qui l'emporte sur l'exemple.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    Project,
    Personal,
    Example,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    /// Le nom du fichier sans `.md` : l'identifiant, et la commande `/nom`.
    pub name: String,
    pub title: String,
    pub description: String,
    pub agent: Option<Provider>,
    /// Se lance comme une mission (Planifier).
    pub mission: bool,
    pub body: String,
    pub origin: Origin,
    pub path: Option<PathBuf>,
    /// Un en-tête mal formé : le modèle reste utilisable, on le signale.
    pub problem: Option<String>,
}

/// Un exemple intégré : son nom et son texte.
#[derive(Clone, Debug)]
pub struct Example {
    pub name: String,
    pub text: String,
}

/// Lit un modèle : l'en-tête entre deux lignes `---`, puis le corps.
pub fn parse(name: &str, text: &str, origin: Origin, path: Option<PathBuf>) -> Template {
    let mut template = Template {
        name: name.to_string(),
        title: name.to_string(),
        description: String::new(),
        agent: None,
        mission: false,
        body: text.trim().to_string(),
        origin,
        path,
        problem: None,
    };
    let text = text.trim_start_matches('\u{feff}');
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return template;
    };
    let Some(end) = rest.find("\n---") else {
        template.problem = Some("en-tête sans fin (---)".into());
        return template;
    };
    let (header, after) = rest.split_at(end);
    let after = after.trim_start_matches("\n---");
    template.body = after.trim().to_string();
    for line in header
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((key, value)) = line.split_once(':') else {
            template.problem = Some(format!("ligne d'en-tête illisible : « {line} »"));
            continue;
        };
        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        match key.trim() {
            "title" => template.title = value,
            "description" => template.description = value,
            "agent" => match value.to_lowercase().as_str() {
                "claude" => template.agent = Some(Provider::Claude),
                "codex" => template.agent = Some(Provider::Codex),
                "grok" => template.agent = Some(Provider::Grok),
                "" => {}
                other => template.problem = Some(format!("agent inconnu : « {other} »")),
            },
            "mission" => {
                template.mission = matches!(value.to_lowercase().as_str(), "true" | "yes" | "oui")
            }
            // Les autres clés (celles de Claude Code, par exemple) sont ignorées.
            _ => {}
        }
    }
    template
}

/// Les morceaux d'un corps : du texte, ou un trou.
enum Piece<'a> {
    Text(&'a str),
    Hole(&'a str),
}

fn pieces(body: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut rest = body;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("{{") {
            out.push(Piece::Text("{"));
            rest = after;
        } else if let Some(after) = rest.strip_prefix("}}") {
            out.push(Piece::Text("}"));
            rest = after;
        } else if let Some(after) = rest.strip_prefix("$ARGUMENTS") {
            out.push(Piece::Hole("arguments"));
            rest = after;
        } else if rest.starts_with('{')
            && let Some(close) = rest.find('}')
            && close > 1
            && rest[1..close]
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            out.push(Piece::Hole(&rest[1..close]));
            rest = &rest[close + 1..];
        } else {
            let next = rest
                .char_indices()
                .skip(1)
                .find(|(_, c)| matches!(c, '{' | '}' | '$'))
                .map_or(rest.len(), |(ix, _)| ix);
            out.push(Piece::Text(&rest[..next]));
            rest = &rest[next..];
        }
    }
    out
}

/// Les trous d'un corps, chacun une fois, dans l'ordre.
pub fn holes(body: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for piece in pieces(body) {
        if let Piece::Hole(name) = piece
            && !found.iter().any(|known| known == name)
        {
            found.push(name.to_string());
        }
    }
    found
}

/// Le corps, trous remplis ; un trou sans valeur disparaît.
pub fn fill(body: &str, values: &HashMap<String, String>) -> String {
    pieces(body)
        .into_iter()
        .map(|piece| match piece {
            Piece::Text(text) => text,
            Piece::Hole(name) => values.get(name).map_or("", String::as_str),
        })
        .collect()
}

/// `~/.solar/library`, ou `SOLAR_LIBRARY` pour les essais.
pub fn personal_dir() -> PathBuf {
    std::env::var_os("SOLAR_LIBRARY").map_or_else(
        || {
            std::env::home_dir()
                .unwrap_or_default()
                .join(".solar/library")
        },
        PathBuf::from,
    )
}

/// Le dossier des modèles d'un projet.
pub fn project_dir(project: &Path) -> PathBuf {
    project.join(".solar/library")
}

fn read_dir(dir: &Path, origin: Origin) -> Vec<Template> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<Template> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().to_string();
            let text = fs::read_to_string(&path).ok()?;
            Some(parse(&name, &text, origin, Some(path)))
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// Les modèles du projet, personnels et exemples, un par nom (le projet
/// l'emporte, puis le personnel), triés par nom.
pub fn load(personal: &Path, project: Option<&Path>, examples: &[Example]) -> Vec<Template> {
    let mut all: Vec<Template> = project
        .map(|project| read_dir(&project_dir(project), Origin::Project))
        .unwrap_or_default();
    all.extend(read_dir(personal, Origin::Personal));
    all.extend(
        examples
            .iter()
            .map(|example| parse(&example.name, &example.text, Origin::Example, None)),
    );
    all.sort_by(|a, b| a.name.cmp(&b.name).then(a.origin.cmp(&b.origin)));
    all.dedup_by(|later, kept| later.name == kept.name);
    all
}

/// Un nom de modèle sert de commande `/nom` et de nom de fichier : lettres
/// minuscules ASCII, chiffres, `-` et `_`, sans tiret en tête.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// `base`, ou `base-2`, `base-3`… : le premier nom libre dans `dir`.
pub fn free_name(dir: &Path, base: &str) -> String {
    (1..)
        .map(|n| match n {
            1 => base.to_string(),
            n => format!("{base}-{n}"),
        })
        .find(|name| !dir.join(format!("{name}.md")).exists())
        .unwrap_or_else(|| base.to_string())
}

#[derive(Debug)]
pub enum DeleteError {
    /// Le fichier a changé depuis sa lecture : rien n'est supprimé.
    ChangedElsewhere,
    Io(std::io::Error),
}

/// Supprime un modèle, à condition qu'il contienne encore `opened`.
pub fn delete(file: &Path, opened: &str) -> Result<(), DeleteError> {
    if fs::read_to_string(file).ok().as_deref() != Some(opened) {
        return Err(DeleteError::ChangedElsewhere);
    }
    fs::remove_file(file).map_err(DeleteError::Io)
}

/// Le dossier contient-il au moins un modèle ?
pub fn has_templates(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| entry.path().extension().is_some_and(|ext| ext == "md"))
    })
}
