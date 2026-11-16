//! Les fichiers d'un projet, en lecture seule : la vue Fichiers et les mentions
//! `@` s'en servent. Rien n'est jamais écrit.

use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::Command;

/// Au-delà, un projet sans git n'est pas parcouru plus loin.
const WALK_LIMIT: usize = 10_000;
/// Dossiers qu'un parcours sans git saute : dépendances et produits de build.
const SKIPPED: [&str; 2] = ["target", "node_modules"];

/// Les fichiers du projet, relatifs à sa racine et triés. Dans un dépôt git :
/// les fichiers suivis et les nouveaux non ignorés. Sinon, un parcours du
/// dossier sans les dossiers cachés ni ceux de `SKIPPED`, borné à `WALK_LIMIT`.
pub fn list(root: &Path) -> Vec<String> {
    let mut git = Command::new("git");
    // La copie d'une mission : son dépôt épinglé, jamais celui que désignerait
    // un `.git` réécrit. Et jamais la commande de surveillance de la config.
    if let Ok(Some(copy)) = crate::checkpoint::CopyGit::of(root) {
        copy.pin(&mut git);
    }
    let git = git
        .args(["-c", "core.fsmonitor=false"])
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .current_dir(root)
        .output();
    let mut files: Vec<String> = match git {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .split('\0')
            .filter(|file| !file.is_empty())
            .map(str::to_string)
            .collect(),
        _ => {
            let mut found = Vec::new();
            walk(root, root, &mut found);
            found
        }
    };
    files.sort();
    files.dedup();
    files
}

fn walk(root: &Path, dir: &Path, found: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if found.len() >= WALK_LIMIT {
            return;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() && !SKIPPED.contains(&name.as_str()) => {
                walk(root, &path, found)
            }
            Ok(kind) if kind.is_file() => {
                if let Ok(inside) = path.strip_prefix(root) {
                    found.push(inside.to_string_lossy().to_string());
                }
            }
            _ => {}
        }
    }
}

/// Le contenu d'un fichier tel qu'on peut l'afficher.
#[derive(Debug, PartialEq)]
pub enum Content {
    /// Texte, coupé à la limite demandée si `truncated`.
    Text {
        text: String,
        truncated: bool,
    },
    Binary,
    Unreadable(String),
}

/// Lit au plus `limit` octets. Un octet nul dans le début du fichier le désigne
/// comme binaire.
pub fn read(path: &Path, limit: usize) -> Content {
    let mut bytes = Vec::new();
    let read =
        fs::File::open(path).and_then(|file| file.take(limit as u64 + 1).read_to_end(&mut bytes));
    if let Err(error) = read {
        return Content::Unreadable(error.to_string());
    }
    if bytes.iter().take(8192).any(|&byte| byte == 0) {
        return Content::Binary;
    }
    let truncated = bytes.len() > limit;
    bytes.truncate(limit);
    Content::Text {
        text: String::from_utf8_lossy(&bytes).into_owned(),
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_without_git_is_walked_without_hidden_or_build_folders() {
        let root = std::env::temp_dir().join(format!("solar-files-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for file in [
            "src/main.rs",
            "README.md",
            ".git-like/x",
            "target/debug/app",
            "node_modules/a/b.js",
        ] {
            let path = root.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "x").unwrap();
        }
        assert_eq!(list(&root), ["README.md", "src/main.rs"]);
    }

    #[test]
    fn text_is_cut_at_the_limit_and_binary_is_recognised() {
        let root = std::env::temp_dir().join(format!("solar-read-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("long.txt"), "abcdef").unwrap();
        fs::write(root.join("image.bin"), [0x89, b'P', 0, 1]).unwrap();
        assert_eq!(
            read(&root.join("long.txt"), 4),
            Content::Text {
                text: "abcd".into(),
                truncated: true
            }
        );
        assert_eq!(
            read(&root.join("long.txt"), 10),
            Content::Text {
                text: "abcdef".into(),
                truncated: false
            }
        );
        assert_eq!(read(&root.join("image.bin"), 10), Content::Binary);
        assert!(matches!(
            read(&root.join("absent"), 10),
            Content::Unreadable(_)
        ));
    }
}
