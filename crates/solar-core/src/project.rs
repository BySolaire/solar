//! Ce que la fiche d'un projet lit : ses conventions, l'état de son dépôt git,
//! et les copies de travail de ses missions. Tout est en lecture, sauf
//! l'enregistrement d'une convention, qui refuse d'écraser un changement fait
//! ailleurs.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

/// Les fichiers d'instructions que lisent les agents, dans l'ordre affiché.
/// Les deux premiers sont proposés même absents : on peut les créer.
pub const CONVENTIONS: [&str; 8] = [
    "CLAUDE.md",
    "AGENTS.md",
    "CLAUDE.local.md",
    ".claude/CLAUDE.md",
    "GEMINI.md",
    ".cursorrules",
    ".windsurfrules",
    ".github/copilot-instructions.md",
];

#[derive(Clone, Debug, PartialEq)]
pub struct Convention {
    pub path: &'static str,
    /// Taille et date de modification ; absent : le fichier n'existe pas.
    pub file: Option<(u64, SystemTime)>,
}

/// Les conventions présentes, plus CLAUDE.md et AGENTS.md même absents.
pub fn conventions(root: &Path) -> Vec<Convention> {
    CONVENTIONS
        .iter()
        .enumerate()
        .filter_map(|(ix, path)| {
            let file = fs::metadata(root.join(path))
                .ok()
                .filter(|meta| meta.is_file())
                .map(|meta| {
                    (
                        meta.len(),
                        meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    )
                });
            (file.is_some() || ix < 2).then_some(Convention { path, file })
        })
        .collect()
}

#[derive(Debug)]
pub enum SaveError {
    /// Le fichier a changé sur le disque depuis qu'il a été ouvert : rien n'est écrit.
    ChangedElsewhere,
    Io(io::Error),
}

/// Écrit `text` dans `file`, à condition qu'il contienne encore `opened`, ce
/// qui était lu à l'ouverture (`None` : il n'existait pas).
pub fn save(file: &Path, opened: Option<&str>, text: &str) -> Result<(), SaveError> {
    let now = fs::read_to_string(file).ok();
    if now.as_deref() != opened {
        return Err(SaveError::ChangedElsewhere);
    }
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(SaveError::Io)?;
    }
    fs::write(file, text).map_err(SaveError::Io)
}

/// Les dossiers où un projet range ses images, explorés sur quelques niveaux.
const IMAGE_DIRS: [&str; 10] = [
    "assets",
    "public",
    "static",
    "resources",
    "images",
    "icons",
    "src/assets",
    "app",
    ".github",
    "docs",
];
/// Jamais fouillés : dépendances, produits de build, archives et images de
/// référence (ce ne sont pas celles du projet).
const SKIPPED: [&str; 9] = [
    "node_modules",
    "target",
    ".git",
    "archive",
    "dist",
    "build",
    "references",
    "reference",
    "comparison",
];
/// Au-delà, ce n'est plus un logo mais une illustration.
const LOGO_MAX: u64 = 1024 * 1024;

/// Le logo du projet s'il en a un : l'image dont le nom dit « icône », « logo »,
/// « favicon » ou « symbole », à peu près carrée (un mot-symbole ou une bannière
/// ne tient pas dans une icône). L'icône d'application passe d'abord, le logo
/// en dernier. À rang égal, un PNG proche de 64 pixels passe avant un SVG :
/// GPUI ne dessine pas tous les SVG (flous, filtres), une image se montre
/// toujours telle quelle. Puis le chemin le plus court.
pub fn logo(root: &Path) -> Option<PathBuf> {
    let mut found = Vec::new();
    collect_images(root, 0, &mut found);
    for dir in IMAGE_DIRS {
        collect_images(&root.join(dir), 1, &mut found);
    }
    found
        .into_iter()
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().to_lowercase();
            let rank = if ["app-icon", "appicon", "macos-icon", "icon_"]
                .iter()
                .any(|k| name.contains(k))
            {
                0
            } else if name.contains("favicon") {
                1
            } else if name.contains("symbol") {
                2
            } else if name.contains("icon") {
                3
            } else if name.contains("logo") && !name.contains("wordmark") {
                4
            } else {
                return None;
            };
            let size = dimensions(&path);
            let square = size.is_none_or(|(w, h)| w.max(h) <= w.min(h) * 3 / 2);
            let svg = path.extension().is_some_and(|ext| ext == "svg");
            // L'écart à 64 pixels pour un PNG ; un SVG passe après.
            let fit = if svg {
                u32::MAX
            } else {
                size.map_or(u32::MAX - 1, |(w, h)| w.max(h).abs_diff(64))
            };
            let small = fs::metadata(&path).is_ok_and(|meta| meta.len() <= LOGO_MAX);
            (square && small).then(|| ((rank, fit, path.as_os_str().len()), path))
        })
        .min_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, path)| path)
}

