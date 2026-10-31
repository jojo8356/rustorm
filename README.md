# Rustorm

Rustorm est un projet expérimental d'ORM pour Rust inspiré de Prisma, fondé sur un schéma externe `.rsm`. Le [cahier des charges](docs/CAHIER_DES_CHARGES.md) décrit la cible, pas l'état actuel. La [fiche de fonctionnalité Relations](docs/features/RELATIONS.md) précise les exigences, paliers et critères de recette relationnels ; elle est également prospective.

## État actuel

- Analyse et validation d'un **sous-ensemble** de `.rsm` : une source PostgreSQL, un générateur facultatif, les types scalaires `Int`, `BigInt`, `Float`, `Boolean`, `String`, la nullabilité, `@id`, `@@id` composé, `@unique`, `@map`, `@@map`, `@@index` sur une seule colonne et certains `@default`. Des formes limitées de relations 1–1, 1–N et N–N explicites ou implicites sont prises en charge. Les actions des FK explicites doivent être déclarées. Les constructions inconnues sont rejetées ; voir `examples/schema.rsm`.
- CLI `init`, `validate`, `generate`, `generate --check` et `version`. Génération déterministe de structures et champs Rust typés, sans écraser les fichiers non générés.
- Requêtes PostgreSQL paramétrées `SELECT`, `INSERT`, `UPDATE`, `DELETE`, avec `RETURNING`, filtres d'égalité et d'ordre, groupes `AND`/`OR`/`NOT`, recherche littérale de chaînes, nullabilité, tri et pagination. Les groupes de filtres vides et les mutations sans filtre sont refusés. Les champs requis d'une insertion sont vérifiés à la compilation de la requête, pas par le compilateur Rust.
- Relations expérimentales 1–1, 1–N, N–N explicites et implicites : contraintes dans la migration initiale, descripteurs typés, chargement par lots et filtres relationnels. Connexion/déconnexion typées pour les N–N implicites. Certaines écritures imbriquées sont prises en charge ; les migrations relationnelles différentielles restent absentes.
- Connecteur PostgreSQL **synchrone et expérimental** optionnel via `libpq`, décodage des lignes en structures générées et transactions explicites avec rollback à l'abandon. Pas de pool, d'async ou de reprise automatique. `Connection::connect` impose `sslmode=verify-full` pour TCP ; `connect_local_unencrypted` est réservé au développement local et utilise uniquement `127.0.0.1`, sans TLS et avec un mot de passe fixe non secret (la base de test doit autoriser `trust`).
- Migrations PostgreSQL forward-only : historique et SHA-256, verrou d'exécution, transactions, statut et résolution manuelle des échecs ; planification additive limitée à l'aide d'une base fantôme séparée.
- Tests unitaires, d'intégration de la CLI et de compilation, et tests optionnels sur PostgreSQL réel. Les tests réels ont été exécutés ici sur PostgreSQL 16.15 et 18.6 locaux.

**Limites importantes :** Rustorm n'est pas encore un ORM complet. Il manque notamment les N–N implicites nommées ou autoréférentes, les FK et sélecteurs uniques composés, la plupart des formes d’écritures relationnelles imbriquées, la génération différentielle complète et la détection exhaustive de dérive, l'introspection générale, le formatage `.rsm`, l'import `.prisma`, les agrégats autres que le comptage simple, les connecteurs supplémentaires et une grande partie des filtres. La compatibilité avec l'ensemble de Prisma n'est pas revendiquée. Le connecteur est un prototype non audité et ne doit pas encore être utilisé en production.

## Démarrer

Installer Rust et Cargo, puis depuis ce dépôt :

```sh
cargo test --offline
cargo install --path .
```

Dans un nouveau projet Rust :

```sh
rustorm init             # crée rustorm/schema.rsm, sans écraser
rustorm validate         # ne demande ni DATABASE_URL ni base
rustorm generate         # crée src/generated/rustorm/mod.rs
rustorm generate --check # code 2 si absent ou différent
```

La CLI accepte `--schema chemin/vers/schema.rsm` et, pour `generate`, `--output chemin/vers/fichier.rs`. `generator.output` désigne un répertoire relatif au schéma ; `--output` désigne directement un fichier relatif au répertoire courant. La sortie doit être dans le répertoire courant et ne pas traverser de lien symbolique. Le code généré ne contient pas l'URL de la base. `url = env("DATABASE_URL")` est pour l'instant seulement validé syntaxiquement : l'application choisit explicitement comment établir sa connexion.

Pour l'exécution, installer aussi la bibliothèque cliente et les fichiers de développement **libpq** de PostgreSQL, puis ajouter la feature à la dépendance du projet :

```toml
[dependencies]
rustorm = { path = "../rustorm", features = ["postgres"] }
```

Avec le schéma généré par `init` et une table correspondante déjà créée :

