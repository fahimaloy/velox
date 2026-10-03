//! What `velox init` writes into `Cargo.toml` must be **resolvable**, not merely
//! well-shaped.
//!
//! The bug this file exists for: `init` pinned every `velox-*` dependency at
//! `rev = <the commit this CLI binary was built from>`, a value baked in by
//! `velox-cli/build.rs`. A CLI built from an unpushed local branch therefore
//! emitted a `Cargo.toml` that no machine could ever resolve, and the user found
//! out from cargo, several steps later, with no hint where the rev came from:
//!
//! ```text
//! error: failed to get `velox-core` as a dependency of package `myapp`
//! ```
//!
//! Every contributor and every local tester hits it, on the first thing they do
//! after `velox init`. So the assertions below check the *resolution
//! precondition* of each pin, not its shape:
//!
//! * a `path` pin must name a directory that really holds that crate's
//!   `Cargo.toml`, with the matching package name;
//! * a `git` pin's rev must be a commit some remote-tracking branch contains —
//!   a commit no remote ref reaches is unresolvable by construction, because
//!   cargo can only check out commits the fetch brought back.
//!
//! A grep for `path =` or `rev =` would pass on a manifest full of pins that
//! resolve to nothing, which is precisely the bug. See `git_remote_refs_resolve`
//! for the one disclosed limit of the git check.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The literal cargo error a manifest whose `velox-*` pins do not resolve
/// produces. Quoted in the failure messages so a reader can match it against
/// what they saw.
const CARGO_UNRESOLVED_DEP: &str = "error: failed to get `velox-core` as a dependency of package";

/// How one `velox-*` dependency in the generated manifest is located.
#[derive(Debug)]
enum Pin {
    /// `path = "…"` — relative to the generated project's directory.
    Path(String),
    /// `git = "…"`, `rev = "…"`.
    Git { url: String, rev: String },
}

impl Pin {
    fn crate_name_of(line: &str) -> Option<&str> {
        let (name, rest) = line.split_once(" = {")?;
        if name.starts_with("velox-") {
            Some(name)
        } else {
            let _ = rest;
            None
        }
    }
}

/// Every `velox-*` pin in a generated `Cargo.toml`, keyed by crate name.
///
/// A text scan rather than a TOML parse, for the same reason
/// `init_scaffold_tests.rs` scans instead of parsing: the failure being guarded
/// is a pin pointing at nothing, which is legible in the source text, and a
/// `toml` dev-dependency would be a second way to be wrong.
fn velox_pins(cargo_toml: &str) -> Vec<(String, Pin)> {
    let mut out = Vec::new();
    for raw in cargo_toml.lines() {
        let line = raw.trim();
        let Some(name) = Pin::crate_name_of(line) else {
            continue;
        };
        let field = |key: &str| -> Option<String> {
            let needle = format!("{key} = \"");
            let start = line.find(&needle)? + needle.len();
            let rest = &line[start..];
            let end = rest.find('"')?;
            Some(rest[..end].to_string())
        };
        let pin = if let Some(p) = field("path") {
            Pin::Path(p)
        } else if let Some(url) = field("git") {
            let rev = field("rev").unwrap_or_default();
            Pin::Git { url, rev }
        } else {
            continue;
        };
        out.push((name.to_string(), pin));
    }
    out
}

/// A project created by really running `velox init`, with the manifest and the
/// stdout it produced.
struct Scaffold {
    project_dir: PathBuf,
    cargo_toml: String,
    stdout: String,
}

impl Scaffold {
    /// Run `velox init myapp` from a scratch directory **outside** the velox
    /// checkout — the position a real user is in when they hit this bug. Inside
    /// the checkout, `init` already found path deps and the git branch below was
    /// never exercised, which is why the defect reached a user at all.
    ///
    /// `VELOX_DEP_SOURCE` / `VELOX_PATH` are cleared so the run depends only on
    /// the detection under test and not on the ambient environment.
    fn init_outside_the_checkout(tag: &str, extra_env: &[(&str, &str)]) -> Self {
        let scratch =
            std::env::temp_dir().join(format!("velox-dep-src-{}-{}", std::process::id(), tag));
        let _ = fs::remove_dir_all(&scratch);
        fs::create_dir_all(&scratch).expect("create scratch dir");
        assert!(
            !Self::velox_checkout()
                .starts_with(fs::canonicalize(&scratch).unwrap_or(scratch.clone())),
            "the scratch dir must live outside the velox checkout or `init` finds \
             path deps by walking up from the cwd and the git pin is never exercised"
        );

        let mut cmd = Command::new(env!("CARGO_BIN_EXE_velox"));
        cmd.arg("init")
            .arg("myapp")
            .current_dir(&scratch)
            .env_remove("VELOX_PATH")
            .env_remove("VELOX_DEP_SOURCE");
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        let out = cmd.output().expect("run velox init");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(
            out.status.success(),
            "velox init failed outside the checkout: {}\n--- stdout ---\n{stdout}",
            String::from_utf8_lossy(&out.stderr)
        );

        let project_dir = scratch.join("myapp");
        let cargo_toml =
            fs::read_to_string(project_dir.join("Cargo.toml")).expect("init wrote a Cargo.toml");
        Self {
            project_dir,
            cargo_toml,
            stdout,
        }
    }

