//! A child boundary literal is either what the child declared or a compile
//! error, checked by COMPILING it.
//!
//! # Why this file compiles code instead of reading it
//!
//! The contract this pins is a compile error in generated code, so no
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
//! # Both directions
//!
//! The boundary has two halves and a proof that only walks one of them is
//! half a proof. Binding every declared prop has to COMPILE, or the contract
//! is unusable; omitting one has to NOT compile, or the generator is quietly
//! inventing a value the author's template never said. `6f15082` shipped the
//! second half backwards: it filled every omitted prop with
//! `Default::default()` so a real authoring mistake stopped being an error,
//! which is the failure this file now pins shut.
//!
//! The fixtures are the SHAPE `veloxc/templates/project` ships, trimmed to
//! the boundary and nothing else: a child with no `Props` struct, a child with
//! two required props, and three parents against that child — fully bound,
//! partly bound, and bound to nothing at all.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

/// Binds BOTH declared props, and binds a props-less child not at all: the
/// two literal shapes that both have to compile.
const FULL_PARENT: &str = r#"
<script setup>
import NoPropsChild from './no_props_child.vx'
import PropsChild from './props_child.vx'

pub struct State {
    pub new_value: String,
    pub hint: String,
}

impl State {
    pub fn new() -> Self {
        State {
            new_value: String::from("bound value"),
            hint: String::from("type it here"),
        }
    }
}
</script>

<template>
<div class="full-parent">
  <NoPropsChild />
  <PropsChild :value="new_value" :placeholder="hint" />
</div>
</template>
"#;

/// Binds ONE of the two props its child declares. The parent's template simply
/// did not say what `placeholder` is, and nothing in the generator can tell
/// that apart from an omission that was always meant to be there: `PropField`
/// carries a name and a type and nothing else, so there is no third state to
/// read. This parent is a bug, and the build must say so.
const PARTIAL_PARENT: &str = r#"
<script setup>
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
  <PropsChild :value="new_value" />
</div>
</template>
"#;

/// Binds NOTHING to a child that requires props. Distinct from the case
/// above: here the literal has no parent-supplied field to start from at all,
/// so every field has to come from somewhere and none of them can.
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
fn scaffold(tag: &str) -> (PathBuf, PathBuf) {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("velox-props-arg-compiles-{tag}-{unique}"));
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
    ] {
        std::fs::write(root.join(file), body).expect("write vx file");
    }
    (root, src)
}

/// Build `parent` as a crate alongside both children, and hand back `cargo`'s
/// own output so the caller can read the compiler's verdict rather than
/// guess at it.
fn build(parent_name: &str, parent: &str) -> (PathBuf, Output) {
    let (root, src) = scaffold(parent_name);
    for (name, body) in [
        ("no_props_child", NO_PROPS_CHILD),
        ("props_child", PROPS_CHILD),
        (parent_name, parent),
    ] {
        std::fs::write(src.join(format!("{name}.rs")), generate(body, name, &root))
            .expect("write generated module");
    }
    let main = format!(
        "mod no_props_child;
mod props_child;
mod {parent_name};

fn main() {{
    let _ = {parent_name}::render();
}}
"
    );
    std::fs::write(src.join("main.rs"), main).expect("write main");

    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .arg("build")
        .current_dir(&root)
        .env("CARGO_TARGET_DIR", root.join("target"))
        .output()
        .expect("cargo build failed to spawn");
    (root, out)
}

/// Both of `out`'s streams as text, for a failure message that has to carry
/// the compiler's own words.
fn report(out: &Output) -> String {
    format!(
        "--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    )
}

/// A parent that binds every prop a child declares compiles.
///
/// This is the half of the contract that makes the other half affordable: if
/// a complete binding did not build, "fail loud on an incomplete one" would
/// just be a louder way to ship a template nobody can use. It also pins the
/// two `PropsArg` shapes apart from each other — the props-less child takes
/// the STRUCT branch of `generate_props_arg` and is handed
/// `values: std::collections::HashMap::new()`, while the child that declares
/// `Props` takes the ALIAS branch and is handed a literal naming both fields.
/// The two branches have to keep disagreeing, because a literal that matched
/// the wrong one is E0063 or E0560.
#[test]
fn a_parent_binding_every_declared_prop_compiles() {
    let (root, out) = build("full_parent", FULL_PARENT);
    assert!(
        out.status.success(),
        "a parent binding every declared prop did not compile. This is the half of the \
         contract that must stay true — failing loud on an omission is only defensible if \
         the complete binding is the easy, working path.\n{}",
        report(&out),
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A prop the child declares and the parent omits is a compile error, not a
/// `Default::default()` the author never wrote.
///
/// `6f15082` filled the gap so that `velox init` would build, and in doing so
/// made a genuine authoring error — binding `:value` and forgetting
/// `:placeholder` — stop being an error at all. The child received an empty
/// `String` that no template had ever described. The generator cannot tell
/// that from an omission that was always meant to be there: `PropField`
/// (`script_index::PropField`) is `{ name, ty }` with no `optional` and no
/// `default`, so there is no third state for a fill to read and no way to ask
/// whether the omission was deliberate.
///
/// So the literal refuses, and the refusal has to name the child and the prop.
/// A bare E0063 points into `OUT_DIR/.../todos.rs` at a struct literal the
/// author never wrote and mentions a field with no owner; the author has to
/// guess which of the components in their template it belongs to.
#[test]
fn a_parent_omitting_a_declared_prop_does_not_compile() {
    let (root, out) = build("partial_parent", PARTIAL_PARENT);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "a parent that omitted the `placeholder` prop COMPILES. Every field of the child's \
         `Props` struct is required, and nothing in the generator records that the parent \
         meant to leave one out, so a missing binding is indistinguishable from a bug. \
         Compiling here means an author's forgotten `:placeholder` reaches the child as a \
         silent default. Generated source was written to:\n{}",
        root.join("src/partial_parent.rs").display(),
    );
    assert!(
        stderr.contains("PropsChild") && stderr.contains("placeholder"),
        "the omission failed to compile, but the diagnostic does not name the offending \
         component and the prop together, so the author still has to guess which component \
         in their template it came from.\n{}",
        report(&out),
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A parent that binds nothing to a props-requiring child names EVERY prop it
/// failed to bind, not just the first.
///
/// With no bound field to start from, the literal is built entirely from what
/// the child declares — which is precisely the case where a fill had something
/// to invent for every field. Each missing prop is its own `compile_error!`,
/// so an author unbinding a two-prop child gets both names in one build
/// rather than fixing them one build at a time.
#[test]
fn a_parent_binding_nothing_names_every_missing_prop() {
    let (root, out) = build("unbound_parent", UNBOUND_PARENT);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "a parent that bound NO prop to a child that declares two compiled. The literal was \
         built entirely from what the child declares and every one of those fields was \
         filled with a value the author's template never said.\n{}",
        root.join("src/unbound_parent.rs").display(),
    );
    for prop in ["value", "placeholder"] {
        assert!(
            stderr.contains(prop),
            "the unbound child failed to compile but the diagnostic never named `{prop}`, so \
             the author cannot tell which of the child's props is missing.\n{}",
            report(&out),
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}