```rust
mod generated { pub mod rustorm; }
use generated::rustorm::user;
use rustorm::postgres::Connection;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // La connexion TCP vérifie le certificat et le nom d'hôte.
    // Configurer la CA de confiance avec les options libpq, par exemple sslrootcert.
    let mut db = Connection::connect(&std::env::var("DATABASE_URL")?)?;
    let create = user::create()
        .set(user::EMAIL, "alice@example.org")
        .set_some(user::NAME, "Alice")
        .returning(user::columns())
        .compile()?;
    let created: Vec<user::Record> = db.fetch(&create)?;

    let posts = db.include_one(post::find_many(), post::author())?;
for row in posts {
    println!("article {} : auteur {:?}", row.child.id, row.parent.map(|p| p.id));
}
let query = user::find_many()
        .where_(user::EMAIL.contains("example.org"))
        .order_by(user::ID.asc())
        .take(20)
        .compile()?;
    let users: Vec<user::Record> = db.fetch(&query)?;
    assert_eq!(created.len(), 1);
    assert!(!users.is_empty());
    Ok(())
}
```

Les appels `execute` renvoient le nombre de lignes affectées ou retournées ; `fetch` décode les lignes et renvoie un vecteur vide si la commande ne retourne rien. `execute_raw` accepte du SQL brut, éventuellement plusieurs commandes : **ne jamais y interpoler des valeurs non fiables**. Les transactions s'utilisent avec `db.begin()`, puis `tx.execute`/`tx.fetch` et `tx.commit()` ou `tx.rollback()` ; l'abandon de la garde déclenche un rollback synchrone. Une erreur du serveur suivie d'un `commit` ne doit pas masquer un rollback. Aucune transaction imbriquée n'est prise en charge.

Les mutations numériques `increment`, `decrement`, `multiply` et `divide` opèrent directement en SQL sur les champs `Int`, `BigInt` et `Float` : elles sont atomiques au niveau de l'instruction, sans lecture puis réécriture côté client. Les variantes `_some` acceptent un champ optionnel et laissent `NULL` inchangé. La division par zéro et les opérandes flottants non finis sont refusés à la compilation de la requête ; les dépassements sont signalés par PostgreSQL. La division entière suit la sémantique PostgreSQL (troncature vers zéro). Les mutations exigent toujours un filtre.

`find_many().select(user::EMAIL).distinct()` sélectionne uniquement le champ indiqué et déduplique les lignes complètes de cette projection. Plusieurs appels à `select` ajoutent des champs ; la requête se lit avec `db.query` et `Row::get`, pas avec le `Record` complet généré, qui exige toutes les colonnes. Pour `DISTINCT`, les champs de tri doivent figurer dans la projection. Ce `DISTINCT` SQL sur toute la projection ne prétend pas reproduire toutes les variantes de `distinct` de Prisma.

Les recherches littérales `contains_insensitive`, `starts_with_insensitive` et `ends_with_insensitive` utilisent `ILIKE`, avec échappement des caractères `%`, `_` et `\\` ; leurs variantes `_some` ignorent les champs NULL selon la logique SQL. Le comportement exact dépend de la collation PostgreSQL et ne constitue pas une promesse de compatibilité Unicode ou Prisma complète.

Le comptage simple utilise `user::count().where_(...).compile()?` et se décode avec `db.fetch::<rustorm::query::CountResult>(&query)?` ; PostgreSQL renvoie une ligne contenant `count: i64`, y compris quand aucun enregistrement ne correspond. Pour compter les valeurs non NULL, utiliser `user::count().field(user::NAME)` ; ajouter `.distinct()` pour ne compter chaque valeur qu'une fois. `.distinct()` exige un champ. Le comptage paginé n'est pas encore disponible.

Pour les champs optionnels, le tri permet de placer explicitement les NULL via `asc_nulls_first`, `asc_nulls_last`, `desc_nulls_first` et `desc_nulls_last`. Sans ces options, PostgreSQL choisit sa position par défaut.

Les champs scalaires proposent aussi `not_equals`, `is_in` et `not_in` ; les champs optionnels exposent `equals_some`, `not_equals_some`, `in_some`, `not_in_some`, les comparaisons d'ordre et les recherches textuelles avec le suffixe `_some`. Les listes vides sont refusées (y compris pour les mutations), ainsi que les valeurs NULL dans les listes ; utiliser `is_null`/`is_not_null` et `Filter::or` pour exprimer explicitement les cas NULL. Comme en SQL, `NOT IN` sur un champ NULL ne sélectionne pas cette ligne. Une requête ne peut pas dépasser 65 535 paramètres PostgreSQL.

Les valeurs sont transmises séparément du SQL et expurgées de la représentation `Debug` des paramètres et des lignes brutes. Les structures `Record` générées contiennent les données en clair et leur `Debug` peut les exposer : ne pas les journaliser sans précaution. Les erreurs renvoyées par Rustorm ne contiennent que le SQLSTATE, pas les détails qui pourraient contenir des secrets. Les journaux du serveur PostgreSQL peuvent toutefois enregistrer des valeurs ou détails sensibles indépendamment de Rustorm. La sémantique des recherches utilise la collation et la logique NULL de PostgreSQL ; leur compatibilité exacte avec Prisma n'a pas encore été démontrée.

