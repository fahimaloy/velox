//! A component tag carrying an event binding, INSIDE a `v-for` over a declared
//! `Vec` field, must generate code that compiles.
//!
//! # Why this file builds the generated module instead of reading it
//!
//! The defect it pins is a type error in emitted code — `Arc::downgrade(&state)`
//! against a `state` that is a plain `script_rs::State`, or against no `state`
//! at all. No string assertion can see either: the offending tokens are exactly
//! the tokens a correct emission produces, and the only difference is which
//! render path surrounds them. Reading the generated `render_with_props` body and
//! asserting `!contains("downgrade")` would pass for a generator that stopped
//! emitting dispatch registrations altogether — a different regression, silenced
//! by the same assertion.
//!
//! So this scaffolds a crate and runs `cargo build` on it. The generated
//! `render_with_props` is the body the parent calls, so rustc type-checks it;
//! a `mismatch` (E0308) or `cannot find value` (E0425) fails the build and fails
//! the test.
//!
//! # Why the loop matters
//!
//! Two conditions have to hold at once, and neither alone reaches the shape:
//!
//! - `v-for` over a `Props` field of `Vec` shape, so the collection is read off
//!   the typed struct and the loop body is emitted by the loop-body path rather
//!   than the plain mode path.
//! - A child component tag with an `@event` or function-prop binding, so there
//!   is a handler to register.
//!
//! The fixture corpus has the second without the first, which is why
//! `set_emit_dispatch` had four definitions and no call sites in a loop body.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use velox_sfc::{
    ComponentResolver, RenderMode, compile_template_to_rs_full_with_mode, parse_sfc,
    to_stub_rs_unwrapped,
};

/// A child with one text prop. It never emits itself; the point is that a tag
/// pointing at it carries a binding, which is what puts a handler into the
/// parent's registration list.
const ROW_CHILD: &str = r#"
<script setup>
pub struct Props {
    pub label: String,
}

pub struct State {
    pub props: Props,
}

impl State {
    pub fn new() -> Self {
        State { props: Props { label: String::new() } }
    }
}
</script>

<template>
<li class="row">{{ label }}</li>
</template>
"#;

/// The same child, with a function-typed prop alongside the text one.
///
/// The function prop is the second way a tag puts a handler into the parent's
/// registration list (`collect_function_prop_bindings` feeds
/// `component_dispatch_handlers` too), so it reaches the same loop-body call
/// site. The declared type is the shape the builder can actually build — see
/// `function_prop_value` — so a failure here is the dispatch registration and
/// not a signature mismatch.
const FN_ROW_CHILD: &str = r#"
<script setup>
pub struct Props {
    pub label: String,
    pub on_confirm: Option<Box<dyn Fn(&str)>>,
}

pub struct State {
    pub props: Props,
}

impl State {
    pub fn new() -> Self {
        State { props: Props { label: String::new(), on_confirm: None } }
    }
}
</script>

<template>
<li class="row">{{ label }}</li>
</template>
"#;

/// The parent: a declared `Vec` prop, a `v-for` over it, and a child tag with a
/// binding inside the loop body.
///
/// The `State` field of the same name is required — every component emits a
/// `render_with_state` body and that body reads the collection off `State`. It
/// is `render_with_props` that is under test, and there the collection comes
/// from `Props`.
const LOOPED_PARENT: &str = r#"
<script setup>
import RowChild from './row_child.vx'
use std::rc::Rc;
use velox_core::signal::Signal;

pub struct Props {
    pub items: Vec<String>,
}

pub struct State {
    pub props: Props,
    pub items: Rc<Signal<Vec<String>>>,
    pub log: Signal<Vec<String>>,
}

impl State {
    pub fn new() -> Self {
        State {
            props: Props { items: Vec::new() },
            items: Rc::new(Signal::new(Vec::new())),
            log: Signal::new(Vec::new()),
        }
    }

    pub fn on_confirm(&self, payload: &str) {
        self.log.update(|mut seen| {
            seen.push(String::from(payload));
            seen
        });
    }
}
</script>

<template>
<ul class="rows">
  <li v-for="item in items">
    <RowChild :label="item" @confirm="on_confirm" />
  </li>
</ul>
</template>
"#;

/// The same shape, with the function-prop spelling instead of `@event`. Both
/// spellings reach `component_dispatch_handlers`, so both reach the loop-body
/// call site; a fix that only closed the `@event` path would leave this one
/// broken.
const LOOPED_FN_PROP_PARENT: &str = r#"
<script setup>
import FnRowChild from './fn_row_child.vx'
use std::rc::Rc;
use velox_core::signal::Signal;

pub struct Props {
    pub items: Vec<String>,
}

pub struct State {
    pub props: Props,
    pub items: Rc<Signal<Vec<String>>>,
}

impl State {
    pub fn new() -> Self {
        State {
            props: Props { items: Vec::new() },
            items: Rc::new(Signal::new(Vec::new())),
        }
    }

    pub fn on_confirm(&self, _payload: &str) {}
}
</script>

<template>
<ul class="rows">
  <li v-for="item in items">
    <FnRowChild :label="item" :on_confirm="on_confirm" />
  </li>
</ul>
</template>
"#;

const CHILD_FILES: [(&str, &str); 2] = [("row_child", ROW_CHILD), ("fn_row_child", FN_ROW_CHILD)];

const ALIASES: &str = "use super::fn_row_child as FnRowChild;\nuse super::row_child as RowChild;\n";

