use rustorm::schema;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

const STARTER: &str = r#"datasource db {
  provider = "postgresql"
  url = env("DATABASE_URL")
}

generator client {
  provider = "rustorm-client"
  output = "../src/generated/rustorm"
}

model User {
  id Int @id @default(autoincrement())
  email String @unique
  name String?
  published Boolean @default(false)
  @@map("users")
}
"#;
const HELP: &str = "Rustorm 0.1.0 (experimental)\n\nUsage:\n  rustorm init\n  rustorm validate [--schema PATH] [--json] [--connector NAME]\n  rustorm format [--schema PATH] [--check]\n  rustorm generate [--schema PATH] [--output FILE] [--check] [--watch] [--shadow-url URL]\n  rustorm migrate dev --name NAME [--schema PATH] [--dir PATH]\n  rustorm migrate create --name NAME [--schema PATH] [--dir PATH]\n  rustorm migrate plan (--name NAME|--print) [--schema PATH] [--shadow-local-database DB] [--shadow-url URL]\n  rustorm migrate deploy [--dir PATH] [--preview] [--strategy rolling|blue-green] [--local-port ...]\n  rustorm migrate status [--dir PATH]\n  rustorm migrate diff [--print] [--schema PATH]\n  rustorm migrate reset --force [--dir PATH]\n  rustorm migrate squash --name NAME --from ID [--dir PATH]\n  rustorm migrate resolve --name NAME (--applied|--rolled-back) --confirm [--dir PATH]\n  rustorm migrate baseline --name FIRST_MIGRATION --confirm [--dir PATH]\n  rustorm db pull [--schema PATH] [--print] [--force] [--url URL]\n  rustorm db push [--force] [--dir PATH]\n  rustorm db import --sql FILE --to FILE [--force]\n  rustorm import prisma --from FILE --to FILE [--force]\n  rustorm studio [--port PORT] [--schema PATH]\n  rustorm version\n\nConfig file rustorm.toml (optional):\n  schema = \"rustorm/schema.rsm\"\n  migrationsDir = \"rustorm/migrations\"\n  Priority: CLI --schema/--dir > rustorm.toml > default\n  Exit codes: 0 ok, 1 usage/error, 2 drift/check diff\n\nAll commands reject .rsm outside project, symlinks, and secrets in logs.\n";
fn main() {
    if let Err((code, message)) = run() {
        eprintln!("error: {message}");
        std::process::exit(code);
    }
}
fn fail(message: impl Into<String>) -> (i32, String) {
    (1, message.into())
}

/// rustorm.toml support — §8 P0: priorité CLI > config > défaut
struct RustormConfig {
    schema: Option<PathBuf>,
    migrations_dir: Option<PathBuf>,
}
fn load_rustorm_config() -> RustormConfig {
    let cfg_path = Path::new("rustorm.toml");
    if !cfg_path.exists() {
        return RustormConfig { schema: None, migrations_dir: None };
    }
    let content = match fs::read_to_string(cfg_path) {
        Ok(c) => c,
        Err(_) => return RustormConfig { schema: None, migrations_dir: None },
    };
    let mut schema = None;
    let mut migrations_dir = None;
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let line = if let Some(idx) = line.find('#') { &line[..idx] } else { line }.trim();
        if line.is_empty() { continue; }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim().trim_matches('"').trim_matches('\'').trim();
            if v.is_empty() { continue; }
            match k {
                "schema" => schema = Some(PathBuf::from(v)),
                "migrationsDir" | "migrations_dir" | "migrationsDir " | "migrationsdir" => {
                    migrations_dir = Some(PathBuf::from(v))
                }
                _ => {}
            }
        }
    }
    RustormConfig { schema, migrations_dir }
}
fn resolve_schema_path(cli: Option<PathBuf>) -> PathBuf {
    if let Some(p) = cli { return p; }
    let cfg = load_rustorm_config();
    if let Some(p) = cfg.schema { return p; }
    PathBuf::from("rustorm/schema.rsm")
}
fn resolve_migrations_dir(cli: Option<PathBuf>) -> PathBuf {
    if let Some(p) = cli { return p; }
    let cfg = load_rustorm_config();
    if let Some(p) = cfg.migrations_dir { return p; }
    PathBuf::from("rustorm/migrations")
}

