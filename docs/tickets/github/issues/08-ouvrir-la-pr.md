Status: ready-for-agent
Blocked by: 07 — Commit et push

# 08 : Ouvrir la PR

Spec : `docs/tickets/github/spec.md`.

**What to build :** Après le push, l'étape 2 du dialogue propose le titre (titre de la mission), la description (plan approuvé, état des tests, verdict du relecteur, agent nommé, « Remplace #n » pour une tentative), la base (branche de l'utilisateur au lancement si elle existe sur `origin`, sinon la branche par défaut) et la case « Brouillon ». « Ouvrir la PR » la crée avec `gh` ; son numéro et son adresse sont gardés. La revue montre alors une ligne d'état (PR #n ouverte, branche → base, « Ouvrir sur GitHub »), la barre de mission dit « PR #n ouverte », la barre latérale garde un point d'état. On peut fermer le dialogue après le push et ouvrir la PR plus tard.

- [ ] `gh pr create` reçoit `--head`, `--base`, le titre, le corps sur l'entrée standard et `--draft` si coché (faux `gh`).
- [ ] Brouillon de description construit à partir de la mission, sans signature publicitaire.
- [ ] Numéro et adresse relus depuis la sortie de `gh` et sauvegardés ; état retrouvé après redémarrage.
- [ ] Test sans fenêtre du parcours bouton → push → PR ; captures FR/EN.
