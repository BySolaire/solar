# Diffuser Solar sans compte Apple Developer (16 novembre 2026)

Solaire ne prend pas l'Apple Developer Program (99 $ par an) ; Solar sera publié en open source sur GitHub. Faits relevés pour la phase 8.

## Signature et Gatekeeper

- Sans certificat « Developer ID », pas de notarisation. Une signature « ad hoc » (`codesign --sign -`) reste possible et gratuite ; sur Apple Silicon, un binaire doit au moins être signé ainsi pour tourner (l'éditeur de liens le fait pour un binaire, à refaire sur le paquet `.app` entier).
- Une app téléchargée porte l'attribut de quarantaine. Non notarisée, son premier lancement est refusé. Aide d'Apple, « Open a Mac app from an unknown developer » (lue le 16 novembre 2026) : Réglages Système → Confidentialité et sécurité → Sécurité → « Ouvrir quand même » ; le bouton reste environ une heure après la tentative d'ouverture, demande le mot de passe, puis l'app est une exception permanente. Autre voie, en Terminal : `xattr -dr com.apple.quarantine /Applications/Solar.app`.
- Une app compilée sur la machine (depuis les sources) n'est pas mise en quarantaine : aucun avertissement.

## Homebrew

- `docs.brew.sh/Acceptable-Casks` (lu le 16 novembre 2026) : sur macOS, une app de `homebrew/cask` doit passer les vérifications Gatekeeper de Homebrew, sans demander de les contourner. Une app non notarisée n'y entre donc pas.
- Même page : un logiciel graphique open source sans distribution compilée va normalement dans `homebrew/core`, comme formule construite depuis les sources. Un « tap » à soi (`bysolaire/tap`) avec une formule qui compile Solar reste possible, sans Gatekeeper puisque compilé sur place.
- Même page : Apple garde le support général de Rosetta 2 jusqu'à macOS 27. Pour Solar, une version Apple Silicon (arm64) suffit en v1 ; à revoir si des utilisateurs Intel le demandent.

## GitHub

- Dépôt public : GitHub Actions est gratuit, macOS compris (règle connue, à reconfirmer au moment de la bascule). Dépôt privé : minutes facturées, multipliées pour macOS.
- L'API des releases (`GET /repos/{owner}/{repo}/releases/latest`) se lit sans jeton pour un dépôt public : de quoi vérifier une mise à jour.

## À vérifier pendant le chantier

- Les notifications système (`UNUserNotificationCenter`) avec un paquet signé ad hoc et un identifiant `com.bysolaire.solar`.
- Le comportement exact de macOS 27 au premier lancement d'une app non notarisée.
