//! The two ends of the child-boundary literal that a source assertion cannot see:
//! what an UNBOUND `Option` prop becomes, and what a function-typed prop the
//! generator cannot build is refused as.
//!
//! # Why this compiles code
//!
//! Both contracts are properties of the generated props literal, and the
//! failure modes are opposite: one is a type error the author never wrote, the
//! other is a `compile_error!` string the generator chose. Asserting the emitted
//! string proves only that the generator emitted the string it decided to emit —
//! it says nothing about whether that string compiles, and a deliberate
//! `compile_error!` is precisely a string that will not compile. So each test
//! here builds a scratch crate and reads back `cargo`'s verdict.
//!
//! # Why this is not `props_arg_compiles.rs`
//!
//! That file is about REQUIRED props: a parent's literal is either what the
//! child declared or an error. The cases here are the two the required-prop
//! contract does not reach. An `Option` prop says what its omission MEANS, so
//! the generator fills it — the relaxation that is pinned here. And a
//! function-typed prop the builder cannot construct is a different refusal again:
//! not "you did not bind it" but "you bound it to a signature I cannot produce".
//!
//! # The function-prop half, in one sentence
//!
//! `function_prop_value` emits exactly one closure —
//! `move |__vx_payload: &str| { Comp::dispatch_emit(name, __vx_payload); }` — so
//! exactly one signature is buildable. The predicate that RECOGNISES a callback
//! is deliberately broad (a field naming a closure is a callback, whatever the
//! argument list); what it must not be is a promise the builder cannot keep. So
//! a recognised-but-unbuildable type gets its own arm, and the author gets a
//! diagnostic naming the prop, the type they wrote, and the signature that
//! works — instead of a type error inside generated code.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use velox_sfc::{
    ComponentResolver, RenderMode, compile_template_to_rs_full_with_mode, parse_sfc,
    to_stub_rs_unwrapped,
};

// ---------------------------------------------------------------------------
// The child. One `Props` struct with an optional callback, an optional text
// field, and a required one, so a single fixture covers the three shapes the
// parent's literal has to distinguish.
// ---------------------------------------------------------------------------

/// `on_confirm` is `Option<Box<dyn Fn(&str)>>` — the widest useful shape, and
/// the one the builder can produce.
///
/// `label` is declared with the FULLY QUALIFIED `std::option::Option`. It names
/// the same type as `Option`; if the generator only recognises the short
/// spelling it treats a declared-optional field as required and refuses a parent
/// that never bound it, which sends the author looking for a prop that is right
/// there in their own file.
const CHILD: &str = r#"
<script setup>
pub struct Props {
    pub on_confirm: Option<Box<dyn Fn(&str)>>,
    pub label: std::option::Option<String>,
    pub required: String,
}

pub struct State {
    pub props: Props,
}

impl State {
    pub fn new() -> Self {
        State {
            props: Props { on_confirm: None, label: None, required: String::new() },
        }
    }

    pub fn fire(&self) {
        if let Some(f) = self.props.on_confirm.as_ref() {
            f("from-child");
        }
    }
}
</script>

<template>
<button class="child" @click="fire">{{ label }}</button>
</template>
"#;

/// A child whose callback is declared `Option<Box<dyn Fn(String) -> bool>>`.
///
/// Not a spelling mistake. It is a plausible declaration — a callback that
/// validates and answers yes/no is a natural thing to want — and before the
/// fix it passed the recogniser and was then handed a value of a different
/// shape, so the failure was a type error pointing into `OUT_DIR` at a props
/// literal the author never wrote.
const UNBUILDABLE_CHILD: &str = r#"
<script setup>
pub struct Props {
    pub on_confirm: Option<Box<dyn Fn(String) -> bool>>,
}

pub struct State {
    pub props: Props,
}

impl State {
    pub fn new() -> Self {
        State { props: Props { on_confirm: None } }
    }
}
</script>

<template>
<button class="child">x</button>
</template>
"#;

/// A child whose callback is `RefCell<Box<dyn FnMut(&str)>>`.
///
/// A mutable callback needs interior mutability on the child's side, and
/// `RefCell` is how a `Props` struct spells that. The wrapper list that rebuilds
/// the value and the predicate that recognises the type have to agree on this
/// spelling: recognising it and then peeling nothing produces a bare closure
/// where the field wants a `RefCell`, which is the same unbuildable-value
/// failure as above wearing a different type.
const REFCELL_CHILD: &str = r#"
<script setup>
pub struct Props {
    pub on_confirm: RefCell<Box<dyn FnMut(&str)>>,
}

