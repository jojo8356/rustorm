# Fiche de fonctionnalité — Génération du client Rust et intégration build

| Métadonnée | Valeur |
|---|---|
| Identifiant | `CLIENT-001` |
| Statut | **Partiel P0 : génération modules simples ; `RustormClient` complet manquant** |
| Version | 1.0 |
| Date | 1 novembre 2026 |
| Références | [Cahier §6.1](../CAHIER_DES_CHARGES.md#61-génération-et-intégration-p0), [README](../../README.md) |
| Priorité | P0 critique |

## 1. Résumé
À partir du `.rsm`, générer le client Rust typé (`RustormClient` + modules par modèle) ou valider la sortie `--check`, avec intégration build et garde-version.

## 2. Contrat §6.1
- `rustorm generate` écrit dans `output` relatif au schéma, sans écraser hors périmètre, sans suivre symlinks
- Généré pour chaque modèle : `Record` (struct), `CreateInput`, `UpdateInput`, `WhereInput`, `WhereUniqueInput`, `OrderBy`, `Select/Include` précurseurs (P1 projections)
- `RustormClient` : `new(url)` ou `from_env`, méthodes `model.{findUnique,findFirst,findMany,create,update,delete,upsert,count,aggregate}` en `async` derrière feature
- Modules respectent `snake_case` Rust ; re-export racine `rustorm_client`
- `build.rs` optionnel : `rustorm::generate_on_build("schema.rsm")` déclenché en `cargo build`
- Garde-version : `cargo` feature `rustorm = { version = "0.x", schema = "schema.rsm" }` ou checksum embarqué ; mismatch → erreur de compilation
- `cargo check` passe sur généré ; pas de `unwrap` dans code généré

## 3. État actuel
- `src/main.rs generate` : valide `provider`+`url`, écrit `client.rs` et `<model>.rs` par modèle avec `Record`, `Field` constants, `columns()`, `find_many()`, `count()`, `create()`, `update_many()`, `delete_many()`, `numeric_mutations` tests
- Vérifs `output` relatif, anti-traversée, `--check` compare bytes
- **Manques** :
  - Pas de `RustormClient` ; seulement modules indépendants sans trait pool
  - Pas de structs typés `Where/WhereUnique/Create/Update` — utilisation dynamique `Field/Filter` uniquement
  - Pas de `findUnique`/`findFirst` typés, pas de `upsert`, `createMany`, `aggregate`, `groupBy`
  - Pas d’`async` ; tout est `postgres::Connection` synchrone
  - Pas de build.rs helper, pas de garde version/checksum
  - Projections `select`/`include` absentes (seulement via descriptors relations expérimentales)
  - Génération ne couvre pas enums (dépendance SCHEMA)

## 4. Règles VAL
| ID | Règle |
|---|---|
| VAL-CG01 | Généré compile `cargo check` |
| VAL-CG02 | `output` hors projet ou symlink → erreur |
| VAL-CG03 | Version mismatch → erreur |

## 5. Critères
| ID | Critère |
|---|---|
| AC-Gen-01 | AC §5.1 `generate` puis `cargo check` ok |
| AC-Gen-02 | `generate --check` renvoie 2 si diff |
| AC-Gen-03 | `RustormClient::new` + `client.user.find_unique(where)` typé compile |
| AC-Gen-04 | build.rs génère au build |

## 6. Tests
- Génération : idempotence, anti-écrasement, symlink piégé, check
- Compilation : `cargo check` sur fixtures
- Intégration : exemple `examples/schema.rsm` → client utilisé dans tests

## 7. Risques
| Risque | Réduction |
|---|---|
| API générée instable | Versionner API, tests de non-régression snapshots |
| Conflit build.rs | Documenter ordre, éviter double génération |

## 8. Livré / restant
- Livré : génération mono-fichier `client.rs`+ modèles, check idempotent
- Restant : RustormClient, structs typés, async, projections, garde-version, build.rs, enums
