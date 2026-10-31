# Rustorm — cahier des charges fonctionnel et technique

**Statut :** proposition de conception, non implémentée · **Version :** 0.1 · **Date :** 30 octobre 2026
**Périmètre :** ORM pour applications Rust, schéma externe `.rsm`, client de requêtes typé inspiré de Prisma.

## 1. Vision et résultats attendus

Rustorm doit permettre de décrire des modèles dans un fichier externe lisible, de gérer leur évolution en base et d'interroger les données depuis Rust avec une ergonomie proche de Prisma, sans renoncer au typage, à `Result`, aux conventions de nommage et à l'asynchronisme de Rust.

Le produit visé comprend : un langage de schéma ; une CLI ; la validation et le formatage ; la génération de types et d'un client Rust ; un moteur de requêtes et ses connecteurs ; les migrations et l'introspection ; une documentation et des tests. L'objectif à terme est une **équivalence fonctionnelle explicitement mesurée**, pas une compatibilité binaire ni une traduction littérale de l'API TypeScript.

**État à la rédaction :** le dépôt ne contenait que `README.md`. Depuis, un prototype partiel a été ajouté ; consulter le [README](../README.md) pour les fonctionnalités réellement disponibles. Sauf indication contraire, les exemples ci-dessous restent des contrats proposés et ne constituent pas une API complète ou testée.

### Principes

1. `.rsm` conserve la structure de schéma de Prisma (`model`, `datasource`, `@id`, relations, etc.) ; les applications utilisent une API Rust idiomatique en `snake_case`. L'extension ne suffit pas à garantir la compatibilité : un sous-ensemble précis et des erreurs explicites sont nécessaires.
2. Typage statique dès que possible : champs, filtres, données d'écriture, clés uniques, projections et résultats dérivent du schéma validé. Pas de chaîne SQL construite à partir de valeurs utilisateur.
3. Une seule source de vérité : le modèle intermédiaire validé (IR) alimente génération, validation, SQL et migrations. Pas d'écriture silencieuse en cas d'information perdue.
4. Architecture par capacités de connecteur : une fonction non prise en charge échoue avec un diagnostic, jamais par une émulation qui modifie silencieusement sa sémantique.
5. Déploiement et migrations sûrs par défaut, fonctionnement local, pas de télémétrie ni appel réseau implicite.

## 2. Analyse de référence et limites

L'analyse a porté sur l'organisation des deux dépôts, leur architecture, des points d'entrée représentatifs et leurs tests, **pas sur une lecture ligne par ligne de l'intégralité de leurs dizaines de milliers de fichiers**. Les sources sont épinglées pour permettre de compléter l'audit et de reproduire les conclusions. Elles ne prouvent pas à elles seules une parité de comportement : celle-ci exige des tests différentiels par version et par connecteur.

