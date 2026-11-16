# Revue de fin de phase 7 (16 novembre 2026)

Revue faite depuis `39164f2` (fin de la phase 6), en profondeur là où une erreur coûte le plus : bots (lancement d'agents sans l'utilisateur, déclencheurs, garde-fous), copies de travail et leur retrait, écriture de fichiers (conventions, bibliothèque), stockage. Lus en surface : palette, réglages, rendu des vues. Non relus ici : `github.rs` et `publish.rs`, revus pendant leur chantier (ticket GitHub 11, revue de sécurité). Charge supposée : une personne, quelques projets, des centaines de conversations au plus.

Corrigé sur la branche `qualite-phase-7` :

- Une exécution de bot dont l'agent ne démarre pas (CLI absent) restait en fond, invisible, et tenait le bot pour occupé : tout déclenchement suivant était sauté (« la précédente t'attend encore »). Elle note maintenant l'erreur et libère le bot. Test sans fenêtre.
- Changer le projet ou l'étiquette d'un bot d'issues gardait les numéros déjà traités : l'issue #3 du nouveau dépôt était ignorée parce que la #3 de l'ancien avait été prise. Les numéros et la date de départ ne valent plus que pour le même dépôt et la même étiquette ; cocher « Aussi les issues déjà ouvertes » ne refait pas les issues traitées.
- La fiche projet laissait retirer la copie de travail d'une mission ouverte qui n'a pas encore sa conversation (agent qui démarre, exécution de bot en fond). Elle est maintenant « en cours d'utilisation ».
- La règle « mission non close » (planification, plan prêt, en cours) était écrite à deux endroits : `MissionStatus::is_open`.

Tickets à décider : `issues/`.