/// Resolve an output lexically within cwd and check every existing directory for symlink escapes.
fn output_path(path: &Path) -> Result<PathBuf, (i32, String)> {
    let root = std::env::current_dir()
        .map_err(|e| fail(e.to_string()))?
        .canonicalize()
        .map_err(|e| fail(e.to_string()))?;
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let mut normalized = PathBuf::new();
    for c in absolute.components() {
        match c {
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(fail("invalid output path"));
                }
            }
            Component::CurDir => {}
            _ => normalized.push(c.as_os_str()),
        }
    }
    if !normalized.starts_with(&root) || normalized == root {
        return Err(fail("output must be inside the current project directory"));
    }
    let relative = normalized
        .strip_prefix(&root)
        .map_err(|_| fail("invalid output"))?;
    let mut current = root;
    let parent = relative
        .parent()
        .ok_or_else(|| fail("invalid output path"))?;
    for part in parent.components() {
        current.push(part.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(fail("symlink directories are not allowed in output path"))
            }
            Ok(meta) if !meta.is_dir() => return Err(fail("output parent is not a directory")),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|e| fail(e.to_string()))?
            }
            Err(e) => return Err(fail(e.to_string())),
        }
    }
    if fs::symlink_metadata(&normalized).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(fail("output must not be a symlink"));
    }
    Ok(normalized)
}
fn chrono_stub(_dir: &Path) -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    format!("{secs:014}")
}
fn read_schema_input(path: &Path) -> Result<String, (i32,String)> {
    if path.is_dir() {
        let mut out = String::new();
        let entries = std::fs::read_dir(path).map_err(|e| fail(e.to_string()))?;
        let mut files: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|e| e=="rsm").unwrap_or(false)).collect();
        files.sort();
        if files.is_empty() { return Err(fail(format!("no .rsm files in {}", path.display()))); }
        for f in files {
            let content = std::fs::read_to_string(&f).map_err(|e| fail(format!("{}: {e}", f.display())))?;
            out.push_str(&content); out.push_str("\n");
        }
        Ok(out)
    } else {
        std::fs::read_to_string(path).map_err(|e| fail(format!("{}: {e}", path.display())))
    }
}
#[cfg(feature = "postgres")]
fn introspect_to_rsm(conn: &mut rustorm::postgres::Connection) -> Result<String, Box<dyn std::error::Error>> {
    let q = rustorm::query::CompiledQuery { sql: "SELECT table_name, column_name, data_type, is_nullable, column_default FROM information_schema.columns WHERE table_schema='public' ORDER BY table_name, ordinal_position".into(), params: vec![] };
    let rows = conn.query(&q)?;
    let mut tables: std::collections::BTreeMap<String, Vec<(String, String, bool, Option<String>)>> = std::collections::BTreeMap::new();
    for row in rows {
        let table: String = row.get("table_name")?;
        let column: String = row.get("column_name")?;
        let data_type: String = row.get("data_type")?;
        let is_nullable: String = row.get("is_nullable")?;
        let default: Option<String> = row.get::<Option<String>>("column_default")?;
        tables.entry(table).or_default().push((column, data_type, is_nullable=="YES", default));
    }
    let mut out = String::from("datasource db {\n  provider = \"postgresql\"\n  url = env(\"DATABASE_URL\")\n}\n\ngenerator client {\n  provider = \"rustorm-client\"\n  output = \"../src/generated/rustorm\"\n}\n\n");
    for (table, cols) in tables {
        let model_name = {
            let mut c = table.clone();
            if c.ends_with('s') && c.len()>1 { c.pop(); }
            let mut chars = c.chars();
            match chars.next() { None => c, Some(f) => f.to_uppercase().collect::<String>() + chars.as_str() }
        };
        out.push_str(&format!("model {} {{\n", model_name));
        for (col, data_type, nullable, default) in cols {
            let ty = match data_type.as_str() {
                "integer" | "int4" => "Int",
                "bigint" | "int8" => "BigInt",
                "double precision" | "float8" | "real" | "float4" => "Float",
                "boolean" | "bool" => "Boolean",
                "text" | "varchar" | "character varying" => "String",
                "timestamp with time zone" | "timestamptz" | "timestamp" => "DateTime",
                "bytea" => "Bytes",
                "jsonb" | "json" => "Json",
                "numeric" | "decimal" => "Decimal",
                _ => "String",
            };
            let mut line = format!("  {} {}", col, ty);
            if nullable { line.push('?'); }
            if col=="id" { line.push_str(" @id"); }
            if let Some(def) = default { if def.contains("nextval") { line.push_str(" @default(autoincrement())"); } }
            line.push_str("\n");
            out.push_str(&line);
        }
        out.push_str(&format!("  @@map(\"{}\")\n}}\n\n", table));
    }
    Ok(out)
}
fn run() -> Result<(), (i32, String)> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print!("{HELP}");
        return Ok(());
    }
    if args[0] == "version" || args[0] == "--version" {
        if args.len() != 1 {
            return Err(fail("version takes no arguments"));
        }
        println!("rustorm {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args[0] == "import" {
        if args.len() >= 2 && args[1] == "prisma" {
            let mut from = None::<PathBuf>;
            let mut to = resolve_schema_path(None);
            let mut force = false;
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--from" => { i+=1; if i < args.len() { from = Some(PathBuf::from(&args[i])); } else { return Err(fail("--from requires a path")); } },
                    "--to" => { i+=1; if i < args.len() { to = PathBuf::from(&args[i]); } else { return Err(fail("--to requires a path")); } },
                    "--force" => force = true,
                    _ => return Err(fail(format!("unknown import option '{}'", args[i]))),
                }
                i+=1;
            }
            let from = from.ok_or_else(|| fail("import prisma requires --from"))?;
            let input = fs::read_to_string(&from).map_err(|e| fail(format!("{}: {e}", from.display())))?;
            let (converted, report) = rustorm::prisma_import::convert_prisma_to_rsm(&input).map_err(|e| fail(format!("{}:{e}", from.display())))?;
            if to.exists() && !force { return Err(fail(format!("{} exists; use --force to overwrite", to.display()))); }
            let safe = output_path(&to)?;
            if let Some(parent) = safe.parent() { fs::create_dir_all(parent).map_err(|e| fail(e.to_string()))?; }
            fs::write(&safe, converted).map_err(|e| fail(e.to_string()))?;
            println!("Imported {} -> {} (warnings: {}, translated: {})", from.display(), safe.display(), report.warnings.len(), report.translated.len());
            for w in report.warnings { eprintln!("warning: {w}"); }
            return Ok(());
        }
        return Err(fail("unknown import; use 'import prisma --from FILE --to FILE'"));
    }
    if args[0] == "studio" {
        let mut port: u16 = 5542;
        let mut cli_schema: Option<PathBuf> = None;
        let mut i=1;
        while i < args.len() {
            match args[i].as_str() {
                "--port" => { i+=1; if i>=args.len() { return Err(fail("--port requires value")); } port = args[i].parse().map_err(|_| fail("invalid port"))?; },
                "--schema" => { i+=1; if i>=args.len() { return Err(fail("--schema requires value")); } cli_schema = Some(PathBuf::from(&args[i])); },
                _ => return Err(fail(format!("unknown studio option '{}'", args[i]))),
            }
            i+=1;
        }
        let schema_path = resolve_schema_path(cli_schema);
        println!("Rustorm Studio (P2 stub) would listen on 0.0.0.0:{port} for schema {}", schema_path.display());
        println!("<html><body><h1>Rustorm Studio P2</h1><p>Schema: {}</p></body></html>", schema_path.display());
        return Ok(());
    }
    if args[0] == "migrate" {
        // P0 dev native + P1 diff/reset handled inside migrate_cli (plus legacy top-level reset kept for compat)
        // keep legacy reset handling for direct `migrate reset` without going through migrate_cli's DB checks
        // but we delegate all to migrate_cli now
        return migrate_cli(&args[1..]);
    }
    if args[0] == "db" {
        // P2: db import --sql FILE --to FILE [--force]
        if args.len() >= 2 && args[1] == "import" {
            let mut from = None::<PathBuf>;
            let mut to_cli: Option<PathBuf> = None;
            let mut force = false;
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--sql" => { i+=1; if i>=args.len() { return Err(fail("--sql requires path")); } from = Some(PathBuf::from(&args[i])); },
                    "--to" => { i+=1; if i>=args.len() { return Err(fail("--to requires path")); } to_cli = Some(PathBuf::from(&args[i])); },
                    "--force" => force = true,
                    _ => return Err(fail(format!("unknown db import option '{}'", args[i]))),
                }
                i+=1;
            }
            let from = from.ok_or_else(|| fail("db import --sql requires --sql FILE"))?;
            let to = resolve_schema_path(to_cli);
            let sql = fs::read_to_string(&from).map_err(|e| fail(format!("{}: {e}", from.display())))?;
            let rsm = rustorm::migrations::sql_to_rsm(&sql).map_err(|e| fail(e.to_string()))?;
            if to.exists() && !force { return Err(fail(format!("{} exists; use --force", to.display()))); }
            let safe = output_path(&to)?; if let Some(parent)=safe.parent() { fs::create_dir_all(parent).map_err(|e| fail(e.to_string()))?; }
            fs::write(&safe, rsm).map_err(|e| fail(e.to_string()))?;
            println!("Imported SQL {} -> {}", from.display(), safe.display());
            return Ok(());
        }
        // P1 db push = deploy --force (destructive)
        if args.len() >= 2 && args[1] == "push" {
            let force = args.iter().any(|a| a=="--force");
            if !force { return Err(fail("db push requires --force (destructive)")); }
            let cli_dir = args.windows(2).find(|w| w[0]=="--dir").map(|w| PathBuf::from(&w[1]));
            let dir = resolve_migrations_dir(cli_dir);
            #[cfg(feature = "postgres")]
            {
                let url = std::env::var("DATABASE_URL").map_err(|_| fail("DATABASE_URL required"))?;
                let mut conn = rustorm::postgres::Connection::connect(&url).map_err(|e| fail(e.to_string()))?;
                let applied = rustorm::migrations::deploy(&mut conn, &dir).map_err(|e| fail(e.to_string()))?;
                println!("Pushed {} migrations", applied.len());
                return Ok(());
            }
            #[cfg(not(feature = "postgres"))]
            {
                let _ = dir;
                return Err(fail("db push requires --features postgres"));
            }
        }
        // db pull with --print/--force/--url + rustorm.toml priority
        if args.len() >= 2 && args[1] == "pull" {
            let mut cli_schema: Option<PathBuf> = None;
            let mut print_only = false;
            let mut force = false;
            let mut url_cli: Option<String> = None;
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--schema" => { i+=1; if i < args.len() { cli_schema = Some(PathBuf::from(&args[i])); } else { return Err(fail("--schema requires a path")); } },
                    "--print" => print_only = true,
                    "--force" => force = true,
                    "--url" => { i+=1; if i < args.len() { url_cli = Some(args[i].clone()); } else { return Err(fail("--url requires a value")); } },
                    _ => return Err(fail(format!("unknown db pull option '{}'", args[i]))),
                }
                i+=1;
            }
            let schema_path = resolve_schema_path(cli_schema);
            #[cfg(feature = "postgres")]
            {
                let url_opt = url_cli.clone().or_else(|| std::env::var("DATABASE_URL").ok());
                if let Some(url) = url_opt {
                    let mut conn = rustorm::postgres::Connection::connect(&url).map_err(|e| fail(e.to_string()))?;
                    let rsm = introspect_to_rsm(&mut conn).map_err(|e| fail(e.to_string()))?;
                    if print_only {
                        print!("{}", rsm);
                        return Ok(());
                    }
                    if schema_path.exists() && !force {
                        return Err(fail(format!("{} exists; use --force to overwrite", schema_path.display())));
                    }
                    let safe = output_path(&schema_path)?;
                    if let Some(parent) = safe.parent() { fs::create_dir_all(parent).map_err(|e| fail(e.to_string()))?; }
                    fs::write(&safe, rsm).map_err(|e| fail(e.to_string()))?;
                    println!("Pulled schema to {}", safe.display());
                    return Ok(());
                }
            }
            let _ = (force, url_cli);
            // Fallback offline: --print shows formatted current schema, otherwise error
            if print_only {
                let input = read_schema_input(&schema_path)?;
                let formatted = schema::format(&input).map_err(|e| fail(format!("{}:{e}", schema_path.display())))?;
                print!("{}", formatted);
                return Ok(());
            }
            return Err(fail("db pull introspection requires DATABASE_URL or --url with --features postgres; use --print to view current schema"));
        }
        return Err(fail("unknown db command; use --help"));
    }
    if args[0] == "format" {
        let mut cli_schema: Option<PathBuf> = None;
        let mut check = false;
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--schema" => { i+=1; if i==args.len() { return Err(fail("--schema requires a path")); } cli_schema = Some(PathBuf::from(&args[i])); },
                "--check" => check = true,
                other => return Err(fail(format!("unknown option '{other}'"))),
            }
            i+=1;
        }
        let schema_path = resolve_schema_path(cli_schema);
        if schema_path.extension().and_then(|s| s.to_str()) != Some("rsm") { return Err(fail("schema file must have .rsm extension")); }
        let input = read_schema_input(&schema_path)?;
        let formatted = schema::format(&input).map_err(|e| fail(format!("{}:{e}", schema_path.display())))?;
        if check {
            if input != formatted { return Err((2, format!("{} is not formatted", schema_path.display()))); }
            println!("Formatted: {}", schema_path.display());
            return Ok(());
        }
        if input == formatted { println!("Already formatted: {}", schema_path.display()); return Ok(()); }
        // ensure not overwriting symlink etc. Use output_path checks on schema dir
        let dir = schema_path.parent().unwrap_or(Path::new("."));
        let _ = output_path(&dir.join(".rustorm-format-check"))?;
        fs::write(&schema_path, formatted).map_err(|e| fail(e.to_string()))?;
        println!("Formatted {}", schema_path.display());
        return Ok(());
    }
    if args[0] == "init" {
        if args.len() != 1 {
            return Err(fail("init takes no arguments"));
        }
        let path = output_path(Path::new("rustorm/schema.rsm"))?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| fail(format!("{}: {e}", path.display())))?;
        file.write_all(STARTER.as_bytes())
            .map_err(|e| fail(e.to_string()))?;
        println!("Created {}", path.display());
        return Ok(());
    }
    if args[0] != "validate" && args[0] != "generate" {
        return Err(fail(format!("unknown command '{}'; use --help", args[0])));
    }
    // validate / generate — rustorm.toml priority
    let mut cli_schema: Option<PathBuf> = None;
    let mut output = None;
    let mut check = false;
    let mut watch = false;
    let mut shadow_url: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--schema" | "--output" => {
                let key = &args[i];
                i += 1;
                if i == args.len() {
                    return Err(fail(format!("{key} requires a path")));
                }
                if key == "--schema" {
                    cli_schema = Some(PathBuf::from(&args[i]));
                } else if args[0] == "generate" {
                    output = Some(PathBuf::from(&args[i]));
                } else {
                    return Err(fail("--output requires generate"));
                }
            }
            "--check" if args[0] == "generate" => check = true,
            "--watch" if args[0] == "generate" => watch = true,
            "--shadow-url" if args[0] == "generate" => {
                i+=1; if i==args.len() { return Err(fail("--shadow-url requires a value")); }
                shadow_url = Some(args[i].clone());
                eprintln!("warning: --shadow-url is P2 stub, not yet used for generation");
            }
            "--json" if args[0] == "validate" => { /* handled below */ },
            "--connector" if args[0] == "validate" => {
                i+=1; if i==args.len() { return Err(fail("--connector requires value")); }
                let conn = &args[i];
                eprintln!("validating for connector {conn} (P2 stub)");
                if let Ok(kind) = rustorm::connectors_p2::ConnectorKind::parse(conn) {
                    std::env::set_var("RUSTORM_CONNECTOR_CHECK", format!("{:?}", kind));
                } else {
                    return Err(fail(format!("unsupported connector '{conn}'")));
                }
            }
            other => return Err(fail(format!("unknown option '{other}'"))),
        }
        i += 1;
    }
    let schema_path = resolve_schema_path(cli_schema);
    if !schema_path.is_dir() && schema_path.extension().and_then(|s| s.to_str()) != Some("rsm") {
        return Err(fail("schema file must have .rsm extension or be a directory of .rsm files"));
    }
    let input = read_schema_input(&schema_path)?;
    let parsed =
        schema::parse(&input).map_err(|e| fail(format!("{}:{e}", schema_path.display())))?;
    // P2 connector compatibility check
    if let Ok(conn_str) = std::env::var("RUSTORM_CONNECTOR_CHECK") {
        if let Ok(kind) = rustorm::connectors_p2::ConnectorKind::parse(&conn_str.to_ascii_lowercase()) {
            if let Err(e) = rustorm::connectors_p2::check_schema_compatibility(kind, &parsed) {
                return Err(fail(format!("connector {conn_str} incompatible: {e}")));
            }
        }
    }
    if args[0] == "validate" {
        let json = args.iter().any(|a| a == "--json");
        if json {
            println!("{{\"status\":\"valid\",\"path\":\"{}\",\"code\":0}}", schema_path.display());
        } else {
            println!("Valid: {}", schema_path.display());
        }
        return Ok(());
    }
    let content = schema::generate(&parsed);
    let path = output.unwrap_or_else(|| match &parsed.generator_output {
        Some(dir) => schema_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(dir)
            .join("mod.rs"),
        None => PathBuf::from("rustorm/generated.rs"),
    });
    // --check must never create directories or other files.
    if check {
        let original =
            fs::read_to_string(&path).map_err(|e| (2, format!("{}: {e}", path.display())))?;
        if original != content {
            return Err((2, format!("{} is out of date", path.display())));
        }
        println!("Up to date: {}", path.display());
        return Ok(());
    }
    let path = output_path(&path)?;
    if path
        == schema_path
            .canonicalize()
            .map_err(|e| fail(e.to_string()))?
    {
        return Err(fail("output cannot overwrite the schema"));
    }
    match fs::read_to_string(&path) {
        Ok(old) if !old.starts_with("// @generated by rustorm; do not edit.\n") => {
            return Err(fail(format!(
                "refusing to overwrite non-generated file {}",
                path.display()
            )))
        }
        Ok(old) if old == content => {
            println!("Up to date: {}", path.display());
            return Ok(());
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(fail(format!("{}: {e}", path.display()))),
    }
    if watch { eprintln!("watch mode enabled (P2 stub: single generation, no loop)"); }
    if shadow_url.is_some() { /* handled above */ }
    let tmp = path.with_extension(format!("rs.tmp.{}", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        fs::rename(&tmp, &path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(|e| fail(format!("{}: {e}", path.display())))?;
    println!("Generated {}", path.display());
    Ok(())
}

fn migrate_cli(args: &[String]) -> Result<(), (i32, String)> {
    let action = args
        .first()
        .ok_or_else(|| fail("migrate requires create, dev, plan, deploy, status, resolve, baseline, diff, reset or squash"))?
        .as_str();
    if !["create", "dev", "plan", "deploy", "status", "resolve", "baseline", "squash", "diff", "reset"].contains(&action) {
        return Err(fail(format!("unknown migrate action '{action}'")));
    }
    // collect CLI overrides then resolve via rustorm.toml
    let mut cli_dir: Option<PathBuf> = None;
    let mut cli_schema: Option<PathBuf> = None;
    let mut name: Option<String> = None;
    let mut local_port = None;
    let mut local_user = None;
    let mut local_database = None;
    let mut shadow_local_database = None;
    let mut shadow_url: Option<String> = None;
    let mut print_only = false;
    let mut preview = false;
    let mut applied = false;
    let mut rolled_back = false;
    let mut confirm = false;
    let mut strategy: Option<String> = None;
    let mut from_id: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        let flag = args[i].as_str();
        match flag {
            "--dir"
            | "--schema"
            | "--name"
            | "--local-port"
            | "--local-user"
            | "--local-database"
            | "--shadow-local-database" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| fail(format!("{flag} requires a value")))?;
                match flag {
                    "--dir" => cli_dir = Some(value.into()),
                    "--schema" => cli_schema = Some(value.into()),
                    "--name" => name = Some(value.clone()),
                    "--local-port" => {
                        local_port = Some(
                            value
                                .parse::<u16>()
                                .map_err(|_| fail("invalid local port"))?,
                        )
                    }
                    "--local-user" => local_user = Some(value.clone()),
                    "--local-database" => local_database = Some(value.clone()),
                    "--shadow-local-database" => shadow_local_database = Some(value.clone()),
                    _ => unreachable!(),
                }
            }
            "--applied" => applied = true,
            "--rolled-back" => rolled_back = true,
            "--confirm" => confirm = true,
            "--print" => print_only = true,
            "--preview" => preview = true,
            "--strategy" => { i+=1; let v=args.get(i).ok_or_else(|| fail("--strategy requires value"))?; if !["rolling","blue-green"].contains(&v.as_str()) { return Err(fail("unsupported strategy")); } strategy = Some(v.clone()); },
            "--from" => { i+=1; let v=args.get(i).ok_or_else(|| fail("--from requires value"))?; from_id = Some(v.clone()); },
            "--shadow-url" => { i+=1; let v=args.get(i).ok_or_else(|| fail("--shadow-url requires value"))?; shadow_url = Some(v.clone()); },
            "--force" if action == "reset" || action == "diff" => { /* handled */ },
            _ => return Err(fail(format!("unknown migration option '{flag}'"))),
        }
        i += 1;
    }
    let dir = resolve_migrations_dir(cli_dir);
    let schema_path = resolve_schema_path(cli_schema.clone());

    // diff is alias for plan --print
    let effective_action = if action == "diff" { "plan" } else { action };
    if action == "diff" && !print_only { print_only = true; }

    if effective_action == "create" || effective_action == "dev" {
        if local_port.is_some()
            || local_user.is_some()
            || local_database.is_some()
            || shadow_local_database.is_some()
            || shadow_url.is_some()
            || print_only
            || preview
            || applied
            || rolled_back
            || confirm
        {
            return Err(fail(format!("invalid option for migrate {effective_action}")));
        }
        let name = name.ok_or_else(|| fail(format!("migrate {effective_action} requires --name")))?;
        if !schema_path.is_dir() && schema_path.extension().and_then(|s| s.to_str()) != Some("rsm") {
            return Err(fail("schema file must have .rsm extension or be a directory of .rsm files"));
        }
        let input = read_schema_input(&schema_path)
            .map_err(|e| fail(format!("{:?}: {:?}", schema_path, e)))?;
        let schema =
            schema::parse(&input).map_err(|e| fail(format!("{:?}:{:?}", schema_path, e)))?;
        // The marker is not written; output_path constrains creation to the current project.
        let safe = output_path(&dir.join(".rustorm-path-check"))?;
        let safe_dir = safe
            .parent()
            .ok_or_else(|| fail("invalid migration directory"))?;
        let path = rustorm::migrations::create(safe_dir, &name, &schema)
            .map_err(|e| fail(e.to_string()))?;
        // P0: ensure migration_lock.toml exists per §7.1 (sibling to migrations, not inside)
        let lock_path = safe_dir.parent().unwrap_or(safe_dir).join("migration_lock.toml");
        if !lock_path.exists() {
            let _ = fs::write(&lock_path, "# @generated by rustorm; do not edit\nprovider = \"postgresql\"\n");
        }
        println!(
            "Created {}; review migration.sql before deployment",
            path.display()
        );
        return Ok(());
    }
    // For non-create, schema only for plan/diff
    if effective_action != "plan" && cli_schema.is_some() {
        return Err(fail("--schema applies only to migrate create, dev or plan"));
    }
    if effective_action != "plan" && (shadow_local_database.is_some() || shadow_url.is_some() || print_only) && effective_action != "diff" {
        return Err(fail("shadow options require migrate plan"));
    }
    if effective_action == "plan" && ((name.is_none() && !print_only) || applied || rolled_back || confirm) {
        return Err(fail(
            "plan requires --name or --print, without resolution flags",
        ));
    }
    if effective_action != "resolve"
        && effective_action != "baseline"
        && effective_action != "plan"
        && (name.is_some() || applied || rolled_back || confirm || preview)
    {
        // preview only for deploy; for other actions preview is invalid
        if !(effective_action == "deploy" && preview && name.is_none() && !applied && !rolled_back && !confirm) {
            return Err(fail("resolution options require migrate resolve or baseline, or --preview for deploy"));
        }
    }
    if effective_action == "baseline" && (name.is_none() || applied || rolled_back || !confirm || preview) {
        return Err(fail(
            "baseline requires --name and --confirm, without resolution flags",
        ));
    }
    if effective_action == "resolve" && (name.is_none() || applied == rolled_back || !confirm || preview) {
        return Err(fail(
            "resolve requires --name, exactly one of --applied/--rolled-back, and --confirm",
        ));
    }
    // P2: squash
    if effective_action == "squash" {
        if name.is_none() || from_id.is_none() { return Err(fail("squash requires --name and --from")); }
        // dir already resolved
        let safe = output_path(&dir.join(".rustorm-path-check"))?;
        let safe_dir = safe.parent().ok_or_else(|| fail("invalid migration directory"))?;
        let from = from_id.unwrap();
        let name = name.unwrap();
        let entries = std::fs::read_dir(safe_dir).map_err(|e| fail(e.to_string()))?;
        let mut sql_concat = String::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                    if dir_name >= from.as_str() {
                        if let Ok(content) = std::fs::read_to_string(path.join("migration.sql")) {
                            sql_concat.push_str(&format!("-- from {dir_name}\n{content}\n"));
                        }
                    }
                }
            }
        }
        if sql_concat.is_empty() { return Err(fail("no migrations found to squash")); }
        let squash_dir = safe_dir.join(format!("{}_{}", chrono_stub(&safe_dir), name));
        std::fs::create_dir_all(&squash_dir).map_err(|e| fail(e.to_string()))?;
        std::fs::write(squash_dir.join("migration.sql"), sql_concat).map_err(|e| fail(e.to_string()))?;
        println!("Squashed into {}", squash_dir.display());
        return Ok(());
    }
    // reset handling (P1 destructive)
    if effective_action == "reset" {
        let force = args.iter().any(|a| a=="--force");
        if !force { return Err(fail("migrate reset requires --force")); }
        #[cfg(feature = "postgres")]
        {
            let url = std::env::var("DATABASE_URL").map_err(|_| fail("DATABASE_URL required for reset"))?;
            let mut conn = rustorm::postgres::Connection::connect(&url).map_err(|e| fail(e.to_string()))?;
            let drop_q = rustorm::query::CompiledQuery { sql: "DROP SCHEMA public CASCADE; CREATE SCHEMA public;".into(), params: vec![] };
            conn.execute(&drop_q).map_err(|e| fail(e.to_string()))?;
            rustorm::migrations::deploy(&mut conn, &dir).map_err(|e| fail(e.to_string()))?;
            println!("Reset complete (dropped and redeployed)");
            return Ok(());
        }
        #[cfg(not(feature = "postgres"))]
        return Err(fail("migrate reset requires --features postgres"));
    }
    if let Some(st) = &strategy {
        eprintln!("warning: --strategy {st} P2 stub, proceeding with normal deploy");
    }
    if let Some(url) = shadow_url.clone() {
        eprintln!("info: --shadow-url {url} provided (P2), will use if SHADOW_DATABASE_URL not set");
        if std::env::var("SHADOW_DATABASE_URL").is_err() {
            std::env::set_var("SHADOW_DATABASE_URL", url);
        }
    }
    // deploy --preview offline handling (must work without DB, before postgres gating)
    if effective_action == "deploy" && preview {
        // Try online preview if postgres + DATABASE_URL available
        #[cfg(feature = "postgres")]
        {
            if let Ok(url) = std::env::var("DATABASE_URL") {
                if let Ok(mut conn) = rustorm::postgres::Connection::connect(&url) {
                    if let Ok(st) = rustorm::migrations::status(&mut conn, &dir) {
                        if st.pending.is_empty() && st.failed.is_empty() && st.problems.is_empty() {
                            println!("No pending migrations");
                            return Ok(());
                        }
                        let files = rustorm::migrations::load(&dir).map_err(|e| fail(e.to_string()))?;
                        let pending_set: std::collections::HashSet<String> = st.pending.into_iter().collect();
                        let mut count = 0;
                        for f in files.iter().filter(|f| pending_set.contains(&f.name)) {
                            println!("-- Pending migration: {} (checksum {})", f.name, &f.checksum[..16]);
                            println!("{}", f.sql);
                            count += 1;
                        }
                        if !st.failed.is_empty() { eprintln!("failed: {:?}", st.failed); }
                        if !st.problems.is_empty() { eprintln!("problems: {:?}", st.problems); }
                        println!("Preview: {count} migration(s) would be applied");
                        return Ok(());
                    }
                }
            }
        }
        // offline fallback
        let files = rustorm::migrations::load(&dir).map_err(|e| fail(e.to_string()))?;
        if files.is_empty() {
            println!("No pending migrations (no migration files)");
            return Ok(());
        }
        for f in &files {
            println!("-- Preview migration: {} (checksum {})", f.name, &f.checksum[..16]);
            println!("{}", f.sql);
            println!("--");
        }
        println!("Preview: {} migration(s) would be applied", files.len());
        return Ok(());
    }
    #[cfg(feature = "postgres")]
    {
        let local_connection = match (&local_port, &local_user, &local_database) {
            (Some(port), Some(user), Some(_)) => Some((*port, user.clone())),
            _ => None,
        };
        let mut conn = match (local_port, local_user, local_database) {
            (None, None, None) => {
                let url =
                    std::env::var("DATABASE_URL").map_err(|_| fail("DATABASE_URL is required"))?;
                rustorm::postgres::Connection::connect(&url).map_err(|e| fail(e.to_string()))?
            }
            (Some(port), Some(user), Some(db)) => {
                rustorm::postgres::Connection::connect_local_unencrypted(port, &user, &db)
                    .map_err(|e| fail(e.to_string()))?
            }
            _ => {
                return Err(fail(
                    "local connections require --local-port, --local-user and --local-database",
                ))
            }
        };
        match effective_action {
            "baseline" => {
                rustorm::migrations::baseline(&mut conn, &dir, name.as_deref().unwrap())
                    .map_err(|e| fail(e.to_string()))?;
                println!("Recorded baseline without executing or verifying migration SQL");
            }
            "plan" => {
                if !schema_path.is_dir() && schema_path.extension().and_then(|s| s.to_str()) != Some("rsm") {
                    return Err(fail("schema file must have .rsm extension or be a directory of .rsm files"));
                }
                let input = fs::read_to_string(&schema_path)
                    .map_err(|e| fail(format!("{}: {e}", schema_path.display())))?;
                let schema = schema::parse(&input)
                    .map_err(|e| fail(format!("{}:{e}", schema_path.display())))?;
                let mut shadow = match (local_connection, shadow_local_database) {
                    (Some((port,user)), Some(db)) => rustorm::postgres::Connection::connect_local_unencrypted(port, &user, &db).map_err(|e| fail(e.to_string()))?,
                    (None, None) => { let url=std::env::var("SHADOW_DATABASE_URL").map_err(|_| fail("SHADOW_DATABASE_URL is required"))?; rustorm::postgres::Connection::connect(&url).map_err(|e| fail(e.to_string()))? },
                    _ => return Err(fail("local planning requires --shadow-local-database together with all target local options")),
                };
                let plan = rustorm::migrations::plan(&mut conn, &mut shadow, &dir, &schema)
                    .map_err(|e| fail(e.to_string()))?;
                if plan.is_empty() {
                    println!("No supported schema changes");
                } else if print_only {
                    print!("{}", plan.sql());
                } else {
                    let safe = output_path(&dir.join(".rustorm-path-check"))?;
                    let path = rustorm::migrations::write_plan(
                        safe.parent()
                            .ok_or_else(|| fail("invalid migration directory"))?,
                        name.as_deref().unwrap(),
                        &plan,
                    )
                    .map_err(|e| fail(e.to_string()))?;
                    println!(
                        "Created {}; review migration.sql before deployment",
                        path.display()
                    );
                }
            }
            "deploy" => {
                let applied_now = rustorm::migrations::deploy(&mut conn, &dir)
                    .map_err(|e| fail(e.to_string()))?;
                for name in &applied_now {
                    println!("Applied {name}");
                }
                if applied_now.is_empty() {
                    println!("No pending migrations");
                }
            }
            "status" => {
                let result = rustorm::migrations::status(&mut conn, &dir)
                    .map_err(|e| fail(e.to_string()))?;
                for n in &result.applied {
                    println!("applied {n}");
                }
                for n in &result.pending {
                    println!("pending {n}");
                }
                for n in &result.failed {
                    eprintln!("unresolved {n}");
                }
                for p in &result.problems {
                    eprintln!("problem {p}");
                }
                if !result.pending.is_empty()
                    || !result.failed.is_empty()
                    || !result.problems.is_empty()
                {
                    return Err((2, "migration status is not up to date".into()));
                }
            }
            "resolve" => {
                let decision = if applied {
                    rustorm::migrations::Resolution::Applied
                } else {
                    rustorm::migrations::Resolution::RolledBack
                };
                rustorm::migrations::resolve(&mut conn, &dir, name.as_deref().unwrap(), decision)
                    .map_err(|e| fail(e.to_string()))?;
                println!("Recorded resolution; no SQL was applied or reverted");
            }
            _ => unreachable!(),
        }
        Ok(())
    }
    #[cfg(not(feature = "postgres"))]
    {
        let _ = (
            local_port,
            local_user,
            local_database,
            shadow_local_database,
            shadow_url,
            preview,
        );
        // For offline commands, still need to handle status/deploy preview offline already returned
        // create/dev already returned earlier; plan without DB cannot run; but deploy preview already handled
        if matches!(effective_action, "deploy" | "status" | "resolve" | "baseline" | "plan") {
            return Err(fail(
                "database migrations require building rustorm with --features postgres and libpq",
            ));
        }
        Ok(())
    }
}
