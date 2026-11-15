# SOLAR — Audit initial du dépôt

Inspection statique du 14 novembre 2026. Les chemins ci-dessous sont relatifs à la racine du projet. Les fonctionnalités sont identifiées dans le code ; aucun parcours réel avec un compte Claude n'a été exécuté pour cette livraison documentaire.

## État du dépôt

- Projet local : `le dossier du dépôt sur la machine du mainteneur`.
- Branche observée : `master`, sans commit initial.
- Les 23 fichiers applicatifs et de configuration inventoriés sont non suivis par Git à l'inspection ; `target/` existe et est ignoré.
- Aucun `CLAUDE.md`, dossier `docs/` ou fichier `AGENTS.md` n'a été trouvé dans le dépôt ; aucun `AGENTS.md` n'a été trouvé dans ses répertoires parents inspectés.
- Une comparaison des empreintes avant/après est nécessaire : un diff Git seul ne suffit pas à garantir la préservation de fichiers non suivis.

Cet état est un instantané, à relire avant toute implémentation. Ne pas créer de commit, nettoyer les fichiers non suivis ou réinitialiser le dépôt comme effet de bord.

## Stack réellement déclarée

| Couche | Existant | Conséquence pour la suite |
|---|---|---|
| Workspace | Cargo, resolver 3, Rust édition 2024, version 0.0.1 | Conserver les deux crates et le lockfile |
| Interface native | `crates/solar`, `gpui-kit = "=0.9.1"` | Réutiliser GPUI ; pas de migration React, Electron ou Tauri |
| Cœur sans fenêtre | `crates/solar-core` | Y placer les futurs modèles et règles métier testables |
| Exécution Claude | Processus local `claude`, échanges JSON via stdin/stdout | Préserver cet adaptateur et ses événements |
| Concurrence UI | `smol` 2, canal entre lecture du processus et entités GPUI | Ne pas bloquer le rendu avec orchestration ou scan |
| Sérialisation | `serde` 1, `serde_json` 1 ; `base64` 0.22 dans le cœur | Évolutions de stockage compatibles |
| Langues | `rust-i18n` 4, `sys-locale` 0.3 ; `locales/app.yml` FR/EN | Tout nouveau texte UI doit avoir ses deux traductions |
| Identité | SVG éclipse, motif animé et thème sombre translucide | Étendre cette identité sans remplacer les assets |

Il n'existe pas de frontend web ni de routeur web dans les fichiers inspectés. `gpui-webview`, `alacritty_terminal`, `portable-pty` et les intégrations GitHub apparaissent comme intentions dans le README, pas comme dépendances actuelles des manifestes. Le « navigateur validé » annoncé dans le README n'est pas une preview intégrée visible dans le code présent.

## Cartographie des responsabilités

| Fichier | Responsabilité observée |
|---|---|
| `crates/solar/src/main.rs` | Initialisation GPUI, langue système FR/EN, thème, fenêtre minimum 720 × 480, fermeture Cmd+Q |
| `crates/solar/src/shell.rs` | Projets par dossier, sessions, onglets, import, synchronisation de l'état et écran vide |
| `crates/solar/src/session_view.rs` | Conversation, saisie, streaming, outils repliables, permissions, images, modèle/effort, suggestions `/` et `@` |
| `crates/solar/src/theme.rs` | Palette et adaptation du thème GPUI ; flou natif et surfaces translucides |
| `crates/solar/src/eclipse.rs` | Éclipse animée en caractères, logo natif et wordmark ; pas un terminal |
| `crates/solar/locales/app.yml` | Textes FR/EN |
| `crates/solar-core/src/claude.rs` | Processus Claude, contrôle, permissions, parsing en événements |
| `crates/solar-core/src/history.rs` | Lecture/import des sessions Claude, classement par date |
| `crates/solar-core/src/state.rs` | Projets, sessions, identifiants d'onglets ; JSON local |
| `assets/logo/solar-icon.svg`, `solar-glyph.svg` | Icône détaillée et symbole à plat existants |

## Capacités présentes et limites

