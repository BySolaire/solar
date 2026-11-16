# Documents du domaine

Comment les skills d'ingénierie lisent la documentation du domaine de SOLAR avant d'explorer le code.

## À lire avant d'explorer

- `docs/design/GLOSSARY.md` : le vocabulaire du projet (projet, conversation, mission, tâche, agent, tentative, copie de travail, tour…).
- `docs/design/adr/` : les décisions d'architecture qui touchent la zone où l'on travaille.
- Les documents de `docs/design/` que `CLAUDE.md` liste en tête.

Si le glossaire ou les ADR n'existent pas encore, **continuer sans le signaler** ni proposer de les créer d'avance : `/domain-modeling` (via `/grill-with-docs` et `/improve-codebase-architecture`) les crée quand un terme ou une décision est réellement tranché.

## Organisation

Un seul contexte : un glossaire et un dossier d'ADR pour tout le dépôt (les deux crates partagent le même vocabulaire).

Noms d'ADR donnés pour l'exemple :

```
docs/design/
├── GLOSSARY.md
└── adr/
    ├── 0001-un-processus-par-tour-pour-codex-et-grok.md
    └── 0002-copie-de-travail-par-mission.md
```

## Employer le vocabulaire du glossaire

Quand une production nomme un concept du domaine (titre de ticket, proposition de refonte, hypothèse, nom de test), employer le terme du glossaire, sans glisser vers les synonymes qu'il écarte. Un concept absent du glossaire est un signal : soit le terme est inventé (à reconsidérer), soit il manque vraiment (à noter pour `/domain-modeling`).

## Signaler les conflits avec un ADR

Si une production contredit un ADR, le dire explicitement au lieu de passer outre :

> _Contredit l'ADR-0002 (copie de travail par mission), mais à rouvrir parce que…_