pub struct State {
    pub props: Props,
}

impl State {
    pub fn new() -> Self {
        State {
            props: Props {
                on_confirm: RefCell::new(Box::new(|_p: &str| {})),
            },
        }
    }
}
</script>

<template>
<button class="child">x</button>
</template>
"#;

const CHILD_FILES: [(&str, &str); 3] = [
    ("child", CHILD),
    ("unbuildable_child", UNBUILDABLE_CHILD),
    ("refcell_child", REFCELL_CHILD),
];

const ALIASES: &str = "\
use super::child as Child;
use super::refcell_child as RefCellChild;
use super::unbuildable_child as UnbuildableChild;
";

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

fn cargo_build(root: &Path) -> Output {
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .arg("build")
        .current_dir(root)
        .env("CARGO_TARGET_DIR", root.join("target"))
        .output()
        .expect("cargo build failed to spawn")
}

fn report(out: &Output) -> String {
    format!(
        "--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    )
}

/// One run's scratch crate, removed when the test that made it ends.
///
/// Repo-local rather than `std::env::temp_dir()`: the generated manifest carries
/// `[workspace]`, so it must not be read as part of the velox workspace, and a
/// repo-local path keeps every byte this test writes inside the checkout.
struct ScratchCrate {
    root: PathBuf,
}

impl Drop for ScratchCrate {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Write the three children and one parent into a throwaway crate and build it.
fn build(parent: &str) -> (ScratchCrate, Output) {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/optional-and-fn-props")
        .join(format!("run-{unique}"));
    let src = root.join("src");
    std::fs::create_dir_all(&src).expect("create scratch src");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("velox-sfc has a parent")
        .to_path_buf();
    let manifest = format!(
        r#"[package]
name = "optional_and_fn_props"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[[bin]]
name = "optional_and_fn_props"
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
        std::fs::write(src.join(format!("{file}.rs")), generate(body, file, &root))
            .expect("write generated child module");
    }
    std::fs::write(src.join("parent.rs"), generate(parent, "parent", &root))
        .expect("write generated parent module");
    std::fs::write(
        src.join("main.rs"),
        "mod child;\nmod parent;\nmod refcell_child;\nmod unbuildable_child;\n\nfn main() {}\n",
    )
    .expect("write main");
    let out = cargo_build(&root);
    (ScratchCrate { root }, out)
}

// ---------------------------------------------------------------------------
// An unbound `Option` prop
// ---------------------------------------------------------------------------

/// A parent that binds NONE of the optional props still compiles, and the
/// omitted ones become `None`.
///
/// The relaxation this pins used to be a `compile_error!`, and it had to change:
/// `Option` is the one declaration that says what an omission MEANS, and
/// filling it is reading the type rather than guessing a default. It is what
/// makes a callback prop usable at all — `<Modal :on_cancel="close">` and
/// `<Modal>` are both legal parents, and the second could not compile before.
///
/// `required` is bound, so the build is not passing by accident: the literal is
/// exhaustive, and it is `required` alone that would have refused.
#[test]
fn an_unbound_optional_prop_becomes_none_and_compiles() {
    let (scratch, out) = build(
        r#"
<script setup>
import Child from './child.vx'

pub struct State {
    pub text: String,
}

impl State {
    pub fn new() -> Self {
        State { text: String::from("v") }
    }
}
</script>

<template>
<Child :required="text" />
</template>
"#,
    );
    assert!(
        out.status.success(),
        "a parent that binds only the required prop must compile: the two it did \
         not bind are declared `Option`, and `None` is what the author asked for by \
         declaring them optional\n{}\n(scratch: {})",
        report(&out),
        scratch.root.display()
    );
}

/// A REQUIRED prop the parent did not bind still refuses, so the relaxation is
/// about `Option` and not about omission in general.
///
/// The counterpart to the test above, and the reason it is not enough to assert
/// the first one: a generator that filled EVERY hole with `None` would pass that
/// one too.
#[test]
fn an_unbound_required_prop_is_still_refused() {
    let (_scratch, out) = build(
        r#"
<script setup>
import Child from './child.vx'

pub struct State {
    pub text: String,
}

impl State {
    pub fn new() -> Self {
        State { text: String::from("v") }
    }
}
</script>

<template>
<Child :on_confirm="on_confirm" :label="text" />
</template>
"#,
    );
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        !out.status.success(),
        "`required` was never bound and the build SUCCEEDED. The generator is \
         inventing a value for a field the author's template said nothing about, \
         which is the mistake `Default::default()` caused once already.\n{}",
        report(&out)
    );
    // The DELIBERATE message, verbatim — not just the prop's name. `required:
    // None` against a `String` field also fails the build and also prints the
    // word `required`, so an assertion on the name alone passes for a generator
    // that filled every hole with `None` and got a type error instead. Only the
    // `velox:`-prefixed sentence is a refusal the author can act on: it says the
    // prop was not bound, and it says what to do about it.
    assert!(
        stderr.contains("velox: <Child> declares prop `required` and this parent did not bind it"),
        "the refusal must be the deliberate one, naming the prop AND saying it was \
         not bound. A type error from a wrongly-filled `None` fails the build for the \
         wrong reason and sends the author looking for a type mismatch instead of a \
         missing binding. The compiler said:\n{stderr}"
    );
    for remedy in ["bind it on <Child>", "drop the prop from its Props struct"] {
        assert!(
            stderr.contains(remedy),
            "the refusal must name every way to fix it, so the author does not have to \
             guess. `{remedy}` is missing from:\n{stderr}"
        );
    }
}

/// A prop declared `std::option::Option<String>` is optional too.
///
/// The fully qualified spelling names the same type as `Option<String>`, and an
/// author who writes the path out has not declared a required field. If only the
/// short spelling counted, `<Child>` would refuse with a `compile_error!` naming
/// a field the child itself declared optional — the diagnostic would send them
/// looking for a mistake that is not in their code.
#[test]
fn a_fully_qualified_optional_is_optional_too() {
    // Every prop is bound EXCEPT `label`, so the one omission is the qualified
    // spelling. Leaving the others unbound too would make this the same build as
    // the test above and prove nothing about which declaration the recogniser read.
    let (scratch, out) = build(
        r#"
<script setup>
import Child from './child.vx'

pub struct State {
    pub text: String,
}

impl State {
    pub fn new() -> Self {
        State { text: String::from("v") }
    }

    pub fn on_confirm(&self, _payload: &str) {}
}
</script>

<template>
<Child :required="text" :on_confirm="on_confirm" />
</template>
"#,
    );
    assert!(
        out.status.success(),
        "`label` is the only prop left unbound and it is declared \
         `std::option::Option<String>`, so the literal must fill it with `None` like \
         any other `Option`\n{}\n(scratch: {})",
        report(&out),
        scratch.root.display()
    );
}

// ---------------------------------------------------------------------------
// A function prop the builder can and cannot build
// ---------------------------------------------------------------------------

/// `:on_confirm="on_confirm"` against `Option<Box<dyn Fn(&str)>>` compiles.
///
/// The shape the whole mechanism exists for, and the one the closure
/// `function_prop_value` emits is exactly the declared type. If the wrapper list
/// and the value builder ever disagree about the ORDER or the spelling of a
/// wrapper, this is the test that fails first.
#[test]
fn a_fn_prop_binding_by_method_name_compiles() {
    let (scratch, out) = build(
        r#"
<script setup>
import Child from './child.vx'

pub struct State {
    pub text: String,
}

impl State {
    pub fn new() -> Self {
        State { text: String::from("v") }
    }

    pub fn on_confirm(&self, _payload: &str) {}
}
</script>

<template>
<Child :required="text" :on_confirm="on_confirm" />
</template>
"#,
    );
    assert!(
        out.status.success(),
        "binding a method name to an `Option<Box<dyn Fn(&str)>>` prop is the \
         documented contract and must build\n{}\n(scratch: {})",
        report(&out),
        scratch.root.display()
    );
}

/// `RefCell<Box<dyn FnMut(&str)>>` is a shape the builder can build, so it
/// builds.
///
/// `FnMut` is accepted because a closure that implements `Fn` satisfies it, and
/// `RefCell` is accepted because the wrapper list that rebuilds the value peels
/// it. Recognising the type and then peeling nothing would hand the field a bare
/// closure where it wants a `RefCell` — the same unbuildable value as the test
/// below, reached by a different spelling.
#[test]
fn a_refcell_fn_mut_prop_binding_compiles() {
    let (scratch, out) = build(
        r#"
<script setup>
import RefCellChild from './refcell_child.vx'

pub struct State {
    pub text: String,
}

impl State {
    pub fn new() -> Self {
        State { text: String::from("v") }
    }

    pub fn on_confirm(&self, _payload: &str) {}
}
</script>

<template>
<RefCellChild :on_confirm="on_confirm" />
</template>
"#,
    );
    assert!(
        out.status.success(),
        "`RefCell<Box<dyn FnMut(&str)>>` is a callback the builder can construct: \
         the emitted closure is `Fn`, which satisfies `FnMut`, and the wrapper list \
         peels `RefCell`.\n{}\n(scratch: {})",
        report(&out),
        scratch.root.display()
    );
}

/// A callback declared with a signature the builder cannot produce is refused,
/// and the refusal names what to write instead.
///
/// `Option<Box<dyn Fn(String) -> bool>>` is a plausible declaration — a callback
/// that validates and answers yes/no — and there is no value the generator can
/// build for it: the one closure it emits takes `&str` and returns `()`. Handing
/// it the conversion-path value instead produced a type error inside a props
/// literal the author never wrote, pointing into `OUT_DIR`.
///
/// So the refusal is deliberate, and it has to be USEFUL. Each of the three
/// things below is a different way the author would otherwise have to guess:
/// which prop, which type they wrote, and which type works.
#[test]
fn a_callback_the_builder_cannot_build_is_refused_by_name() {
    let (_scratch, out) = build(
        r#"
<script setup>
import UnbuildableChild from './unbuildable_child.vx'

pub struct State {
    pub text: String,
}

impl State {
    pub fn new() -> Self {
        State { text: String::from("v") }
    }

    pub fn on_confirm(&self, _payload: &str) {}
}
</script>

<template>
<UnbuildableChild :on_confirm="on_confirm" />
</template>
"#,
    );
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        !out.status.success(),
        "the parent bound a method name to a prop declared \
         `Option<Box<dyn Fn(String) -> bool>>`, which no generated closure can \
         satisfy, and the build SUCCEEDED — so the value handed to the field is \
         one the field's type cannot hold.\n{}",
        report(&out)
    );
    // The DELIBERATE message, verbatim. Each of its three halves is a different
    // thing the author would otherwise have to guess: which prop, which type
    // they actually wrote, and which type works. Matching the sentence as a
    // whole also rules out a pass on a compiler diagnostic that happens to quote
    // the same types — the point of the arm is that the author is TOLD, not that
    // the build failed.
    assert!(
        stderr.contains(
            "velox: <UnbuildableChild> declares prop `on_confirm` as \
             `Option<Box<dyn Fn(String) -> bool>>`"
        ),
        "the refusal must name the prop and quote the declared type back, in the \
         generator's own voice. The compiler said:\n{stderr}"
    );
    for remedy in ["Fn(&str)", "dispatches by name", "f(payload)"] {
        assert!(
            stderr.contains(remedy),
            "the refusal must also say what DOES work — `{remedy}` is missing from:\n{stderr}"
        );
    }
}

/// The same child, the same parent, but the prop is LEFT UNBOUND — and that
/// compiles.
///
/// The unbuildable signature is a property of the DECLARATION, not of the
/// binding. A parent that never bound it has nothing to hand over, so `None` is
/// the whole answer and refusing would refuse a parent that did nothing wrong.
/// The two tests together are what make the refusal in the previous one a
/// deliberate refusal rather than a blanket ban on the declaration.
#[test]
fn an_unbuildable_callback_left_unbound_still_compiles() {
    let (scratch, out) = build(
        r#"
<script setup>
import UnbuildableChild from './unbuildable_child.vx'

pub struct State {
    pub text: String,
}

impl State {
    pub fn new() -> Self {
        State { text: String::from("v") }
    }
}
</script>

<template>
<UnbuildableChild />
</template>
"#,
    );
    assert!(
        out.status.success(),
        "nothing was bound, so there is nothing to build: the prop is declared \
         `Option` and becomes `None`, exactly as any other unbound optional prop \
         does\n{}\n(scratch: {})",
        report(&out),
        scratch.root.display()
    );
}