## Relations expérimentales

Pour une relation 1–N explicite, le sous-ensemble pris en charge exige une relation inverse en liste, une seule FK scalaire (`Int`, `BigInt`, `String` ou `Boolean`) et des actions `onDelete` et `onUpdate` explicites. Les types et la nullabilité doivent correspondre ; la cible doit être une clé non nulle `@id` ou `@unique`. Les FK composées et `Float` comme clé de relation sont refusées. Les 1–N et 1–1 peuvent être nommées ou autoréférentes selon les règles ci-dessous. Les actions acceptées sont `Cascade`, `Restrict`, `NoAction` et, pour une FK nullable, `SetNull`.

```prisma
model User {
  id    Int    @id
  posts Post[]
  @@map("users")
}
model Post {
  id       Int  @id
  title    String
  authorId Int  @map("author_id")
  author   User @relation(fields: [authorId], references: [id], onDelete: Restrict, onUpdate: Cascade)
  @@index([authorId])
  @@map("posts")
}
```

Après génération et application de la **migration initiale relue**, le client expose des descripteurs de relation sur les deux modèles :

```rust,ignore
let rows = db.include_many(user::find_many().order_by(user::ID.asc()), user::posts())?;
for row in rows {
    println!("{}: {} article(s)", row.parent.id, row.children.len());
}
let query = user::find_many()
    .where_(user::posts().some(post::ID.gte(1)))
    .compile()?;
let users: Vec<user::Record> = db.fetch(&query)?;
```

Les chargements utilisent des lots de 500 clés distinctes au maximum. `include_one` préserve l’ordre des enfants et renvoie `WithOne { child, parent: Option<_> }` : `None` représente une FK nulle, tandis qu’un parent manquant pour une FK non nulle est une erreur (y compris si une écriture concurrente le supprime). Le cas obligatoire retourne également un `Option`. Les comparaisons relationnelles utilisent la collation PostgreSQL, même si les clés textuelles ont des octets différents. L'ordre des enfants n'est pas garanti. `include_many` et `include_one` exigent une sélection complète du bon modèle ; les projections partielles et `include` imbriqués ne sont pas pris en charge. Les lectures utilisent plusieurs requêtes : sans stratégie d'isolation appropriée elles peuvent observer des changements concurrents ; une transaction `READ COMMITTED` ne garantit pas à elle seule un instantané unique. `Transaction::include_many` et `Transaction::include_one` utilisent la transaction existante sans la valider.

Les filtres `some`, `none`, `every` utilisent des sous-requêtes paramétrées. `some` et `none` considèrent qu'un enfant satisfait le prédicat uniquement si celui-ci est vrai ; `every` exige que le prédicat soit vrai pour tous les enfants, et renvoie vrai sur une liste vide. Un prédicat SQL `UNKNOWN` empêche `every` d'être vrai. Ces choix ne sont pas encore comparés systématiquement à Prisma.

### Relation 1–1

```prisma
model User {
  id Int @id
  profile Profile?
}
model Profile {
  id Int @id
  userId Int @unique
  user User @relation(fields: [userId], references: [id], onDelete: Cascade, onUpdate: Cascade)
}
```

La FK propriétaire doit être unique (`@unique` ou `@id`) ; l'inverse sans FK doit être optionnel. Une FK nullable doit correspondre à une relation propriétaire nullable. Le chargement est explicite :

```rust,ignore
let users = db.include_inverse_one(user::find_many(), user::profile())?;
let profiles = db.include_one(profile::find_many(), profile::user())?;
let filtered = user::find_many().where_(user::profile().is(profile::ID.equals(1))).compile()?;
```

`include_inverse_one` renvoie `WithInverseOne { parent, child: Option<_> }` et refuse plusieurs enfants pour un même parent. Les filtres `is` et `is_not` signifient respectivement « une relation satisfait le prédicat » et « aucune relation ne satisfait le prédicat » ; `is_not` est donc vrai si la relation est absente. Cette sémantique n'est pas encore comparée systématiquement à Prisma.

### Relation N–N implicite

```prisma
model Post {
  id Int @id
  categories Category[]
}
model Category {
  id String @id
  posts Post[]
}
```

Les deux côtés doivent être des listes non ambiguës entre modèles distincts, ayant chacun une clé primaire scalaire supportée. La migration initiale crée une table `rustorm_m2m_<32 premiers caractères SHA-256>` : le hash porte sur les noms logiques de modèles triés, séparés par un octet NUL. Les colonnes `a` et `b` référencent respectivement le premier et le second modèle dans cet ordre, indépendamment de l'ordre du fichier ou des `@@map`. Une clé primaire sur `(a,b)`, deux FK `ON DELETE/UPDATE CASCADE` et un index sur `b` protègent et accélèrent l'association. Ces noms et actions sont propres à Rustorm, sans garantie de compatibilité Prisma. Les collisions de noms sont refusées.

