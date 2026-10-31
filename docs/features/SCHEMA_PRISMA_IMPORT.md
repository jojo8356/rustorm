# Fiche de fonctionnalité — Import et conversion `.prisma` → `.rsm`

| Métadonnée | Valeur |
|---|---|
| Identifiant | `SCHEMA-003` |
| Statut | **Spécification P1 ; non implémenté** |
| Version | 1.0 |
| Date | 1 novembre 2026 |
| Références | [Cahier §5.4](../CAHIER_DES_CHARGES.md#54-conversion-depuis-prisma-prisma--rsm--p1) |
| Priorité | P1 |

## 1. Résumé
Importer des schémas Prisma (PSL) 5 et 7 vers `.rsm` de façon non destructive, avec rapports d’incompatibilité.

## 2. Contrat §5.4 (P1)
- Commande `rustorm import prisma --from schema.prisma --to schema.rsm` (ou `migrate import` selon CLI finale)
- Entrée : PSL avec `datasource db { provider="postgresql" }`, `model`, `enum`, previewFeatures ignorées avec avertissement
- Traduction : `datasource→datasource`, `generator client→generator rustorm-client`, modèles/champs/enums/attributs équivalents ; attributs inconnus → préservés en commentaire `// @prisma.xxx` ou rejetés avec rapport
- Sortie idempotente, formatée ; n’écrase jamais sans `--force`
- Rapport : éléments traduits, ignorés, manuels requis

## 3. État actuel
- Aucun import Prisma n’existe. `examples/schema.rsm` est manuscrit. Aucun test de parité PSL→.rsm.

## 4. Règles VAL
| ID | Règle |
|---|---|
| VAL-P01 | `provider` non-postgresql → avertissement + blocage generation |
| VAL-P02 | Attribut `@prisma.*` inconnu → commentaire + entrée rapport |
| VAL-P03 | Fichier sortie existant sans `--force` → erreur |

## 5. Critères
| ID | Critère |
|---|---|
| AC-Import-01 | `schema.prisma` AC §5.1 converti → validate/generate/migrate dev ok |
| AC-Import-02 | PreviewFeatures → rapport warnings, pas d’échec silencieux |
| AC-Import-03 | Round-trip `import` 2× → bytes identiques |

## 6. Tests
- Fixtures Prisma 5 et 7 (blog, e-commerce) → .rsm → DDL identique
- Attributs inconnus → rapport
- N’écrase pas sans force

## 7. Risques
| Risque | Réduction |
|---|---|
| Divergence PSL versions | Pinner versions testées, documenter incompatibilités |
| Perte sémantique | Toujours échouer plutôt qu’ignorer silencieusement |

## 8. Livré / restant
- Livré : rien
- Restant : toute la fonctionnalité P1
