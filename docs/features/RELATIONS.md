# Fiche de fonctionnalité — Relations et opérations relationnelles

| Métadonnée | Valeur |
|---|---|
| Identifiant | `REL-001` |
| Produit | Rustorm — ORM Rust et langage de schéma `.rsm` |
| Statut | **Spécification proposée ; implémentation partielle expérimentale, non approuvée** |
| Version du document | 1.1 |
| Date | 31 octobre 2026 |
| Responsables produit / technique / sécurité | À désigner avant démarrage |
| Priorité | Critique : préalable à un ORM relationnel utilisable |
| Périmètre initial | PostgreSQL, fichiers `.rsm`, client Rust, migrations et tests |
| Références | [Cahier des charges](../CAHIER_DES_CHARGES.md), [README : état réel](../../README.md) |

> **Important.** Cette fiche décrit la cible complète. Un sous-ensemble expérimental existe : 1–N et 1–1 à FK scalaire simple, y compris certaines relations nommées ou autoréférentes ; N–N implicite non nommé entre modèles distincts ; et N–N explicite via un modèle d'association avec `@@id` composé. Les migrations initiales, chargements et filtres sont pris en charge ; `connect_through`/`disconnect_through` gèrent les liens N–N implicites. Le planificateur différentiel refuse les relations, clés primaires composées et index ; FK composées et N–N implicites nommées ou autoréférentes restent absentes. Un sous-ensemble limité d’écritures imbriquées (création 1–N/1–1 et création et liaison N–N, connect/disconnect/update/delete N–N ciblés, set 1–N nullable et N–N implicite, update/delete ciblés et disconnect 1–N/1–1 à FK nullable) est disponible ; la majorité du contrat R2 reste à faire, notamment la validation préalable des sélecteurs uniques pour update/delete (disconnect la réalise déjà). Le client est synchrone ; les exemples ci-dessous sont des propositions de forme. Consulter le [README](../../README.md#relations-expérimentales) pour l'API réelle. Aucune compatibilité Prisma n'est affirmée sans tests différentiels.

---

## 1. Résumé exécutif

Permettre à une application Rust de déclarer dans un schéma `.rsm` les associations entre modèles, puis de les créer, lire, filtrer et modifier de façon **typée, cohérente et transactionnelle**. Les contraintes persistées en base doivent protéger l'intégrité même si un autre outil écrit directement dans PostgreSQL.

La fonctionnalité couvre :

- les cardinalités **un-à-un (1–1)**, **un-à-plusieurs (1–N)** et **plusieurs-à-plusieurs (N–N)**, y compris les formes explicite et implicite du N–N dans leurs paliers respectifs ;
- les clés étrangères, références à des clés uniques et actions référentielles ;
- le chargement explicite des données associées avec `include` ;
- les filtres relationnels `some`, `none`, `every`, `is` et `is_not` ;
- les écritures imbriquées, dont `create`, `connect`, `disconnect`, `set`, `update`, `delete`, `upsert` et `connect_or_create` ;
- les migrations, diagnostics, sécurité, observabilité et tests nécessaires à ces usages.

**Valeur attendue :** éviter au développeur de maintenir lui-même les jointures, l'ordre des écritures et les vérifications d'intégrité, sans masquer les coûts ou les comportements destructeurs. La conception vise une ergonomie Rust inspirée de Prisma ; la parité exacte se démontre par version et connecteur, pas par la seule ressemblance de syntaxe.

## 2. Objectifs, indicateurs et non-objectifs

### 2.1 Objectifs mesurables

| ID | Objectif | Mesure de recette |
|---|---|---|
| O-01 | Modéliser une relation correctement | Les schémas valides de chaque palier se valident et se génèrent ; les schémas invalides sont refusés avec diagnostic localisé. |
| O-02 | Assurer l'intégrité en base | PK, unicité, FK et actions déclarées sont présentes et vérifiées sur PostgreSQL réel. |
| O-03 | Éviter les lectures N+1 non bornées | La stratégie de chargement respecte la borne de requêtes définie en §9, vérifiée par instrumentation. |
| O-04 | Éviter les écritures partielles | Toute écriture imbriquée réussit intégralement ou est annulée, sous réserve de l'issue du commit (§10). |
| O-05 | Établir la compatibilité revendiquée | Une matrice différentielle versionnée Prisma / PostgreSQL / fonctionnalité est verte pour chaque cas annoncé compatible. |
| O-06 | Éviter toute perte silencieuse | Une construction ou un changement non pris en charge échoue explicitement ; aucune relation n'est ignorée. |

Aucune cible de latence ou de débit chiffrée n'est fixée sans référence mesurée ; les budgets seront approuvés avant la mise en production (§14).

### 2.2 Hors périmètre de cette fiche

Interface web d'administration ; intégration JavaScript/TypeScript ; logique d'autorisation métier automatique ; compatibilité SQL universelle ; réplication, sharding et relations entre bases ou connecteurs différents ; transactions distribuées ; modes sans clés étrangères tant qu'ils ne sont pas spécifiés ; chargement paresseux implicite ; promesse de compatibilité avec toute version de Prisma. SQLite, MySQL/MariaDB et autres connecteurs font l'objet de matrices distinctes. Les vues, types composites et relations polymorphes nécessitent une spécification séparée.

## 3. Utilisateurs, cas d'usage et périmètre de livraison

### 3.1 Acteurs

- **Développeur applicatif :** définit les modèles, écrit les requêtes et exploite les erreurs typées.
- **Responsable de base / exploitation :** examine les SQL, sauvegardes, verrous, durées et plans de déploiement.
- **Mainteneur Rustorm :** implémente et teste schéma, moteur, connecteur, migrations et génération.
- **Responsable sécurité / données :** valide isolation, permissions, confidentialité et changements destructeurs.

### 3.2 Scénarios métier