```rust,ignore
let posts = db.include_through(post::find_many(), post::categories())?;
let categories = db.include_through(category::find_many(), category::posts())?;
let linked = db.connect_through(post::categories(), 1, "rust")?;
let removed = db.disconnect_through(post::categories(), 1, "rust")?;
let query = post::find_many().where_(post::categories().some(category::ID.equals("rust"))).compile()?;
```

`connect_through` est idempotent (retourne `false` si le lien existe), mais échoue si une extrémité n'existe pas. `disconnect_through` ne supprime que l'association et retourne `false` si elle était absente. Les clés des deux côtés sont typées ; les opérations sont également disponibles sur une transaction. Les filtres `some`, `none`, `every` et leur composition sont pris en charge. Les N–N implicites nommées ou autoréférentes, les inclusions imbriquées et la plupart des opérations imbriquées ne sont pas encore implémentées. Les mêmes réserves de projection, d'ordre et d'isolation s'appliquent aux chargements 1–1 et N–N qu'aux chargements 1–N.

### N–N explicite et clé primaire composée

Un modèle d'association peut porter des attributs et deux relations 1–N ordinaires :

```prisma
model Post {
  id Int @id
  categories PostCategory[]
}
model Category {
  id String @id
  posts PostCategory[]
}
model PostCategory {
  postId Int
  categoryId String
  assignedBy String
  post Post @relation(fields: [postId], references: [id], onDelete: Cascade, onUpdate: Cascade)
  category Category @relation(fields: [categoryId], references: [id], onDelete: Cascade, onUpdate: Cascade)
  @@id([postId, categoryId])
}
```

`@@id` requiert de deux à 32 champs scalaires distincts non nullables et ne peut coexister avec un `@id`. La migration initiale crée la clé composée et les deux FK, en respectant `@map`. Les composants ne sont pas uniques individuellement. La liaison est un modèle à part entière : `post::categories()` charge les `PostCategory`, et `post_category::category()` charge leur catégorie. Les créations et suppressions utilisent les builders scalaires et des filtres explicites sur les deux colonnes ; il n'existe pas encore de sélecteur unique composé ni d'`include` imbriqué automatique. Une FK ne peut pas encore cibler une clé composée.

### Noms et auto-relations 1–N/1–1

Plusieurs relations entre deux modèles doivent porter des noms identiques sur chaque paire, par exemple `authored Post[] @relation("Written")` et `author User @relation("Written", fields: [authorId], references: [id], onDelete: Restrict, onUpdate: Cascade)`. Les auto-relations 1–N ou 1–1 exigent elles aussi un nom explicite sur les deux champs :

```prisma
model Node {
  id Int @id
  parentId Int?
  parent Node? @relation("Tree", fields: [parentId], references: [id], onDelete: SetNull, onUpdate: Cascade)
  children Node[] @relation("Tree")
}
```

Les noms vides, les paires ambiguës ou non appariées sont rejetés. Les relations 1–1 conservent l'exigence d'une FK unique et d'un inverse optionnel. Les N–N implicites nommées ou autoréférentes ne sont pas encore acceptées.

### Écritures imbriquées expérimentales

Pour les relations actuellement prises en charge, un plan est validé avant toute requête. Par exemple, avec les modèles `User` et `Post` ci-dessus :

```rust,ignore
use rustorm::relations::{NestedCreate, NestedChange};

let plan = NestedCreate::many(
    user::create().set(user::ID, 1),
    user::posts(),
    vec![post::create().set(post::ID, 10).set(post::TITLE, "Bonjour")],
)?;
let user_id: i32 = db.nested_create(plan)?;

let change = NestedChange::update(
    user::posts(), user_id, post::ID.equals(10),
    post::update_many().set(post::TITLE, "Mis à jour"),
)?;
db.nested_change(change)?;
```

