# Où les agents lisent leurs skills (16 novembre 2026)

Relevé sur la machine de Solaire pour la vue « Skills » de la bibliothèque (lecture seule). Un skill est un dossier qui contient un `SKILL.md` à en-tête YAML (`name`, `description`).

## Claude Code

- `~/.claude/skills/<nom>/SKILL.md` (31 sur la machine) et `<projet>/.claude/skills/`.
- Les extensions installées : `~/.claude/plugins/cache/<marché>/<extension>/<version>/skills/<nom>/SKILL.md` ; plusieurs versions peuvent coexister, seule l'extension activée (réglage `enabledPlugins`) compte.
- Ce que Claude voit vraiment arrive aussi par son lancement : la liste `commands` de la réponse `initialize`, que Solar lit déjà pour le menu `/` (noms et descriptions, sans chemin).
- `~/.agents/skills` n'est pas lu par Claude Code d'après ce relevé (non vérifié dans sa documentation).

## Codex (0.169)

D'après les chaînes du binaire (`codex-home-skills`, `user-agent-skills`, `system-skills`, `plugins/cache`) et son skill d'installation : `$CODEX_HOME/skills` (`~/.codex/skills`, vide ici), `~/.agents/skills` (14 ici), `.agents/skills` du dépôt, ses skills système et ceux de ses extensions (`~/.codex/plugins/cache`). Ordre de priorité non vérifié.

## Grok (1.0.77)

Sa documentation locale (`~/.grok/docs/user-guide/08-skills.md`) : `./.grok/skills` et `commands`, ceux de la racine du dépôt, `~/.grok/skills`, `~/.claude/skills` et `./.claude/skills` (compatibilité Claude, désactivable), `~/.cursor/skills`, et `.agents/skills` à chaque niveau ; dédoublonnage par nom, le plus prioritaire gagne ; `[skills] disabled` / `ignore` dans `~/.grok/config.toml`. Ses skills intégrés sont dans `~/.grok/bundled/skills`.

## Conséquence pour Solar

La vue liste chaque skill une fois par dossier, avec les agents qui lisent ce dossier d'après ce relevé ; elle ne promet pas qu'un skill est actif (extensions désactivées, `disabled` de Grok, dossiers non approuvés). Les règles par agent restent dans un seul endroit du code, à revoir quand un CLI change.