| ID | Scénario | Résultat |
|---|---|---|
| US-01 | Associer plusieurs articles à un auteur | Chaque article possède un auteur valide ; un auteur peut ne posséder aucun article. |
| US-02 | Associer au plus un profil à un utilisateur | La base empêche deux profils de référencer le même utilisateur. |
| US-03 | Associer des articles à plusieurs catégories | Les associations ne sont pas dupliquées ; leur suppression ne supprime pas les catégories. |
| US-04 | Lire des auteurs et leurs articles | Le chargement est explicite ; les résultats sans article contiennent une liste vide. |
| US-05 | Trouver les auteurs ayant certains articles | `some`, `none` et `every` appliquent une sémantique définie, y compris pour une liste vide. |
| US-06 | Créer un auteur et ses articles en une opération | Une erreur sur un article annule toutes les écritures de l'opération. |
| US-07 | Déployer un changement de relation | Le SQL et les risques sont visibles avant application ; aucune donnée n'est supprimée silencieusement. |

### 3.3 Paliers et dépendances

| Palier | Contenu livrable | Dépendances et limites |
|---|---|---|
| **R0 — fondation 1–N** | Analyse et validation 1–N, FK PostgreSQL, `include` non filtré borné, migration initiale/additive prise en charge, tests et diagnostics. | Étend le P0 du cahier des charges. Aucun comportement non couvert n'est activé. |
| **R1 — cardinalités et filtres** | 1–1, N–N explicite puis implicite, relations nommées et autoréférentes spécifiées, filtres relationnels. | Clés uniques/composées, métadonnées de contraintes et migration différentielle selon les cas. |
| **R2 — écritures imbriquées** | Opérations de §10, atomicité, concurrence, erreurs et suppressions sécurisées. | Sélecteurs uniques typés, planificateur d'opérations, transactions et tests de courses. |
| **R3 — extensions et parité** | Includes filtrés/imbriqués, pagination par parent, introspection, compatibilité Prisma mesurée et connecteurs additionnels. | Capacités par connecteur, analyse de version Prisma et tests spécifiques. |

Ces paliers découpent la livraison ; **ils ne réduisent pas la cible complète** de la fiche. Aucun palier n'est considéré terminé sans ses tests, sa documentation et les critères de §16 applicables.

## 4. Terminologie et invariants

- **Relation logique :** association entre deux modèles, composée de deux champs relationnels inverses, à distinguer des colonnes scalaires.
- **Côté propriétaire :** côté qui porte `fields`, `references` et la FK dans une relation matérialisée par une colonne.
- **Côté inverse :** champ permettant de parcourir la relation sans posséder la FK.
- **FK :** contrainte en base qui garantit que les valeurs non nulles pointent vers une clé référencée valide.
- **Sélecteur unique :** valeur ou tuple de valeurs correspondant à une contrainte unique définie et utilisable pour identifier au plus une ligne.
- **Liste vide / relation absente :** une liste de relations sans ligne renvoie `[]` ; une relation singulière optionnelle absente renvoie `None`. Ce ne sont pas des erreurs.
- **Relation non chargée :** elle n'est pas disponible dans le résultat et ne déclenche aucune requête cachée ; elle n'est pas assimilée à une relation chargée vide.
- Une relation singulière requise signifie que la FK de son côté propriétaire ne peut être nulle ; elle **n'impose pas** que chaque ligne du côté inverse possède une ligne associée. Un 1–1 requiert également l'unicité de la FK.
- Le nom logique, le nom SQL mappé et l'identité stable de la contrainte ne doivent pas être confondus. Les valeurs utilisateur ne sont jamais interpolées dans le SQL.

## 5. Contrat de schéma `.rsm`

Les exemples suivants sont **cibles**, non acceptés actuellement. Ils supposent les blocs `datasource` et `generator` usuels du cahier des charges. Les actions référentielles sont explicites ici pour éviter de dépendre de valeurs par défaut non encore vérifiées.

### 5.1 Relation 1–N obligatoire

```prisma
model User {
  id    Int    @id @default(autoincrement())
  email String @unique
  posts Post[]
  @@map("users")
}

model Post {
  id       Int    @id @default(autoincrement())
  title    String
  authorId Int    @map("author_id")
  author   User   @relation(fields: [authorId], references: [id], onDelete: Restrict, onUpdate: Cascade)
  @@index([authorId])
  @@map("posts")
}
```

`authorId` est la colonne ; `author` et `posts` sont des champs virtuels. Chaque article a exactement un auteur ; un utilisateur peut avoir zéro ou plusieurs articles. L'index n'est pas créé par une FK PostgreSQL implicitement : il doit être déclaré ou une politique explicite d'indexation doit être documentée. R0 exige la prise en charge du `@@index` utilisé ici ou un diagnostic explicite tant qu'il n'est pas implémenté.

### 5.2 Relation 1–1, côté propriétaire obligatoire

```prisma
model User {
  id      Int      @id @default(autoincrement())
  profile Profile?
}
model Profile {
  id     Int  @id @default(autoincrement())
  userId Int  @unique
  user   User @relation(fields: [userId], references: [id], onDelete: Cascade, onUpdate: Cascade)
}
```

La contrainte `@unique` sur `Profile.userId` rend la relation 1–1 ; `User.profile` reste optionnel, car rien n'impose qu'un profil existe pour chaque utilisateur. Supprimer l'utilisateur peut supprimer son profil **seulement** en raison du `Cascade` explicitement déclaré.

### 5.3 N–N explicite avec attributs sur l'association

```prisma
model Post {
  id         Int            @id @default(autoincrement())
  categories PostCategory[]
}
model Category {
  id    Int            @id @default(autoincrement())
  posts PostCategory[]
}
model PostCategory {
  postId     Int
  categoryId Int
  post       Post     @relation(fields: [postId], references: [id], onDelete: Cascade, onUpdate: Cascade)
  category   Category @relation(fields: [categoryId], references: [id], onDelete: Cascade, onUpdate: Cascade)
  @@id([postId, categoryId])
}
```