- `NestedCreate::many` crée le parent et 1 à 1 000 enfants 1–N ; `NestedCreate::one` crée un parent et son enfant 1–1 côté inverse. Le client injecte la FK du parent : la fournir soi-même est une erreur. La valeur retournée est la clé référencée du parent, décodée depuis `RETURNING`, y compris si elle est générée par la base.
- `NestedCreate::connect` crée un parent et le relie à 1 à 1 000 cibles **existantes** d'une relation N–N implicite, à partir de leurs clés typées. Les doublons d'entrée sont rejetés et une cible absente fait échouer toute l'opération. Les contraintes PostgreSQL restent l'arbitre final, notamment lorsque la collation considère deux clés textuelles distinctes comme égales.
- `NestedChange::connect_through` et `disconnect_through` construisent des plans pour une paire N–N existante, exécutables avec `nested_change`. Le premier est idempotent et vérifie effectivement le lien sous verrou ; le second échoue si la paire est absente et ne supprime aucune extrémité. Les FK et la contrainte de la table de jonction doivent être déployées ; une cible absente échoue. Pour créer une cible N–N et la lier à un parent existant, `NestedCreate::connect` peut utiliser le descripteur inverse et l'insert de la cible, par exemple `NestedCreate::connect(tag::create().set(tag::ID, "nouveau"), tag::users(), vec![user_id])`.
- `NestedChange::update_through` modifie une cible N–N existante seulement si elle est liée au parent ; il verrouille la paire jusqu'à la fin de la transaction et interdit la modification de sa clé. **La cible peut être partagée : sa modification est visible depuis tous ses parents.** L'application doit vérifier les autorisations et accepter cet effet avant l'appel ; les autres liens ne sont pas supprimés. Les interblocages et délais de verrouillage sont remontés sans retry automatique.
- `NestedChange::delete_through` supprime une cible N–N seulement si le parent indiqué y est lié et qu’aucun autre parent de **cette relation** ne l’est. La cible est verrouillée contre de nouvelles liaisons ; une cible partagée entraîne `RelationConflict`. L’opération exige l’isolation `READ COMMITTED` et une visibilité complète de la table de jonction : elle refuse une RLS active ou un autre niveau d’isolation. Les autres FK et cascades configurées sur la cible restent applicables ; cela ne remplace pas l’autorisation applicative ni la revue des cascades.
- `NestedSet::through(user::tags(), user_id, vec!["rust".to_owned()])` remplace atomiquement les liaisons d’une collection N–N implicite existante via `db.nested_set(plan)` ; `NestedSet::clear_through(user::tags(), user_id)` exprime explicitement une liste vide. Les cibles doivent exister, les doublons sont refusés (y compris ceux équivalents selon la collation de la base) et les autres parents ne sont pas modifiés. La collection existante et la liste demandée sont limitées à 1 000 liens. L’opération exige `READ COMMITTED`, la visibilité complète des trois tables et deux FK immédiates et validées ; elle refuse triggers/règles ou FK externes visant la table de liaison. Les verrous sérialisent les remplacements ; erreurs de verrouillage et deadlocks sont remontés sans reprise automatique.
- `NestedSet::many` remplace une collection 1–N à FK nullable, par exemple `NestedSet::many(user::posts(), user_id, post::ID, vec![10, 11])` ; `clear_many` confirme explicitement le détachement de tous les enfants. Aucun enfant n’est supprimé et un enfant lié à un autre parent entraîne `RelationConflict`, sans réaffectation. La clé cible doit être scalaire et unique. Les limites de 1 000 lignes, les contrôles de visibilité, d’isolation et de triggers s’appliquent aussi ; les tables héritées et les FK externes référençant la colonne modifiée sont refusées.
- `NestedSetExt::one_owned` et `clear_one_owned` remplacent une liaison 1–1 côté propriétaire (FK unique, au plus une cible) avec les mêmes garde-fous transactionnels ; `one_inverse`/`clear_one_inverse` font de même côté inverse. Exemple : `db.nested_set_ext(NestedSetExt::one_owned(profile::user(), 1, profile::ID, vec![101])?)?`.
- `NestedSetExt::many_required` gère le `set` sur 1–N à FK obligatoire **lorsqu’aucun détachement n’est nécessaire** : si la collection existante est vide ou sous-ensemble de la cible, l’attachement se fait sans détachement ; sinon l’opération retourne `InvalidInput` (détachement requis mais FK obligatoire). `clear_many_required` sur FK obligatoire est toujours refusé. Les vérifications d’isolation, de visibilité et de contrainte unique restent identiques.
- `NestedSetExt::many_composite` et `clear_many_composite` couvrent les clés composées `@@id` : chaque cible est un `Filter` devant contenir une égalité non nulle sur chaque composante de `@@id` (ex. `Filter::and(vec![item::ID.equals(1), item::LANG.equals("fr")])`). La validation a lieu avant I/O ; la détection des doublons collationnels est arbitrée par la base.
- `NestedSetExt::explicit_through` et `clear_explicit_through` étendent `set` au N–N explicite via un modèle d’association (`PostTag` avec `@@id([postId, tagId])`) : `ExplicitThrough::new("posts","id","tags","id","post_tags","post_id","tag_id", &["post_id","tag_id"])`. La table de liaison explicite est verrouillée, les FK validées et les triggers refusés comme pour l’implicite.
- `NestedUpsert::many` (1–N), `one` (1–N propriétaire), `inverse_one` (1–1 inverse) et `through` (N–N implicite) implémentent `upsert` : si un enfant lié correspond au sélecteur unique (scalaire ou composite), il est mis à jour, sinon il est créé avec la FK parent. Le sélecteur est validé avant I/O (`NonUniqueNestedSelector` sinon) ; l’exécution verrouille la table enfant et le parent, exige `READ COMMITTED`, et s’appuie sur la contrainte unique en base pour arbitrer les courses : deux connexions concurrentes sur la même clé unique ne dupliquent jamais la cible. Exemple : `db.nested_upsert(NestedUpsert::many(user::posts(), 1, post::TITLE.equals("nouveau"), post::create().set(post::ID, 99).set(post::TITLE, "nouveau"), post::update_many().set(post::TITLE, "maj"))?)?`.
- `NestedConnectOrCreate::many` (1–N) et `through_implicit`/`through_explicit` (N–N) implémentent `connect_or_create` atomiquement : si la cible unique existe, elle est connectée (FK mise à jour ou lien inséré avec `ON CONFLICT DO NOTHING` + vérification du lien pour détecter un trigger supprimant l’insertion) ; sinon elle est créée puis liée. La contrainte unique garantit qu’au plus une cible existe ; un succès mensonger sous concurrence est impossible. Exemple : `db.nested_connect_or_create(NestedConnectOrCreate::through_implicit(user::tags(), 1, tag::ID.equals("rust"), tag::create().set(tag::ID, "rust"))?)?`.
- `NestedGraph` couvre les graphes d’écritures multi-niveaux : `Create`, `CreateChild`, `ConnectThrough`, `DisconnectThrough`, `Update`, `Delete`, `Upsert`, `ConnectOrCreate`, `SetMany`, `SetThrough`. Les dépendances sont déclarées via `add_dependency(before, after)` ; un tri topologique ordonne les étapes et un DFS détecte les cycles (ex. `user→post` et `post→user` → `EmptyFilterGroup` avant I/O). La validation des contradictions (même paire `connect`+`disconnect` dans le même graphe) a lieu avant I/O. L’exécution se fait dans une seule transaction (ou savepoint) : un échec (ex. `update` ambigu affectant plusieurs lignes) provoque le rollback de toutes les étapes, y compris les créations parentes déjà exécutées. Les cycles via FK nullable peuvent être résolus en deux étapes (INSERT sans FK puis UPDATE) si les dépendances le permettent ; les cycles obligatoires sans stratégie sûre sont rejetés. Exemple multi-niveaux `User → Post → Tag` : `let u = graph.add_create(user::create().set(user::ID, 10))?; let p = graph.add_create_child(RelationRef { parent_table:"users", parent_key:"id", child_table:"posts", child_key:"author_id", nullable:false }, post::create().set(post::ID, 200).set(post::TITLE, "multi"))?; graph.add_dependency(u,p)?; graph.add_connect_through("post_tags","post_id","tag_id", Value::Int(200), Value::String("rust".into()))?; db.nested_graph(graph)?;`.
- `NestedChange::disconnect` (1–N), `disconnect_one` (inverse 1–1) et `disconnect_owner` (côté FK 1–N ou 1–1) détachent exactement une ligne liée en mettant sa FK nullable à NULL, sans la supprimer. Une FK obligatoire est refusée avec `RequiredRelationViolation`; un lien absent ou un filtre ambigu provoque `RelationNotFound` ou `RelationAmbiguous` et un rollback. Pour éviter un succès mensonger, cette déconnexion exige `READ COMMITTED` et refuse tout trigger utilisateur actif ou règle de réécriture sur la table porteuse de la FK ; la table est verrouillée contre les changements DDL pendant l’opération. Le parent est identifié par la clé référencée, et le filtre porte sur la ligne propriétaire de la FK. Il doit contenir une égalité non nulle sur un champ `@id`/`@unique`, ou une conjonction d’égalités non nulles sur toutes les composantes de `@@id`, faute de quoi le plan échoue avec `NonUniqueNestedSelector` avant toute écriture. Les filtres supplémentaires sont autorisés sous `AND` ; `OR` et `NOT` ne prouvent pas l’unicité. Le client généré doit correspondre au schéma effectivement déployé ; une dérive qui retire la contrainte reste détectée par le contrôle du nombre de lignes, avec rollback. Exemple : `db.nested_change(NestedChange::disconnect(user::posts(), user_id, post::ID.equals(10))?)?` pour une FK nullable.
- `NestedChange::update` et `delete` modifient ou suppriment exactement un enfant 1–N lié à la clé parent donnée ; `update_one` et `delete_one` font de même côté inverse 1–1. Le filtre est contrôlé à l'exécution : zéro ou plusieurs lignes entraînent une erreur et un rollback. Une mise à jour ne peut pas modifier la FK de cette relation. Une suppression supprime l'enfant, et les éventuelles cascades déclarées en base s'appliquent. La vérification du lien ne remplace pas les autorisations applicatives.

