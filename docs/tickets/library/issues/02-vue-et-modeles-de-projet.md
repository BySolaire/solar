Status: ready-for-agent
Blocked by: 01 — Modèles personnels par `/` et ⌘K

# 02 : Vue Bibliothèque et modèles de projet

Spec : `docs/tickets/library/spec.md`.

**What to build :** Une vue pleine « Bibliothèque » (comme Réglages) liste les modèles du projet courant, personnels et exemples ; créer (personnel ou projet), modifier (zone de texte, refus d'écraser un fichier changé ailleurs), dupliquer (dont les exemples), supprimer après confirmation. Les modèles de `<projet>/.solar/library/` sont lus et l'emportent sur les personnels de même nom. Entrée de la barre latérale dès qu'un modèle existe ; joignable par ⌘K avant.

- [ ] Tests du cœur : fusion projet > personnel ; écriture refusée si le fichier a changé ; suppression ; dossiers créés seulement au premier enregistrement.
- [ ] Test sans fenêtre de la création et de la duplication d'un exemple.
- [ ] À l'écran en 1024 et 720, FR et EN ; clavier.