    fn velox_checkout() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("velox-cli has a parent")
            .to_path_buf()
    }
}

impl Drop for Scaffold {
    fn drop(&mut self) {
        if let Some(scratch) = self.project_dir.parent() {
            let _ = fs::remove_dir_all(scratch);
        }
    }
}

/// Whether some remote-tracking branch of the velox checkout contains `rev`.
///
/// This is the precondition for `git = …, rev = …` resolving at all: cargo
/// fetches the remote, then checks out the commit, so a commit no remote ref
/// reaches cannot be produced. Disclosed limit: remote-tracking refs are the
/// snapshot from the last `git fetch`, not a live query of the server, so this
/// can report a commit as unreachable when a *newer* fetch would reveal it.
/// That direction is harmless for the bug being guarded — the failure here is a
/// rev that has provably never left the machine, which no fetch can fix — but
/// it means a pass is "no evidence of unreachability", not "proven reachable".
fn git_remote_refs_resolve(rev: &str) -> Result<bool, String> {
    let checkout = Scaffold::velox_checkout();
    let out = Command::new("git")
        .arg("-C")
        .arg(&checkout)
        .args(["branch", "-r", "--contains", rev])
        .output()
        .map_err(|e| format!("could not run git in {}: {e}", checkout.display()))?;
    if !out.status.success() {
        return Err(format!(
            "git branch -r --contains {rev} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let refs = String::from_utf8_lossy(&out.stdout);
    Ok(refs.lines().any(|l| !l.trim().is_empty()))
}

/// The reproduction. Run from outside the checkout so the git-rev branch of
/// `generate_cargo_toml` is the one under test.
#[test]
fn scaffolded_velox_dependencies_are_resolvable() {
    let s = Scaffold::init_outside_the_checkout("repro", &[]);
    let pins = velox_pins(&s.cargo_toml);
    assert!(
        pins.len() >= 5,
        "expected all five velox-* crates to be pinned, found {} in:\n{}",
        pins.len(),
        s.cargo_toml
    );

    let mut unresolvable = Vec::new();
    for (name, pin) in &pins {
        match pin {
            Pin::Path(rel) => {
                let dir = s.project_dir.join(rel);
                let manifest = dir.join("Cargo.toml");
                if !manifest.is_file() {
                    unresolvable.push(format!(
                        "{name}: path = \"{rel}\" -> {} is not a crate directory \
                         (no Cargo.toml). Cargo would fail with \"{CARGO_UNRESOLVED_DEP}\".",
                        dir.display()
                    ));
                    continue;
                }
                let text = fs::read_to_string(&manifest).unwrap_or_default();
                let declared = text
                    .lines()
                    .find_map(|l| l.trim().strip_prefix("name"))
                    .and_then(|l| l.split('"').nth(1))
                    .unwrap_or("");
                if declared != name {
                    unresolvable.push(format!(
                        "{name}: path = \"{rel}\" -> {} declares package name \
                         \"{declared}\", not \"{name}\".",
                        dir.display()
                    ));
                }
            }
            Pin::Git { url, rev } => match git_remote_refs_resolve(rev) {
                Ok(true) => {}
                Ok(false) => unresolvable.push(format!(
                    "{name}: git = \"{url}\", rev = \"{rev}\" — no remote branch of the \
                     velox repository contains that commit, so nobody can check it out. \
                     Cargo fails with \"{CARGO_UNRESOLVED_DEP}\". That rev is the commit \
                     the velox binary was built from, baked in by build.rs; if it was \
                     never pushed, every `velox init` on this machine is unbuildable."
                )),
                Err(e) => panic!("{e}"),
            },
        }
    }

    assert!(
        unresolvable.is_empty(),
        "`velox init` produced a Cargo.toml whose dependencies cannot be resolved — \
         {} of {} pins point at nothing:\n  {}\n--- generated Cargo.toml ---\n{}\n\
         --- init stdout ---\n{}",
        unresolvable.len(),
        pins.len(),
        unresolvable.join("\n  "),
        s.cargo_toml,
        s.stdout,
    );
}
