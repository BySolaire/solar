//! Les bots : une consigne lancée sur un projet par un déclencheur, qui
//! devient une mission à examiner. Rangés dans `state.json` ; rien ne tourne
//! quand Solar est fermé.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use chrono::{Datelike, Duration, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};

use crate::agent::Provider;
use crate::github::Issue;
use crate::library::{Template, fill, holes};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bot {
    pub id: String,
    pub name: String,
    pub project: PathBuf,
    #[serde(default)]
    pub trigger: Trigger,
    pub prompt: Prompt,
    #[serde(default)]
    pub provider: Provider,
    /// À défaut, ceux de la configuration de l'agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Le mode de permission de la mission lancée.
    pub mode: String,
    #[serde(default)]
    pub scope: Scope,
    /// En pause : aucun déclencheur ne le lance ; « Lancer maintenant » reste possible.
    pub enabled: bool,
    /// Les exécutions récentes, de la plus ancienne à la plus récente.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<Run>,
    /// Jusqu'où l'horaire a été regardé : les échéances d'après sont à venir.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<String>,
    /// Les échéances passées pendant que Solar était fermé, pas encore traitées.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub missed: u32,
    /// Les issues déjà confiées à une mission : jamais deux fois.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handled: Vec<u64>,
    /// Au plus ce nombre d'exécutions déclenchées par jour.
    #[serde(default = "default_cap", skip_serializing_if = "is_default_cap")]
    pub daily_cap: u32,
}

fn default_cap() -> u32 {
    3
}

fn is_default_cap(cap: &u32) -> bool {
    *cap == default_cap()
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Trigger {
    /// Seulement « Lancer maintenant ».
    #[default]
    Manual,
    /// Chaque jour (ou chaque jour ouvré) à une heure locale, « 09:00 ».
    Daily { time: String, weekdays_only: bool },
    /// Toutes les N heures, à heures fixes depuis minuit.
    Every { hours: u32 },
    /// Une nouvelle issue GitHub ouverte avec cette étiquette ; `since` : seules
    /// les issues créées après comptent (`None` : aussi celles déjà ouvertes).
    Issues {
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        since: Option<String>,
    },
}

/// La première échéance strictement après `after`, en heure locale.
pub fn next_due(trigger: &Trigger, after: NaiveDateTime) -> Option<NaiveDateTime> {
    match trigger {
        Trigger::Manual | Trigger::Issues { .. } => None,
        Trigger::Daily {
            time,
            weekdays_only,
        } => {
            let time = NaiveTime::parse_from_str(time, "%H:%M").ok()?;
            (0..=7)
                .map(|days| (after.date() + Duration::days(days)).and_time(time))
                .filter(|due| *due > after)
                .find(|due| !weekdays_only || due.weekday().number_from_monday() <= 5)
        }
        Trigger::Every { hours } if (1..=24).contains(hours) => {
            let midnight = after.date().and_time(NaiveTime::MIN);
            (0..24 / hours + 1)
                .map(|n| midnight + Duration::hours(i64::from(n * hours)))
                .chain(std::iter::once(midnight + Duration::days(1)))
                .find(|due| *due > after)
        }
        Trigger::Every { .. } => None,
    }
}

/// Le nombre d'échéances dans `(since, now]`, plafonné : seul le fait d'en
/// avoir manqué compte, pas leur total exact après une longue absence.
pub fn due_between(trigger: &Trigger, since: NaiveDateTime, now: NaiveDateTime) -> u32 {
    let mut count = 0;
    let mut cursor = since;
    while count < MAX_COUNTED {
        match next_due(trigger, cursor) {
            Some(due) if due <= now => {
                count += 1;
                cursor = due;
            }
            _ => break,
        }
    }
    count
}

const MAX_COUNTED: u32 = 100;

/// La consigne : un modèle de la bibliothèque aux trous remplis d'avance, ou
/// un texte.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Prompt {
    Text(String),
    Template {
        name: String,
        #[serde(default)]
        values: BTreeMap<String, String>,
    },
}

/// Jusqu'où va une exécution sans toi.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Le plan, en lecture seule ; il attend ton accord.
    #[default]
    Plan,
    /// Toute la mission, dans sa copie de travail, jusqu'au tour à examiner.
    Mission,
}

/// Une exécution : la conversation qu'elle a ouverte, ou pourquoi elle n'a
/// pas pu partir.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// L'issue traitée, pour un bot d'issues.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue: Option<u64>,
    /// Ce qu'a coûté l'exécution, quand l'agent le dit (en dollars).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    /// Un déclenchement sauté (garde-fou) ; `problem` dit pourquoi.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skipped: bool,
}

impl Run {
    /// L'exécution est vraiment partie : ni sautée, ni en échec.
    pub fn started(&self) -> bool {
        !self.skipped && self.problem.is_none()
    }
}

/// Ce que fait un déclenchement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Go,
    /// La précédente exécution attend encore ta revue.
    Busy,
    /// Le plafond du jour est atteint.
    Capped,
}

