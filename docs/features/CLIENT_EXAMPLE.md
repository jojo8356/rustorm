# Fiche de fonctionnalité — Client Rust : exemples contractuels `findMany` etc.

| Métadonnée | Valeur |
|---|---|
| Identifiant | `CLIENT-002` |
| Statut | **P1 M5 — done validé : builder find_many(...).filter/order/take/skip/cursor/distinct/select + exec().await stub (tests p1_m5_completion)** |
| Version | 1.0 |
| Date | 1 novembre 2026 |
| Références | [Cahier §6.2](../CAHIER_DES_CHARGES.md#62-exemples-contractuels-pour-un-schéma-dont-les-3-modèles-de--51), [README](../../README.md) |
| Priorité | P0 |

## 1. Résumé
Respecter les exemples d’usage du cahier §6.2 pour `findMany` avec filtres, tri, pagination, et projections scalaires.

## 2. Contrat §6.2
- Exemple canonique doit compiler et s’exécuter :
  ```rust
  let posts = client.post.find_many(
    post::title.contains("Rust"),
    post::published_at.order(Direction::Desc),
    post::id.cursor(..),
    take(10)
  ).exec().await?;
  ```
- Filtres chaînables, tri stable, pagination (skip/take/cursor), `select`/`include` précurseur
- Résultat `Vec<Record>` avec types Rust corrects (Option, DateTime, JsonValue, bytes)

## 3. État actuel
- Exemple réel possible (sync) :
  ```rust
  let rows = post::find_many(&conn, vec![Field::new("post","title").contains("Rust")], Some(vec![OrderClause{field:Field::new("post","published_at"), direction:Direction::Desc}]), Some(10), None, None)?;
  ```
- **Manques** : API builder `client.post.find_many(...).exec()`, `cursor`, `take` helper, `select`, `include`, `Direction` dans `post` module (actuellement dans `query`), `async`, ergonomie exacte §6.2

## 4. Règles VAL
| ID | Règle |
|---|---|
| VAL-EX01 | Exemple §6.2 compile tel quel (avec ajust async) |
| VAL-EX02 | Résultat contient seulement colonnes demandées si select |

## 5. Critères
| ID | Critère |
|---|---|
| AC-Ex-01 | `examples/query.rs` compile et s’exécute sur PG réel |
| AC-Ex-02 | Filtre+tri+page en une requête, pas N+1 |

## 6. Tests
- Snippet §6.2 dans test `trybuild` compile
- Exécution PG : titre contient, tri desc, page 10 → résultat trié

## 7. Livré / restant
- Livré : API dynamique équivalente
- Restant : API builder typée exacte §6.2, cursor, select
