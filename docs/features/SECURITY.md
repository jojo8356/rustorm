# Fiche de fonctionnalité — Sécurité, secrets et durcissement

| Métadonnée | Valeur |
|---|---|
| Identifiant | `SEC-001` |
| Statut | **Partiel : paramétré + env() ; RLS/trigger/tests manquants** |
| Version | 1.0 |
| Date | 1 novembre 2026 |
| Références | [Cahier §9](../CAHIER_DES_CHARGES.md#9-sécurité-performance-et-exploitation), §10 paliers |
| Priorité | P0/P1 |

## 1. Résumé
Empêcher fuites de secrets, injections, writes N–N dangereux, et documenter isolation/RLS/triggers.

## 2. Contrat §9 P0
- `datasource.url = env("VAR")` seule forme ; jamais de secret en clair dans schéma, logs, erreurs, traces
- Requêtes toujours paramétrées (`$N`), test fuzz injection
- N–N `set/update/delete` : suppression restreinte à la relation sauf `onDelete:Cascade` explicite documenté, garde-fou par défaut
- `include` : garde-fou profondeur/N+1 (instrumentation, borne documentée)
- Shadow DB éphémère supprimée même en erreur

## 3. Contrat §10 paliers renforcés
- R2+ : DDL sans `DEFERRABLE`, sans `SET DEFAULT`, verrou index `AccessExclusiveLock`/`ShareLock`, `READ COMMITTED`, RLS/trigger/policy non contournables, `ON UPDATE CASCADE` testé
- §9 P1 : TLS, pool, fuzz, secrets expurgés partout, rapport sécu

## 4. État actuel
- `schema.rs` : `url env()` vérifié, génère erreur sinon ; `lib.rs` expurge `DATABASE_URL` dans `redacted_url`
- `query.rs`/`postgres.rs` : paramétré `$N` + `TEXT`, test `query::tests::injection_attempt_treated_as_value_not_sql`
- `relations.rs` : garde-fou `delete/update` N–N (`requires_on_delete_cascade`) partiellement, testé sur palier R1
- `nested_writes.rs` R2 : verrous, transactions, pas de deferrable, pas de set_default, pas de RLS/trigger côté appli (délégué DB)
- **Manques** :
  - Pas de vérif `DEFERRABLE`/`SET DEFAULT` dans `schema.rs` (pourrait accepter sans erreur)
  - Pas de tests RLS/trigger/policy réels (matrice manquante)
  - Expurgation incomplète : certains chemins `DbError` pourraient logger URL
  - Fuzzing planifié non exécuté
  - Pas de TLS explicite, pas de doc isolation
  - Shadow DB non éphémère (pas de shadow)

## 5. Critères
| ID | Critère |
|---|---|
| AC-Sec-01 | Grep logs/erreurs → 0 occurrence secret |
| AC-Sec-02 | Injection `'; DROP` → aucune DDL |
| AC-Sec-03 | N–N delete sans Cascade → erreur, pas de suppression |
| AC-Sec-04 | RLS bloquant → opération `PermissionDenied` |

## 6. Tests
- Expurgation : toute erreur contient `redacted_url`
- Injection : payloads OWASP
- N–N delete guard : déjà dans `relations.rs` tests PG
- À ajouter : DEFERRABLE refusé, SET DEFAULT refusé, RLS/trigger fixtures

## 7. Livré / restant
- Livré : env() obligatoire, paramétré, N–N guard partiel, redacted_url
- Restant : DEFERRABLE/SET DEFAULT rejet, tests RLS/trigger, fuzz, doc isolation, expurgation complète