/// Un déclenchement part-il ? `busy` : une exécution du bot attend encore
/// (plan, travail ou revue) ; `today` : le jour local.
pub fn decide(bot: &Bot, busy: bool, today: chrono::NaiveDate) -> Decision {
    if busy {
        return Decision::Busy;
    }
    let started_today = bot
        .runs
        .iter()
        .filter(|run| run.started())
        .filter(|run| {
            chrono::DateTime::parse_from_rfc3339(&run.at)
                .is_ok_and(|at| at.with_timezone(&chrono::Local).date_naive() == today)
        })
        .count();
    if started_today >= bot.daily_cap as usize {
        Decision::Capped
    } else {
        Decision::Go
    }
}

const KEPT_RUNS: usize = 20;

impl Bot {
    pub fn record(&mut self, run: Run) {
        self.runs.push(run);
        let extra = self.runs.len().saturating_sub(KEPT_RUNS);
        self.runs.drain(..extra);
    }

    pub fn last_run(&self) -> Option<&Run> {
        self.runs.last()
    }

    /// Note un déclenchement sauté, une fois tant que la raison ne change pas.
    pub fn skip(&mut self, at: String, reason: String) {
        let repeated = self
            .runs
            .last()
            .is_some_and(|run| run.skipped && run.problem.as_deref() == Some(reason.as_str()));
        if !repeated {
            self.record(Run {
                at,
                session: None,
                problem: Some(reason),
                issue: None,
                cost: None,
                skipped: true,
            });
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptError {
    Empty,
    MissingTemplate(String),
    /// Les trous restés vides, dans l'ordre du modèle.
    Unfilled(Vec<String>),
}

/// La consigne qu'envoie le bot, d'après les modèles du projet et personnels.
pub fn prompt(bot: &Bot, templates: &[Template]) -> Result<String, PromptError> {
    prompt_for(bot, templates, None)
}

/// La consigne pour une issue : ses trous `{issue_number}`, `{issue_title}`,
/// `{issue_body}` et `{issue_url}` sont remplis ; sans eux, l'issue suit la
/// consigne.
pub fn prompt_for(
    bot: &Bot,
    templates: &[Template],
    issue: Option<&Issue>,
) -> Result<String, PromptError> {
    let mut values: HashMap<String, String> = match &bot.prompt {
        Prompt::Template { values, .. } => values
            .iter()
            .filter(|(_, v)| !v.trim().is_empty())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        Prompt::Text(_) => HashMap::new(),
    };
    if let Some(issue) = issue {
        values.insert("issue_number".into(), issue.number.to_string());
        values.insert("issue_title".into(), issue.title.clone());
        values.insert("issue_body".into(), issue.body.clone());
        values.insert("issue_url".into(), issue.url.clone());
    }
    let body = match &bot.prompt {
        Prompt::Text(text) => text.clone(),
        Prompt::Template { name, .. } => templates
            .iter()
            .find(|template| template.name == *name)
            .ok_or_else(|| PromptError::MissingTemplate(name.clone()))?
            .body
            .clone(),
    };
    let found = holes(&body);
    let unfilled: Vec<String> = found
        .iter()
        .filter(|hole| !values.contains_key(*hole))
        .cloned()
        .collect();
    // Un texte libre garde ses accolades ; un modèle doit être complet.
    if !unfilled.is_empty() && matches!(bot.prompt, Prompt::Template { .. }) {
        return Err(PromptError::Unfilled(unfilled));
    }
    let mut text = fill_known(&body, &values);
    if let Some(issue) = issue
        && !found.iter().any(|hole| hole.starts_with("issue_"))
    {
        text = format!(
            "{}\n\n#{} · {}\n{}\n\n{}",
            text.trim(),
            issue.number,
            issue.title,
            issue.url,
            issue.body.trim()
        );
    }
    let text = text.trim();
    if text.is_empty() {
        return Err(PromptError::Empty);
    }
    Ok(text.to_string())
}

/// Les trous connus remplis ; les autres restent tels quels.
fn fill_known(body: &str, values: &HashMap<String, String>) -> String {
    let mut known = values.clone();
    for hole in holes(body) {
        known.entry(hole.clone()).or_insert_with(|| {
            if hole == "ARGUMENTS" {
                "$ARGUMENTS".into()
            } else {
                format!("{{{hole}}}")
            }
        });
    }
    fill(body, &known)
}

/// Les issues à traiter : pas encore confiées, créées après le bot (sauf
/// « aussi les issues déjà ouvertes »), les plus anciennes d'abord.
pub fn new_issues<'a>(bot: &Bot, issues: &'a [Issue]) -> Vec<&'a Issue> {
    let since = match &bot.trigger {
        Trigger::Issues { since, .. } => since
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()),
        _ => return Vec::new(),
    };
    let mut found: Vec<&Issue> = issues
        .iter()
        .filter(|issue| !bot.handled.contains(&issue.number))
        .filter(|issue| {
            since.is_none_or(|since| {
                chrono::DateTime::parse_from_rfc3339(&issue.created_at)
                    .is_ok_and(|created| created > since)
            })
        })
        .collect();
    found.sort_by_key(|issue| issue.number);
    found
}
