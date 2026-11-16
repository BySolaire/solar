# Carte : diffusion (phase 8)

## Destination

Solar publié en open source sur GitHub et installable gratuitement sur macOS (Apple Silicon) : un paquet `Solar.app` construit par un script, une release téléchargeable avec ses instructions, la compilation depuis les sources documentée, une CI, et Solar qui signale une nouvelle version. Sans compte Apple Developer.

## Notes

- Faits établis : `docs/research/2026-11-16-diffusion-sans-compte-apple.md`, `docs/research/2026-11-16-notifications-gpui.md`.
- Skills : `security-audit` complet avant la bascule en public ; `wizard` pour les gestes que Solaire seul peut faire (bascule du dépôt) ; `impeccable` pour l'annonce d'une mise à jour.
- Préférences de Solaire : aucune dépense (pas de compte Apple, pas de minutes payantes) ; questions de cadrage avec recommandation ; rien de publié sans son accord.

## Decisions so far

- Pas de notarisation : paquet signé ad hoc ; instructions « Ouvrir quand même » ou `xattr` ; compilation depuis les sources sans avertissement.
- Apple Silicon seulement en v1 ; Windows et Linux sont des chantiers à part.
- Paquet construit par un script du dépôt, sans dépendance nouvelle ; identifiant `com.bysolaire.solar`, icône `assets/logo/identity-kit/macOS/Solar.icns`.
- Mises à jour v1 : vérification des GitHub Releases au lancement, annonce discrète avec le lien ; aucune installation automatique.
- Avant la bascule en public : audit de sécurité complet, passe sur tout l'historique, licence MIT ou Apache 2.0, droits des fichiers tiers (police, icônes, logos des agents).
- Ordre : paquet local, vérifications pour le public, bascule (Solaire), CI, première release, annonce des mises à jour.

- Fait le 16 novembre 2026 : tickets 01 à 06. Release 0.1.0 publiée ; dépôt public ; CI verte.

## Not yet specified

- Un tap Homebrew construit depuis les sources.
- Sparkle (mise à jour automatique) sans notarisation.

## Out of scope

- Notarisation, Mac App Store, Windows, Linux, mobile.
