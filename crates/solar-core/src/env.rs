//! L'environnement d'une app lancée depuis le Finder : macOS ne lui donne
//! qu'un PATH minimal (`/usr/bin:/bin:/usr/sbin:/sbin`), sans les dossiers où
//! vivent `claude`, `codex`, `grok` et `gh`. Il est complété par celui du shell
//! de connexion de l'utilisateur.

use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const MARK: &str = "__SOLAR_PATH__";

/// `current`, suivi des dossiers de `shell` qu'il n'a pas, sans doublon.
pub fn merge_paths(current: &str, shell: &str) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for dir in current.split(':').chain(shell.split(':')) {
        if !dir.is_empty() && !seen.contains(&dir) {
            seen.push(dir);
        }
    }
    seen.join(":")
}

/// Le PATH écrit entre deux balises, quoi que le shell affiche autour.
pub fn path_from_shell_output(out: &str) -> Option<String> {
    let start = out.find(MARK)? + MARK.len();
    let len = out[start..].find(MARK)?;
    Some(out[start..start + len].to_string())
}

/// Le PATH du shell de connexion, interactif pour lire aussi `.zshrc` ; rien
/// s'il ne répond pas en `timeout`.
pub fn login_shell_path(timeout: Duration) -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let child = Command::new(shell)
        .args([
            "-l",
            "-i",
            "-c",
            &format!("printf '{MARK}%s{MARK}' \"$PATH\""),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let id = child.id();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(child.wait_with_output());
    });
    match receiver.recv_timeout(timeout) {
        Ok(Ok(output)) => path_from_shell_output(&String::from_utf8_lossy(&output.stdout)),
        _ => {
            // Un shell bloqué ne doit pas retenir le démarrage.
            let _ = Command::new("kill").arg(id.to_string()).status();
            None
        }
    }
}
