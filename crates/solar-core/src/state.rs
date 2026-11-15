//! Ce que Solar retient entre deux lancements : projets, sessions, onglets et
//! missions. Les champs ajoutés ont une valeur par défaut : un ancien fichier
//! reste lisible.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub projects: Vec<Project>,
    /// Identifiants des sessions ouvertes en onglets, dans l'ordre.
    pub open_tabs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub path: PathBuf,
    pub sessions: Vec<Session>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// Identifiant donné par Claude Code, celui qu'attend `--resume`.
    pub id: String,
    pub title: String,
    /// La conversation porte une mission : un plan préparé sans rien écrire,
    /// puis lancé. Absent pour une conversation ordinaire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mission: Option<Mission>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mission {
    pub status: MissionStatus,
    /// Versions successives du plan proposé par Claude ; la dernière est en fin.
    #[serde(default)]
    pub plans: Vec<String>,
    /// Mode de permission choisi au lancement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launched_mode: Option<String>,
    /// Instantané du projet pris au lancement : la base de la revue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<String>,
    /// L'état exact du projet que l'utilisateur a accepté.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted: Option<String>,
    /// Les relectures faites par un agent relecteur, la plus récente en fin.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviews: Vec<AgentReview>,
    /// Numéro de la tentative : 1 au premier lancement, puis un de plus à
    /// chaque relance, qui ouvre une conversation neuve.
    #[serde(default = "first_attempt")]
    pub attempt: u32,
    /// Cette tentative a été relancée : le numéro de la suivante.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_attempt: Option<u32>,
    /// La copie de travail (worktree git) où la mission travaille, hors du
    /// projet ; absente, la mission travaille dans le projet même.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<std::path::PathBuf>,
}

fn first_attempt() -> u32 {
    1
}

impl Default for Mission {
    fn default() -> Self {
        Self {
            status: MissionStatus::default(),
            plans: Vec::new(),
            launched_mode: None,
            checkpoint: None,
            accepted: None,
            reviews: Vec::new(),
            attempt: first_attempt(),
            next_attempt: None,
            worktree: None,
        }
    }
}

/// Le verdict d'un agent relecteur sur un état exact du projet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentReview {
    /// L'agent qui a relu, tel qu'affiché : « Claude ».
    pub agent: String,
    /// L'arbre exact du projet relu : un verdict sur un autre état est périmé.
    pub tree: String,
    /// `Some(true)` : rien à redire ; `Some(false)` : des problèmes ; `None` :
    /// le relecteur n'a pas donné de verdict lisible.
    pub ok: Option<bool>,
    pub text: String,
    /// Heure de fin, au format RFC 3339.
    pub at: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionStatus {
    /// Claude explore en lecture seule et prépare son plan.
    #[default]
    Planning,
    /// Un plan attend d'être corrigé ou lancé.
    Ready,
    /// Le plan est approuvé : Claude l'exécute.
    Running,
    /// Abandonnée avant son lancement ; la conversation reste.
    Cancelled,
    /// Résultat accepté : une décision, sans commit.
    Accepted,
    /// Les fichiers de la mission ont été remis dans leur état d'avant.
    Restored,
}

impl State {
    // shortcut: un dossier fixe dans le home, identique sur les trois systèmes ;
    // passer aux dossiers de config de chaque plateforme à la publication.
    /// `SOLAR_STATE` permet de lancer l'app sur un état jetable, pour les essais.
    pub fn default_path() -> PathBuf {
        std::env::var_os("SOLAR_STATE").map_or_else(
            || {
                std::env::home_dir()
                    .unwrap_or_default()
                    .join(".solar/state.json")
            },
            PathBuf::from,
        )
    }

    /// Un fichier absent donne un état vide. Un fichier illisible est mis de
    /// côté en `.bad` plutôt qu'écrasé à la prochaine sauvegarde.
    pub fn load(path: &Path) -> Self {
        let Ok(text) = fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_else(|_| {
            let _ = fs::rename(path, path.with_extension("json.bad"));
            Self::default()
        })
    }

    /// Écrit à côté puis renomme, pour ne jamais laisser un fichier à moitié écrit.
    /// Le fichier d'avant les missions est copié une fois en `.bak` : le format
    /// change, un retour en arrière reste possible.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let backup = path.with_extension("json.bak");
        if path.exists() && !backup.exists() {
            fs::copy(path, &backup)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        fs::rename(tmp, path)
    }
}