| Domaine | Présent dans les sources | Limite ou cible nouvelle |
|---|---|---|
| Projets | Sélection native de dossiers, liste des sessions par projet | Pas de fiche projet ni de contexte structuré persistant |
| Conversations | Envoi, Markdown en streaming, outils/résultats dépliables | Pas de modèle de mission ; le texte du raisonnement n'est pas exposé |
| Sessions | Import et relecture d'historique, reprise par identifiant Claude | Pas de garantie de reprise d'une exécution interrompue |
| Navigation | Onglets de sessions restaurés, sidebar des projets | Sidebar actuellement non rétractable ; pas de Home globale avec composer autonome |
| Modèles | Catalogue reçu du CLI, effort selon modèle | Un seul provider implémenté : Claude |
| Permissions | Autoriser/refuser une demande d'outil, sélecteur de modes | Les nouvelles sessions commencent actuellement en `bypassPermissions` |
| Pièces jointes | Collage/dépôt d'images, limite locale de 5 Mio par image | Pas d'attachement de documents arbitraires ; envoi sans texte actuellement ignoré |
| Suggestions | `/` et `@`, complétion par Tab | `@` liste seulement les fichiers suivis par `git ls-files` ; aucun fichier proposé dans un dépôt entièrement non suivi |
| Contrôle | Interruption, notes d'erreur, coût/tokens reçus | Pas de pause durable, budget de mission ou supervision multi-agents |
| État | JSON atomique, fichier invalide mis de côté en `.json.bad` | Ne stocke pas missions, tâches, brouillons, panneau actif ou approbations |
| Revue/preview | Aucun espace dédié trouvé | Explorer, diff, checkpoints et preview restent à construire |

### Stockage et cycle de vie à préserver

- `State::default_path()` utilise `SOLAR_STATE` si défini, sinon `~/.solar/state.json`.
- `State` contient `projects` et `open_tabs` ; un `Project` contient `path` et `sessions`, une `Session` contient l'identifiant Claude et le titre.
- L'historique est lu dans `CLAUDE_CONFIG_DIR/projects` ou `~/.claude/projects`, via un nom dérivé du chemin canonique. C'est une convention observée dans l'adaptateur, à revalider selon le CLI installé.
- Un processus Claude est activé lorsqu'un onglet est affiché. Le `Drop` de l'adaptateur tue le processus : fermer une session n'est donc pas une simple navigation neutre.
- Un résultat de tour n'est pas un résultat de mission. Une réouverture de conversation n'établit pas que le travail interrompu continue.
- Les lignes avec `parent_tool_use_id` sont ignorées par le parser en direct ; l'historique exclut les lignes `isSidechain`. Les sous-agents Claude ne sont pas un moteur d'orchestration SOLAR déjà disponible.

### Écarts avec le nouveau brief

1. **Terminal** : envisagé par le README, absent de l'UI inspectée. La nouvelle décision exclut toute vue terminal. Les processus internes restent possibles.
2. **Approbation humaine** : une UI de permission existe, mais le défaut `MODES[0] = bypassPermissions` ne doit pas être décrit comme un mode contrôlé. Prévoir une évolution explicite, documentée et testée ; ne pas changer silencieusement les préférences enregistrées ou présumer que le simple affichage d'un plan empêche des écritures.
3. **Identité visuelle** : le thème actuel utilise `#F7F5ED`, `#E9B86B`, `#1B1B1F`, `#0A0A0C`, des opacités et du flou. La palette Aerospace proposée est une cible, pas l'état actuel.
4. **Desktop/mobile** : fenêtre native macOS d'abord, minimum 720 × 480. L'expérience mobile complémentaire est une vision ultérieure, sans client mobile existant.
5. **Fonctions annoncées** : worktrees, diff, navigateur, GitHub et bots sont des étapes du README. Ne pas transformer une case de roadmap en preuve d'implémentation.

## Vérification existante repérée

- `solar-core/tests/claude.rs` : parsing de fixture réelle nettoyée, interruption, catalogue.
- `solar-core/tests/history.rs` : reconstruction du fil, filtrage des messages techniques.
- `solar-core/tests/state.rs` : aller-retour de stockage et préservation d'un JSON corrompu.
- `solar/src/session_view.rs` : tests GPUI avec un faux CLI pour streaming/permissions et complétions `/`/`@`.
- `solar/src/eclipse.rs` : géométrie/lumière et animation de l'éclipse.

Ces tests n'ont pas été exécutés dans la livraison documentaire. Aucun build ni test visuel n'est revendiqué. Pour de futurs changements applicatifs, suivre la [matrice de validation](DEVELOPMENT_PLAN.md#validation-par-type-de-changement).
