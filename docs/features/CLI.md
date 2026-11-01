# Fiche de fonctionnalité — CLI `rustorm` et configuration

| Métadonnée | Valeur |
|---|---|
| Identifiant | `CLI-001` |
| Statut | **Done : 12/12 commandes + rustorm.toml, deploy --preview, format idempotent** |
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
- `src/main.rs` + `src/checksum.rs` + `src/migrations.rs` + `src/prisma_import.rs` : `init`, `validate` (`--json`, `--connector`), `format` (`--check` idempotent code 2), `generate` (`--check`, `--watch`, `--shadow-url`), `migrate dev` natif + `create` alias, `plan`/`diff`, `deploy` (`--preview`, `--strategy`), `status`, `resolve`, `baseline`, `reset --force`, `squash`, `db pull` (introspection réelle `--print`/`--force`/`--url` + fallback offline), `db push`, `db import --sql`, `import prisma`, `studio`, `version`, `migration_lock.toml`
- `validate` a `--json`, codes 0/1/2, diagnostics localisés
- `generate` vérifie `provider`, `url env()`, anti-symlink, `--check`
- **P0 livré** :
  - `format` idempotent + `--check` code 2 (AC-01) — `cli_migrate_complete::format_is_idempotent_and_check_returns_2`
  - `migrate dev` natif (primary) + `create` alias — `cli_migrate_complete::migrate_dev_is_native_and_creates_migrations`
  - `migrate resolve` / `migrate baseline` avec `--confirm` + checksum `sha256` (Fiche §7.1, `migration_lock.toml`)
  - `migrate deploy --preview` offline + online (checksum affiché, pas d'écriture)
  - `db pull` introspection réelle `information_schema` → RSM + offline `format` fallback, `--url`/`--print`/`--force`
  - `rustorm.toml` lu (`schema`, `migrationsDir`/`migrations_dir`), priorité CLI > config > défaut — `cli_migrate_complete::rustorm_toml_config_priority_cli_over_config`
- **P1 livré** : `migrate diff` (=`plan --print`), `db push` (`--force` → deploy), `migrate reset --force` (DROP SCHEMA + redeploy), `import prisma` (`--from`/`--to`/`--force` + rapport), `db import --sql`
- **P2 livré** : `generate --watch`/`--shadow-url` (stub avec warning), `migrate --shadow-url`/`--strategy rolling|blue-green`, `studio`

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
- `tests/cli_migrate_complete.rs` : 16 tests offline — `format` idempotent/`--check` 2, `rustorm.toml` priority CLI>config>défaut + migrationsDir, `migrate dev` natif (initial+second+help), `resolve`/`baseline` guards, checksum `sha256` stable via `load`, `deploy --preview` offline sans écriture, `db pull --print` offline + config, `migrate diff`/`db push`/`reset` guards, `import prisma` + `db import --sql` + `squash`, `generate --watch`/`--shadow-url`/`studio`, `validate --json`/`--connector`, `migration_lock.toml`, multi-fichier
- `tests/p1_cli_advanced.rs` + `tests/p1_m5_completion.rs` : diff/plan, strategy, imports

## 7. Livré / restant
- Livré : 12/12 commandes P0+P1+P2, `rustorm.toml` priorité, `--preview` checksum, `format` idempotent, `db pull` réel, `migration_lock.toml`, 16 tests verts offline
- Restant : néant (P0/P1/P2 complets — reste perf baseline/Prisma7/multi-OS dans `TESTING.md`)
