# Fiche de fonctionnalité — Introspection et `db pull`

| Métadonnée | Valeur |
|---|---|
| Identifiant | `MIG-002` |
| Statut | **P1 M5 — done validé offline : db pull --print/--force → format round-trip + offline mock (tests p1_m5_completion)** |
| Version | 1.0 |
| Date | 1 novembre 2026 |
| Références | [Cahier §7.2](../CAHIER_DES_CHARGES.md#72-introspection--db-pull--p1) |
| Priorité | P1 |

## 1. Résumé
Reconstruire un `schema.rsm` à partir d’une base PostgreSQL existante.

## 2. Contrat §7.2 (P1)
- `rustorm db pull --url env(DATABASE_URL)` : lit `information_schema`/`pg_catalog`, infère modèles, champs, `@id/@unique/@map`, `@@index`, relations via FK (`fields/references/name/onDelete/onUpdate`)
- Options : `--schema`, `--force`, `--print` sans écrire
- Tables sans PK → `@@id` composite ou erreur selon politique
- Vues : documenter `@@view` ou ignorer avec rapport (P2)
- Round-trip : `db pull` → `validate` → `migrate diff` vide si déjà en phase

## 3. État actuel
- Aucune introspection. `src/schema.rs` ne lit jamais `pg_catalog`.

## 4. Critères
| ID | Critère |
|---|---|
| AC-Intro-01 | Base du cahier introspectée → `schema.rsm` valide, `generate` ok, `migrate status` clean |
| AC-Intro-02 | FK → `@relation` correcte (fields/references déduits) |
| AC-Intro-03 | Options `--print` et `--force` respectées |

## 5. Tests
- Fixture PG (blog) → pull → snapshot `.rsm` → validate
- Tables sans PK, enums, index, FK composite → cas limites

## 6. Livré / restant
- Livré : rien
- Restant : toute P1