Le modèle d'association est adressable et peut porter d'autres champs. La clé composée empêche le doublon ; sa génération et ses sélecteurs sont des prérequis R1. Effacer une ligne d'association ne doit pas effacer les entités reliées.

### 5.4 N–N implicite

```prisma
model Post {
  id         Int        @id @default(autoincrement())
  categories Category[]
}
model Category {
  id    Int    @id @default(autoincrement())
  posts Post[]
}
```

Une table de liaison interne porte deux FK et une contrainte d'unicité sur la paire. Le nom, l'ordre des colonnes et les actions sont **déterministes**, indépendants de l'ordre des définitions dans le fichier, et documentés par version du format. Les noms peuvent différer de Prisma tant que la compatibilité d'introspection/migration n'a pas été démontrée. R1 limite d'abord l'implicite à une clé primaire scalaire unique par modèle ; clés composées ou attributs sur la liaison exigent la forme explicite. Ne jamais détruire/recréer automatiquement la table à cause d'un simple réordonnancement du schéma.

### 5.5 Relations nommées, ambiguës et autoréférentes

Plusieurs relations entre les mêmes modèles doivent être distinguées par un nom explicite, identique sur les deux côtés. Une relation autoréférente distingue ses rôles, y compris pour la table N–N ; la détermination des rôles et l'orientation ne doivent pas changer lors d'un réordonnancement du fichier. Une relation non nommée n'est déduite que si son association inverse est non ambiguë. Tout cas ambigu ou non pris en charge est rejeté, avec les positions des deux déclarations quand disponibles. Le support effectif des cycles d'écriture requiert une stratégie distincte (§10.6).

## 6. Règles de validation et diagnostics

| ID | Règle normative | Résultat si non respectée |
|---|---|---|
| VAL-01 | Chaque modèle et champ référencé existe ; une relation possède un inverse cohérent et une cardinalité compatible. | Diagnostic localisé ; aucune génération. |
| VAL-02 | Une relation avec FK a un seul propriétaire ; `fields` et `references` sont non vides, de même longueur et sans doublon. | Rejet ; ne pas déduire silencieusement le propriétaire. |
| VAL-03 | Chaque champ FK est scalaire, persistant et de type SQL compatible avec le champ référencé, dans le même ordre. | Rejet ; détail des champs incompatibles. |
| VAL-04 | `references` cible une PK ou une contrainte unique réelle correspondant exactement au tuple ; un simple index ne suffit pas. | Rejet avant SQL. |
| VAL-05 | En 1–1, le tuple FK est unique ; la nullabilité du champ relationnel propriétaire est cohérente avec ses scalaires FK. | Rejet ; expliquer la contrainte manquante. |
| VAL-06 | Pour une FK composée optionnelle, la première version exige que **toutes** ses composantes soient nulles ou toutes non nulles ; mélanges de nullabilité et valeurs partiellement nulles sont rejetés/protégés par contrainte appropriée. | Diagnostic ; le SQL doit garantir cet invariant. |
| VAL-07 | `SetNull` n'est accepté que si toutes les colonnes FK concernées peuvent devenir NULL ; toute autre action non prise en charge est refusée. | Diagnostic avec action et colonnes. |
| VAL-08 | Les noms de modèles, champs, relations, tables, colonnes, contraintes et identifiants générés ne produisent pas de collision après mapping ou conversion Rust/SQL. | Rejet ; pas de nom tronqué pouvant entrer en collision. |
| VAL-09 | Les deux extrémités d'une relation nommée se correspondent ; rôles et noms implicites N–N sont non ambigus. | Rejet, y compris pour auto-relations ambiguës. |
| VAL-10 | Une construction relationnelle future ou propre à un connecteur non pris en charge n'est jamais ignorée. | Erreur de capacité, aucune écriture de fichier ou en base. |

La première livraison PostgreSQL prend en charge `Cascade`, `Restrict`, `NoAction` et `SetNull` lorsque leurs conditions sont satisfaites ; `SetDefault`, FK différables, modes sans FK et actions spécifiques non spécifiées sont refusés. `Restrict` et `NoAction` ne doivent pas être présentés comme identiques sans tenir compte du moment de contrôle PostgreSQL. **Décision provisoire :** demander des actions explicites tant que les valeurs par défaut Prisma de la version retenue n'ont pas été reproduites et validées. L'absence de valeur par défaut compatible est une différence documentée.

Les diagnostics publics doivent contenir un code stable, une sévérité, le fichier, ligne/colonne et la plage disponible, une explication et une suggestion. Les codes ci-dessous sont un contrat proposé, à figer avant publication : `RSM_REL_UNKNOWN_MODEL`, `RSM_REL_MISSING_INVERSE`, `RSM_REL_AMBIGUOUS`, `RSM_REL_INVALID_FIELDS`, `RSM_REL_TYPE_MISMATCH`, `RSM_REL_NOT_UNIQUE`, `RSM_REL_NULLABILITY`, `RSM_REL_ACTION_UNSUPPORTED`, `RSM_REL_NAME_COLLISION`. Ne jamais exposer de secret dans le diagnostic.

## 7. Modèle interne et architecture

Le parseur produit un AST avec positions ; la validation transforme cet AST en **IR relationnel validé** consommé par le générateur, le planificateur de requêtes, les migrations et l'introspection. Une relation de l'IR porte au minimum : identifiant stable, modèles source/cible, champs des deux côtés, cardinalités, caractère optionnel, propriétaire, champs FK ordonnés, clé cible, noms logiques et physiques, actions référentielles, mode implicite/explicite, capacités connecteur et positions source. L'IR ne contient ni mot de passe ni URL en clair.

Chaîne proposée :

```text
.rsm → AST/diagnostics → IR validé → génération du client
                                   → plan de requête → SQL paramétré → connecteur
                                   → diff de schéma → SQL de migration révisable
```

