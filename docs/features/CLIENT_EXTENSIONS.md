# Fiche de fonctionnalité — Extensions et middlewares client (P2)

| Métadonnée | Valeur |
|---|---|
| Identifiant | `CLIENT-EXTENSIONS` |
| Statut | **P2** |
| Version | 1.0 |
| Date | 31 octobre 2026 |
| Références | Cahier §6 P2, Prisma `$use`/`$extends` |
| Priorité | P2 |

## 1. Résumé
Permettre d'étendre le client via `client.$use(middleware)` interceptant requêtes, et `client.$extends(extension)` ajoutant méthodes/computed fields.

## 2. Contrat
- `Middleware = Fn(QueryContext, Next) -> Result<QueryResult>`
- `Extension { name, query: { model: { operation: fn } }, result: { field: { compute } } }`
- API : `client.use_middleware(|ctx, next| { trace(ctx); next(ctx) })`
- Computed fields : `result.user.fullName { needs: [firstName,lastName], compute: |r| format!(...) }`

## 3. Critères
- Middleware chaîné, ordre préservé, peut court-circuiter
- Extension ajoute méthode typée vérifiée à la compilation

## 4. Tests
- `middleware_intercepts`, `extension_computed_field`
