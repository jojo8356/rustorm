# Fiche de fonctionnalité — CLI `rustorm` et configuration

| Métadonnée | Valeur |
|---|---|
| Identifiant | `CLI-001` |
| Statut | **Partiel : 6 commandes sur ~12 attendues** |
| Version | 1.0 |
| Date | 1 novembre 2026 |
| Références | [Cahier §8](../CAHIER_DES_CHARGES.md#8-cli-et-configuration), §11 jalons |
| Priorité | P0/P1/P2 |

## 1. Résumé
CLI unique `rustorm` avec commandes normalisées, codes de sortie, sortie texte/JSON et configuration `rustorm.toml` optionnelle.

## 2. Contrat §8
- Commandes P0 : `init` (crée `schema.rsm` + dossier `migrations/` depuis template), `validate` (`--schema`, `--json`, code 0/1), `format` (`--check`), `generate` (`--schema`, `--check`), `migrate dev|deploy|status|resolve|baseline`, `db pull` (P1), `version`
- P1 : `migrate diff`, `db push`, `migrate reset`, `import prisma`
- P2 : `generate --watch`, `migrate --shadow-url`
- Config : `rustorm.toml` (`schema`, `migrationsDir`), priorité CLI > config > défaut ; `--schema` partout
- Sortie : texte humain par défaut, `--json` structuré, code 0 ok, 1 erreur usage, 2 diff/check
- Aucun secret en clair dans logs

## 3. État actuel
- `src/main.rs` + `src/checksum.rs` : `init`, `validate`, `generate`, `generate --check`, `version`, `migrate create/status/deploy/plan`, `checksum`
- `validate` a `--json`, codes 0/1/2, diagnostics localisés
- `generate` vérifie `provider`, `url env()`, anti-symlink, `--check`
- **Manques P0** :
  - `format` absent (spécifié mais non implémenté)
  - `migrate dev` vs `create` : commande actuelle `migrate create` pas `migrate dev`
  - `migrate resolve` absent
  - `migrate baseline` incomplet
  - `migrate deploy` sans `--preview`
  - `db pull` absent
  - `rustorm.toml` non lu (seulement `schema.rsm` direct)
- **Manques P1** : `migrate diff`, `db push`, `migrate reset`, `import prisma`
- **Manques P2** : `generate --watch`, `--shadow-url`

## 4. Règles VAL
| ID | Règle |
|---|---|
| VAL-CLI01 | Toute commande sans `--schema` utilise config ou défaut |
| VAL-CLI02 | `--json` valide JSON avec champ `code` |
| VAL-CLI03 | `init` n’écrase pas sans `--force` |

## 5. Critères
| ID | Critère |
|---|---|
| AC-CLI-01 | `init` → `validate` → `generate` → `cargo check` 0 |
| AC-CLI-02 | `format --check` idempotent (AC-01) |
| AC-CLI-03 | `migrate dev`→`status`→`deploy` cycle complet |
| AC-CLI-04 | `--json` sur erreur fournit code+message parsable |

## 6. Tests
- `tests/cli.rs` : init, validate json/texte, generate, check, diagnostics
- À ajouter : format, migrate dev/resolve/baseline, db pull, config toml

## 7. Livré / restant
- Livré : init/validate/generate/version/plan/status/deploy partiels, tests CLI
- Restant : format, migrate dev/resolve/baseline complets, db pull, config, P1/P2
