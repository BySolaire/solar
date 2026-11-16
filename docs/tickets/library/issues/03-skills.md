Status: resolved
Blocked by: 02 — Vue Bibliothèque et modèles de projet

# 03 : Consultation des skills

Spec : `docs/tickets/library/spec.md`.

**What to build :** La vue Bibliothèque gagne une section « Skills » en lecture seule : les skills des dossiers de Claude (dont les extensions activées), de Codex et de Grok, avec nom, description, origine et « lu par », groupés par origine, cherchables (et dans ⌘K) ; « Afficher dans le Finder ». Un `SKILL.md` illisible est signalé sans casser la liste.

- [x] Tests du cœur sur dossiers jetables : skills par dossier, sans description, dossier sans `SKILL.md`, extension désactivée ignorée, agents lecteurs par dossier.
- [x] Vu à l'écran avec les vrais dossiers de la machine (lecture seule).
- [x] Plan de développement, design system et README à jour.

## Comments

16 novembre 2026, branche `library-v1`. Cœur : `library::skills(home, projet)` : dossiers du projet (`.claude`, `.agents`, `.grok`), `~/.claude/skills`, extensions de Claude activées (`enabledPlugins` de `~/.claude/settings.json`, dernière version du cache, skills rangés jusqu'à trois niveaux), `~/.codex/skills` (ou `CODEX_HOME`), `~/.agents/skills`, `~/.grok/skills` ; en-tête YAML lu sans dépendance, description sur plusieurs lignes (`>`, `|`) comprise ; dossier sans `SKILL.md` ignoré, en-tête absent signalé ; lecteurs par dossier. Interface : section « Skills » de la vue, groupée par origine, « lu par », description sur deux lignes, « Afficher dans le Finder » ; skills dans la catégorie « Bibliothèque » de ⌘K (ouvrent la vue). Corrigé au passage : la vue ne défilait pas (conteneur flex), elle défile comme les Réglages. Vu à l'écran avec les vrais dossiers de la machine (lecture seule). Docs : plan, design system, README.
