Status: ready-for-agent

# Spec : diffusion v1 (macOS, open source, sans compte Apple)

Cadrage du 16 novembre 2026 (deux tours, recommandations acceptées ; Solaire : aucune dépense, publication open source). Faits : `docs/research/2026-11-16-diffusion-sans-compte-apple.md`.

## Problem Statement

SOLAR ne se lance aujourd'hui que par `cargo run` depuis le dépôt : pas d'app dans Applications, pas de notifications (elles demandent un paquet `.app`), et personne d'autre ne peut l'installer. Je veux le publier en open source sans payer de compte Apple.

## Solution

Un script construit `SOLAR.app` (release, icône, Info.plist, signature ad hoc) et peut l'installer dans Applications. Le dépôt devient public après un audit et une passe sur l'historique, avec une licence. Une CI gratuite vérifie chaque push ; une release publiée sur un tag porte le paquet zippé et les instructions de premier lancement. SOLAR vérifie au lancement s'il existe une version plus récente et le dit, avec le lien.

## User Stories

1. En tant qu'utilisateur, je veux construire `SOLAR.app` avec une commande et l'installer dans Applications.
2. En tant qu'utilisateur, je veux que les notifications des bots s'affichent avec l'app installée.
3. En tant que visiteur du dépôt, je veux télécharger SOLAR depuis les releases et savoir comment l'ouvrir la première fois sans compte Apple du développeur.
4. En tant que développeur, je veux compiler SOLAR depuis les sources, sans avertissement de macOS.
5. En tant que mainteneur, je veux qu'aucun secret ni fichier personnel ne parte avec l'historique public.
6. En tant que mainteneur, je veux une licence claire et les mentions des fichiers tiers.
7. En tant que mainteneur, je veux une CI qui lance format, clippy et tests à chaque push et PR.
8. En tant que mainteneur, je veux qu'un tag `v*` produise une release (brouillon) avec le paquet zippé.
9. En tant qu'utilisateur, je veux savoir qu'une nouvelle version existe, sans installation automatique ni question à chaque lancement ; je veux pouvoir couper cette vérification.

## Implementation Decisions

- `scripts/bundle-macos.sh` : `cargo build --release -p solar`, puis `SOLAR.app/Contents/{MacOS/solar, Resources/Solar.icns, Info.plist}` ; Info.plist : `CFBundleIdentifier` `com.bysolaire.solar`, `CFBundleName` SOLAR, version de `Cargo.toml`, `LSMinimumSystemVersion`, `NSHighResolutionCapable` ; `codesign --force --deep --sign -` ; option `--install` vers `/Applications` (remplace l'ancienne app après confirmation du script). Aucune dépendance nouvelle.
- Licence : `LICENSE-MIT` et `LICENSE-APACHE`, `license = "MIT OR Apache-2.0"` dans le workspace ; mentions tierces dans `THIRD_PARTY.md` (Space Grotesk OFL, icônes Lucide ISC via gpui-kit, logos des agents selon la décision prise).
- Passe sur l'historique : recherche de secrets (motifs de jetons, clés), de chemins et fichiers personnels, de captures, de fichiers d'état ; constats rapportés à Solaire avant toute réécriture (aucune réécriture d'historique sans son accord).
- CI : `.github/workflows/ci.yml` sur `macos-latest` (fmt, clippy `-D warnings`, tests) ; `release.yml` sur tag `v*` : paquet, zip, release en brouillon.
- Mises à jour : au lancement, en arrière-plan, au plus une fois par jour, `GET https://api.github.com/repos/BySolaire/solar/releases/latest` ; si sa version dépasse celle de l'app, une ligne discrète (barre latérale ou réglages) avec le lien vers la release ; réglage « Vérifier les mises à jour » (actif par défaut), noté dans `state.json`. Aucune donnée envoyée hors de la requête elle-même.

## Testing Decisions

- Script : construit un paquet dont `codesign --verify` passe et dont l'Info.plist porte l'identifiant et la version ; lancement du paquet vérifié à l'écran ; notification d'un bot vue.
- Comparaison de versions et lecture de la réponse de l'API : tests du cœur (`0.1.10` après `0.1.9`, préversions, réponse invalide).
- CI : un push sur une branche fait passer le workflow.

## Out of Scope

- Notarisation, Sparkle, Mac App Store, Homebrew, Intel, Windows, Linux.

## Further Notes

- La bascule du dépôt en public est un geste de Solaire (ou le mien sur son feu vert explicite), après les vérifications.
