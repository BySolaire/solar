Status: resolved
Blocked by: 10 — Fin de vie et tentatives

# 11 : Essai réel et revue de sécurité

Spec : `docs/tickets/github/spec.md`.

**What to build :** Le parcours complet est éprouvé sur un dépôt GitHub privé d'essai (créé par Solaire ou avec son accord, jamais sur ses dépôts de travail), avec une CI minimale qui échoue puis passe. Revue `security-audit` ciblée sur le lancement de `gh` et `git`, le réseau et l'écriture dans le dépôt. Plan de développement, design system et README mis à jour avec ce qui est livré et ses limites.

- [x] Dépôt d'essai fourni ou autorisé par Solaire (intervention humaine).
- [x] Publication, PR, check en échec envoyé à l'agent, correction poussée, check réussi, merge fait par Solaire sur GitHub, copie retirée.
- [x] Constats de la revue de sécurité corrigés ou notés.
- [x] Docs à jour ; captures FR/EN en 1024 et 720.

## Comments

16 novembre 2026. Dépôt d'essai privé `BySolaire/solar-essai` créé par l'agent avec l'accord de Solaire (README, `check.sh`, CI GitHub Actions qui attend « Bonjour, monde ! »). Parcours réel avec Haiku, SOLAR sur un état jetable : plan puis mission en copie, « Bonjour ! » écrit, commit et push (`solar/ajouter-bonjour-txt`), PR #1, check « accueil » en échec avec son journal, envoyé à l'agent, qui lit `check.sh` et corrige, second commit poussé sur la PR, checks réussis (dont GitGuardian), merge fait sur GitHub à la place de Solaire, « PR #1 mergée », copie retirée ; projet de l'utilisateur intact.

Constats corrigés pendant l'essai : titre de conversation coupé au milieu d'un mot repris dans la branche et le commit (le titre du plan sert désormais, « Plan : » retiré) ; chemin local de la copie recopié dans la description de la PR (retiré) ; journal réel avec marque d'ordre, couleurs ANSI et marqueurs `##[group]` (nettoyés) ; checks pas encore créés juste après le push, le suivi s'arrêtait (il attend quelques minutes) ; message d'une correction identique au premier (« Correction : … ») ; mission rouverte en accès complet au lieu du mode de son lancement (corrigé, test) ; note de revue en ambre (blanc).

Revue de sécurité ciblée (mode guidage du skill `security-audit`) sur `gh`, `git`, le réseau et l'écriture dans le dépôt :
- aucun jeton lu ni gardé (`gh auth status --json hosts`, jamais `--show-token`) ; push par l'aide d'identification de `gh` passée le temps de la commande, aucune configuration écrite ;
- tout passe en arguments séparés ou sur l'entrée standard, sans shell ; nom de branche validé par `git check-ref-format`, fichiers après `--` ;
- adresse d'`origin` (contrôlée par le dépôt) : hôte, propriétaire et dépôt limités à `[A-Za-z0-9._-]`, sans tiret ni point en tête (ni option déguisée, ni chemin détourné vers une autre adresse de l'API) — ajouté, testé ;
- liens venus de GitHub (checks, PR) ouverts seulement s'ils sont en `http(s)` — ajouté ;
- identifiant de job limité aux chiffres ; journal affiché en texte ;
- risque résiduel noté : un journal de CI envoyé à l'agent est du texte que le dépôt contrôle (injection possible) ; l'envoi est un geste de l'utilisateur et le mode de la mission s'applique.