Les champs de relation n'apparaissent pas comme colonnes physiques dans un `INSERT` scalaire. Une relation 1–N/1–1 se matérialise par FK/contrainte unique ; une N–N implicite par table de liaison. Un moteur de capacités interdit de générer une requête ou migration dont la sémantique n'est pas prise en charge. Toute écriture imbriquée est représentée par un graphe d'opérations : dépendances, résolution des identifiants, vérifications de cardinalité et transaction explicite.

## 8. Client Rust et résultats typés

### 8.1 Principes

- Les modules générés exposent des descripteurs de relations liés au modèle, des sélecteurs uniques et des builders ; un champ d'un autre modèle doit échouer à la compilation lorsque possible, sinon avant exécution.
- `include` est explicite. Un `Record` scalaire ne contient pas une relation non chargée. Le résultat contenant la relation est un type différent ou un type générique dont les champs sont connus au compilateur.
- Une relation singulière requise chargée renvoie le type cible ; une relation singulière optionnelle renvoie `Option<T>` ; une liste renvoie `Vec<T>`. Si une FK requise est incohérente suite à une base non conforme, renvoyer une erreur d'intégrité, pas `None`.
- Le contrat public distingue zéro résultat, cible introuvable, conflit d'unicité et erreur technique. Les projections partielles ne sont pas décodées comme un `Record` complet.
- Les noms exacts, les génériques et l'API synchrone/asynchrone sont des décisions à figer par des tests de compilation ; ne pas annoncer une API avant son implémentation. La couche actuelle `compile()` / `Connection::fetch` reste compatible tant qu'une évolution versionnée n'est pas décidée.

### 8.2 Exemples de forme cible

```rust
// Pseudocode contractuel, non compilable avec le prototype actuel.
let users: Vec<user::WithPosts> = client.user()
    .find_many()
    .include(user::posts())
    .exec()?;
// Chaque résultat contient le user et Vec<post::Record> ; aucun accès implicite en base.

let posts: Vec<post::WithAuthor> = client.post()
    .find_many()
    .include(post::author())
    .exec()?;
```

Les opérations `find_unique`, les sélecteurs uniques et `client.user()` ci-dessus sont également des capacités cibles non disponibles à ce jour. R0 peut livrer une API cohérente avec le connecteur synchrone existant ; une API asynchrone ne doit pas être simulée par une exécution bloquante cachée.

### 8.3 Composition

Par palier, préciser dans une matrice exécutable les combinaisons `select`, `include`, `omit`, tri, filtres, pagination et includes imbriqués. R0 ne promet que l'include 1–N non filtré ; les formes non prises en charge échouent avant SQL. À R3, la pagination d'une relation enfant est **par parent**, pas une limite globale partagée. Une pagination stable nécessite un ordre déterministe et un tie-breaker unique explicite ou documenté ; sans ordre spécifié, aucun ordre n'est garanti.

## 9. Lecture, performances et cohérence

### 9.1 Algorithme et borne de requêtes

Le choix `JOIN`, agrégation JSON ou chargement par lots appartient au planificateur, sous réserve d'équivalence sémantique, de paramètres bindés et de limites mémoire. Pour le chargement par lots, exécuter une requête racine puis au plus `ceil(P / K)` requêtes par chemin relationnel chargé pour `P` clés parentes distinctes et une taille de lot `K` **positive et bornée**, choisie en fonction du nombre de paramètres autorisés par PostgreSQL, des clés composées et des autres paramètres. `P = 0` implique zéro requête enfant. Les niveaux imbriqués sont bornés de la même manière à chaque chemin ; l'instrumentation compte les requêtes effectivement émises. Aucun accès paresseux par parent ni N+1 non borné n'est autorisé.

La requête racine doit conserver ses filtres, son tri et sa pagination avant l'expansion des enfants ; une jointure ne doit pas dupliquer ni supprimer des parents. Les enfants sont répartis par clé parent même quand celle-ci est composée. Définir une limite configurable de profondeur, de nombre d'éléments retournés et de taille de lot ; en dépassement, erreur explicite, pas de résultat tronqué silencieusement. La validation doit précéder les accès réseau dans la mesure du possible.

### 9.2 Isolation et instantané

Sous `READ COMMITTED`, plusieurs requêtes dans une même transaction peuvent voir des instantanés différents : **ne pas promettre une lecture cohérente unique** dans ce mode. Le contrat doit proposer soit une stratégie mono-instruction, soit une option explicite exécutant les lectures sur la même connexion dans une transaction `REPEATABLE READ` compatible, avec gestion des erreurs et limites documentées. Les tests incluent un écrivain concurrent ; si l'option n'est pas disponible au palier livré, la limite est visible et l'option refusée, jamais simulée.

## 10. Filtres et écritures imbriquées

### 10.1 Filtres relationnels

Pour une relation liste, avec une condition enfant `P` :

| Opérateur | Contrat cible | Relation vide |
|---|---|---|
| `some(P)` | Au moins un enfant satisfait `P`. | Faux |
| `none(P)` | Aucun enfant ne satisfait `P`. | Vrai |
| `every(P)` | Chaque enfant satisfait `P`. | Vrai |

Contrat cible Rustorm pour un prédicat SQL à trois valeurs : un enfant « satisfait `P` » uniquement si `P` vaut `TRUE`. `some(P)` est vrai s'il existe un enfant `TRUE` ; `none(P)` est vrai s'il n'en existe aucun ; `every(P)` n'est vrai que si **tous** les enfants valent `TRUE`. Ainsi, un enfant `UNKNOWN` empêche `every(P)` d'être vrai, mais ne fait pas échouer `none(P)` à lui seul. Une traduction possible de `every` est `NOT EXISTS (... WHERE P IS NOT TRUE)` ; un simple `WHERE NOT P` est insuffisant pour `UNKNOWN`.

