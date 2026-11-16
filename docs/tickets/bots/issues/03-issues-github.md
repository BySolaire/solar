Status: in-progress
Blocked by: 01 — Bot, vue Bots et « Lancer maintenant »

# 03 : Issues GitHub

Spec : `docs/tickets/bots/spec.md`.

**What to build :** Un déclencheur « nouvelle issue avec l'étiquette X » : lecture par `gh` toutes les 5 minutes, issues créées après le bot (option pour les anciennes), une mission par issue ; le numéro, le titre, le texte et l'adresse de l'issue remplissent la consigne (`{issue_number}`, `{issue_title}`, `{issue_body}`, `{issue_url}`).

- [x] Tests du cœur et faux `gh` : filtre par date et étiquette, issues déjà traitées, consigne remplie.
- [ ] Essai réel court sur un dépôt jetable, avec l'accord de Solaire.

## Comments

16 novembre 2026, branche `bots-v1`. Cœur : `Trigger::Issues { label, since }` (`since` absent : aussi les issues déjà ouvertes), `github::Issue` et `Gh::issues` (`gh issue list --label … --state open --limit 100 --json …`, dans le dossier du projet), `new_issues` (pas encore traitées, créées après `since`, les plus anciennes d'abord), `prompt_for` (trous `{issue_number}`, `{issue_title}`, `{issue_body}`, `{issue_url}` ; sans eux, l'issue suit la consigne), `Bot.handled`, `Run.issue`. Shell : lecture toutes les 5 minutes SOLAR ouvert, « Lancer maintenant » relit tout de suite ; une seule nouvelle issue par lecture (un arriéré ne part pas d'un coup) ; l'issue est notée traitée dès que sa mission part ; mission titrée « bot · #n ». Une exécution venue d'un déclencheur qui ne peut pas partir (consigne, mode, dossier) met le bot en pause, pour ne pas se répéter. Vue : déclencheur « Nouvelle issue GitHub », étiquette, « Aussi les issues déjà ouvertes », aide sur les trous ; erreur de `gh` affichée sous le bot (« Issues illisibles : … ») ; « Aucune nouvelle issue » à la main.

Vérifié : trois tests du cœur (filtre, consigne, lecture par faux `gh`) et un sans fenêtre (faux `gh` et faux Claude : l'issue nouvelle part une fois, l'ancienne et la déjà traitée restent de côté) ; à l'écran en 1024 FR (éditeur, erreur `gh` réelle sur un dépôt sans remote). Captures : `/tmp/solar-qa/captures/bots-03/`.

Reste : l'essai réel sur un dépôt GitHub jetable, qui attend l'accord de Solaire.
