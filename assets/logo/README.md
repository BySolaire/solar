# Logo de Solar

`identity-kit/` est le kit d'identité officiel de Solar (« Solar Official Identity Kit »), copié tel quel depuis l'ancien dépôt Solar (`assets/logo/identity-kit/`, fourni par l'utilisateur le 15 novembre 2026 ; archive `SOLAR_Official_Identity_Kit.zip`, SHA-256 `f481e7c0427b7ddae549d421548984a13ce0f0565ebf163f4c9ffadd7929ea88`). Sa notice `identity-kit/README.md` fait foi pour la marque.

Images dérivées (22 novembre 2026), à partir de `identity-kit/assets/png/solar-macos-1024.png`, luminance convertie en opacité (blanc sur transparent) :

- `src-tauri/macos/AppIcon.icon/Assets/icon.png` : l'arc seul, posé par Icon Composer sur le fond noir de `icon.json` ;
- `crates/solar/assets/solar.png` : le même arc, recadré sur (160, 123, 864, 827) puis agrandi à 1024, pour la marque affichée dans l'app ;
- `public/solar.png` : la tuile du kit telle quelle (icône Linux).

Ces fichiers remplacent le logo de Monocode, dont Solar est issu.

Mise à jour du 23 novembre 2026 (branche `design/solar-theme`) : le client a retenu l'option C. Le « S » calligraphique redessiné par le Studio Design remplace l'arc dans l'icône d'app (`AppIcon.icon`, `Assets.car`, `src-tauri/icons/`, `public/solar.png`) et dans la marque de l'app (`crates/solar/assets/solar.png`). Le mot-symbole SOLAR de `identity-kit/` reste la référence pour le texte et les écrans de lancement. Détails : `scripts/solar-theme/README.md`.
