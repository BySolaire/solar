//! Instantanés d'un projet, pris avant une mission, pour montrer exactement ce
//! qu'elle a changé et pouvoir le restaurer. Ils vivent dans un dépôt git privé
//! (`~/.solar/checkpoints/<projet>.git`) qui prend le projet pour arbre de
//! travail : le dépôt de l'utilisateur, sa branche et son index ne sont jamais
//! touchés, et un dossier sans git ni premier commit marche de la même façon.
//! Le `.gitignore` du projet est respecté.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Ignorés même sans `.gitignore` : dépendances et produits de build, souvent
/// énormes, qu'une mission ne doit pas faire copier.
const ALWAYS_IGNORED: &str = "node_modules/\ntarget/\n.DS_Store\n";

pub struct Store {
    git_dir: PathBuf,
    project: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

/// Un fichier changé entre deux états. Des comptes absents : fichier binaire.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub path: String,
    pub kind: ChangeKind,
    pub lines: Option<(u32, u32)>,
}

#[derive(Debug)]
pub enum RestoreError {
    /// Ces fichiers ont changé depuis l'examen : rien n'a été restauré.
    ChangedSinceReview(Vec<String>),
    Io(io::Error),
}

impl From<io::Error> for RestoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl Store {
    /// Le dépôt privé d'un projet, rangé dans `dir`.
    pub fn for_project(dir: &Path, project: &Path) -> Self {
        let project = fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
        let name: String = project
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();
        Self {
            git_dir: dir.join(format!("{name}.git")),
            project,
        }
    }

    /// `~/.solar/checkpoints`, ou le dossier `SOLAR_CHECKPOINTS` pour les essais.
    pub fn default_dir() -> PathBuf {
        std::env::var_os("SOLAR_CHECKPOINTS").map_or_else(
            || {
                std::env::home_dir()
                    .unwrap_or_default()
                    .join(".solar/checkpoints")
            },
            PathBuf::from,
        )
    }

    fn git(&self, args: &[&str]) -> io::Result<Output> {
        let output = Command::new("git")
            .arg(format!("--git-dir={}", self.git_dir.display()))
            .arg(format!("--work-tree={}", self.project.display()))
            .args(["-c", "user.name=SOLAR", "-c", "user.email=solar@localhost"])
            .args(["-c", "core.autocrlf=false", "-c", "core.quotepath=false"])
            .args(args)
            // L'index est celui du dépôt privé, jamais celui du projet.
            .env("GIT_INDEX_FILE", self.git_dir.join("index"))
            .current_dir(&self.project)
            .output()?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(io::Error::other(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ))
        }
    }

    fn text(&self, args: &[&str]) -> io::Result<String> {
        Ok(String::from_utf8_lossy(&self.git(args)?.stdout)
            .trim()
            .to_string())
    }

    fn ensure(&self) -> io::Result<()> {
        if self.git_dir.join("HEAD").exists() {
            return Ok(());
        }
        fs::create_dir_all(&self.git_dir)?;
        let init = Command::new("git")
            .args(["init", "--bare", "--quiet"])
            .arg(&self.git_dir)
            .output()?;
        if !init.status.success() {
            return Err(io::Error::other(
                String::from_utf8_lossy(&init.stderr).to_string(),
            ));
        }
        fs::create_dir_all(self.git_dir.join("info"))?;
        fs::write(self.git_dir.join("info/exclude"), ALWAYS_IGNORED)
    }

    /// L'état actuel du projet, sous forme d'arbre git. Rien n'est écrit dans
    /// le projet.
    pub fn snapshot(&self) -> io::Result<String> {
        self.ensure()?;
        self.git(&["add", "--all", "--", "."])?;
        self.text(&["write-tree"])
    }

    /// Un instantané gardé : un commit hors de toute branche, retenu par une
    /// référence pour qu'aucun nettoyage ne l'efface.
    pub fn checkpoint(&self) -> io::Result<String> {
        let tree = self.snapshot()?;
        let commit = self.text(&["commit-tree", &tree, "-m", "SOLAR : avant la mission"])?;
        self.git(&["update-ref", &format!("refs/solar/{commit}"), &commit])?;
        Ok(commit)
    }

    /// Les fichiers qui diffèrent entre `base` et `tree`.
    pub fn changes(&self, base: &str, tree: &str) -> io::Result<Vec<Change>> {
        let status = self.git(&[
            "diff-tree",
            "-r",
            "--no-renames",
            "--name-status",
            "-z",
            base,
            tree,
        ])?;
        let counts = self.git(&[
            "diff-tree",
            "-r",
            "--no-renames",
            "--numstat",
            "-z",
            base,
            tree,
        ])?;
        let counts = String::from_utf8_lossy(&counts.stdout).into_owned();
        // Un fichier binaire n'a pas de comptes : « - » à la place.
        let lines_of = |path: &str| -> Option<(u32, u32)> {
            counts
                .split('\0')
                .find_map(|entry| {
                    let mut parts = entry.splitn(3, '\t');
                    let (added, removed, file) = (parts.next()?, parts.next()?, parts.next()?);
                    (file == path).then(|| added.parse().ok().zip(removed.parse().ok()))
                })
                .flatten()
        };
        let status = String::from_utf8_lossy(&status.stdout).into_owned();
        let mut fields = status.split('\0').filter(|field| !field.is_empty());
        let mut changes = Vec::new();
        while let (Some(kind), Some(path)) = (fields.next(), fields.next()) {
            let kind = match kind {
                "A" => ChangeKind::Added,
                "D" => ChangeKind::Deleted,
                _ => ChangeKind::Modified,
            };
            changes.push(Change {
                path: path.to_string(),
                kind,
                lines: lines_of(path),
            });
        }
        Ok(changes)
    }

    /// Le patch unifié de `base` à `tree`, tel que le lit le composant de diff.
    pub fn patch(&self, base: &str, tree: &str) -> io::Result<String> {
        let output = self.git(&[
            "diff",
            "--no-renames",
            "--no-color",
            "--no-ext-diff",
            base,
            tree,
        ])?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Remet `paths` dans leur état de `base`, et rien d'autre. Refuse tout si
    /// l'un d'eux a changé depuis l'état `reviewed` montré à l'utilisateur.
    pub fn restore(
        &self,
        base: &str,
        reviewed: &str,
        paths: &[String],
    ) -> Result<(), RestoreError> {
        let now = self.snapshot()?;
        let blob = |tree: &str, path: &str| {
            self.text(&[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{tree}:{path}"),
            ])
            .ok()
        };
        let moved: Vec<String> = paths
            .iter()
            .filter(|path| blob(&now, path) != blob(reviewed, path))
            .cloned()
            .collect();
        if !moved.is_empty() {
            return Err(RestoreError::ChangedSinceReview(moved));
        }
        for path in paths {
            let file = self.project.join(path);
            match blob(base, path) {
                Some(id) => {
                    let content = self.git(&["cat-file", "blob", &id])?.stdout;
                    if let Some(dir) = file.parent() {
                        fs::create_dir_all(dir)?;
                    }
                    fs::write(&file, content)?;
                }
                None => match fs::remove_file(&file) {
                    Err(error) if error.kind() != io::ErrorKind::NotFound => {
                        return Err(error.into());
                    }
                    _ => {}
                },
            }
        }
        Ok(())
    }
}
