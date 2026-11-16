//! La preview d'un projet web : retrouver l'adresse d'un serveur de
//! développement, deviner la commande qui le lance, et piloter le seul serveur
//! que Solar a démarré. Un serveur lancé par quelqu'un d'autre n'est jamais
//! arrêté.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

/// La première adresse locale dans un texte (sortie d'un serveur ou d'une
/// commande de Claude), sans les codes de couleur du terminal.
pub fn find_url(text: &str) -> Option<String> {
    let clean = strip_ansi(text);
    [
        "http://localhost:",
        "http://127.0.0.1:",
        "http://0.0.0.0:",
        "http://[::1]:",
    ]
    .iter()
    .filter_map(|prefix| clean.find(prefix).map(|at| (at, prefix)))
    .min_by_key(|(at, _)| *at)
    .and_then(|(at, prefix)| {
        let rest = &clean[at + prefix.len()..];
        let port: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if port.is_empty() {
            return None;
        }
        let path: String = rest[port.len()..]
            .chars()
            .take_while(|c| !c.is_whitespace() && !"\"'`)>,".contains(*c))
            .collect();
        // Un chemin, une requête ou une ancre seulement : `@` ou `.` après le
        // port changeraient l'hôte.
        let path = if path.starts_with(['/', '?', '#']) {
            path
        } else {
            String::new()
        };
        // 0.0.0.0 écoute partout mais ne s'ouvre pas : on passe par localhost.
        Some(format!("http://localhost:{port}{path}"))
    })
}

fn strip_ansi(text: &str) -> String {
    let mut clean = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Une séquence CSI se termine par une lettre.
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            clean.push(c);
        }
    }
    clean
}

/// La commande qui lance le serveur de développement d'un projet web : le
/// script `dev`, `start` ou `preview` de `package.json`, avec le gestionnaire
/// de paquets que désigne le fichier de verrouillage.
pub fn dev_command(project: &Path) -> Option<Vec<String>> {
    let manifest = std::fs::read_to_string(project.join("package.json")).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest).ok()?;
    let script = ["dev", "start", "preview"]
        .into_iter()
        .find(|name| manifest["scripts"][name].is_string())?;
    let manager = [
        ("pnpm-lock.yaml", "pnpm"),
        ("yarn.lock", "yarn"),
        ("bun.lockb", "bun"),
        ("bun.lock", "bun"),
    ]
    .into_iter()
    .find(|(lock, _)| project.join(lock).exists())
    .map_or("npm", |(_, manager)| manager);
    Some(vec![manager.into(), "run".into(), script.into()])
}

/// Ce que dit le serveur lancé par Solar.
#[derive(Debug, PartialEq)]
pub enum ServerEvent {
    Line(String),
    /// Le serveur s'est arrêté, de lui-même ou parce qu'on l'a arrêté.
    Exited,
}

/// Un serveur lancé par Solar, dans son propre groupe de processus : l'arrêter
/// arrête aussi ce qu'il a lancé (npm lance vite, par exemple). Il s'arrête
/// quand on le lâche.
pub struct Server {
    child: Child,
    /// Surveille Solar et arrête le serveur s'il disparaît sans prévenir
    /// (plantage, arrêt forcé) : le serveur ne lui survit jamais.
    guard: Option<Child>,
    stopped: bool,
}

impl Server {
    pub fn start(
        project: &Path,
        command: &[String],
        on_event: impl Fn(ServerEvent) + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        let (program, args) = command
            .split_first()
            .ok_or_else(|| std::io::Error::other("commande vide"))?;
        let mut builder = Command::new(program);
        builder
            .args(args)
            .current_dir(project)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Pas de couleurs : la sortie est lue, pas affichée dans un terminal.
            .env("NO_COLOR", "1")
            .env("FORCE_COLOR", "0")
            .env("BROWSER", "none");
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut builder, 0);
        let mut child = builder.spawn()?;
        let on_event = std::sync::Arc::new(on_event);
        let readers: Vec<Box<dyn std::io::Read + Send>> = vec![
            Box::new(child.stdout.take().expect("sortie redirigée")),
            Box::new(child.stderr.take().expect("erreurs redirigées")),
        ];
        let mut threads = Vec::new();
        for reader in readers {
            let on_event = on_event.clone();
            threads.push(std::thread::spawn(move || {
                for line in BufReader::new(reader).lines().map_while(Result::ok) {
                    on_event(ServerEvent::Line(line));
                }
            }));
        }
        // Quand les deux sorties se ferment, le processus a fini.
        std::thread::spawn(move || {
            for thread in threads {
                let _ = thread.join();
            }
            on_event(ServerEvent::Exited);
        });
        let guard = guard(child.id());
        Ok(Self {
            child,
            guard,
            stopped: false,
        })
    }

    pub fn stop(&mut self) {
        if std::mem::replace(&mut self.stopped, true) {
            return;
        }
        // shortcut: arrêt par groupe de processus, propre à macOS et Linux ;
        // passer par un « job object » quand Windows sera pris en charge.
        #[cfg(unix)]
        let _ = Command::new("kill")
            .args(["-TERM", &format!("-{}", self.child.id())])
            .stderr(Stdio::null())
            .status();
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Le serveur est arrêté : le gardien ne doit plus rien viser, son numéro
        // de groupe pourrait être réattribué.
        if let Some(mut guard) = self.guard.take() {
            let _ = guard.kill();
            let _ = guard.wait();
        }
    }
}

