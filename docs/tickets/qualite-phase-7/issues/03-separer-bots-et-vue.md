Status: needs-triage

# 03 : Séparer la logique des bots de leur vue

`shell/bots_view.rs` (environ 1 550 lignes) mêle le rendu (liste, éditeur) et la conduite des bots (lancement, garde-fous, horaire, lecture des issues, suivi et notifications). Proposition : la conduite dans `shell/bots.rs`, le rendu dans `bots_view.rs` ; déplacement seul.