Pour une relation singulière, `is(P)` exige une cible existante dont le prédicat vaut `TRUE`. `is_not(P)` exige une cible existante dont le prédicat vaut `FALSE` : une cible absente ou un prédicat `UNKNOWN` ne correspond pas. `is_none` / `is_not_none` testent explicitement l'absence / présence. Ces choix forment une **proposition Rustorm**, pas une affirmation sur Prisma : les comparer à la version retenue avant toute revendication de parité et documenter tout écart. Éviter les jointures qui dupliquent les parents ; `EXISTS`/`NOT EXISTS` sont des stratégies possibles. Combinaisons booléennes et profondeur bornées.

### 10.2 Opérations attendues

| Opération | Intention | Préconditions / effets à vérifier |
|---|---|---|
| `create` | Créer la cible et la lier. | Champs requis et contraintes valides ; aucune écriture partielle. |
| `connect` | Lier une cible existante par clé unique. | Cible existante ; vérifier la cardinalité, les conflits et la politique de réaffectation. |
| `disconnect` | Retirer une liaison sans supprimer la cible. | FK nullable pour une liaison portée par une colonne, ou suppression de la ligne de liaison N–N ; refuser si l'intégrité l'interdit. |
| `set` | Remplacer l'ensemble des liaisons d'une liste. | Résoudre toutes les cibles d'abord ; vérifier détachements possibles, doublons, concurrence et atomicité. |
| `update` | Modifier une cible liée. | Vérifier qu'elle est liée au parent sélectionné ; aucun accès à une cible arbitraire par simple identifiant. |
| `delete` | Supprimer une cible liée. | Vérifier le lien et appliquer les FK/actions réellement déclarées ; les effets en cascade sont documentés. |
| `upsert` | Créer ou modifier selon l'état spécifié. | Sémantique et atomicité définies par cardinalité et connecteur ; vérifier les courses. |
| `connect_or_create` | Connecter une cible unique existante ou la créer. | Contrainte unique effective ; conflit concurrent maîtrisé et erreur/reprise documentée. |
| `create_many`, `update_many`, `delete_many` | Opérations en lot imbriquées. | Filtre, limite et résultat explicites ; pas de suppression totale accidentelle. |

La matrice de §10.3 fixe les comportements cibles des principaux cas sensibles. Ils doivent être comparés à Prisma et consignés dans une matrice de compatibilité exécutable avant livraison R2. Aucun ordre d'exécution ou comportement idempotent supplémentaire ne peut être supposé ; les cas non spécifiés sont refusés. Une mutation liée n'autorise pas implicitement des opérations sur d'autres parents ; les règles d'autorisation applicatives restent à la charge de l'application.

### 10.3 Matrice de comportement cible et garde-fous

Les règles ci-dessous sont le contrat Rustorm proposé pour rendre les cas sensibles testables ; elles peuvent volontairement différer de Prisma et seront comparées avant l'annonce d'une compatibilité. « Enfant » désigne la ligne porteuse de la FK en 1–N, et « cible » la ligne sélectionnée par une clé unique.

| Cas | Règle cible | Effet / erreur attendu |
|---|---|---|
| `connect` : cible absente | Ne pas créer implicitement la cible. | `NotFound` ; aucune modification. |
| `connect` : même lien déjà présent | L'opération est idempotente sur le lien existant. | Succès sans doublon, y compris en N–N sous concurrence. |
| `connect` 1–N : enfant lié à un autre parent | Ne pas réaffecter implicitement. | `RelationConflict` ; aucun ancien lien modifié. Une API de déplacement explicite nécessite sa propre spécification. |
| `connect` 1–1 : l'une des deux extrémités est déjà liée à un tiers | Ne pas détacher automatiquement le tiers. | `RelationConflict` ; intégrité et données inchangées. |
| `connect` N–N : lien inexistant | Créer uniquement la ligne de liaison après vérification des deux extrémités. | Une seule paire persistée ; la contrainte unique arbitre les courses. |
| `disconnect` : lien présent et FK nullable | Détacher en mettant **toutes** les composantes de la FK à NULL. | Entité cible préservée ; cohérence de nullabilité vérifiée. |
| `disconnect` : FK obligatoire ou action incompatible | Ne pas supprimer l'entité pour simuler une déconnexion. | `RequiredRelationViolation` ; aucun changement. |
| `disconnect` N–N | Supprimer uniquement la paire de liaison demandée. | Deux entités préservées ; les autres liens ne changent pas. |
| `disconnect` : lien absent | Refuser une intention qui ne correspond à aucun lien. | `NotFound` ou erreur relationnelle stable définie dans l'API ; aucun changement. |
| `set` d'une liste | Résoudre et valider **toutes** les clés avant d'effectuer le remplacement ; refuser les doublons d'entrée. | Remplacement atomique ; erreur si un détachement ou rattachement est interdit. |
| `set([])` | Exiger un marqueur d'intention explicite `clear`/confirmation distincte d'une liste vide accidentelle. | Détachement de tous les liens si permis, jamais suppression implicite des entités. |
| `update` / `delete` d'une cible non liée | Ne jamais viser une ligne extérieure au parent sélectionné. | Erreur relationnelle ; aucun changement, même si l'identifiant existe ailleurs. |
| `delete` d'une cible liée | Supprimer effectivement la cible, contrairement à `disconnect`. | Les actions référentielles déclarées en base s'appliquent ; documenter les cascades possibles. |
| `connect_or_create` concurrent | Unicité garantie par contrainte en base, transaction unique. | Au plus une cible unique ; un conflit peut être renvoyé pour reprise explicite, jamais de succès mensonger. |
| Plusieurs opérations contradictoires sur le même lien | Ne pas choisir silencieusement un ordre. | Erreur de validation avant toute écriture. |

`set` doit sérialiser ou détecter les courses portant sur la même collection ; l'exact mécanisme PostgreSQL et le traitement des échecs sont à arrêter et tester avant R2. Les conditions de succès et d'échec ci-dessus restent obligatoires indépendamment de la stratégie. Les codes définitifs de la ligne « lien absent » et les capacités d'un éventuel déplacement explicite restent à figer avant publication, pas à déduire silencieusement.