/// Compile one `.vx` source into the module file a build would write.
fn generate(source: &str, name: &str, base: &Path) -> String {
    let sfc = parse_sfc(source).unwrap_or_else(|e| panic!("{name}: parse failed: {e}"));
    let script_setup = sfc.script_setup.as_ref().map(|s| s.content.as_str());
    let tpl = sfc
        .template
        .as_ref()
        .map(|t| t.content.as_str())
        .unwrap_or("");
    let mut resolver = ComponentResolver::new(base.to_path_buf());
    if let Some(setup) = script_setup {
        resolver.parse_imports(setup);
    }
    let render = compile_template_to_rs_full_with_mode(
        tpl,
        name,
        Some(&mut resolver),
        script_setup,
        None,
        RenderMode::State,
    )
    .unwrap_or_else(|e| panic!("{name}: template compilation failed: {e}"));
    let mut module = to_stub_rs_unwrapped(&sfc, name, Some(base));
    module.push('\n');
    module.push_str(ALIASES);
    module.push_str("\n\n");
    module.push_str(&render);
    module
}

/// Scaffold a throwaway crate inside this repository's `target/` and build the
/// generated modules.
///
/// The crate names only `velox-core` and `velox-dom` — the generated code names
/// neither `velox-style` nor `velox-renderer`, so this builds in seconds and
/// needs no skia. It carries `[workspace]` so cargo does not read it as part of
/// the velox workspace, and it lives under this repo's `target/` so every byte
/// the test writes stays inside the checkout.
///
/// The name carries a nanosecond suffix: this file has two `#[test]`s and the
/// same integration-test binary runs them on parallel threads, so a stable path
/// would have two tests rewriting one `Cargo.toml` and racing one `target/`.
fn scaffold_and_build(parent_name: &str, parent_body: &str) -> (ScratchCrate, Output) {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/props-loop-dispatch");
    std::fs::create_dir_all(&scratch).expect("create scratch dir");
    let root = scratch.join(format!("{parent_name}-{unique}"));
    let src = root.join("src");
    std::fs::create_dir_all(&src).expect("create temp crate src");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("velox-sfc has a parent")
        .to_path_buf();

    let manifest = format!(
        r#"[package]
name = "props_loop_dispatch"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[[bin]]
name = "props_loop_dispatch"
path = "src/main.rs"

[dependencies]
velox-core = {{ path = "{}" }}
velox-dom = {{ path = "{}" }}
"#,
        workspace.join("velox-core").display(),
        workspace.join("velox-dom").display(),
    );
    std::fs::write(root.join("Cargo.toml"), manifest).expect("write manifest");

    for (file, body) in CHILD_FILES {
        std::fs::write(root.join(format!("{file}.vx")), body).expect("write vx file");
    }
    for (name, body) in CHILD_FILES {
        std::fs::write(src.join(format!("{name}.rs")), generate(body, name, &root))
            .expect("write generated child module");
    }
    std::fs::write(
        src.join(format!("{parent_name}.rs")),
        generate(parent_body, parent_name, &root),
    )
    .expect("write generated parent module");
    std::fs::write(
        src.join("main.rs"),
        format!("mod fn_row_child;\nmod row_child;\nmod {parent_name};\n\nfn main() {{}}\n"),
    )
    .expect("write main");

    let out = cargo_in(&root, "build");
    (ScratchCrate { root }, out)
}

/// One run's scratch directory, removed when the test that made it ends.
///
/// Both `#[test]`s in this binary write here, so the path is unique per run —
/// otherwise they would rewrite one another's crate while both `cargo build`s
/// were live. The price of uniqueness is that no dependency build is reused, and
/// the directory is deleted on the way out so the price is paid per run rather
/// than accumulated. `Drop` rather than a cleanup line, so a panicking test does
/// not leave it behind either.
struct ScratchCrate {
    root: PathBuf,
}

impl Drop for ScratchCrate {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn cargo_in(root: &Path, subcommand: &str) -> Output {
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .arg(subcommand)
        .current_dir(root)
        .env("CARGO_TARGET_DIR", root.join("target"))
        .output()
        .expect("cargo failed to spawn")
}

fn report(out: &Output) -> String {
    format!(
        "--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    )
}

/// A bound child tag in a loop over a declared `Vec` prop must generate code
/// that compiles.
///
/// The body under test is `render_with_props`: it binds `state` as a plain
/// `script_rs::State` (or not at all), and it is the path a parent actually
/// calls. The registration it used to emit reached for `state` as an
/// `Arc<State>`, which fails to compile with E0308, or reached for a `state` that
/// was never bound, which fails with E0425. Either is a build failure here.
#[test]
fn a_bound_child_tag_in_a_props_loop_generates_code_that_compiles() {
    let (scratch, out) = scaffold_and_build("looped_parent", LOOPED_PARENT);
    assert!(
        out.status.success(),
        "the generated module for a `@event` bound child inside a `v-for` over a \
         declared `Vec` prop did not build\n{}\n(scratch: {})",
        report(&out),
        scratch.root.display()
    );
}

/// The same shape with the function-prop spelling, for the same reason: both
/// spellings feed `component_dispatch_handlers`, so a gate that closed only
/// `@event` would leave the function prop broken in the loop body.
#[test]
fn a_fn_prop_child_tag_in_a_props_loop_generates_code_that_compiles() {
    let (scratch, out) = scaffold_and_build("looped_fn_prop_parent", LOOPED_FN_PROP_PARENT);
    assert!(
        out.status.success(),
        "the generated module for a function-prop bound child inside a `v-for` over \
         a declared `Vec` prop did not build\n{}\n(scratch: {})",
        report(&out),
        scratch.root.display()
    );
}
