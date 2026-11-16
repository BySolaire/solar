Status: ready-for-agent
Blocked by: 02 — Vue Bibliothèque et modèles de projet

# 03 : Consultation des skills

Spec : `docs/tickets/library/spec.md`.

**What to build :** La vue Bibliothèque gagne une section « Skills » en lecture seule : les skills des dossiers de Claude (dont les extensions activées), de Codex et de Grok, avec nom, description, origine et « lu par », groupés par origine, cherchables (et dans ⌘K) ; « Afficher dans le Finder ». Un `SKILL.md` illisible est signalé sans casser la liste.

- [ ] Tests du cœur sur dossiers jetables : skills par dossier, sans description, dossier sans `SKILL.md`, extension désactivée ignorée, agents lecteurs par dossier.
- [ ] Vu à l'écran avec les vrais dossiers de la machine (lecture seule).
- [ ] Plan de développement, design system et README à jour.