### 10.4 Validation préalable

Un plan doit refuser : sélecteur non unique, type erroné, relation d'un autre modèle, cible impossible, champ obligatoire absent, opérations contradictoires ou dépendances cycliques non prises en charge. Les sélecteurs et valeurs sont paramétrés ; aucune modification n'est effectuée si la validation échoue. Les conflits détectables uniquement en base restent traités dans la transaction.

### 10.5 Transaction et concurrence

Une opération imbriquée constitue une unité transactionnelle sur **une même connexion**. Dans une transaction existante, elle participe à cette transaction et ne la valide pas implicitement ; la gestion d'une erreur et l'état de transaction suivent le connecteur. Hors transaction, le moteur ouvre une transaction, exécute les étapes et valide une seule fois. Un échec avant commit provoque rollback ; une perte de connexion pendant le commit peut laisser une **issue inconnue**, à remonter explicitement sans retry automatique ni affirmation abusive de rollback.

Les FK, contraintes uniques et clauses SQL atomiques constituent l'ultime protection contre les courses. Les stratégies de verrouillage/isolation et le comportement sur deadlock, sérialisation et conflit unique doivent être documentés et testés avec plusieurs connexions. Les reprises éventuelles ne sont possibles qu'avec une politique explicite et des opérations idempotentes ; jamais de retry silencieux d'un callback applicatif à effets externes.

### 10.6 Cycles et contraintes

L'ordre d'insertion est dérivé du graphe de dépendances. Les cycles obligatoires sans stratégie sûre sont rejetés avant modification. Les cycles via FK nullable peuvent être traités par des étapes de création et mise à jour **dans une seule transaction** si le schéma et l'action le permettent ; sinon erreur explicite. Aucune désactivation globale de contraintes n'est autorisée. La prise en charge de contraintes différables est un travail séparé et ne doit pas être présumée.

## 11. Intégrité, migrations et introspection

| ID | Exigence |
|---|---|
| MIG-01 | Générer PK, unique, FK, nullabilité et index déclarés avec noms déterministes ; ajouter d'abord tables/colonnes cibles puis contraintes selon les dépendances. |
| MIG-02 | Rejouer l'historique dans une base fantôme dédiée et comparer le catalogue pris en charge, y compris colonnes FK, clé référencée, unicité, actions, validation et noms pertinents. Les objets non inspectables bloquent le plan ; l'introspection partielle n'est pas appelée exhaustive. |
| MIG-03 | Planifier les ajouts simples seulement après vérification du schéma et du SQL produit ; contrôler les données existantes susceptibles de violer `NOT NULL`, unique ou FK. Une vérification sur base fantôme vide ne prouve pas la validité des données de la cible. |
| MIG-04 | Présenter explicitement les suppressions de relation/colonne/table, changements de cardinalité/actions, risque de cascade et suppression de table N–N. Aucun renommage n'est déduit silencieusement. |
| MIG-05 | Une opération destructive exige revue et confirmation explicite selon la commande ; `deploy` n'invente ni ne modifie le plan. Sauvegarde et stratégie de restauration sont documentées. |
| MIG-06 | La migration est rejouable depuis une base vierge, déterministe, contrôlée par checksum et verrouillée ; un échec bloque la suite et ne réécrit pas l'historique. |
| MIG-07 | `db pull` doit identifier les relations représentables et signaler toutes les contraintes/objets non représentables ; aucun écrasement silencieux d'un `.rsm`. Tester introspection → génération → diff vide sur le sous-ensemble revendiqué. |
| MIG-08 | Le mode existant doit continuer à **refuser** une relation/FK qu'il ne sait pas inspecter. Ne pas contourner la vérification du catalogue pour faire passer un test. |

Distinguer suppression d'une **liaison** et suppression d'une **entité** ; pour une table N–N implicite, sa suppression détruit les liaisons même si les deux tables métier restent intactes. Ajouter une FK à une base déjà renseignée exige un contrôle des orphelins sur la cible et peut bloquer ou verrouiller des tables ; proposer si nécessaire un plan expand/contract revu manuellement. Les contraintes `NOT VALID`, validation différée et migrations en ligne ne sont promises qu'après conception et tests dédiés.

## 12. Sécurité, erreurs et confidentialité

- **SEC-01 :** paramètres bindés, identifiants issus de l'IR et correctement quotés ; aucune construction de SQL à partir de valeurs non fiables.
- **SEC-02 :** aucune URL, clé secrète, valeur de filtre, contenu d'enregistrement ou paramètre de requête dans les logs/erreurs par défaut. La structure `Record` générée peut contenir des données sensibles : documentation explicite de son `Debug`.
- **SEC-03 :** la vérification « la cible est liée à ce parent » ne remplace pas les contrôles d'accès de l'application. Tester les références croisées non autorisées dans les opérations imbriquées.
- **SEC-04 :** profondeur, largeur, taille des lots et nombre de paramètres bornés ; pas de chargement exponentiel ou d'épuisement mémoire silencieux. Valeurs par défaut à fixer après mesures.
- **SEC-05 :** permissions minimales pour lectures, mutations et migrations ; distinguer compte applicatif et compte de migration.
- **SEC-06 :** aucune suppression en cascade inattendue liée à une valeur par défaut implicite non documentée ; l'outil montre les actions connues avant déploiement.

Catégories publiques cibles : `Validation`, `Unsupported`, `NotFound`, `UniqueViolation`, `ForeignKeyViolation`, `RelationConflict`, `RequiredRelationViolation`, `SerializationFailure`, `Deadlock`, `Timeout`, `Connection`, `CommitOutcomeUnknown`, `Decode` et `Internal`. Conserver le code SQLSTATE et une source expurgée si disponible, sans dépendre du texte serveur ; les noms définitifs et correspondances sont à figer et tester. Les messages PostgreSQL et journaux serveur peuvent eux-mêmes contenir des données : configuration et accès à ces journaux relèvent de l'exploitation.

