//! Les nouvelles versions : Solar demande à GitHub la dernière release, au
//! plus une fois par jour, et la signale si elle est plus récente. Rien n'est
//! installé ; la requête n'envoie que ce que porte une requête web.

use std::io;
use std::process::{Command, Stdio};

use serde_json::Value;

/// La release publique la plus récente de Solar.
const LATEST: &str = "https://api.github.com/repos/BySolaire/solar/releases/latest";
/// Le seul préfixe accepté pour le lien d'une release.
const RELEASES: &str = "https://github.com/BySolaire/solar/releases/";

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Release {
    /// Sans le `v` du tag.
    pub version: String,
    pub url: String,
}

/// Ce que Solar a appris de GitHub à sa dernière vérification réussie.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Checked {
    /// RFC 3339.
    pub at: String,
    /// Rien si GitHub n'a pas de release lisible.
    pub latest: Option<Release>,
}

/// `candidate` est-elle plus récente que `current` ? Numéro par numéro ; une
/// préversion (`-beta.1`) précède la version qu'elle prépare. Un numéro
/// illisible n'est jamais plus récent.
pub fn newer(current: &str, candidate: &str) -> bool {
    match (parse(current), parse(candidate)) {
        (Some(current), Some(candidate)) => candidate > current,
        _ => false,
    }
}

/// `1.2.3-beta` : les nombres, puis l'absence de préversion qui passe devant.
fn parse(version: &str) -> Option<(Vec<u64>, bool, String)> {
    let version = version.trim().trim_start_matches('v');
    let (core, pre) = match version.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (version, None),
    };
    let numbers = core
        .split('.')
        .map(|part| part.parse().ok())
        .collect::<Option<Vec<u64>>>()?;
    Some((numbers, pre.is_none(), pre.unwrap_or_default().to_string()))
}

/// La release décrite par la réponse de GitHub ; rien pour un brouillon, une
/// préversion, une réponse illisible ou un lien hors du dépôt de Solar.
pub fn latest_from_json(json: &str) -> Option<Release> {
    let value: Value = serde_json::from_str(json).ok()?;
    if value["draft"].as_bool() != Some(false) || value["prerelease"].as_bool() != Some(false) {
        return None;
    }
    let tag = value["tag_name"].as_str()?;
    let url = value["html_url"].as_str()?;
    if !url.starts_with(RELEASES) {
        return None;
    }
    parse(tag)?;
    Some(Release {
        version: tag.trim_start_matches('v').to_string(),
        url: url.to_string(),
    })
}

/// Faut-il redemander ? Oui sans vérification notée, si la dernière a plus
/// d'un jour, ou si sa date est illisible ou dans le futur.
pub fn due(checked_at: Option<&str>, now: &str) -> bool {
    let (Some(checked), Ok(now)) = (
        checked_at.and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok()),
        chrono::DateTime::parse_from_rfc3339(now),
    ) else {
        return true;
    };
    let age = now.signed_duration_since(checked);
    age < chrono::TimeDelta::zero() || age >= chrono::TimeDelta::days(1)
}

/// La dernière release, demandée à GitHub par `curl` (10 s au plus).
pub fn fetch_latest() -> io::Result<Option<Release>> {
    let output = Command::new("curl")
        .args(["--silent", "--show-error", "--fail", "--location"])
        .args(["--max-time", "10", "--proto", "=https"])
        .args(["--header", "Accept: application/vnd.github+json"])
        .arg(LATEST)
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(latest_from_json(&String::from_utf8_lossy(&output.stdout)))
}
