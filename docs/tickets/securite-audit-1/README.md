# Corrections de l'audit de sécurité (run-1, 16 novembre 2026)

Audit `security-audit` en profil rapide sur `e60c854`. Rapport et preuves hors du dépôt : `~/security-audit-skill/solar/run-1/`. Les corrections passent avant que le dépôt devienne public (décision de Solaire, 16 novembre 2026).

Fil commun : la copie de travail d'une mission est écrite par l'agent. Solar ne doit ni suivre ce qu'elle contient hors d'elle (liens, `.git`), ni laisser git y trouver une configuration choisie par l'agent.
