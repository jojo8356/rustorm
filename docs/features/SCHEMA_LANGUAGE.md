# Fiche de fonctionnalité — Langage `.rsm` : grammaire, validation et formatage

| Métadonnée | Valeur |
|---|---|
| Identifiant | `SCHEMA-001` |
| Produit | Rustorm — ORM Rust et langage de schéma `.rsm` |
| Statut | **Spécification proposée ; implémentation partielle P0** |
| Version | 1.0 |
| Date | 1 novembre 2026 |
| Références | [Cahier des charges §5](../CAHIER_DES_CHARGES.md#5-langage-rsm), [README](../../README.md) |
| Priorité | P0 critique |
| Périmètre | Fichier `.rsm` unique, modèles, grammaire EBNF, diagnostics, formatage |

## 1. Résumé
Définir, parser et valider le langage de schéma `.rsm` (extension obligatoire) avec messages localisés, et fournir un formateur idempotent. Le schéma est la source de vérité pour IR, génération et migrations.

## 2. Objectifs mesurables
| ID | Objectif | Mesure |
|---|---|---|
| O-01 | Parser conforme EBNF §5.2 | Tests de conformité grammaire passent, erreurs localisées ligne/colonne |
| O-02 | Diagnostics collectés | Plusieurs erreurs par fichier avec code stable, sévérité, suggestion |
| O-03 | Formatage idempotent | `format` deux fois → bytes identiques, préserve ordre/commentaires, ne formate pas fichier invalide |

## 3. Contrat §5.1–5.2
- UTF-8, commentaires `//`, `///`, `/*…*/`, extension `.rsm` obligatoire pour CLI
- Blocs `datasource`, `generator`, `model`, `enum` ; champs `ident type {@attr}` ; `Type?` nullable, `Type[]` liste relation ; `Type[]?` rejeté
- `datasource` : `provider="postgresql"`, `url=env("VAR")` seule forme autorisée ; `generator` : `provider="rustorm-client"`, `output` relatif au schéma, pas de sortie hors projet, pas de lien symbolique traversé
- Identifiants `[A-Za-z_][A-Za-z0-9_]*`, chaînes JSON avec échappements, nombres décimaux signés, retours LF/CRLF, virgule finale autorisée
- Formateur : préserve sémantique, idempotent, ne détruit jamais contenu invalide

## 4. État actuel (octobre 2026)
- Lexer/parser manuel dans `src/schema.rs` : gère `datasource`, `generator`, `model` avec champs scalaires et relations 1–1/1–N/N–N limitées, `@id`, `@@id`, `@unique`, `@map`, `@@map`, `@@index` mono-colonne, `@default(autoincrement()|string|number|boolean)`
- Diagnostics avec ligne/colonne ; modes texte/JSON
- **Manques** : `enum` non parsé, `Type[]?` correctement rejeté mais message à améliorer, commentaires `///` non conservés, formateur absent (`rustorm format` n’existe pas), validation `env()` limitée à littéral mais pas de vérification d’existence, `output` déjà vérifié mais pas de test de traversée de lien symbolique dans tous les cas, EBNF non vérifiée formellement

## 5. Règles de validation (VAL)
| ID | Règle | Effet |
|---|---|---|
| VAL-G01 | Identifier enum et ses variantes, `@map` sur variante | Manque → diagnostic |
| VAL-G02 | Rejeter `enum` avec doublons, type inconnu | Erreur localisée |
| VAL-G03 | Parser doit conserver spans pour chaque token | Pour diagnostics précis |
| VAL-F01 | `rustorm format` idempotent | Test AC-01 |

## 6. Critères d’acceptation
| ID | Critère |
|---|---|
| AC-Schema-01 | Exemple §5.1 `validate`, `format`×2, `generate`×2 → succès idempotent |
| AC-Schema-02 | Fichier avec `enum Role { ADMIN }` valide, génère enum Rust ; doublon → erreur |
| AC-Schema-03 | `format --check` code 2 si diff, 0 sinon |

## 7. Tests
- Parseur : validés, invalides (type inconnu, attribut erroné, `Type[]?`), fuzzing sans panic
- Format : round-trip, idempotence, conservation commentaires
- CLI : `validate --json` contient code+position

## 8. Risques
| Risque | Réduction |
|---|---|
| Grammaire divergente de Prisma | EBNF versionnée, tests contre fixtures Prisma 7 |
| Format destructif | Interdire format sur invalide + tests |

## 9. Décisions ouvertes
- Version exacte de la grammaire PSL à suivre ; politique de commentaires `///`
- Emplacement du formateur (CLI vs crate séparée)

## 10. Périmètre livré / restant
- Livré : parse `model`/`datasource`/`generator` de base, validation doublons, diagnostics
- Restant : `enum`, `format`, tests EBNF complets, préservation commentaires