`Connection::nested_create`, `nested_change`, `nested_set`, `nested_set_ext`, `nested_upsert`, `nested_connect_or_create` et `nested_graph` exigent une connexion inactive, ouvrent une transaction et ne committent qu'une fois. En cas d'erreur de commit autre qu'une transaction explicitement annulée, `CommitOutcomeUnknown` indique qu'il faut vérifier l'état avant toute reprise : aucun retry automatique n'est effectué. Dans une `Transaction`, ces méthodes utilisent un savepoint et ne committent pas la transaction extérieure ; un échec récupérable annule uniquement l'opération imbriquée. Un échec de nettoyage du savepoint invalide la connexion. Après chaque mutation, une vérification `SELECT` du lien ou de la ligne cible est effectuée : si un trigger `BEFORE`/`AFTER` a modifié ou supprimé la ligne (ex. `RETURN NULL` sur INSERT, ou réécriture de la FK après UPDATE), l’opération retourne `RelationNotFound`/`UnexpectedResult` au lieu de promettre un succès mensonger. Les écritures enfants sont séquentielles, non une requête groupée ; les séquences PostgreSQL peuvent avancer même lors d'un rollback et les effets externes de triggers non transactionnels ne sont pas annulables.

**Périmètre désormais complet pour R2 :** toutes les opérations `set` (1–N nullable/obligatoire, 1–1, clés composées, N–N implicite et explicite), `upsert`, `connect_or_create`, les mutations N–N arbitraires et les graphes multi-niveaux sont implémentés, documentés et testés. Les filtres de `update`/`delete` restent contrôlés à l’exécution pour les cas non `disconnect` ; la parité Prisma n’est pas revendiquée sans matrice différentielle.

