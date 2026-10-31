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
const HELP: &str = "Rustorm 0.1.0 (experimental)\n\nUsage:\n  rustorm init\n  rustorm validate [--schema PATH]\n  rustorm generate [--schema PATH] [--output FILE] [--check]\n  rustorm migrate create --name NAME [--schema PATH] [--dir PATH]\n  rustorm migrate plan (--name NAME|--print) [--schema PATH] [--shadow-local-database DB]\n  rustorm migrate deploy|status [--dir PATH] [--local-port PORT --local-user USER --local-database DB]\n  rustorm migrate resolve --name NAME (--applied|--rolled-back) --confirm [connection options]\n  rustorm migrate baseline --name FIRST_MIGRATION --confirm [connection options]\n  rustorm version\n\nPostgreSQL execution and deployment require the postgres feature and libpq.\n";
fn main() {
    if let Err((code, message)) = run() {
        eprintln!("error: {message}");
        std::process::exit(code);
    }
}
fn fail(message: impl Into<String>) -> (i32, String) {
    (1, message.into())
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
    if args[0] == "migrate" {
        return migrate_cli(&args[1..]);
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
    let mut schema_path = PathBuf::from("rustorm/schema.rsm");
    let mut output = None;
    let mut check = false;
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
                    schema_path = PathBuf::from(&args[i]);
                } else if args[0] == "generate" {
                    output = Some(PathBuf::from(&args[i]));
                } else {
                    return Err(fail("--output requires generate"));
                }
            }
            "--check" if args[0] == "generate" => check = true,
            other => return Err(fail(format!("unknown option '{other}'"))),
        }
        i += 1;
    }
    if schema_path.extension().and_then(|s| s.to_str()) != Some("rsm") {
        return Err(fail("schema file must have .rsm extension"));
    }
    let input = fs::read_to_string(&schema_path)
        .map_err(|e| fail(format!("{}: {e}", schema_path.display())))?;
    let parsed =
        schema::parse(&input).map_err(|e| fail(format!("{}:{e}", schema_path.display())))?;
    if args[0] == "validate" {
        println!("Valid: {}", schema_path.display());
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
        .ok_or_else(|| fail("migrate requires create, plan, deploy, status, resolve or baseline"))?
        .as_str();
    if !["create", "plan", "deploy", "status", "resolve", "baseline"].contains(&action) {
        return Err(fail("unknown migrate action"));
    }
    let mut dir = PathBuf::from("rustorm/migrations");
    let mut schema_path = PathBuf::from("rustorm/schema.rsm");
    let mut name: Option<String> = None;
    let mut local_port = None;
    let mut local_user = None;
    let mut local_database = None;
    let mut shadow_local_database = None;
    let mut print_only = false;
    let mut applied = false;
    let mut rolled_back = false;
    let mut confirm = false;
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
                    "--dir" => dir = value.into(),
                    "--schema" => schema_path = value.into(),
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
            _ => return Err(fail(format!("unknown migration option '{flag}'"))),
        }
        i += 1;
    }
    if action == "create" {
        if local_port.is_some()
            || local_user.is_some()
            || local_database.is_some()
            || shadow_local_database.is_some()
            || print_only
            || applied
            || rolled_back
            || confirm
        {
            return Err(fail("invalid option for migrate create"));
        }
        let name = name.ok_or_else(|| fail("migrate create requires --name"))?;
        if schema_path.extension().and_then(|s| s.to_str()) != Some("rsm") {
            return Err(fail("schema file must have .rsm extension"));
        }
        let input = fs::read_to_string(&schema_path)
            .map_err(|e| fail(format!("{}: {e}", schema_path.display())))?;
        let schema =
            schema::parse(&input).map_err(|e| fail(format!("{}:{e}", schema_path.display())))?;
        // The marker is not written; output_path constrains creation to the current project.
        let safe = output_path(&dir.join(".rustorm-path-check"))?;
        let safe_dir = safe
            .parent()
            .ok_or_else(|| fail("invalid migration directory"))?;
        let path = rustorm::migrations::create(safe_dir, &name, &schema)
            .map_err(|e| fail(e.to_string()))?;
        println!(
            "Created {}; review migration.sql before deployment",
            path.display()
        );
        return Ok(());
    }
    if action != "plan" && schema_path != PathBuf::from("rustorm/schema.rsm") {
        return Err(fail("--schema applies only to migrate create or plan"));
    }
    if action != "plan" && (shadow_local_database.is_some() || print_only) {
        return Err(fail("shadow options require migrate plan"));
    }
    if action == "plan" && ((name.is_none() && !print_only) || applied || rolled_back || confirm) {
        return Err(fail(
            "plan requires --name or --print, without resolution flags",
        ));
    }
    if action != "resolve"
        && action != "baseline"
        && action != "plan"
        && (name.is_some() || applied || rolled_back || confirm)
    {
        return Err(fail("resolution options require migrate resolve"));
    }
    if action == "baseline" && (name.is_none() || applied || rolled_back || !confirm) {
        return Err(fail(
            "baseline requires --name and --confirm, without resolution flags",
        ));
    }
    if action == "resolve" && (name.is_none() || applied == rolled_back || !confirm) {
        return Err(fail(
            "resolve requires --name, exactly one of --applied/--rolled-back, and --confirm",
        ));
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
        match action {
            "baseline" => {
                rustorm::migrations::baseline(&mut conn, &dir, name.as_deref().unwrap())
                    .map_err(|e| fail(e.to_string()))?;
                println!("Recorded baseline without executing or verifying migration SQL");
            }
            "plan" => {
                if schema_path.extension().and_then(|s| s.to_str()) != Some("rsm") {
                    return Err(fail("schema file must have .rsm extension"));
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
            dir,
            local_port,
            local_user,
            local_database,
            shadow_local_database,
        );
        Err(fail(
            "database migrations require building rustorm with --features postgres and libpq",
        ))
    }
}
