Status: resolved
Blocked by: None (can start immediately)

# 04 : Bots d'issues : aucun mode qui agit sans clic hors bac à sable

Source : `docs/tickets/securite-audit-1/README.md`.

Piste `solar/issue-bot-auto-mode-unattended-command-execution` : un bot d'issues peut tourner en mode `auto`, où le relecteur du CLI, un modèle, approuve les commandes ; le texte de l'issue vient de son auteur. Vérificateur indépendant : à valider. Côté Solar, le chemin est confirmé ; reste à savoir si le mode `auto` des CLI exécute des commandes sans demande. Corrigé quand même : sans `auto`, la question ne se pose plus.

**What to build :** `bot_modes` ne garde pour un bot d'issues que des modes où une commande demande un clic ou tourne dans un bac à sable déterministe ; contrôle à l'exécution aussi.

- [x] Test : un bot d'issues ne peut pas être enregistré ni lancé en `auto`.

## Answer

`bot_modes` retire `auto` en plus de `bypassPermissions` pour un bot d'issues. Le contrôle à l'exécution (`start_run`) passe par la même fonction : un bot d'issues déjà enregistré en `auto` est refusé avec le motif « mode » existant, et l'éditeur lui propose un mode permis. Restent `acceptEdits` (Claude, Grok : commandes soumises à un clic ; Codex : bac à sable `workspace-write`) et `manual`. Test : `an_issue_bot_never_gets_full_access_nor_auto`.
