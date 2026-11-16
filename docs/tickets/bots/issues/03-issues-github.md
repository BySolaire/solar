Status: ready-for-agent
Blocked by: 01 — Bot, vue Bots et « Lancer maintenant »

# 03 : Issues GitHub

Spec : `docs/tickets/bots/spec.md`.

**What to build :** Un déclencheur « nouvelle issue avec l'étiquette X » : lecture par `gh` toutes les 5 minutes, issues créées après le bot (option pour les anciennes), une mission par issue ; le numéro, le titre, le texte et l'adresse de l'issue remplissent la consigne (`{issue_number}`, `{issue_title}`, `{issue_body}`, `{issue_url}`).

- [ ] Tests du cœur et faux `gh` : filtre par date et étiquette, issues déjà traitées, consigne remplie.
- [ ] Essai réel court sur un dépôt jetable, avec l'accord de Solaire.
