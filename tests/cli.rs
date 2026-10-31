use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "rustorm-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_rustorm"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn path(&self, p: &str) -> PathBuf {
        self.0.join(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn success(out: &Output) {
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
#[test]
fn init_validate_generate_check_and_reject_overwrite() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    success(&t.run(&["validate"]));
    assert!(!t.run(&["init"]).status.success());
    success(&t.run(&["generate"]));
    let generated = t.path("src/generated/rustorm/mod.rs");
    let content = fs::read_to_string(&generated).unwrap();
    assert!(content.contains("pub mod user"));
    success(&t.run(&["generate", "--check"]));
    success(&t.run(&["generate"]));
    fs::write(&generated, "// handwritten\n").unwrap();
    assert_eq!(t.run(&["generate", "--check"]).status.code(), Some(2));
    assert!(!t.run(&["generate"]).status.success());
    assert_eq!(fs::read_to_string(generated).unwrap(), "// handwritten\n");
}
#[test]
fn check_has_no_side_effects_and_output_cannot_escape_project() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    assert_eq!(
        t.run(&["generate", "--output", "does/not/exist.rs", "--check"])
            .status
            .code(),
        Some(2)
    );
    assert!(!t.path("does").exists());
    assert!(!t
        .run(&["generate", "--output", "../outside.rs"])
        .status
        .success());
    assert!(!t
        .run(&["generate", "--output", "rustorm/schema.rsm"])
        .status
        .success());
}
#[cfg(unix)]
#[test]
fn refuses_symlink_output_and_symlink_parent() {
    use std::os::unix::fs::symlink;
    let t = Temp::new();
    let external = Temp::new();
    success(&t.run(&["init"]));
    symlink(&external.0, t.path("link")).unwrap();
    assert!(!t
        .run(&["generate", "--output", "link/generated.rs"])
        .status
        .success());
    fs::write(external.path("target"), "keep").unwrap();
    symlink(external.path("target"), t.path("generated.rs")).unwrap();
    assert!(!t
        .run(&["generate", "--output", "generated.rs"])
        .status
        .success());
    assert_eq!(fs::read_to_string(external.path("target")).unwrap(), "keep");
}
#[test]
fn generated_client_compiles_and_bad_filter_type_does_not() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    success(&t.run(&["generate"]));
    let crate_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .to_str()
        .unwrap()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    fs::write(t.path("Cargo.toml"), format!("[package]\nname = \"consumer\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\nrustorm = {{ path = \"{crate_path}\" }}\n")).unwrap();
    fs::write(
        t.path("src/main.rs"),
        r#"mod generated { pub mod rustorm; }
fn main() {
 let q = generated::rustorm::user::find_many()
    .where_(generated::rustorm::user::EMAIL.equals("hello"))
    .where_(generated::rustorm::user::NAME.contains_some("A"))
    .where_(generated::rustorm::user::ID.is_in([1, 2]))
    .compile().unwrap();
 assert_eq!(q.params.len(), 4);
 let count = generated::rustorm::user::count().where_(generated::rustorm::user::ID.is_in([1, 2])).compile().unwrap();
 assert_eq!(count.params.len(), 2);
 let insert = generated::rustorm::user::create()
    .set(generated::rustorm::user::EMAIL, "hello")
    .set_some(generated::rustorm::user::NAME, "Name")
    .compile().unwrap();
 assert_eq!(insert.params.len(), 2);
 assert!(generated::rustorm::user::create().compile().is_err());
 let update = generated::rustorm::user::update_many()
    .set(generated::rustorm::user::PUBLISHED, true)
    .where_(generated::rustorm::user::ID.equals(1)).compile().unwrap();
 assert_eq!(update.params.len(), 2);
 let update = generated::rustorm::user::update_many().increment(generated::rustorm::user::ID, 1).where_(generated::rustorm::user::ID.equals(4)).compile().unwrap();
 assert_eq!(update.params.len(), 2);
 let select = generated::rustorm::user::find_many().select(generated::rustorm::user::EMAIL).distinct().where_(generated::rustorm::user::EMAIL.contains_insensitive("A")).compile().unwrap();
 assert_eq!(select.params.len(), 1);
 let delete = generated::rustorm::user::delete_many()
    .where_(generated::rustorm::user::ID.equals(1)).compile().unwrap();
 assert_eq!(delete.params.len(), 1);
}
"#,
    )
    .unwrap();
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let run = |arg: &str| {
        Command::new(&cargo)
            .current_dir(&t.0)
            .args([arg, "--offline", "--quiet"])
            .output()
            .unwrap()
    };
    let out = run("run");
    success(&out);
    fs::write(t.path("src/main.rs"), "mod generated { pub mod rustorm; }\nfn main() { let _ = generated::rustorm::user::EMAIL.equals(123_i32); }\n").unwrap();
    let out = run("check");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("IntoFieldValue"));
    fs::write(t.path("src/main.rs"), "mod generated { pub mod rustorm; }\nfn main() { let _ = generated::rustorm::user::ID.is_in([\"wrong\"]); }\n").unwrap();
    let out = run("check");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("IntoFieldValue"));
    fs::write(t.path("src/main.rs"), "mod generated { pub mod rustorm; }\nfn main() { let _ = generated::rustorm::user::update_many().increment(generated::rustorm::user::EMAIL, 1); }\n").unwrap();
    let out = run("check");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("NumericField"));
    fs::write(t.path("src/main.rs"), "mod generated { pub mod rustorm; }\nfn main() { let _ = generated::rustorm::user::update_many().increment(generated::rustorm::user::ID, 1_i64); }\n").unwrap();
    let out = run("check");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("mismatched types"));
}

#[test]
fn migration_create_is_offline_safe_and_requires_review() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    let initial = t.run(&["migrate", "create", "--name", "initial"]);
    success(&initial);
    let root = t.path("rustorm/migrations");
    let dirs: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(dirs.len(), 1);
    assert!(std::fs::read_to_string(dirs[0].join("migration.sql"))
        .unwrap()
        .contains("CREATE TABLE"));
    assert!(!t
        .run(&["migrate", "create", "--name", "../evil"])
        .status
        .success());
    assert!(!t
        .run(&[
            "migrate",
            "create",
            "--name",
            "escape",
            "--dir",
            "../outside"
        ])
        .status
        .success());
    let next = t.run(&["migrate", "create", "--name", "second"]);
    success(&next);
    let mut dirs: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    dirs.sort();
    assert!(std::fs::read_to_string(dirs[1].join("migration.sql"))
        .unwrap()
        .contains("Add reviewed SQL"));
    assert!(!t
        .run(&["migrate", "baseline", "--name", "x"])
        .status
        .success());
    assert!(!t
        .run(&["migrate", "baseline", "--confirm"])
        .status
        .success());
    assert!(!t
        .run(&[
            "migrate",
            "baseline",
            "--name",
            "x",
            "--confirm",
            "--applied"
        ])
        .status
        .success());
    assert!(!t
        .run(&["migrate", "resolve", "--name", "x", "--applied"])
        .status
        .success());
    assert!(!t
        .run(&[
            "migrate",
            "resolve",
            "--name",
            "x",
            "--applied",
            "--rolled-back",
            "--confirm"
        ])
        .status
        .success());
}