## 13. Observabilité, documentation et exploitation

- Métriques facultatives : nom d'opération, durée, succès/échec, nombre de requêtes, taille des lots, profondeur, nombre de lignes et catégorie d'erreur. Pas de valeurs, identifiants sensibles ni SQL interpolé par défaut ; cardinalité des étiquettes bornée.
- Traces facultatives corrélables par identifiant de requête ; pas de télémétrie réseau implicite.
- Documentation utilisateur : schémas complets, 1–1/1–N/N–N, actions, nullabilité, `include`, filtres, mutations, limites, coûts et exemples réellement exécutés par les tests.
- Guide d'exploitation : privilèges, analyse d'un verrou ou d'une erreur FK, réconciliation après commit incertain, surveillance des tables de liaison, sauvegarde/restauration, procédure expand/contract et limites de rollback.
- Rétrocompatibilité : schémas scalaires existants toujours valides ; fichiers générés régénérables ; migrations appliquées immuables. Les changements d'API ou de noms internes durables nécessitent une politique de version et un chemin de mise à niveau.

## 14. Performance et budgets

Cas de référence minimum : zéro parent, un parent, nombreux parents, zéro enfant, enfants fortement déséquilibrés, plusieurs relations, clés composées, pagination et concurrence. Mesurer sur matériel et dataset documentés : nombre de requêtes, lignes transférées, temps p50/p95/p99, allocations et pic mémoire ; comparer au SQL équivalent et suivre les régressions. Les bornes de §9 sont obligatoires. Les seuils chiffrés de temps et mémoire, ainsi que les valeurs de configuration par défaut, sont **à décider après benchmark**, et doivent être inscrits dans la matrice de recette avant mise en production. Ne pas prétendre à une performance équivalente à Prisma sans mesure reproductible.

## 15. Stratégie de tests

### 15.1 Matrice obligatoire

| Niveau | Cas minimum | Références |
|---|---|---|
| Parseur / validation | 1–1, 1–N, N–N, auto-relation, noms, mappings, erreurs de cardinalité/type/nullable/action, clés composées et collisions. | VAL-01 à 10 |
| Génération / compilation | Include correct, typage des résultats et sélecteurs ; refus à la compilation des champs inexistants, mauvais modèles/types et relations non incluses ; sortie déterministe. | §8 |
| SQL / propriétés | Instantanés de plans, paramètres et identifiants ; `EXISTS`, NULL/UNKNOWN, ensembles vides, pagination par parent ; fuzzing borné et absence de panic. | §9–10 |
| PostgreSQL réel | Tous les paliers livrés sur chaque version officiellement annoncée ; FK, actions, unicité, CRUD et résultats réels. | MIG-01, §10 |
| Transactions / concurrence | Rollback à chaque étape, conflit de FK/unique, deux connexions pour `connect_or_create` et `set`, deadlock/sérialisation et état après erreur ; commit incertain simulé lorsque possible. | §10.5 |
| Migrations | Base vierge et base peuplée, orphelins, données dupliquées, dérive, noms, cycles, shadow DB, échec/reprise, opérations destructrices et réexécution. | MIG-01 à 08 |
| Sécurité / robustesse | Valeurs hostiles, fuite de secrets, chemins croisés, bornes de profondeur/taille, schémas malformés et limitation des privilèges. | SEC-01 à 06 |
| Performance | Cas §14 et assertion de la borne de requêtes, petits et grands jeux. | O-03, §9 |
| Différentiel Prisma | Mêmes fixtures et données, version Prisma fixée, même PostgreSQL, comparaison des résultats, effets et erreurs normalisées ; écarts volontaires documentés. | O-05 |

Les tests réels nécessitent des bases **jetables** : certains créent/suppriment tables et historiques. Ils doivent être isolés entre tests parallèles et ne jamais viser une base de production. Les tests critiques réellement exécutés ne doivent pas être confondus avec les tests simplement présents ou ignorés ; publier la version et la configuration utilisées. Reprendre des tests upstream uniquement après vérification de leur licence et attribution appropriée.

### 15.2 Cas limites obligatoires

Relations vides ; cible absente ; NULL ; prédicats `UNKNOWN` ; valeurs dupliquées ; clé composée partiellement nulle ; réaffectation d'un enfant ; cible déjà liée ; suppression `Restrict`/`Cascade`/`SetNull`/`NoAction` ; auto-relation et cycle ; identifiants SQL quotés et mappés ; migration contenant des orphelins ; perte réseau au commit ; listes importantes dépassant un lot ; incohérence due à une base modifiée hors Rustorm ; concurrence sur une même association. Chaque cas est documenté avec résultat attendu et connecteur.

## 16. Critères d'acceptation et définition de terminé

