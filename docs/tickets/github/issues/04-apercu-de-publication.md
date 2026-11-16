Type: prototype
Status: resolved

## Question

À quoi ressemblent l'aperçu de « Commit et push » et celui d'« Ouvrir la PR » dans la vue Modifications : branche, fichiers (dont ceux à confirmer), message de commit, titre et corps de la PR, case « Brouillon », erreurs (hook en échec, push refusé, droits insuffisants), puis l'état « PR ouverte » avec ses checks ? Un prototype jetable, à juger à l'écran avec Solaire, en 1024 × 768 et 720 × 480.

## Answer

Prototype jetable : trois variantes dans la vraie vue Modifications, données fictives (branche locale `prototype/apercu-publication`, commit `8ba1eb8`, non poussée ; lancement : `SOLAR_PROTO=A|B|C SOLAR_PROTO_STEP=1-4`). A, en ligne dans l'en-tête de la revue : le diff disparaît, illisible en 720. B, panneau à droite de 360 : le plus lisible en 1024, sans place en 720. C, dialogue en deux étapes : concentré, tient en 720 (boutons à fixer en bas).

Retenu le 16 novembre 2026 (Solaire suit la recommandation) :

1. Le dialogue de C pour les deux gestes, « Commit et push » puis « Ouvrir la PR », étapes affichées en tête ; ses boutons restent fixés en bas, le contenu défile.
2. Une fois la PR ouverte : une ligne d'état en tête de la revue (numéro, branche → base, « Ouvrir sur GitHub », checks), et l'état dans la barre de mission.
3. Check en échec : son journal déplié dessous, limité aux 20 dernières lignes, lien vers le journal complet sur GitHub, puis « Envoyer l'échec à l'agent ».
4. Un fichier que l'utilisateur avait modifié avant le lancement est décoché par défaut, avec son explication ; le cocher vaut confirmation.
5. Un hook en échec s'affiche en tête du dialogue (sortie en chasse fixe), rien n'est commité ni poussé, « Réessayer ».
6. Aucun jaune : les avertissements sont en blanc avec une icône, le vert et le rouge restent aux seuls états.