| Référence inspectée | Constats utiles pour Rustorm |
|---|---|
| [Prisma ORM v7, commit `97ea359`](https://github.com/prisma/orm/tree/97ea359250335c1a174504c9b2b7e5fe030ab937) | [Générateur de client](https://github.com/prisma/orm/tree/97ea359250335c1a174504c9b2b7e5fe030ab937/packages/client-generator-ts/src/TSClient), [CLI](https://github.com/prisma/orm/tree/97ea359250335c1a174504c9b2b7e5fe030ab937/packages/cli/src), [migrations](https://github.com/prisma/orm/tree/97ea359250335c1a174504c9b2b7e5fe030ab937/packages/migrate/src) et [tests de types du client](https://github.com/prisma/orm/tree/97ea359250335c1a174504c9b2b7e5fe030ab937/packages/client/src/__tests__/types) : un modèle génère des opérations et des types dédiés ; CLI et migrations constituent des produits à part entière. Référence principale pour la forme `prisma.user.findMany`. |
| [Prisma Engines, commit `d1b3f37`](https://github.com/prisma/prisma-engines/tree/d1b3f377ca835d73fd50954a5e22128b9ebdd6b8) | [Grammaire PSL](https://github.com/prisma/prisma-engines/blob/d1b3f377ca835d73fd50954a5e22128b9ebdd6b8/psl/schema-ast/src/parser/datamodel.pest), [validation PSL](https://github.com/prisma/prisma-engines/tree/d1b3f377ca835d73fd50954a5e22128b9ebdd6b8/psl), [graphe de requêtes](https://github.com/prisma/prisma-engines/tree/d1b3f377ca835d73fd50954a5e22128b9ebdd6b8/query-compiler/core/src/query_graph_builder), [structures de requêtes](https://github.com/prisma/prisma-engines/tree/d1b3f377ca835d73fd50954a5e22128b9ebdd6b8/query-compiler/query-structure) et [architecture des migrations](https://github.com/prisma/prisma-engines/blob/d1b3f377ca835d73fd50954a5e22128b9ebdd6b8/schema-engine/ARCHITECTURE.md) : séparer syntaxe, sémantique, planification, connecteurs et historique des migrations. Ce commit est une référence d'architecture indépendante ; sa correspondance exacte avec la branche v7 ci-dessus n'a pas été vérifiée. |
| [Prisma ORM 8.0.0-rc.22, commit `4f70a33`](https://github.com/prisma/orm/tree/4f70a331c2c2b36bf3125a20cd79bf26a990cea5) | [Architecture contract-first](https://github.com/prisma/orm/blob/4f70a331c2c2b36bf3125a20cd79bf26a990cea5/ARCHITECTURE.md), [client ORM SQL](https://github.com/prisma/orm/tree/4f70a331c2c2b36bf3125a20cd79bf26a990cea5/packages/3-extensions/sql-orm-client/src) et [matrice de fonctionnalités](https://github.com/prisma/orm/blob/4f70a331c2c2b36bf3125a20cd79bf26a990cea5/scorecard.md). Prisma 8 est une réécriture TypeScript en version candidate : son approche contrats/AST/capacités inspire l'IR, mais sa DSL et ses niveaux de support ne sont pas assimilables automatiquement à ceux de v7. |

Les dépôts étudiés affichent une licence Apache-2.0. La présente proposition privilégie une implémentation originale. Si du code, des tests ou une grammaire sont repris, conserver licence, notices et attributions applicables, vérifier les dépendances et consulter un conseil juridique ; ne pas utiliser les marques Prisma comme marque de Rustorm. Les objectifs de parité ne constituent pas une autorisation de copie sans conditions.

## 3. Périmètre et priorités

**P0** = indispensable à la première version utilisable ; **P1** = parité relationnelle étendue ; **P2** = extension ultérieure. Les références v7 et v8 étant différentes, chaque annonce de compatibilité doit nommer version Prisma, connecteur, fonctionnalité et tests.

| Domaine | P0 : PostgreSQL | P1 | P2 |
|---|---|---|---|
| Schéma | fichier `.rsm` unique, modèles, types P0, enum, clés, index, relations 1–N, diagnostics et formatage | multi-fichiers, relations 1–1 et N–N, types natifs supplémentaires | vues, composites, extensions spécialisées |
| Client | génération, CRUD scalaire, filtres, tri, pagination par offset, chargement 1–N, transactions | curseurs, agrégats, `group_by`, écritures imbriquées, projections avancées | extensions et middlewares publics |
| Connecteurs | PostgreSQL 15–17 | SQLite, MySQL/MariaDB après matrices propres | CockroachDB, SQL Server, MongoDB après étude séparée |
| Migration | création SQL révisable, shadow DB, déploiement, état, résolution, introspection limitée | diff/introspection avancés, gestion de capacités spécifiques | autres stratégies et connecteurs |
| Outils | CLI locale, configuration, CI, messages lisibles et JSON | import `.prisma`, LSP | Studio, import SQL avancé |

**Hors périmètre P0 :** MongoDB, service hébergé, Prisma Accelerate, Prisma Studio, API JavaScript, parité complète, exécution synchrone, support no_std, multi-tenant automatique, génération de validations métier et interface HTTP. SQLite n'est pas présenté comme disponible avant sa propre suite de tests. Le P0 est une cible de livraison, pas une promesse d'existence actuelle.

## 4. Architecture proposée

```text
schema.rsm ─► lexer/parser/CST ─► AST ─► validation + capacités ─► IR versionné
                    │                   │                          ├─► générateur Rust
                    └─► formateur       └─► diagnostics            ├─► diff/migrations
                                                                    └─► contrat de requête
application ─► client généré ─► AST requête typé ─► plan ─► SQL + paramètres
                                                       └─► adaptateur PostgreSQL ─► pool
base existante ─► introspection ─► IR ─► .rsm / diff
```

Workspace Cargo proposé : `rustorm-schema` (parseur, spans, CST, validation), `rustorm-ir` (types normalisés versionnés), `rustorm-codegen`, `rustorm-query`, `rustorm-runtime`, `rustorm-connector-postgres`, `rustorm-migrate`, `rustorm-cli` et façade `rustorm`. Les dépendances vont du haut vers les abstractions, sans dépendance de `schema` vers un pilote de base. Les traits de connecteur décrivent les capacités et opérations ; un connecteur ne reçoit pas de chaînes arbitraires en lieu et place d'un AST vérifié. Choix d'un pilote et du runtime async à figer après prototype et revue licence ; le P0 doit documenter un seul runtime effectivement testé, avec pool et TLS configurables.

IR déterministe : ordre stable, format versionné et hash du contenu canonique (sans secrets ni chemins machine). Le générateur embarque version de protocole et hash ; au démarrage, détecter incompatibilité client/runtime. Ce hash n'est pas une preuve que la base réelle correspond au schéma ; la détection de dérive relève de l'introspection/migrations. Sortie des fichiers générés déterministe et atomique.

## 5. Langage `.rsm`

### 5.1 Contrat général

UTF-8, extension obligatoire `.rsm` pour la CLI par défaut, commentaires `//`, `///` et `/* … */`. Les constructions et les annotations ci-dessous reprennent volontairement la forme PSL ; ce **n'est pas du Rust**. Les types et résultats de requête, eux, sont du Rust. À P0, un seul schéma et un `datasource` PostgreSQL ; les noms des modèles/champs sont sensibles à la casse. Les valeurs et les noms de champs de base ne sont jamais normalisés silencieusement. Les différences avec PSL et les constructions non prises en charge sont listées dans les diagnostics et la matrice de compatibilité.

Exemple canonique :

```prisma
// rustorm/schema.rsm
datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

generator client {
  provider = "rustorm-client"
  output   = "../src/generated/rustorm"
}

model User {
  id        Int      @id @default(autoincrement())
  email     String   @unique
  name      String?
  published Boolean  @default(false)
  posts     Post[]

  @@map("users")
}

model Post {
  id        Int     @id @default(autoincrement())
  title     String
  authorId  Int     @map("author_id")
  author    User    @relation(fields: [authorId], references: [id], onDelete: Cascade)

  @@index([authorId])
  @@map("posts")
}
```

`url` est une convention Rustorm proposée, y compris si une version de Prisma déplace la configuration de connexion ailleurs. Sa valeur `env` est résolue à l'exécution des commandes qui en ont besoin ou à l'initialisation explicite du client, **jamais intégrée au code généré**. `validate` et `generate` ne nécessitent pas l'accès à la base ni la présence de la variable ; elles valident l'expression. `output` est relatif au répertoire du fichier source. Interdire les sorties hors projet sauf option explicite ; ne pas écraser de fichiers non générés.

### 5.2 Grammaire normative du sous-ensemble P0

La notation ci-dessous est une EBNF simplifiée ; une implémentation doit fournir une grammaire exécutable et des tests de conformité. Les retours à la ligne séparent champs et propriétés ; espaces et commentaires sont ignorés hors littéraux. Un bloc ou champ inconnu est une erreur, pas une instruction ignorée.

```ebnf
schema       = { datasource | generator | model | enum | comment } ;
datasource   = "datasource" ident "{" { ident "=" expr newline } "}" ;
generator    = "generator" ident "{" { ident "=" expr newline } "}" ;
model        = "model" ident "{" { field | block_attr | comment } "}" ;
enum         = "enum" ident "{" { ident [ field_attr ] newline | block_attr | comment } "}" ;
field        = ident type { field_attr } newline ;
type         = ident [ "?" | "[]" ] ;
field_attr   = "@" path [ "(" [ args ] ")" ] ;
block_attr   = "@@" path [ "(" [ args ] ")" ] newline ;
args         = arg { "," arg } [ "," ] ;
arg          = [ ident ":" ] expr ;
expr         = string | number | "true" | "false" | path
             | path "(" [ args ] ")" | "[" [ expr { "," expr } ] "]" ;
path         = ident { "." ident } ;
```

Identifiants P0 : `[A-Za-z_][A-Za-z0-9_]*`. Chaînes avec échappements JSON ; nombres décimaux signés ; retours de ligne LF ou CRLF ; virgule finale autorisée dans les arguments. `Type?` = nullable ; `Type[]` = liste de relations dans P0 ; `Type[]?`, types scalaires en liste et champs avec `:` sont rejetés en P0. Les types non reconnus, fonctions, paramètres et annotations inconnus produisent une erreur localisée. `env` n'accepte qu'une chaîne littérale. La grammaire décrit la forme ; la validation sémantique impose les paramètres et types autorisés. Un formateur doit préserver l'ordre, les commentaires et les sémantiques, être idempotent et ne jamais formater un fichier syntaxiquement invalide en détruisant son contenu.

### 5.3 Types, attributs et validation

| Type `.rsm` | Rust généré | PostgreSQL P0 / contraintes |
|---|---|---|
| `Int` | `i32` | `integer` ; débordement = erreur |
| `BigInt` | `i64` | `bigint` |
| `Float` | `f64` | `double precision` ; valeurs non finies explicitement testées |
| `Boolean` | `bool` | `boolean` |
| `String` | `String` | `text` |
| `DateTime` | type UTC choisi et réexporté par `rustorm` | `timestamptz` ; préciser précision et arrondis dans la documentation d'implémentation |
| `Bytes` | `Vec<u8>` | `bytea` |
| `Json` | `serde_json::Value` | `jsonb` ; distinguer NULL SQL et JSON null via types explicites |
| enum | enum Rust généré | enum PostgreSQL nommé ; mapping et évolution validés |
| `T?` | `Option<T>` | colonne nullable, sauf relation virtuelle |

`Decimal`, UUID natif, types `@db.*`, tableaux scalaires, `Unsupported` et géométrie sont P1/P2 ; échec explicite en P0. Les types de relation ne créent pas de colonne ; les scalaires mentionnés dans `fields` la créent. Un schéma peut importer un ensemble plus large uniquement lorsque les capacités correspondantes sont implémentées.

Attributs P0 : `@id`, `@unique`, `@default(valeur | autoincrement() | now())`, `@map("nom")`, `@relation(fields: [...], references: [...], onDelete: ..., onUpdate: ...)`, `@@id([...])`, `@@unique([...])`, `@@index([...])`, `@@map("nom")` et `@map` sur variante d'enum. Une clé est soit `@id`, soit `@@id` ; listes de champs non vides et sans doublons. Limiter `autoincrement()` à `Int`/`BigInt` compatibles ; `now()` à `DateTime`. Les valeurs par défaut doivent correspondre au type du champ ; `null` n'est pas un défaut implicite pour un champ requis. Champs requis sans valeur par défaut sont obligatoires dans `Create` ; nullable peut être omis et devient NULL. `@default` correspond à un défaut de base et un champ omis ne doit pas être envoyé comme NULL.

Pour les relations 1–N P0 : une relation inverse sur le modèle cible ; un seul côté porte `fields` et `references` ; même nombre, ordre et types compatibles ; `references` cible une clé unique ; les champs scalaires forment une FK ; noms et ambiguïtés validés. Actions supportées explicitement : `Cascade`, `Restrict`, `NoAction`, `SetNull`, seulement si PostgreSQL et la nullabilité le permettent. Les valeurs par défaut des actions doivent être fixées et testées contre la version Prisma visée avant la mention « compatible » ; Rustorm ne prétend pas les reproduire sans test. `SetDefault`, relations autoréférentes ambiguës, mode sans FK et plusieurs relations ambiguës : erreur P0. `@@id`, `@@unique` définissent des sélecteurs de clé composés typés ; `@@index` ne rend pas les valeurs uniques.

Valider doublons, types, références, clés absentes, defaults invalides, cycles de noms, collisions SQL et collisions de noms Rust après conversion en `snake_case`/`PascalCase`. Les mots réservés Rust sont échappés en `r#ident` lorsque possible ; toute collision restante est une erreur indiquant les deux emplacements. Les annotations `@map` et `@@map` concernent la base, pas le renommage Rust. Diagnostic : code stable, sévérité, fichier, ligne/colonne et plage, contexte et suggestion ; plusieurs erreurs collectées si possible. Modes texte et JSON ont le même code et les mêmes positions.

### 5.4 Conversion `.prisma` → `.rsm`

P1 : `rustorm import prisma --input prisma/schema.prisma --output rustorm/schema.rsm`. **Ce n'est pas un renommage aveugle.** Parser, inventorier version et fournisseurs, convertir les fonctions connues, traiter le `generator` et la configuration externe séparément, puis valider selon le connecteur Rustorm. Produire un rapport de compatibilité par construction, avec chemin, ligne, adaptation et diagnostic. Les éléments non représentables bloquent l'écriture par défaut ; aucune perte silencieuse. `--report-only` ne modifie rien ; `--force` autorise uniquement l'écriture d'une copie accompagnée d'erreurs, jamais son utilisation comme schéma valide. L'import n'exécute pas de migrations et ne modifie ni source ni base. Tester les cas Prisma 7, y compris URL configurée hors schéma, multi-fichiers, annotations inconnues et collisions de noms. Ne pas affirmer compatibilité générale Prisma 8 sans analyse et matrice dédiée.

## 6. Client Rust et requêtes

### 6.1 Génération et intégration

Le générateur fournit `RustormClient`, un module par modèle (`user`, `post`), les types d'enregistrement, `Where`, `WhereUnique`, `Create`, `Update`, les projections et des builders. Traitements asynchrones et résultats `Result<T, RustormError>`. Les méthodes utilisent `snake_case` : `findMany` → `find_many`, `findUnique` → `find_unique`. Les noms de champs `.rsm` sont convertis pour l'API (`authorId` → `author_id`) sans modifier leur mapping SQL. Générer à la demande via CLI, puis inclure le module dans le projet ; un mode `build.rs` est envisageable plus tard. Versionner ou ignorer les fichiers générés au choix du projet, documenter les deux et détecter les différences via `generate --check`.

**Intégration cible d’un projet consommateur :**

```text
mon-app/
  Cargo.toml                  # dépendance rustorm/runtime compatible
  rustorm/schema.rsm
  rustorm/migrations/
  src/main.rs
  src/generated/rustorm/mod.rs  # généré, ne pas modifier à la main
```

Après installation de la CLI compatible, le parcours prévu est `rustorm validate`, `rustorm format`, `rustorm migrate dev --name initial`, `rustorm generate`, puis `cargo build`. `src/main.rs` déclare `mod generated` (avec son `mod.rs`) et utilise le client généré. Le numéro de version de l'outil, du runtime et du format généré doit être vérifié ; un message prescrit la régénération lorsqu'ils sont incompatibles. Les fichiers supprimés par régénération doivent appartenir à un manifeste de génération Rustorm, jamais à un utilisateur ; un échec de génération ne laisse pas une sortie partiellement écrite. Le schéma et les migrations restent sous contrôle de version ; aucun secret n'y figure.

### 6.2 Exemples contractuels proposés

Les exemples ci-dessous expriment l'API souhaitée ; noms et signatures seront figés par tests de compilation avant implémentation stable. Le schéma est celui de §5.1. `Create::new` reçoit tous les champs scalaires requis sans défaut, puis les champs facultatifs sont des builders. Une méthode `exec` exécute réellement la requête ; la construction seule n'accède pas à la base.

```rust
use crate::generated::rustorm::{post, user, RustormClient};

let db = RustormClient::connect_env().await?; // lit DATABASE_URL

let alice = db.user().create(
    user::Create::new("alice@example.org".to_owned())
        .name(Some("Alice".to_owned()))
).exec().await?;

let found: Option<user::Record> = db.user()
    .find_unique(user::WhereUnique::id(alice.id))
    .exec().await?;

let users: Vec<user::Record> = db.user()
    .find_many()
    .where_(user::email::contains("example.org"))
    .where_(user::published::equals(false)) // AND avec le filtre précédent
    .order_by(user::id::asc())
    .skip(0)
    .take(20)
    .exec().await?;

let updated = db.user()
    .update(user::WhereUnique::id(alice.id), user::Update::new().published(true))
    .exec().await?;

let article = db.post().create(
    post::Create::new("Premier article".to_owned(), alice.id)
).exec().await?;
```

Chargement relationnel P0 explicite, sans N+1 non borné :

```rust
let rows: Vec<user::WithPosts> = db.user()
    .find_many()
    .include(user::posts())
    .exec().await?;
// WithPosts { user: user::Record, posts: Vec<post::Record> }
```

Le type retourné change lors de `include` ; les relations ne sont pas cachées dans `Record` et ne déclenchent pas de chargement implicite. P0 n'impose que cet include 1–N non filtré ; `select` et includes imbriqués/filtrés sont P1. Les résultats de projection devront être des types générés ou des types génériques inférés au compilateur ; un champ non sélectionné ne doit pas être accessible. Tester explicitement les combinaisons avec `select`, `include` et `omit` ; refuser celles dont la sémantique n'est pas implémentée.

### 6.3 Contrat des opérations

| Opération Prisma de référence | Rustorm proposé | Résultat / comportement |
|---|---|---|
| `findUnique` | `find_unique(unique).exec()` | `Result<Option<Record>>`, clé unique obligatoire |
| `findFirst` | `find_first().…exec()` | `Result<Option<Record>>` |
| `findMany` | `find_many().…exec()` | `Result<Vec<Record>>` |
| `create` | `create(data).exec()` | enregistrement créé |
| `update` / `delete` | `update(unique, data)` / `delete(unique)` | enregistrement ; `NotFound` si absent |
| `createMany` | `create_many(rows)` | nombre inséré, sans `skip_duplicates` implicite |
| `updateMany` / `deleteMany` | `update_many(filter, data)` / `delete_many(filter)` | nombre affecté ; filtre vide explicite obligatoire |
| `upsert` | `upsert(unique, create, update)` | P1 ; définir atomicité et courses par connecteur |
| `count`, `aggregate`, `groupBy` | `count`, `aggregate`, `group_by` | P1 ; résultats et champs valides typés |

Les opérations P0 de cette table sont toutes obligatoires sauf celles marquées P1. `find_first` sans tri n'offre aucun ordre garanti ; documenter les requêtes concernées. `find_unique` ne choisit pas arbitrairement un enregistrement non unique. Les données `Update` distinguent `Unset` de `Set(None)` : omission ≠ écriture de NULL ; un champ non nullable ne peut recevoir `None`. Les identifiants et noms sont issus de l'IR et correctement quotés ; les valeurs sont des paramètres bindés. En P0, un filtre vide explicite pour `update_many`/`delete_many` requiert un constructeur `AllRows::confirm()` spécifique, non un simple `[]` accidentel.

Filtres P0 : `equals`, `not`, `in`, `not_in`, `lt/lte/gt/gte` pour types ordonnables ; `contains`, `starts_with`, `ends_with` pour chaînes ; `is_null`/`is_not_null` sur colonnes optionnelles ; composition `and`, `or`, `not`. API générée interdit les opérateurs non pertinents. `%`, `_` et le caractère d'échappement des recherches littérales doivent être échappés ; préciser collation et sensibilité à la casse PostgreSQL. `equals(None)` n'est pas une comparaison `= NULL`. Préciser et tester la logique SQL à trois valeurs pour `not`, `not_in` et combinaisons ; ensembles vides ont une définition stable (`in([])` faux, `not_in([])` vrai, y compris pour les lignes nulles : ce choix est propre à Rustorm et doit être comparé à Prisma avant toute revendication de parité). Les filtres de relation `some/every/none/is/is_not`, JSON et recherche insensible à la casse sont P1.

Tri P0 : champ scalaire, `asc`/`desc`, plusieurs clauses dans l'ordre fourni ; ordre de NULL explicité par API ou documentation, testé par connecteur. `skip` et `take` sont entiers non négatifs, limites contrôlées ; pagination stable exige un tri unique explicite et la documentation alerte si absent. Curseur basé sur clé unique, `distinct`, filtres relationnels, agrégats, écriture imbriquée `create/connect/disconnect/set/delete/update/upsert`, opérations atomiques `increment/decrement` sont P1 avec spécifications et tests propres. `every` sur ensemble vide est vrai ; `some` faux ; `none` vrai.

### 6.4 Transactions, erreurs et ressources

P0 : transaction interactive via garde explicite, une connexion réservée au pool et toutes les requêtes sur cette garde ; `commit` ou `rollback`, retour au pool après nettoyage. Si la garde est abandonnée ou si la future est annulée, rollback ou invalidation de connexion avant réutilisation ; ne jamais promettre qu'un rollback asynchrone a fini au moment de `Drop`. Exemple contractuel :

```rust
let mut tx = db.begin().await?;
let user = tx.user().create(user::Create::new("bob@example.org".into())).exec().await?;
tx.post().create(post::Create::new("Bonjour".into(), user.id)).exec().await?;
tx.commit().await?;
```

Un échec avant commit conduit à une annulation ; `commit` après erreur ne doit pas réussir silencieusement. Documenter niveaux d'isolation, timeout acquisition/transaction et erreurs de deadlock/sérialisation. Pas de retry automatique de closure potentiellement à effets externes ; documenter les reprises idempotentes. Transactions imbriquées/savepoints, batch transaction et options de niveau d'isolation : P1. `RustormClient` est clonable à faible coût et partage le pool ; fermeture explicite et ordre de shutdown documentés.

Erreurs publiques structurées, non dépendantes des messages du pilote : `Validation`, `Connection`, `Timeout`, `UniqueViolation`, `ForeignKeyViolation`, `NotFound`, `SerializationFailure`, `Migration`, `Unsupported`, `Decode`, `Internal`. Conserver une source et un code de base quand disponible sans exposer secrets ni valeurs sensibles ; `Display` utile, `Debug` expurgé. Pas de `panic` pour une entrée utilisateur ou une erreur base.

## 7. Migrations et introspection

### 7.1 Artéfacts et invariants

```text
rustorm/
  schema.rsm
  migrations/
    20261030120000_initial/
      migration.sql
    migration_lock.toml
```

Chaque migration contient du SQL révisable ; `migration_lock.toml` fixe le fournisseur. L'historique `_rustorm_migrations` est propre à Rustorm : identifiant, nom, checksum SHA-256 du fichier, début, fin, statut, erreur expurgée et résolution. Ne jamais réutiliser automatiquement `_prisma_migrations` ni supposer que des migrations Prisma ont été appliquées par Rustorm. Prévoir commande de baseline documentée pour une base existante, avec vérification opérateur, sans exécuter rétroactivement les scripts.

`migrate dev` : vérifier les checksums et l'historique, rejouer l'historique dans une **shadow DB distincte explicitement configurée**, comparer au schéma souhaité, produire une migration SQL, signaler les opérations destructrices, puis appliquer seulement après confirmation. Refuser une shadow DB égale à la base cible ; ne pas supprimer une base arbitraire. Pas de reset automatique en production ; reset uniquement explicite et confirmé en environnement de développement. Sans accès à une shadow DB sûre, échouer avec explication, pas deviner l'état antérieur.

`migrate deploy` : non interactif ; acquérir un verrou par base avant comparaison et application, appliquer dans l'ordre les migrations en attente ; ne pas générer de migration, réinitialiser ou corriger une dérive silencieusement. Un checksum modifié, une migration échouée non résolue, un historique incompatible ou un verrou non acquis bloque le déploiement avec code de sortie non nul. Selon les garanties DDL du connecteur, une migration peut être partielle : enregistrer l'état et exiger une résolution opérateur ; ne pas promettre une atomicité universelle. `migrate resolve` marque `applied` ou `rolled-back` après intervention humaine sans prétendre annuler le SQL. `status` distingue attente, échec, historique divergent et réussite. Les journaux ne contiennent pas d'URL complète ni de paramètres secrets.

Détecter suppression de colonne/table, changement de type risqué, ajout de NOT NULL sans défaut et modification de contrainte ; afficher plan et avertissement avant création/application en dev. Ne pas déduire automatiquement un renommage d'une suppression suivie d'un ajout. En production, documenter expand/contract et sauvegardes. La détection de dérive compare la base réelle au résultat attendu ; une impossibilité d'inspection ne vaut pas absence de dérive.

### 7.2 Introspection

P0 : `db pull` inspecte tables, colonnes, PK/FK, unicité, index, défauts et enums PostgreSQL pris en charge et propose un `.rsm` ; option `--print` sans écriture, sauvegarde/confirmation avant remplacement. Signaler explicitement triggers, vues, types, index ou expressions non représentables ; ne jamais prétendre à une restitution sans perte. Ne pas détruire un fichier existant avec des commentaires/mappings non conservés sans avertissement. Tester introspection → génération → diff vide sur le sous-ensemble supporté, et diagnostics sur éléments non représentables.

## 8. Interface CLI et configuration

Toutes les commandes ci-dessous sont proposées ; leur présence dépend du jalon. Schéma par défaut `rustorm/schema.rsm`, surcharge `--schema PATH`. Aucun appel réseau externe, hormis connexion explicitement demandée à la base. Aucune variable secrète dans les fichiers générés ou logs. La CLI n'exécute pas de programme défini par le schéma.

| Commande | Fonction | Priorité |
|---|---|---|
| `rustorm init` | créer squelette sans écraser de fichier | P0 |
| `rustorm validate [--json]` | analyser et valider sans base | P0 |
| `rustorm format [--check]` | formatter / vérifier sans réécrire | P0 |
| `rustorm generate [--check]` | générer / vérifier les fichiers Rust sans base | P0 |
| `rustorm migrate dev --name NAME` | planifier, confirmer et appliquer en dev | P0 |
| `rustorm migrate deploy` | appliquer en attente, non interactif | P0 |
| `rustorm migrate status` / `resolve` | état / résolution explicite | P0 |
| `rustorm migrate baseline` | enregistrer un historique initial vérifié | P0 |
| `rustorm db pull [--print]` | introspection limitée | P0 |
| `rustorm import prisma` | conversion avec rapport | P1 |
| `rustorm migrate diff` | comparer sources supportées, sans appliquer | P1 |
| `rustorm db push` / `migrate reset` | opérations de dev explicitement destructrices | P1 |
| `rustorm version` | versions CLI/runtime et formats | P0 |

Codes de sortie : `0` succès, `1` erreur d'entrée ou exécution, `2` incompatibilité/dérive ou `--check` non conforme ; documenter toute exception. `--json` fournit un objet versionné avec codes, diagnostic et chemin, sans ANSI. `--help` est local. Résolution des chemins et priorité de configuration documentées ; l'URL est fournie par variable d'environnement ou API de construction, sans défaut distant implicite. Les versions PostgreSQL réellement testées et les dépendances système sont précisées avant publication.

## 9. Sécurité, performance et exploitation

- SQL paramétré pour valeurs, identifiants provenant du schéma validé et quotés par le connecteur. SQL brut éventuel P1 : API explicitement nommée `raw`, valeurs paramétrées par défaut, chemins dangereux identifiés et documentés.
- Identifiants, URL, paramètres, résultats et secrets expurgés des erreurs et traces ; le logging des valeurs est désactivé par défaut. TLS et vérification des certificats configurables avec défauts sûrs ; pool borné, timeout et backpressure documentés.
- Extensions/plugins ne doivent pas contourner les frontières de validation ; pas d'exécution automatique depuis un fichier téléchargé. Examiner les licences et maintenir SBOM, audit de dépendances et processus de divulgation de vulnérabilité.
- Prévenir les requêtes démesurées : taille max de paramètres et profondeur de filtres configurables, inclure le coût et le nombre de requêtes des relations, éviter N+1. Instrumentation optionnelle : durée, nom opération, nombre de requêtes, succès/échec, trace ID, sans valeurs par défaut.
- Pas de chiffres de performance inventés : avant optimisation établir une baseline reproductible (machine, PostgreSQL, schéma, taille du jeu de données, concurrence, pool, latences p50/p95/p99, allocations et débit). Comparer SQL/temps à une requête paramétrée équivalente ; fixer les budgets à la fin du prototype, puis éviter les régressions mesurables en CI adaptée.
- Support initial Linux/macOS/Windows sur cibles à préciser après CI ; définir MSRV lors du premier prototype et la tester. Versionnement SemVer : API publique et format `.rsm`/IR ont politique d'évolution documentée ; migrations de format avec diagnostic avant changement incompatible.

## 10. Tests et critères de recette

Chaque fonctionnalité doit avoir des tests unitaires, tests d'intégration sur chaque connecteur revendiqué et, pour les exemples d'API, des tests de compilation réussie et d'échec (`trybuild` ou équivalent). Les jeux de test doivent être isolés et reproductibles, sans secrets réels.

| ID | Cas d'acceptation | Résultat attendu |
|---|---|---|
| AC-01 | Exemple §5.1 : `validate`, `format` deux fois et `generate` deux fois | valide ; format et génération idempotents, bytes identiques |
| AC-02 | Type inconnu, attribut erroné, FK incompatible, collision Rust | échec, code stable et positions des éléments concernés ; aucune génération |
| AC-03 | Construire les exemples §6 avec le client généré | compilation et exécution sur PostgreSQL réel, résultats typés attendus |
| AC-04 | Champ inexistant, mauvais type de filtre, `None` sur champ requis | échec à la compilation ; message intelligible |
| AC-05 | CRUD, unique manquant/dupliqué et FK invalide | valeurs et variantes d'erreur documentées, aucune perte silencieuse |
| AC-06 | `Unset`, `Set(None)`, défaut de base, JSON null et SQL NULL | quatre sémantiques distinctes ; round-trip correct |
| AC-07 | Filtres, valeur contenant `'`, `%`, `_`, séquences SQL hostiles | résultats attendus, valeurs bindées, aucun SQL injecté |
| AC-08 | `AND`/`OR`/`NOT`, listes vides, valeurs NULL, tri/pagination | résultats conformes aux règles §6 et aux tests de référence |
| AC-09 | Inclure les posts de plusieurs users, dont zéro post | résultat typé correct, nombre de requêtes borné indépendamment du nombre d'utilisateurs |
| AC-10 | Transaction : commit, erreur, annulation et abandon | commit persiste ; aucun changement partiel après rollback/nettoyage ; pool réutilisable |
| AC-11 | `migrate dev` avec shadow dédiée puis `deploy` sur base vierge | même schéma final ; déploiement répété sans réapplication |
| AC-12 | Fichier de migration modifié, échec partiel, accès simultané | déploiement bloqué et diagnostic ; un seul processus applique |
| AC-13 | Shadow identique à la cible ou suppression risquée | refus dans le premier cas ; avertissement/confirmation dans le second |
| AC-14 | Introspection d'un schéma pris en charge et d'un élément non pris en charge | round-trip sans diff pour le premier ; avertissement ou erreur explicite pour le second |
| AC-15 | Absence de `DATABASE_URL` | validate/generate fonctionnent ; connexion échoue proprement sans fuite de secret |
| AC-16 | Import Prisma avec syntaxe non prise en charge | rapport localisé, aucun écrasement ni transformation silencieuse |
| AC-17 | Logs et erreurs sur URL contenant mot de passe | aucun secret dans texte, JSON ou `Debug` |

En complément : fuzzing parseur et formateur (pas de panic, terminaison et conservation), snapshots SQL/IR déterministes, tests de propriétés sur relations et filtres, tests migration multi-processus et interruption, tests de performance, tests de sécurité, tests multi-OS. Définir une matrice différentielle reproductible face à une **version fixée** de Prisma 7 et les mêmes bases/fixtures pour chaque fonctionnalité revendiquée ; comparer résultats, nullabilité, erreurs normalisées et effets en base, non les noms de codes d'erreur internes. Les différences volontaires doivent être documentées. Utiliser les tests upstream conformément à leur licence si repris ; aucune affirmation de 100 % avant couverture démontrée.

## 11. Plan de réalisation et jalons

Les jalons sont ordonnés par dépendance et non chiffrés en dates : estimer après prototype, recrutement et choix techniques. Chaque jalon se ferme uniquement lorsque ses tests et sa documentation sont validés.

1. **M0 — cadrage et prototype :** architecture, choix runtime/pilote, MSRV, limites du langage, politique licence, maquettes de types générés, installation PostgreSQL CI, baseline performance. Livrables : ADR et exemple minimal compilable.
2. **M1 — schéma :** parser, CST, IR, validation P0, diagnostics, formateur et tests de propriétés ; AC-01/02. Aucun SQL ni migration implicite.
3. **M2 — client scalaire :** génération déterministe, types et CRUD, filtres, typage, pool et erreurs ; AC-03 à 08 et 15/17.
4. **M3 — relations et transactions :** 1–N, `include`, FK, garde transactionnelle, annulation et pool ; AC-09/10. Mesure N+1.
5. **M4 — migrations et livraison P0 :** migrations, shadow DB, état/résolution/baseline, introspection limitée, sécurité et CI ; AC-11 à 15 ; audit de compatibilité et documentation. La publication P0 requiert tous les AC P0, y compris compilations positives et négatives, sans test critique ignoré.
6. **M5 — P1 :** import `.prisma` et AC-16 ; SQLite puis MySQL, chacun avec matrices et suites dédiées ; relations supplémentaires, select/include avancés, curseurs, nested writes, agrégats, upsert et LSP. Prioriser par valeur et difficultés, pas par un simple copier-coller de l'API.
7. **M6 — P2 :** autres fournisseurs, extensions, fonctionnalités avancées et compatibilité Prisma 8 après décision documentée et tests.

### Risques et décisions à prendre

| Risque | Réduction / décision |
|---|---|
| « Tout Prisma » représente plusieurs produits, versions et bases | maintenir une matrice détaillée, marquer chaque cellule par preuve de test ; ne pas annoncer la parité globale |
| DSL Prisma/objets dynamiques peu naturels en Rust | builders et enums générés ; vérifier la lisibilité et temps de compilation sur grands schémas |
| Dérive schéma/base ou migration partielle | shadow isolée, checksums, verrou, statut, procédures de récupération et revue SQL |
| Portabilité SQL, NULL, collations, pagination | capacités explicites et tests de conformité par connecteur |
| Dépendance à une implémentation amont changeante | références épinglées, IR propre et tests différentiels versionnés |
| Compatibilité légale et pérennité | revue licences, attribution, dépendances et gouvernance avant réutilisation |
| Génération complexe et compilation lente | benchmark de schémas volumineux, découpage des modules, budget après prototype |

**Questions ouvertes à valider avec les mainteneurs :** nom public du crate et disponibilité ; choix du pilote/runtime et MSRV ; politique de release ; connecteurs P1 dans quel ordre ; soutien aux schémas multi-fichiers ; version Prisma exacte pour tests différentiels ; sélection des fonctionnalités avancées avant toute date de parité ; priorité du mode synchrone et du Studio. Ces choix ne bloquent pas la rédaction, mais doivent être tranchés avant de figer les API concernées.