| ID | Critère vérifiable | Liens |
|---|---|---|
| AC-REL-01 | Les schémas des cardinalités livrées valident, génèrent et créent les contraintes attendues sur une base PostgreSQL vierge. | VAL-01 à 10, MIG-01 |
| AC-REL-02 | Tous les schémas invalides de §6 échouent avec un diagnostic localisé ; aucun fichier ou SQL partiel n'est émis. | VAL-01 à 10 |
| AC-REL-03 | Un `include` retourne le bon type, une liste vide ou `None` selon la cardinalité, sans chargement implicite ni N+1 non borné. | O-03, §8–9 |
| AC-REL-04 | Les filtres relationnels livrés respectent la table de vérité documentée, notamment listes vides et NULL, et ne dupliquent pas les parents. | §10.1 |
| AC-REL-05 | Chaque écriture imbriquée livrée couvre succès, cible absente, conflit, doublon et rollback ; les effets des opérations non spécifiées sont refusés. | §10.2–10.5 |
| AC-REL-06 | Une suppression respecte les contraintes réellement configurées ; `disconnect` n'efface pas l'entité cible. | §10, SEC-06 |
| AC-REL-07 | Les migrations relationnelles livrées sont révisables, rejouables et testées sur base peuplée ; un changement destructif est visible avant application. | MIG-01 à 08 |
| AC-REL-08 | Les valeurs hostiles restent des paramètres ; erreurs, logs et instrumentation ne divulguent pas de secrets ou valeurs par défaut. | SEC-01 à 05 |
| AC-REL-09 | Les tests simultanés vérifient unicité et atomicité ; aucune garantie d'instantané ou de rollback n'est annoncée sans preuve. | §9.2, §10.5 |
| AC-REL-10 | La documentation, la matrice des capacités et les exemples exécutables correspondent à ce qui est réellement activé. | §13, §15 |
| AC-REL-11 | Pour chaque compatibilité Prisma revendiquée, la version, le connecteur, les cas différentiels et les différences restantes sont publiés. | O-05 |

Un palier est terminé quand : code et migrations revus, tests applicables exécutés sur les versions déclarées, documentation et exemples à jour, limites explicitement publiées, règles de sécurité validées, CI verte, et responsable produit/technique ayant accepté les écarts éventuels. Un test ignoré, une capacité non implémentée ou une CI configurée mais non exécutée ne vaut pas validation. La parité complète exige en outre la couverture de **tous** les paliers et de la matrice Prisma retenue, pas uniquement AC-REL-01 à 10 sur PostgreSQL.

## 17. Déploiement, retour arrière et risques

### 17.1 Déploiement progressif

1. Introduire l'IR et les validateurs sans changer le comportement des schémas scalaires ; préserver le rejet des constructions non prises en charge.
2. Activer chaque capacité relationnelle derrière une capacité/version de format explicite, désactivée par défaut tant que le palier n'est pas validé ; publier diagnostics et limites.
3. Tester les migrations sur une copie représentative, mesurer les verrous, vérifier les orphelins et sauvegarder avant déploiement.
4. Déployer d'abord sur environnement jetable puis préproduction, avec supervision des erreurs FK, latences et nombre de requêtes ; n'activer en production qu'après approbation.
5. Publier une matrice des versions/connecteurs testés et les écarts Prisma ; ne pas modifier automatiquement des bases existantes à la génération du client.

### 17.2 Retour arrière

Désactiver l'utilisation de l'API et revenir au client précédent ne supprime pas les contraintes ou données déjà migrées. Le retour du schéma de base requiert une migration inverse **révisée et testée** ou une restauration à partir d'une sauvegarde, avec fenêtre de données et impacts sur les écritures évalués. Ne jamais réécrire une migration appliquée ni promettre une inversion automatique sans perte. Pour un commit incertain, inspecter l'état réel avant toute reprise.

### 17.3 Registre de risques

| Risque | Impact | Réduction / contrôle |
|---|---|---|
| Suppression en cascade mal comprise | Perte de données | Actions explicites, revue SQL, avertissement, tests et sauvegarde. |
| Relations incohérentes dans une base existante | Migration échouée ou corruption apparente | Inspection des orphelins et doublons sur la cible ; plan de remédiation. |
| Explosion N+1 ou volume de données | Latence et saturation | Borne §9, instrumentation, tests de charge et limites. |
| Courses lors d'écritures imbriquées | Conflits, associations inattendues | Contraintes BD, transaction unique, tests multi-connexions. |
| Sémantique NULL différente de Prisma | Résultats incorrects | Tables de vérité et tests différentiels versionnés. |
| Noms implicites instables | Diff destructif inutile | Algorithme stable versionné et tests de réordonnancement. |
| Diff/introspection partiels | Faux sentiment de sécurité | Refus explicite de tout objet non inspecté ; limites publiées. |
| Rupture des schémas ou clients existants | Régression utilisateur | Tests de compatibilité, versionnement et migration documentée. |

## 18. Décisions à prendre avant implémentation ou annonce de compatibilité

| ID | Décision | Responsable | Porte de sortie |
|---|---|---|---|
| DEC-01 | Version exacte de Prisma et révisions de référence ; distinction ORM local / services hébergés. | Produit + technique | Matrice de compatibilité épinglée et jeux différentiels disponibles. |
| DEC-02 | Valeurs par défaut des actions référentielles et écarts de syntaxe `.rsm` / `.prisma`. | Technique + données | Tests différentiels verts ; en attendant, actions explicites. |
| DEC-03 | Validation différentielle de `is_not`, `every`, listes vides et NULL/UNKNOWN et décision sur les écarts au contrat cible. | Technique | Table de vérité et tests réels par connecteur. |
| DEC-04 | Validation différentielle des règles de §10.3, codes définitifs des liens absents et éventuelle API de déplacement explicite. | Produit + technique | Matrice d'opérations et résultats en concurrence approuvés. |
| DEC-05 | Forme définitive des types `include`, projections et API synchrone/asynchrone, compatibilité avec le client actuel. | Technique | Exemples compilables et tests négatifs stabilisés. |
| DEC-06 | Noms physiques/versionnement des tables N–N et stratégie d'introspection/import Prisma. | Technique + données | Algorithme documenté, stable et tests de round-trip. |
| DEC-07 | Limites par défaut, budget de requêtes/mémoire/latence et isolation des lectures multi-requêtes. | Technique + exploitation | Benchmarks reproductibles et configuration documentée. |
| DEC-08 | Politique de sécurité, rôle des comptes et règles d'avertissement/confirmation des migrations. | Sécurité + exploitation | Revue approuvée, guide et tests. |

**Dépendance bloquante générale :** aucune décision ouverte ne doit être masquée par une valeur choisie silencieusement. Si un cas ne dispose pas encore d'un contrat et de tests, le rejeter explicitement et le signaler dans la matrice de capacités.