**Limite importante :** la génération initiale crée les FK et les index simples déclarés, mais `migrate plan` refuse les schémas relationnels, les clés primaires composées ou les index personnalisés : son comparateur ne sait pas encore inspecter ces objets. Aucun changement relationnel ultérieur n'est planifié automatiquement ; une migration manuelle doit être revue. Ne pas utiliser ces capacités expérimentales en production sans audit.

## Migrations PostgreSQL expérimentales

Pour disposer des commandes connectées, installer `libpq` et construire **la CLI** avec sa feature (une installation sans `--features postgres` permet de créer un fichier, mais pas de se connecter) :

```sh
cargo install --path . --features postgres
rustorm migrate create --name initial       # valide schema.rsm et crée le premier SQL
# Examiner rustorm/migrations/<identifiant>_initial/migration.sql avant déploiement
rustorm migrate status                      # code 2 si attente, échec ou divergence d'historique
DATABASE_URL='postgresql://…?sslrootcert=…' rustorm migrate deploy
```

`DATABASE_URL` est lu uniquement pour les commandes connectées et la connexion TCP impose `sslmode=verify-full`. Pour une **base locale jetable** avec authentification `trust` uniquement, les commandes connectées acceptent à la place `--local-port 55432 --local-user rustorm --local-database postgres` ; cette option est non chiffrée et destinée au développement.

Les fichiers sont des répertoires ordonnés contenant chacun `migration.sql`. Le premier `create` génère `CREATE TABLE` pour les types et attributs du sous-ensemble actuellement validé ; les créations manuelles suivantes produisent un **fichier vide à compléter**. Elles ne calculent pas la différence avec le schéma ; utiliser `plan` ci-dessous pour les changements additifs pris en charge. Un fichier vide ne peut pas être déployé. Une fois appliqué, son contenu exact est immuable : toute modification de ses octets fait échouer les déploiements suivants. La suppression ou le réordonnancement d'une migration appliquée est aussi bloqué. Ne pas modifier les fichiers pendant un déploiement.

### Adopter une base existante (baseline)

Pour une base contenant déjà le schéma de la **première** migration, vérifiez indépendamment et soigneusement que son état correspond exactement au SQL de cette migration. Sauvegardez la base et révisez le fichier ; Rustorm ne vérifie pas cette correspondance. Avec le nom complet du répertoire de migration :

```sh
rustorm migrate baseline --name 20260101000000_initial --confirm
```

Cette commande enregistre la première migration et son checksum comme appliqués **sans exécuter son SQL ni modifier les données applicatives**, puis `migrate deploy` peut appliquer les suivantes. Elle exige une migration non vide et un historique Rustorm entièrement vide ; elle ne permet ni de sauter une migration ultérieure ni de résoudre un échec. Une confirmation erronée peut rendre l'historique trompeur et provoquer des erreurs ou des pertes lors des migrations futures. L'accès concurrent par des outils externes n'est pas protégé.

### Planifier les changements additifs

Avec une base fantôme **jetable et dédiée**, d'un nom de base différent de la cible, une cible dont toutes les migrations présentes sont appliquées, la même version majeure PostgreSQL (15 ou plus), ainsi que le même encodage et les mêmes paramètres de locale sur les deux bases (fournisseur libc uniquement pour le moment) :

```sh
# DATABASE_URL et SHADOW_DATABASE_URL sont fournis par l'environnement ;
# les deux connexions TCP exigent sslmode=verify-full et une CA de confiance.
rustorm migrate plan --print              # prévisualiser, sans créer de fichier
rustorm migrate plan --name add_fields    # écrire le plan à relire
rustorm migrate deploy                    # appliquer après revue
```

En développement local uniquement, les options cible `--local-port`, `--local-user`, `--local-database` doivent être accompagnées de `--shadow-local-database` ; les deux connexions locales utilisent le même port/utilisateur sans TLS. La base fantôme doit exister, avoir `public` vide et n'avoir aucun autre schéma utilisateur. Rustorm ne la crée, ne la réinitialise et ne la supprime pas. Il refuse même deux bases de même nom sur des serveurs différents, par prudence.

