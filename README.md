# Solar

Environnement de travail agentique natif, en Rust et GPUI : piloter Claude Code depuis une app sobre, avec à terme terminal, diff, GitHub, navigateur intégré et bots. macOS d'abord, Windows et Linux ensuite. Codex et Grok viendront après Claude.

## Lancer

```sh
cargo run -p solar          # l'app
cargo test --workspace      # les tests
SOLAR_WINDOW=1024x768 SOLAR_LANG=en cargo run -p solar   # taille de fenêtre et langue forcées, pour les revues visuelles
SOLAR_STATE=/tmp/essai.json cargo run -p solar   # sur un état jetable, sans toucher à ~/.solar
```

Prérequis : Rust et le CLI `claude` installé et connecté. Solar lance ce CLI tel quel, avec ton compte et ta configuration Claude Code (skills, serveurs MCP, plugins, mémoire).

## Structure

- `crates/solar-core` : le cœur sans interface, testable sans fenêtre.
  - `claude.rs` : lance le CLI en mode `stream-json` et traduit sa sortie en événements.
  - `history.rs` : relit les sessions que Claude Code range dans `~/.claude/projects`.
  - `state.rs` : projets, sessions et onglets, dans `~/.solar/state.json`.
- `crates/solar` : l'app GPUI.
  - `shell.rs` : barre latérale des projets, accueil, onglets, import de sessions.
  - `session_view.rs` : une conversation, son fil et sa zone de saisie ; `session_view/home.rs` est son accueil, tant qu'elle n'a pas commencé.
  - `assets.rs` : embarque le logo de `assets/logo` et les icônes Lucide de gpui-kit.
  - `theme.rs` : les tokens (noir, blanc spectral, surfaces fantômes, Space Grotesk) et le verre dépoli de la fenêtre ; voir `docs/design/DESIGN_SYSTEM.md`.
  - `eclipse.rs` : la marque dans l'app, logo de la barre de titre et planète de l'accueil, tirés de `assets/logo`.
  - `locales/app.yml` : tous les textes, en anglais et en français.
- `assets/logo` : le logo, en SVG.
- `assets/fonts/space-grotesk` : la police de l'interface, sous licence OFL.

## Choix techniques

| Besoin | Choix |
|---|---|
| Interface | `gpui-kit` (Apache-2.0), version fixée |
| Claude | le CLI officiel `claude`, piloté en `stream-json` |
| Navigateur (étape 4) | `gpui-webview` (WKWebView sur macOS), validé à l'étape 0 |
| Terminal (étape 2) | `alacritty_terminal` + `portable-pty` (les crates de Zed sont GPL, donc exclus) |
| Git et GitHub | CLI `git` et `gh` |
| Bots (étape 5) | locaux, dans l'app : un agent, une consigne, un déclencheur |

Le protocole `stream-json` et le format des fichiers de session ne sont pas documentés par Anthropic. Solar ignore toute ligne qu'il ne connaît pas, et les tests rejouent des lignes enregistrées lors de vraies sessions (`crates/solar-core/tests/fixtures/`) : à réenregistrer si une mise à jour du CLI casse quelque chose.

Le code emprunté à Monocode Native (MIT) sera crédité dans un fichier `NOTICE` au moment de l'emprunt.

## Étapes

- [x] 0. Faisabilité : fenêtre, échange avec les agents, webview
- [x] 1. Chat : projets, sessions en onglets, streaming, outils repliés, permissions, modèle, effort, images, menus `/` et `@`, historique, import
- [ ] 2. Missions : plan préparé sans écriture, corrigé puis lancé (premier incrément fait)
- [ ] 2 bis. Revue : diff, worktrees
- [ ] 3. GitHub : PR, checks, issues
- [ ] 4. Navigateur : onglets, barre d'adresse
- [ ] 5. Bots : déclencheurs, exécutions, notifications
- [ ] 6. Diffusion : build signé, mise à jour, puis Windows et Linux
