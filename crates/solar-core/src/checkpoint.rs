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
    /// Chaque copie de travail a son index : deux missions en parallèle ne se
    /// gênent pas.
    index: PathBuf,
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
        let git_dir = dir.join(format!("{name}.git"));
        Self {
            index: git_dir.join("index"),
            git_dir,
            project,
        }
    }

    /// Le même dépôt privé, sur la copie de travail d'une mission.
    pub fn in_worktree(&self, worktree: &Path) -> Self {
        let name: String = worktree
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();
        Self {
            git_dir: self.git_dir.clone(),
            project: worktree.to_path_buf(),
            index: self.git_dir.join(format!("index-{name}")),
        }
    }

    /// Le git de l'utilisateur, sans ses hooks : SOLAR n'y lance rien.
    fn user_git(&self, dir: &Path, args: &[&str]) -> io::Result<String> {
        let output = Command::new("git")
            .args(["-c", "core.hooksPath=/dev/null", "-C"])
            .arg(dir)
            .args(args)
            .output()?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(io::Error::other(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ))
        }
    }

    /// Une copie de travail pour une mission : un worktree git détaché, hors du
    /// projet, qui reprend aussi les changements non commités. Rend l'instantané
    /// de départ et le dossier où travailler ; rien si le projet n'est pas dans
    /// un dépôt git avec un commit.
    pub fn isolate(&self, id: &str) -> io::Result<Option<(String, PathBuf)>> {
        if self
            .user_git(&self.project, &["rev-parse", "--verify", "--quiet", "HEAD"])
            .is_err()
        {
            return Ok(None);
        }
        let prefix = self.user_git(&self.project, &["rev-parse", "--show-prefix"])?;
        let base = self.checkpoint()?;
        // Un chemin court : Claude range ses conversations d'après lui.
        let root = self.git_dir.with_file_name("worktrees").join(id);
        fs::create_dir_all(root.parent().unwrap_or(&root))?;
        let root_arg = root.to_string_lossy();
        self.user_git(
            &self.project,
            &["worktree", "add", "--detach", "--quiet", &root_arg, "HEAD"],
        )?;
        let root = fs::canonicalize(root)?;
        // « app/ » dans un sous-dossier, vide à la racine du dépôt.
        let dir = match prefix.trim_end_matches('/') {
            "" => root,
            prefix => root.join(prefix),
        };
        let copy = self.in_worktree(&dir);
        let start = copy.snapshot()?;
        let paths: Vec<String> = copy
            .changes(&start, &base)?
            .into_iter()
            .map(|change| change.path)
            .collect();
        copy.write(&base, &paths)?;
        Ok(Some((base, dir)))
    }

    /// Retire la copie de travail d'une mission ; le projet n'est pas touché.
    pub fn remove_worktree(&self, dir: &Path) -> io::Result<()> {
        let _ = fs::remove_file(&self.in_worktree(dir).index);
        let Ok(root) = self.user_git(dir, &["rev-parse", "--show-toplevel"]) else {
            return Ok(()); // déjà retirée
        };
        let removed = self.user_git(&self.project, &["worktree", "remove", "--force", &root]);
        if removed.is_err() {
            match fs::remove_dir_all(&root) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
            let _ = self.user_git(&self.project, &["worktree", "prune"]);
        }
        Ok(())
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
            .env("GIT_INDEX_FILE", &self.index)
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

    /// Met `paths` dans leur état de `target`, et rien d'autre. Refuse tout si
    /// l'un d'eux diffère de `expected`, l'état que l'utilisateur a vu : pour
    /// restaurer, `target` est la base et `expected` l'état examiné ; pour
    /// appliquer le résultat d'une copie de travail, l'inverse.
    pub fn put(&self, target: &str, expected: &str, paths: &[String]) -> Result<(), RestoreError> {
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
            .filter(|path| blob(&now, path) != blob(expected, path))
            .cloned()
            .collect();
        if !moved.is_empty() {
            return Err(RestoreError::ChangedSinceReview(moved));
        }
        Ok(self.write(target, paths)?)
    }

    /// Écrit `paths` dans leur état de `tree` ; un fichier absent de `tree`
    /// est supprimé.
    fn write(&self, tree: &str, paths: &[String]) -> io::Result<()> {
        for path in paths {
            let file = self.project.join(path);
            // « <mode> blob <id>\t<chemin> », vide si le fichier n'y est pas.
            let entry = self.text(&["ls-tree", tree, "--", path])?;
            let mut fields = entry.split_whitespace();
            match (fields.next(), fields.nth(1)) {
                (Some(mode), Some(id)) => {
                    // shortcut: un lien symbolique est écrit comme un fichier
                    // contenant sa cible ; à traiter si une mission en crée.
                    let content = self.git(&["cat-file", "blob", id])?.stdout;
                    if let Some(dir) = file.parent() {
                        fs::create_dir_all(dir)?;
                    }
                    fs::write(&file, content)?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let bits = if mode == "100755" { 0o755 } else { 0o644 };
                        fs::set_permissions(&file, fs::Permissions::from_mode(bits))?;
                    }
                }
                _ => match fs::remove_file(&file) {
                    Err(error) if error.kind() != io::ErrorKind::NotFound => {
                        return Err(error);
                    }
                    _ => {}
                },
            }
        }
        Ok(())
    }
}
