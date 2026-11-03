# Fiche de fonctionnalité — Sécurité, secrets et durcissement

| Métadonnée | Valeur |
|---|---|
| Identifiant | `SEC-001` |
| Statut | **Done — tous critères §9/§10 vérifiés 2026-11-02** |
| Version | 1.1 |
| Date | 2 novembre 2026 |
| Références | [Cahier §9](../CAHIER_DES_CHARGES.md#9-sécurité-performance-et-exploitation), §10 paliers |
| Priorité | P0/P1 |

## 1. Résumé
Empêcher fuites de secrets, injections, writes N–N dangereux, et documenter isolation/RLS/triggers. **Livré complet.**

## 2. Contrat §9 P0 — Tous verts
- `datasource.url = env("VAR")` seule forme ; jamais de secret en clair dans schéma, logs, erreurs, traces — **fait** : `schema.rs` parse rejette toute autre forme, `postgres.rs::redacted_url` retourne `[REDACTED]`, `PoolConfig`/`Pool`/`Connection` Debug expurgent, tests `security_fuzz::expurgation_*` vérifient 0 occurrence `postgres://`/secret.
- Requêtes toujours paramétrées (`$N`), test fuzz injection — **fait** : tous les builders `query.rs` produisent `sql` avec `$N` et `params: Vec<Value>` séparés, `security_fuzz::fuzz_injection_payloads_are_parameterized` injecte 10 payloads OWASP (`'; DROP`, `UNION`, etc.) et vérifie qu'ils restent dans `params` sans fuir dans `sql`, fuzz `1000` cas aléatoires sans panic.
- N–N `set/update/delete` : suppression restreinte à la relation sauf `onDelete:Cascade` explicite documenté, garde-fou par défaut — **fait** : `relations.rs`/`nested_writes.rs` exigent `onDelete:Cascade` sinon `InvalidInput`, testé dans `relations_postgres` et `security_postgres`.
- `include` : garde-fou profondeur/N+1 (instrumentation, borne documentée) — fait dans `relations::include_*` avec batch limité `BATCH_SIZE=100`.
- Shadow DB éphémère supprimée même en erreur — **fait** : `migration_plan::plan` utilise `ShadowTxGuard` (`Drop` rollback) qui `take().rollback()` en succès et rollback automatique en erreur/panic, test `security_postgres::shadow_ephemeral_even_on_error` vérifie `pg_namespace` ne contient pas `_rustorm_plan_expected` après succès ou échec.

## 3. Contrat §10 paliers renforcés — Tous verts
- **DDL sans `DEFERRABLE`, sans `SET DEFAULT`** — `schema.rs` rejette `deferrable`/`initially`/`set default`/`setdefault` (case-insensitive, mot entier) au parse avec message `DEFERRABLE/INITIALLY/SET DEFAULT is not supported`, et rejette `onDelete: SetDefault` explicite avec `SET DEFAULT is not supported`; test `security_fuzz::fuzz_schema_rejects_deferrable_set_default` couvre 7 variantes.
- **Verrou index `AccessExclusiveLock`/`ShareLock`** — `nested_writes.rs` prend `ACCESS EXCLUSIVE` sur écriture et `SHARE` sur lecture, testé dans `nested_writes_postgres`.
- **`READ COMMITTED`** — `postgres::TransactionOptions` défaut `ReadCommitted`, `Connection::begin_with_options` fait `SET TRANSACTION ISOLATION LEVEL READ COMMITTED`, `nested_writes` vérifie `current_setting('transaction_isolation') == 'read committed'` sinon `InvalidInput`, test `security_postgres::isolation_must_be_read_committed` couvre `SERIALIZABLE` bloqué et `read committed` par défaut.
- **RLS/trigger/policy non contournables** — `nested_writes` vérifie `row_security_active(regclass)` OR `relforcerowsecurity`, `pg_trigger` non interne `tgenabled<>'D'`, `pg_rewrite` `rulename<>'_RETURN'` ; si l'un est vrai, erreur `requires READ COMMITTED, full visibility, no trigger/rule, valid FK`. Tests `security_postgres::{rls_blocks_nested_write, trigger_blocks_nested_write, rule_blocks_nested_write}` créent RLS `FORCE`, trigger, rule et vérifient le blocage, avec `row_security_active` et `pg_trigger`/`pg_rewrite` détectés via requêtes directes.
- **`ON UPDATE CASCADE` testé** — `nested_writes` exige FK `convalidated AND NOT condeferrable` avec `onUpdate=CASCADE`, test `security_postgres::on_update_cascade_is_tested` vérifie FK créée avec `ON UPDATE CASCADE` n'est pas deferrable et passe.

## 4. Expurgation complète
| Lieu | Avant | Après | Test |
|---|---|---|---|
| `postgres::PoolConfig` Debug | `#[derive(Debug)]` fuyait `url` | `impl Debug` avec `redacted_url() -> "[REDACTED]"` | `security_fuzz::expurgation_poolconfig_does_not_leak_url` |
| `postgres::Pool` Debug | déjà expurgé | ok | id |
| `postgres::PgManager` Debug | pas de Debug → pas de fuite, maintenant expurgé | `impl Debug` avec `[REDACTED]` | — |
| `postgres::AsyncPool` Debug | pas de Debug | `impl Debug` expurgé via `PoolConfig` | — |
| `postgres::Connection` Debug | déjà `[REDACTED]` | ok | — |
| `postgres::Error` Display/Debug | déjà sans URL (`PostgreSQL connection failed`) | ok | `security_fuzz::expurgation_errors_redact_url` |
| `migrations::Error` | contient `postgres::Error` déjà expurgé | ok | — |
| logs/traces `trace_query` | `redacted_sql` 200c | ok | — |

**Grep CI** : `grep -R "postgres://.*secret" --include="*.rs" --include="*.log"` → 0, vérifié en offline `cargo test --offline`.

## 5. Critères
| ID | Critère | État | Preuve |
|---|---|---|---|
| AC-Sec-01 | Grep logs/erreurs → 0 occurrence secret | ✅ | `redacted_url` partout, tests `expurgation_*` |
| AC-Sec-02 | Injection `'; DROP` → aucune DDL, paramétré `$N` | ✅ | `fuzz_injection_payloads_are_parameterized`, `fuzz_all_query_builders_are_parameterized` |
| AC-Sec-03 | N–N delete sans Cascade → erreur | ✅ | `relations.rs` guard + PG test |
| AC-Sec-04 | RLS bloquant → `InvalidInput` `requires READ COMMITTED` | ✅ | `security_postgres::rls_blocks_nested_write` |
| AC-Sec-05 | DEFERRABLE/SET DEFAULT → rejet parse | ✅ | `fuzz_schema_rejects_deferrable_set_default` |
| AC-Sec-06 | Shadow éphémère → 0 objet après échec | ✅ | `shadow_ephemeral_even_on_error` |
| AC-Sec-07 | `READ COMMITTED` défaut | ✅ | `isolation_must_be_read_committed`, `transaction_uses_read_committed_default` |

## 6. Tests
- **Offline** `cargo test --offline` : `security_fuzz` 6 tests (injection, deferrable, fuzz no-panic 1000, parameterized, expurgation).
- **PG** `cargo test --features postgres -- --ignored` : `security_postgres` 7 tests (RLS, trigger, rule, isolation, on_update_cascade, shadow, read_committed).
- **Migration** `shadow_postgres` existants + nouveau `shadow_ephemeral`.

## 7. Isolation documentée
- **Niveau par défaut** : `READ COMMITTED` (`TransactionOptions::default().isolation == ReadCommitted`, `Pool::transaction` et `Connection::begin_with_options` le posent via `SET TRANSACTION ISOLATION LEVEL READ COMMITTED`).
- **Vérification** : `nested_writes` exécute `current_setting('transaction_isolation')` et bloque si ≠ `read committed`; test PG le vérifie pour `SERIALIZABLE`.
- **RLS** : `row_security_active(regclass)` et `relforcerowsecurity` détectés; si actif, opération refusee; documenté ici et dans `nested_writes.rs` ligne 140.
- **Triggers/Rules** : `pg_trigger` (`NOT tgisinternal AND tgenabled<>'D'`) et `pg_rewrite` (`rulename<>'_RETURN'`) détectés; même chemin d'erreur.
- **Contraintes deferrable** : `pg_constraint.condeferrable` doit être faux pour FK; sinon erreur.

## 8. Livré
- Rejet `DEFERRABLE`/`INITIALLY`/`SET DEFAULT` au parse + `SetDefault` relation.
- Tests RLS/trigger/policy réels avec fixtures PG (3 tables `sec_parent`/`sec_child`, `FORCE RLS`, trigger `sec_trigger_fn`, rule `sec_rule`).
- Fuzzing 1000 entrées + OWASP, `$N` garanti.
- Expurgation complète URL dans tous les `DbError`/`Pool`/`AsyncPool`.
- Shadow DB éphémère `ShadowTxGuard` avec `Drop` rollback, testée même en erreur.
- Doc isolation `SECURITY.md` v1.1 Done + `MATRIX.md` § sécurité.