/// Les SVG et PNG de `dir`, en descendant jusqu'à six niveaux.
fn collect_images(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if depth > 0 && depth < 6 && !SKIPPED.iter().any(|skip| name == *skip) {
                collect_images(&path, depth + 1, found);
            }
        } else if path
            .extension()
            .is_some_and(|ext| ext == "svg" || ext == "png")
        {
            found.push(path);
        }
    }
}

/// La largeur et la hauteur d'une image, lues dans l'en-tête d'un PNG ou le
/// `viewBox` d'un SVG ; `None` si elles ne se lisent pas.
fn dimensions(path: &Path) -> Option<(u32, u32)> {
    if path.extension().is_some_and(|ext| ext == "png") {
        let mut header = [0u8; 24];
        std::io::Read::read_exact(&mut fs::File::open(path).ok()?, &mut header).ok()?;
        let read =
            |at: usize| u32::from_be_bytes(header[at..at + 4].try_into().unwrap_or_default());
        return Some((read(16), read(20)));
    }
    let text = fs::read_to_string(path).ok()?;
    let start = text.find("viewBox=\"")? + "viewBox=\"".len();
    let numbers: Vec<f32> = text[start..]
        .split('"')
        .next()?
        .split([' ', ','])
        .filter_map(|part| part.trim().parse().ok())
        .collect();
    match numbers[..] {
        [_, _, w, h] if w > 0. && h > 0. => Some((w as u32, h as u32)),
        _ => None,
    }
}

/// L'état du dépôt, lu sans rien verrouiller ni modifier.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GitSummary {
    pub branch: Option<String>,
    pub changed: usize,
    pub untracked: usize,
    /// Identifiant court, sujet et date du dernier commit.
    pub last_commit: Option<(String, String, SystemTime)>,
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        // Une lecture ne doit ni prendre le verrou de l'index ni lancer de hook.
        .args(["--no-optional-locks", "-c", "core.fsmonitor=false", "-C"])
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `None` : le dossier n'est pas dans un dépôt git.
pub fn git_summary(root: &Path) -> Option<GitSummary> {
    let status = git(root, &["status", "--porcelain=v1", "--branch", "--", "."])?;
    let mut summary = GitSummary::default();
    for line in status.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            // « main...origin/main [ahead 1] », ou « No commits yet on main ».
            let head = head.strip_prefix("No commits yet on ").unwrap_or(head);
            let name = head.split("...").next().unwrap_or(head);
            summary.branch = Some(name.split_whitespace().next().unwrap_or(name).to_string());
        } else if line.starts_with("??") {
            summary.untracked += 1;
        } else if !line.is_empty() {
            summary.changed += 1;
        }
    }
    summary.last_commit = git(root, &["log", "-1", "--format=%h%x00%s%x00%ct"]).and_then(|out| {
        let mut parts = out.trim_end().splitn(3, '\0');
        let (hash, subject) = (parts.next()?.to_string(), parts.next()?.to_string());
        let secs: u64 = parts.next()?.parse().ok()?;
        Some((
            hash,
            subject,
            SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs),
        ))
    });
    Some(summary)
}

/// Une copie de travail d'une mission de ce projet, présente sur le disque.
#[derive(Clone, Debug, PartialEq)]
pub struct Copy {
    /// Le dossier de la copie (`…/worktrees/<id>`).
    pub root: PathBuf,
    pub bytes: u64,
}

/// Les copies rangées dans `worktrees` qui appartiennent au dépôt de `project`.
pub fn copies(worktrees: &Path, project: &Path) -> Vec<Copy> {
    let common = |dir: &Path| {
        git(
            dir,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )
        .map(|out| PathBuf::from(out.trim()))
        .and_then(|path| fs::canonicalize(path).ok())
    };
    let Some(repo) = common(project) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(worktrees) else {
        return Vec::new();
    };
    let mut copies: Vec<Copy> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|dir| dir.is_dir() && common(dir).as_ref() == Some(&repo))
        .map(|root| Copy {
            bytes: size(&root),
            root: fs::canonicalize(&root).unwrap_or(root),
        })
        .collect();
    copies.sort_by(|a, b| a.root.cmp(&b.root));
    copies
}

/// La place occupée par un dossier ; les liens ne sont pas suivis.
fn size(dir: &Path) -> u64 {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => size(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map_or(0, |meta| meta.len()),
            _ => 0,
        })
        .sum()
}
