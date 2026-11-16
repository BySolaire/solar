Type: grilling
Status: resolved

## Question

Une mission relancée (tentative n) après une PR déjà ouverte : la nouvelle tentative pousse-t-elle sur la même branche de mission (nouveaux commits dans la même PR), ou ouvre-t-elle une branche `solar/<titre>-n` et une nouvelle PR, l'ancienne restant à fermer par l'utilisateur ?

## Answer

Réponses de Solaire le 16 novembre 2026 (recommandations acceptées). Fait de départ : une tentative ne naît qu'après une restauration ou un abandon, dans une conversation et une copie neuves prises sur l'état actuel du projet ; son historique ne prolonge pas l'ancienne branche, et la pousser dessus demanderait un force-push, exclu.

1. Chaque tentative publie sur sa propre branche, `solar/<titre>-<n>`, et ouvre sa propre PR ; l'ancienne PR n'est pas modifiée.
2. Abandonner la copie d'une mission dont la PR est ouverte : la confirmation rappelle que la PR reste ouverte sur GitHub et propose une case « Fermer aussi la PR #n », décochée par défaut. La branche distante n'est jamais supprimée.
3. La PR d'une tentative porte « Remplace #n » dans sa description (lien GitHub, sans fermeture automatique).
4. Dans une même tentative, chaque correction (demandée, venue du relecteur ou d'un check en échec) ajoute un commit sur la même branche (rappel du cadrage, Q9 et Q16).
