//! A `PropsArg` literal a build compiles, checked by COMPILING it.
//!
//! # Why this file compiles code instead of reading it
//!
//! The defect this pins is a compile error in generated code, so no
//! in-process observation of the `velox_dom::VNode` can see it: the crate
//! compiles, the *generated* code does not. Two weaker proofs were available
//! and both are wrong here. Asserting the emitted string would prove only that
//! the compiler emitted the string it was told to emit — the objection
//! `props_collection.rs` records for its own tests. And adding a case to
//! `tests/integration_compile.rs` would put the proof behind `#[ignore]`,
//! which is how a shipped `velox init` template stayed broken in this repo
//! while every visible test stayed green.
//!
//! So this runs `cargo build` by default, and does it without skia: generated
//! code names neither `velox_style` nor `velox_renderer`, so the temporary
//! crate depends on `velox-core` and `velox-dom` only and builds in seconds.
//! It takes the same `CARGO_TARGET_DIR` redirect as
//! `root_vif_class_behaviour.rs` — a build inside a test would otherwise wait
//! on the workspace target lock that is already held by `cargo test` itself.
//!
//! The fixtures are the SHAPE `velox-cli/templates/project` ships, trimmed to
//! the boundary and nothing else: a child with no `Props` struct, a child with
//! two required props of which the parent binds one, and a parent that binds
//! none at all. Three distinct construction sites, three ways to get it wrong.

use std::path::{Path, PathBuf};
use std::process::Command;

use velox_sfc::{
    ComponentResolver, RenderMode, compile_template_to_rs_full_with_mode, parse_sfc,
    to_stub_rs_unwrapped,
};

// ---------------------------------------------------------------------------
// Fixtures. Inputs, not expectations: nothing here states what the parent
// should be given, only what each component declares.
// ---------------------------------------------------------------------------

/// Declares NO `Props` struct, so `generate_props_arg` gives it the STRUCT
/// branch — a `PropsArg` with a `values` map. A parent that binds it nothing
/// must still name that map.
const NO_PROPS_CHILD: &str = r#"
<script setup>
pub struct State {
    pub label: String,
}

impl State {
    pub fn new() -> Self {
        State { label: String::from("no-props-child") }
    }
}
</script>

<template>
<p class="no-props-child">a child that declares no Props</p>
</template>
"#;

/// Declares TWO required props. Every field of a `Props` struct is a field the
/// parent's literal has to name, whatever the parent's template says about it.
const PROPS_CHILD: &str = r#"
<script setup>
pub struct Props {
    pub value: String,
    pub placeholder: String,
}

pub struct State {
    pub props: Props,
}

impl State {
    pub fn new() -> Self {
        State {
            props: Props {
                value: String::new(),
                placeholder: String::from("What goes here?"),
            },
        }
    }
}
</script>

<template>
<input class="props-child" placeholder="type here" />
</template>
"#;

/// Binds ONE of the two fields it can see, and binds a props-less child not
/// at all: the two literal shapes that used to be emitted empty.
const PARTIAL_PARENT: &str = r#"
<script setup>
import NoPropsChild from './no_props_child.vx'
import PropsChild from './props_child.vx'

pub struct State {
    pub new_value: String,
}

impl State {
    pub fn new() -> Self {
        State { new_value: String::from("bound value") }
    }
}
</script>

<template>
<div class="partial-parent">
  <NoPropsChild />
  <PropsChild :value="new_value" />
</div>
</template>
"#;

/// Binds NOTHING to a child that requires props. Distinct from the two cases
/// above: here the literal has no parent-supplied field to start from at all,
/// so it is built entirely from what the child declares.
const UNBOUND_PARENT: &str = r#"
<script setup>
import PropsChild from './props_child.vx'

pub struct State {
    pub tick: i32,
}

impl State {
    pub fn new() -> Self {
        State { tick: 0 }
    }
}
</script>

<template>
<div class="unbound-parent">
  <PropsChild />
</div>
</template>
"#;

/// The alias each parent needs for the generated `Child::PropsArg` path to
/// resolve. The generated code names the child by its import alias, so the
/// module has to bring that alias into scope itself.
const ALIASES: &str = "\
use super::no_props_child as NoPropsChild;
use super::props_child as PropsChild;
";

/// Compile one `.vx` source into the module file a build would write.
///
/// This is the same pair of entry points a build uses, joined in the order the
/// CLI joins them: the stub first, then the render functions.
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

/// Write the `.vx` tree the resolvers read, and return the temporary crate root.
fn scaffold() -> (PathBuf, PathBuf) {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("velox-props-arg-compiles-{unique}"));
    let src = root.join("src");
    std::fs::create_dir_all(&src).expect("create temp crate src");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("velox-sfc has a parent")
        .to_path_buf();

    let manifest = format!(
        r#"[package]
name = "props_arg_compiles"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[[bin]]
name = "props_arg_compiles"
path = "src/main.rs"

[dependencies]
velox-core = {{ path = "{}" }}
velox-dom = {{ path = "{}" }}
"#,
        workspace.join("velox-core").display(),
        workspace.join("velox-dom").display(),
    );
    std::fs::write(root.join("Cargo.toml"), manifest).expect("write manifest");

    for (file, body) in [
        ("no_props_child.vx", NO_PROPS_CHILD),
        ("props_child.vx", PROPS_CHILD),
        ("partial_parent.vx", PARTIAL_PARENT),
        ("unbound_parent.vx", UNBOUND_PARENT),
    ] {
        std::fs::write(root.join(file), body).expect("write vx file");
    }
    (root, src)
}

/// Every `PropsArg` literal a build constructs has to be one Rust accepts.
///
/// The name of this test is the claim: each child boundary is a struct or a
/// type alias the child chose, and a literal that does not match it does not
/// compile. A missing field is `E0063`, and a build that emits one ships an
/// `init` template that no user can build.
#[test]
fn every_props_arg_literal_a_parent_emits_compiles() {
    let (root, src) = scaffold();
    for (name, body) in [
        ("no_props_child", NO_PROPS_CHILD),
        ("props_child", PROPS_CHILD),
        ("partial_parent", PARTIAL_PARENT),
        ("unbound_parent", UNBOUND_PARENT),
    ] {
        std::fs::write(src.join(format!("{name}.rs")), generate(body, name, &root))
            .expect("write generated module");
    }
    let main = r#"mod no_props_child;
mod props_child;
mod partial_parent;
mod unbound_parent;

fn main() {
    let _ = partial_parent::render();
    let _ = unbound_parent::render();
}
"#;
    std::fs::write(src.join("main.rs"), main).expect("write main");

    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .arg("build")
        .current_dir(&root)
        .env("CARGO_TARGET_DIR", root.join("target"))
        .output()
        .expect("cargo build failed to spawn");
    assert!(
        out.status.success(),
        "generated `PropsArg` literals did not compile.\n\
         This is the defect `velox init` shipped: a child boundary literal that \
         does not match the shape the child declares.\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let _ = std::fs::remove_dir_all(&root);
}
