# Réutiliser Rustorm dans un autre projet Rust

Ce guide montre comment consommer **rustorm** comme bibliothèque + CLI dans un projet externe, avec une release binaire précompilée.

## 1. Dépendance Cargo (recommandé : git tag)

Dans le `Cargo.toml` du projet consommateur :

```toml
[dependencies]
# Bibliothèque pure (parser, codegen, query builder, migrations)
rustorm = { git = "https://github.com/jojo8356/rustorm", tag = "v0.1.0" }

# Avec connecteur Postgres synchrone (libpq, features postgres)
# rustorm = { git = "https://github.com/jojo8356/rustorm", tag = "v0.1.0", features = ["postgres"] }

# Avec connecteur async sqlx + pool deadpool
# rustorm = { git = "https://github.com/jojo8356/rustorm", tag = "v0.1.0", features = ["sqlx"] }

tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

Alternative crates.io (quand publié) :

```toml
rustorm = { version = "0.1", features = ["postgres"] }
```

## 2. Schéma .rsm

Crée le squelette :

```bash
cargo run -p rustorm -- init          # génère rustorm/schema.rsm
# ou avec la CLI installée :
rustorm init
rustorm validate
rustorm format
```

Exemple `rustorm/schema.rsm` :

```prisma
datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}
generator client {
  provider = "rustorm-client"
  output   = "../src/generated/rustorm"
}
model User {
  id    Int    @id
  email String @unique
  name  String?
  posts Post[]
}
model Post {
  id       Int    @id
  title    String
  authorId Int
  author   User   @relation(fields: [authorId], references: [id], onDelete: Cascade, onUpdate: Cascade)
}
```

## 3. Génération en build.rs (zéro CLI manuelle)

Dans `build.rs` à la racine du projet :

```rust
fn main() {
    rustorm::generate_on_build("rustorm/schema.rsm");
}
```

Et dans `src/main.rs` :

```rust
include!(concat!(env!("OUT_DIR"), "/rustorm_generated.rs"));
use rustorm::query::*;
use rustorm::postgres::Connection;

fn main() -> anyhow::Result<()> {
    let mut db = Connection::connect(&std::env::var("DATABASE_URL")?)?;
    let q = user::find_many().where_(user::EMAIL.contains("example.org")).compile()?;
    let users: Vec<user::Record> = db.fetch(&q)?;
    println!("{} users", users.len());
    Ok(())
}
```

Le fichier généré est placé dans `OUT_DIR/rustorm_generated.rs`, versionné par `cargo:rerun-if-changed`.

## 4. CLI précompilée (release)

Chaque release GitHub fournit des binaires :

- `rustorm-x86_64-unknown-linux-gnu` (Linux)
- `rustorm-x86_64-apple-darwin` (macOS Intel)
- `rustorm-aarch64-apple-darwin` (macOS Apple Silicon)
- `rustorm-x86_64-pc-windows-msvc.exe` (Windows)

Installation :

```bash
# depuis la release GitHub
curl -L https://github.com/jojo8356/rustorm/releases/download/v0.1.0/rustorm-x86_64-unknown-linux-gnu -o rustorm
chmod +x rustorm
./rustorm --help

# ou via cargo
cargo install --git https://github.com/jojo8356/rustorm --tag v0.1.0 rustorm
```

Commandes CLI :

```bash
rustorm validate --schema rustorm/schema.rsm
rustorm format --check
rustorm generate --output src/generated/rustorm/mod.rs
rustorm migrate dev --name initial
rustorm migrate deploy
rustorm db pull --print
rustorm import prisma --input prisma/schema.prisma --output rustorm/schema.rsm
```

## 5. Artefacts de release

- **Crate** : `cargo package` → `target/package/rustorm-0.1.0.crate` (uploadé en asset)
- **Docs** : `cargo doc --no-deps` → `target/doc/` (uploadé en `rustorm-docs-v0.1.0.tar.gz`)
- **Lib compilée** : `cargo build --release` → `librustorm.rlib` + `librustorm.rmeta` dans le workflow `release.yml`

Tous les artefacts sont attachés à https://github.com/jojo8356/rustorm/releases/tag/v0.1.0

## 6. MSRV & compatibilité

- MSRV `1.78` (testée sur `ubuntu/macos/windows` offline, PG 16/18)
- `docs/MATRIX.md` : matrices Postgres / multi-OS / Prisma 5-7 / perf p50/p95/p99
- SemVer `0.1.x` : breaking changes possibles, format `.rsm`/IR versionné avec diagnostic

## 7. Exemple complet minimal

Voir `examples/` et `tests/p2_client_example.rs` pour un projet d'exemple clonable.