/// Un petit processus qui attend la fin de Solar puis arrête le groupe du
/// serveur. Il ne vit que le temps du serveur.
#[cfg(unix)]
fn guard(group: u32) -> Option<Child> {
    let solar = std::process::id();
    let watch = format!(
        "while kill -0 {solar} 2>/dev/null; do sleep 1; done; kill -TERM -{group} 2>/dev/null"
    );
    Command::new("sh")
        .args(["-c", &watch])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

#[cfg(not(unix))]
fn guard(_group: u32) -> Option<Child> {
    None
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_local_address_is_found_in_server_output() {
        assert_eq!(
            find_url(
                "  \u{1b}[32m➜\u{1b}[39m  Local:   \u{1b}[36mhttp://localhost:\u{1b}[1m5173\u{1b}[22m/\u{1b}[39m"
            ),
            Some("http://localhost:5173/".into())
        );
        assert_eq!(
            find_url("ready - started server on 0.0.0.0:3000, url: http://0.0.0.0:3000"),
            Some("http://localhost:3000".into())
        );
        assert_eq!(
            find_url("Serving at http://127.0.0.1:8000/app)."),
            Some("http://localhost:8000/app".into())
        );
        assert_eq!(find_url("see https://example.com"), None);
        // Ce qui suit le port n'est gardé que s'il commence un chemin : sinon
        // `@` ferait de localhost un identifiant devant un hôte distant.
        assert_eq!(
            find_url("http://localhost:1@example.com/").as_deref(),
            Some("http://localhost:1")
        );
        assert_eq!(
            find_url("http://localhost:5173/?x=1#y").as_deref(),
            Some("http://localhost:5173/?x=1#y")
        );
    }

    #[test]
    fn the_dev_script_and_package_manager_are_found() {
        let dir = std::env::temp_dir().join(format!("solar-preview-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(dev_command(&dir), None, "pas de package.json");
        std::fs::write(
            dir.join("package.json"),
            r#"{"scripts":{"start":"node a.js","dev":"vite"}}"#,
        )
        .unwrap();
        assert_eq!(dev_command(&dir).unwrap(), ["npm", "run", "dev"]);
        std::fs::write(dir.join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(dev_command(&dir).unwrap(), ["pnpm", "run", "dev"]);
    }

    #[test]
    fn a_server_reports_its_lines_and_stops_with_its_children() {
        let dir = std::env::temp_dir();
        let (tx, rx) = std::sync::mpsc::channel();
        // Le serveur lance un enfant qui doit mourir avec lui.
        let script = "sleep 30 & echo \"$!\"; echo 'Local: http://localhost:4321/'; wait";
        let mut server = Server::start(
            &dir,
            &["sh".into(), "-c".into(), script.into()],
            move |event| {
                let _ = tx.send(event);
            },
        )
        .unwrap();
        let next = || rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        let ServerEvent::Line(child) = next() else {
            panic!("pid attendu")
        };
        assert_eq!(
            next(),
            ServerEvent::Line("Local: http://localhost:4321/".into())
        );
        server.stop();
        assert_eq!(next(), ServerEvent::Exited);
        let alive = Command::new("kill")
            .args(["-0", child.trim()])
            .status()
            .unwrap();
        assert!(!alive.success(), "l'enfant du serveur est arrêté avec lui");
    }
}
