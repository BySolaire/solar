Status: resolved
Blocked by: 01 — Modèles personnels par `/` et ⌘K

# 02 : Vue Bibliothèque et modèles de projet

Spec : `docs/tickets/library/spec.md`.

**What to build :** Une vue pleine « Bibliothèque » (comme Réglages) liste les modèles du projet courant, personnels et exemples ; créer (personnel ou projet), modifier (zone de texte, refus d'écraser un fichier changé ailleurs), dupliquer (dont les exemples), supprimer après confirmation. Les modèles de `<projet>/.solar/library/` sont lus et l'emportent sur les personnels de même nom. Entrée de la barre latérale dès qu'un modèle existe ; joignable par ⌘K avant.

- [x] Tests du cœur : fusion projet > personnel ; écriture refusée si le fichier a changé ; suppression ; dossiers créés seulement au premier enregistrement.
- [x] Test sans fenêtre de la création et de la duplication d'un exemple.
- [x] À l'écran en 1024 et 720, FR et EN ; clavier.

## Comments

16 novembre 2026, branche `library-v1`. Cœur : `valid_name`, `free_name`, `delete` (refusée si le fichier a changé), `has_templates` ; la fusion projet > personnel est testée ; l'enregistrement passe par `project::save` (refus d'écraser, dossiers créés au premier enregistrement). Interface : vue pleine « Bibliothèque » (titre, explication, « Nouveau modèle » et « Nouveau modèle pour <projet> », éditeur avec nom et dossier de destination, sections Modèles du projet / Mes modèles / Exemples, Modifier, Dupliquer, Supprimer avec confirmation en ligne) ; entrée de la barre latérale au-dessus de Réglages dès qu'un modèle personnel ou d'un projet connu existe ; ⌘K « Ouvrir la bibliothèque » ; chaque conversation relit ses modèles après un changement. Test sans fenêtre : modèle de projet créé dans `.solar/library/`, exemple dupliqué dans le dossier personnel, entrée de la barre latérale apparue. Vu à l'écran en 1024 FR (création pour le projet, suppression confirmée, duplication) et 720 EN. Un exemple dupliqué sous le même nom le remplace dans la liste (le modèle personnel l'emporte, comme prévu).