Le planificateur rejoue les fichiers de migration **de confiance** dans une transaction fantôme, compare le sous-ensemble scalaire pris en charge à la cible, construit le schéma souhaité dans un schéma temporaire, puis teste le SQL proposé avant de revenir en arrière. Il ne planifie que des ajouts de tables ou de colonnes non uniques, nullables ou munies d'un défaut. Les suppressions, renommages, changements de type/défaut/nullabilité/clé et colonnes requises sans défaut nécessitent une migration SQL manuelle. Les vues, clés étrangères, index personnalisés, triggers, fonctions et autres objets non pris en charge provoquent une erreur plutôt qu'une conversion silencieuse. Les scripts peuvent néanmoins avoir des effets externes non transactionnels : ne planifier que des migrations vérifiées.

La comparaison de dérive n'est **pas exhaustive** : elle concerne uniquement le schéma `public` et les propriétés du sous-ensemble scalaire inspecté. Elle ne vérifie ni les données ni tous les paramètres de PostgreSQL. Le verrou protège les outils Rustorm coopérants, pas les changements DDL externes ; ne pas modifier la base ni les fichiers simultanément. Le SQL est à relire avant `deploy`. Après un crash pendant l'écriture, un répertoire de verrou `.migrations.rustorm-write-lock` peut rester ; ne le retirer qu'après avoir confirmé qu'aucun autre rédacteur n'est actif.

`deploy` prend un verrou PostgreSQL et enregistre chaque tentative dans `public._rustorm_migrations`, séparément de `_prisma_migrations`. Chaque script est exécuté dans une transaction PostgreSQL ; l'historique passe à `applied` dans la même transaction. Les commandes de gestion de transaction, `COPY`, `SET`, `DO` et `CALL` sont refusées dans les scripts ; les commandes non transactionnelles telles que `CREATE INDEX CONCURRENTLY` échouent. Les scripts doivent être de confiance et relus : ils peuvent supprimer des données et aucun avertissement automatique de destruction n'est encore fourni. Certains effets externes déclenchés par du SQL ne sont pas annulables par PostgreSQL.

Un échec ou une interruption bloque les déploiements suivants. Examiner l'historique et la base, réparer manuellement si nécessaire, puis enregistrer explicitement la décision :

```sh
rustorm migrate resolve --name <nom-complet-du-repertoire> --rolled-back --confirm
# ou, uniquement après avoir vérifié que le SQL a été appliqué manuellement :
rustorm migrate resolve --name <nom-complet-du-repertoire> --applied --confirm
```

La résolution **n'exécute ni n'annule aucun SQL**. Si une connexion est perdue pendant un commit, vérifier l'état réel avant toute résolution : son résultat peut être incertain. Sauvegarder et tester les migrations sur une base jetable avant déploiement.

Ce moteur ne fournit pas encore `migrate dev` automatique, comparaison complète de schémas, détection exhaustive de dérive, reset ni import d'historique Prisma. Une base existante ne doit pas être marquée compatible sans contrôle manuel ; aucun objectif de parité complète avec Prisma n'est revendiqué.

## Tests

```sh
cargo test --offline
cargo test --offline --features postgres # nécessite libpq ; tests réels ignorés par défaut
```

Pour les tests réels, créer une **base locale jetable** avec authentification `trust`, puis indiquer le port et, si nécessaire, l'utilisateur et le nom de la base. Ces tests créent et suppriment `public._rustorm_migrations` et les tables `rustorm_mig_*`, `rustorm_integration_test`, `rustorm_query_features` et `rustorm_generated_integration` : ne jamais les lancer sur une base contenant des données à conserver.

```sh
RUSTORM_TEST_DATABASE_PORT=55432 \
RUSTORM_TEST_DATABASE_USER=rustorm \
RUSTORM_TEST_DATABASE_NAME=postgres \
cargo test --offline --features postgres --test postgres -- --ignored

# Les tests de migrations suppriment aussi public._rustorm_migrations :
RUSTORM_TEST_DATABASE_PORT=55432 \
RUSTORM_TEST_DATABASE_USER=rustorm \
RUSTORM_TEST_DATABASE_NAME=postgres \
cargo test --offline --features postgres --test migrations_postgres -- --ignored
# Crée et supprime une base jetable ; le rôle de test doit disposer de CREATEDB :
cargo test --offline --features postgres --test relations_postgres -- --ignored
cargo test --offline --features postgres --test cardinalities_postgres -- --ignored
cargo test --offline --features postgres --test composite_postgres -- --ignored
cargo test --offline --features postgres --test named_postgres -- --ignored
cargo test --offline --features postgres --test nested_create_postgres -- --ignored
cargo test --offline --features postgres --test nested_disconnect_postgres -- --ignored

# Requiert aussi le privilège CREATEDB : crée et supprime deux bases temporaires.
RUSTORM_TEST_DATABASE_PORT=55432 \
RUSTORM_TEST_DATABASE_USER=rustorm \
RUSTORM_TEST_DATABASE_NAME=postgres \
cargo test --offline --features postgres --test shadow_postgres -- --ignored
```

Le projet n'a pas de dépendance Cargo externe ; la feature PostgreSQL requiert la bibliothèque système libpq. Les tests différentiels contre Prisma, le TLS avec une vraie CA, les autres versions et plateformes de PostgreSQL et les scénarios réseau difficiles restent à ajouter.
