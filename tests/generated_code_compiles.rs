//! Every shipped example must generate Rust that compiles and passes its own
//! generated tests.
//!
//! This is the check that was missing: the crate's product is emitted Rust, and
//! until now nothing verified the emitted Rust was even syntactically valid. Most
//! of what was fixed in #9, #12 and #13 — invalid guard identifiers, guards
//! emitted as field access, `State::[*]`, an event enum referenced but not
//! emitted, a wildcard arm tripping `unreachable_patterns` — would have been
//! caught here before anyone reported it.
//!
//! Because the generated file carries its own `#[cfg(test)] mod generated_tests`,
//! compiling with `--test` and running the binary also exercises the machine's
//! behaviour, not just its syntax.

use std::path::{Path, PathBuf};
use std::process::Command;

use oxidate_fsm::{generate_rust_code, parse_fsm};

/// Examples that are expected *not* to generate, with the reason.
///
/// An entry here is a known gap, not a passing case. When the feature lands,
/// remove the entry and the example starts being compiled like the others.
const KNOWN_REJECTED: &[(&str, &str)] = &[(
    "form_submission",
    "choice points are not implemented in the generator",
)];

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples")
}

fn fsm_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(examples_dir())
        .expect("examples/ should exist")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "fsm").unwrap_or(false))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no .fsm examples found");
    files
}

fn stem(path: &Path) -> String {
    path.file_stem().unwrap().to_string_lossy().into_owned()
}

/// Compiles `source` as a test binary and runs it. Returns stderr on failure.
fn compile_and_run(name: &str, source: &str) -> Result<String, String> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("generated");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let rs = dir.join(format!("{name}.rs"));
    let bin = dir.join(format!("{name}_test"));
    std::fs::write(&rs, source).map_err(|e| e.to_string())?;

    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let compile = Command::new(&rustc)
        .args(["--test", "--edition", "2021", "--deny", "warnings"])
        .arg("-o")
        .arg(&bin)
        .arg(&rs)
        .output()
        .map_err(|e| format!("could not run {rustc}: {e}"))?;

    if !compile.status.success() {
        return Err(format!(
            "generated code for '{name}' does not compile:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        ));
    }

    let run = Command::new(&bin)
        .output()
        .map_err(|e| format!("could not run the test binary: {e}"))?;

    if !run.status.success() {
        return Err(format!(
            "generated tests for '{name}' failed:\n{}\n{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&run.stdout).into_owned())
}

#[test]
fn every_example_generates_code_that_compiles_and_passes_its_tests() {
    let mut failures: Vec<String> = Vec::new();

    for path in fsm_files() {
        let name = stem(&path);
        let expected_failure = KNOWN_REJECTED.iter().find(|(n, _)| *n == name);

        let source = std::fs::read_to_string(&path).expect("example should be readable");
        let fsms = match parse_fsm(&source) {
            Ok(fsms) => fsms,
            Err(e) => {
                failures.push(format!("'{name}' does not parse: {e}"));
                continue;
            }
        };

        for fsm in &fsms {
            match (generate_rust_code(fsm), expected_failure) {
                // Generates, and is expected to: compile it and run its tests.
                (Ok(code), None) => {
                    if let Err(e) = compile_and_run(&name, &code) {
                        failures.push(e);
                    }
                }
                // Rejected, and known to be: nothing to do.
                (Err(_), Some(_)) => {}
                // Rejected but not expected to be: a regression.
                (Err(errors), None) => failures.push(format!(
                    "'{name}' was rejected by validation:\n  {}",
                    errors.join("\n  ")
                )),
                // Now generates, but is still listed as rejected: the feature
                // landed and KNOWN_REJECTED is stale.
                (Ok(_), Some((_, reason))) => failures.push(format!(
                    "'{name}' now generates, but KNOWN_REJECTED still lists it \
                     ({reason}). Remove the entry so it gets compiled."
                )),
            }
        }
    }

    assert!(failures.is_empty(), "\n\n{}\n", failures.join("\n\n"));
}

#[test]
fn known_rejected_entries_refer_to_real_examples() {
    // Guards against a typo silently disabling the check for an example.
    let names: Vec<String> = fsm_files().iter().map(|p| stem(p)).collect();
    for (name, _) in KNOWN_REJECTED {
        assert!(
            names.iter().any(|n| n == name),
            "KNOWN_REJECTED names '{name}', which is not in examples/"
        );
    }
}
