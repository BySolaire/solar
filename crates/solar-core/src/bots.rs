//! Les bots : une consigne lancée sur un projet par un déclencheur, qui
//! devient une mission à examiner. Rangés dans `state.json` ; rien ne tourne
//! quand SOLAR est fermé.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use chrono::{Datelike, Duration, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};

use crate::agent::Provider;
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
    /// Les échéances passées pendant que SOLAR était fermé, pas encore traitées.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub missed: u32,
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
}

/// La première échéance strictement après `after`, en heure locale.
pub fn next_due(trigger: &Trigger, after: NaiveDateTime) -> Option<NaiveDateTime> {
    match trigger {
        Trigger::Manual => None,
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
    let text = match &bot.prompt {
        Prompt::Text(text) => text.clone(),
        Prompt::Template { name, values } => {
            let template = templates
                .iter()
                .find(|template| template.name == *name)
                .ok_or_else(|| PromptError::MissingTemplate(name.clone()))?;
            let unfilled: Vec<String> = holes(&template.body)
                .into_iter()
                .filter(|hole| values.get(hole).is_none_or(|v| v.trim().is_empty()))
                .collect();
            if !unfilled.is_empty() {
                return Err(PromptError::Unfilled(unfilled));
            }
            let values: HashMap<String, String> =
                values.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            fill(&template.body, &values)
        }
    };
    let text = text.trim();
    if text.is_empty() {
        return Err(PromptError::Empty);
    }
    Ok(text.to_string())
}
