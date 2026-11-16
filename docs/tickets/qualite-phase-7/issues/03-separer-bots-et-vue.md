Status: resolved

# 03 : Séparer la logique des bots de leur vue

`shell/bots_view.rs` (environ 1 550 lignes) mêle le rendu (liste, éditeur) et la conduite des bots (lancement, garde-fous, horaire, lecture des issues, suivi et notifications). Proposition : la conduite dans `shell/bots.rs`, le rendu dans `bots_view.rs` ; déplacement seul.

## Comments

16 novembre 2026 : fait, déplacement seul. `shell/bots.rs` (lancement, garde-fous, horaire, issues, suivi, notifications, outils partagés) et `shell/bots_view.rs` (liste, éditeur) ; 458 et 1 120 lignes. Tests inchangés, tous verts.
