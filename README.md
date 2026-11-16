# Solar

Environnement de travail agentique natif, en Rust et GPUI : piloter Claude Code, Codex et Grok depuis une app sobre, sans terminal visible. Conversations et missions (plan, exécution dans une copie de travail, revue, relecture par un second agent), preview web, recherche, réglages, fiche projet et publication sur GitHub (branche, commit, pull request, checks), bibliothèque de modèles de messages et consultation des skills ; à terme bots. macOS d'abord, Windows et Linux ensuite.

## Lancer

```sh
cargo run -p solar          # l'app
cargo test --workspace      # les tests
SOLAR_WINDOW=1024x768 SOLAR_LANG=en cargo run -p solar   # taille de fenêtre et langue forcées, pour les revues visuelles
SOLAR_STATE=/tmp/essai.json cargo run -p solar   # sur un état jetable, sans toucher à ~/.solar
```

Prérequis : Rust et le CLI `claude` installé et connecté. `codex` et `grok` sont facultatifs : installés et connectés, ils deviennent des agents au choix. Solar lance ces CLI tels quels, avec ton compte et leur configuration (skills, serveurs MCP, plugins, mémoire).

## Structure

- `crates/solar-core` : le cœur sans interface, testable sans fenêtre.
  - `claude.rs` : lance le CLI en mode `stream-json` et traduit sa sortie en événements.
  - `codex.rs`, `grok.rs` : la commande et la traduction de Codex et Grok, pilotés un processus par tour (`turns.rs`).
  - `agent.rs` : les fournisseurs (Claude, Codex, Grok) derrière les mêmes gestes, leurs modèles et leur état.
  - `history.rs` : relit les sessions que Claude Code range dans `~/.claude/projects`.
  - `checkpoint.rs` : instantanés privés et copies de travail (worktrees) des missions, sans toucher au dépôt de l'utilisateur.
  - `preview.rs`, `files.rs`, `project.rs`, `search.rs` : preview web, fichiers, fiche projet, recherche.
  - `state.rs` : projets, sessions, onglets, missions et réglages, dans `~/.solar/state.json`.
- `crates/solar` : l'app GPUI.
  - `shell.rs` : barre latérale des projets, accueil, onglets, import de sessions ; `shell/palette.rs` (recherche ⌘K), `shell/settings.rs` (réglages ⌘,), `shell/card.rs` (fiche projet).
  - `session_view.rs` : une conversation, son fil et sa zone de saisie ; `session_view/home.rs` est son accueil tant qu'elle n'a pas commencé ; `pickers.rs` les menus du composer ; `workspace.rs`, `review.rs`, `reviewer.rs`, `preview.rs` l'espace de travail d'une mission.
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
| Codex, Grok | `codex exec --json` et `grok -p --output-format streaming-json`, un processus par tour |
| Preview | `gpui-webview` 0.9.1 (WKWebView sur macOS, par `lb-wry`) |
| Terminal | aucun terminal visible : les commandes des agents sont montrées comme des actions structurées |
| Git et GitHub | CLI `git` et `gh` |
| Bots (à venir) | locaux, dans l'app : un agent, une consigne, un déclencheur |

Ces protocoles et les formats des fichiers de session ne sont pas documentés par leurs éditeurs. Solar ignore toute ligne qu'il ne connaît pas, et les tests rejouent des lignes enregistrées lors de vraies sessions (`crates/solar-core/tests/fixtures/`) : à réenregistrer si une mise à jour du CLI casse quelque chose.

Le code emprunté à Monocode Native (MIT) sera crédité dans un fichier `NOTICE` au moment de l'emprunt ; ses valeurs de design (rayons des menus) en sont seulement inspirées.

## Étapes

Détail et état de chaque phase dans `docs/design/DEVELOPMENT_PLAN.md`.

- [x] 0. Faisabilité : fenêtre, échange avec les agents, webview
- [x] 1. Chat : projets, sessions en onglets, streaming, outils repliés, permissions, modèle, effort, images, menus `/` et `@`, historique, import
- [x] 2. Missions : plan préparé sans écriture, corrigé puis lancé dans le mode choisi
- [x] 3. Espace de travail : Conversation, Plan, Fichiers
- [x] 4. Revue : diff exact, validation, accepter ou restaurer
- [x] 5. Preview web intégrée
- [x] 6. Plusieurs agents : relecteur, tentatives, missions parallèles dans leur copie, Codex et Grok
- [ ] 7. Gestion et extensions : recherche, réglages, fiche projet, publication GitHub et bibliothèque faits ; bots à venir
- [ ] 8. Diffusion : build signé, mise à jour, puis Windows et Linux
