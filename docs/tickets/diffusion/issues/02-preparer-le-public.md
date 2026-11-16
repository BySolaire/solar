Status: ready-for-agent
Blocked by: None (can start immediately)

# 02 : Préparer la publication open source

Spec : `docs/tickets/diffusion/spec.md`.

**What to build :** Audit de sécurité complet ; passe sur tout l'historique (secrets, chemins et fichiers personnels, captures, état) avec rapport à Solaire ; licence MIT ou Apache 2.0 ; `THIRD_PARTY.md` ; décision sur les logos des agents (garder avec mention, ou retirer).

- [ ] Rapport de l'historique remis à Solaire, aucune réécriture sans son accord.
- [x] Licences et mentions présentes ; `cargo` lit la licence du workspace.

## Avancement (16 novembre 2026)

**Historique** (toutes les branches locales, 86 commits, 615 contenus de fichiers, lus un par un) :
- Aucun secret trouvé : clés Anthropic, OpenAI, xAI, AWS, Slack, jetons GitHub, clés privées, JWT, affectations `token`/`password`/`secret`.
- Auteur de tous les commits : `Solaire <…@users.noreply.github.com>`, aucune adresse personnelle. Aucune autre adresse dans les fichiers que `u@example.com` et `git@github.com` (tests).
- Aucun fichier d'état, `.env`, journal ou capture de l'app ; les fixtures de test sont anonymisées (`/home/user`, `/work`).
- Informations personnelles : retirées des fichiers actuels à la demande du mainteneur (« aucune information personnelle ») ; le prénom remplacé par le pseudo public Solaire, le chemin local et le lien de la conversation ChatGPT retirés, deux planches qui affichaient le prénom sorties du dépôt (copie gardée hors du dépôt). Elles restent dans les anciens commits : réécriture de l'historique à décider.
- La branche locale `prototype/apercu-publication` n'est pas sur GitHub et n'y partira pas sans push.

**Licences** : `LICENSE-MIT` (« Solaire »), `LICENSE-APACHE`, `license = "MIT OR Apache-2.0"` lu par `cargo metadata` pour les deux crates ; `THIRD_PARTY.md` (police, icônes, logo de Claude, marques, licences des crates, références) ; section Licence du README. Logos des agents : seul celui de Claude est dans le dépôt (simple-icons, CC0, mention de marque déjà dans `assets/providers/README.md`) ; gardé avec mention. Le nom Solar et son logo sont exclus de la licence du code (`THIRD_PARTY.md`). Le nom s'écrit Solar (app, paquet `Solar.app`, textes).

**Audit de sécurité complet** : à faire.
