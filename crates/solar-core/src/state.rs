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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Mission {
    pub status: MissionStatus,
    /// Versions successives du plan proposé par Claude ; la dernière est en fin.
    #[serde(default)]
    pub plans: Vec<String>,
    /// Mode de permission choisi au lancement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launched_mode: Option<String>,
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
