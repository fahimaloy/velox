//! `emit()` runs the parent's handler, checked by RUNNING the generated module.
//!
//! # Why this file runs code instead of reading it
//!
//! The defect `emit()` used to carry is invisible to a source assertion. It
//! looked up the handler name the parent bound, threw the name away, and
//! returned `Some(handler)` — so a component that emitted from its own code
//! compiled, generated every token a wiring check would look for, and did
//! nothing at all. `tests/emit_system_tests.rs` asserted on those tokens and
//! stayed green. No string the generator emits can tell a wired handler from an
//! unwired one, because both are the same tokens in a different order: the
//! dispatcher is only ever called if the name survives to a call site.
//!
//! So the proof is the parent's `Signal<Vec<String>>`. The child emits, and the
//! test reads back whether the parent's method ran with the payload it was
//! given. An `emit()` that looks the name up and drops it leaves the log empty
//! and this test fails.
//!
//! # Why `cargo run` and not `cargo build`
//!
//! A build proves the wiring type-checks; it cannot prove the closure is ever
//! called. The child is fired from `main`, after the parent has rendered — the
//! registration the render performs is the thing under test, and it is invisible
//! until something emits afterwards.
//!
//! # Firing a child that is not the instance the parent rendered
//!
//! `<StringChild @confirm="on_confirm" />` has an event binding, so it is NOT
//! rendered through its persistent state field: a tag with a binding or a `:prop`
//! always renders through the props path, and that path builds a fresh
//! throwaway child `State` per render. `main` therefore fires a *different
//! instance* of the child's state — which is exactly the point. The handler
//! registry is per generated FILE (`thread_local!` in that module, not crate
//! global), so it is keyed by handler name, not by which instance emitted. The
//! parent's rendered call site installed the binding; the payload arrives at the
//! parent's method; nothing about the two instances is required to line up.
//!
//! # Both prop spellings, one parent
//!
//! `@confirm="on_confirm"` (a string handler NAME) and `:on_confirm="on_confirm"`
//! (the method as a FUNCTION prop) are the two ways a caller hands a child a
//! handler, and the second one could not be written before this change: every
//! bound prop was stringified, so a bound closure became its source text. Both
//! are asserted in the same run because they differ only in how the parent
//! registers the handler, and a wiring that is right for one and wrong for the
//! other would otherwise be reported as one passing test.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use velox_sfc::{
    ComponentResolver, RenderMode, compile_template_to_rs_full_with_mode, parse_sfc,
    to_stub_rs_unwrapped,
};

// ---------------------------------------------------------------------------
// Fixtures. Inputs, not expectations: nothing here says what the parent should
// receive, only what each component declares.
// ---------------------------------------------------------------------------

/// A child with NO props. It emits from a method its own button calls, which is
/// the shape a Confirm's confirm/cancel buttons have.
const STRING_CHILD: &str = r#"
<script setup>
pub struct State;

impl State {
    pub fn new() -> Self {
        State
    }

    pub fn fire(&self) {
        emit("confirm", "from-string-child");
    }
}
</script>

<template>
<button class="ok" @click="fire">ok</button>
</template>
"#;

/// A child whose ONLY prop is a handler, declared as a function so the caller
/// can hand over behaviour rather than the name of some behaviour.
///
/// `Option<Box<dyn Fn(&str)>>` is the widest useful shape: it is `None` when a
/// caller binds nothing, so the prop is not required, and it is a plain `Fn`
/// rather than `FnMut`, so a `Box` is enough to own it and no `RefCell` is
/// needed inside the child.
///
/// `fire` CALLS the prop, which is the whole point of declaring one, and falls
/// back to `emit` when no caller handed one over. That fallback is what makes
/// the two children in `PARENT` a comparison rather than a pair: each reaches
/// the same parent method over a different channel — `FnPropChild` through the
/// closure it was given, `StringChild` through an event name — so a wiring that
/// is right for one and wrong for the other shows up as one payload missing.
const FN_PROP_CHILD: &str = r#"
<script setup>
pub struct Props {
    pub on_confirm: Option<Box<dyn Fn(&str)>>,
}

pub struct State {
    pub props: Props,
}

impl State {
    pub fn new() -> Self {
        State {
            props: Props { on_confirm: None },
        }
    }

    pub fn fire(&self) {
        let payload = "from-fn-prop-child";
        match self.props.on_confirm.as_ref() {
            Some(f) => f(payload),
            None => emit("confirm", payload),
        }
    }
}
</script>

<template>
<button class="ok" @click="fire">ok</button>
</template>
"#;

/// Binds one child by handler NAME and one by handler FUNCTION, and keeps a log
/// both have to write to. The two children sit side by side on purpose: this is
/// the file a Modal author is looking at when they ask which of the two
/// spellings to use.
const PARENT: &str = r#"
<script setup>
import FnPropChild from './fn_prop_child.vx'
import StringChild from './string_child.vx'
use std::sync::Arc;
use velox_core::signal::Signal;

pub struct State {
    pub log: Signal<Vec<String>>,
}

impl State {
    pub fn new() -> Self {
        Self { log: Signal::new(Vec::new()) }
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
<div class="host">
  <StringChild @confirm="on_confirm" />
  <FnPropChild :on_confirm="on_confirm" />
</div>
</template>
"#;

/// The same parent with a function prop bound to something the generator cannot
/// name. `on_confirm` here is a CALL, and a closure built from it would have to
/// re-evaluate the call inside the child.
const CALL_BOUND_PARENT: &str = r#"
<script setup>
import FnPropChild from './fn_prop_child.vx'
use std::sync::Arc;
use velox_core::signal::Signal;

pub struct State {
    pub log: Signal<Vec<String>>,
}

impl State {
    pub fn new() -> Self {
        Self { log: Signal::new(Vec::new()) }
    }

    pub fn on_confirm(&self, payload: &str) {
        self.log.update(|mut seen| {
            seen.push(String::from(payload));
            seen
        });
    }

    pub fn other(&self, payload: &str) {
        self.on_confirm(payload)
    }
}
</script>

<template>
<div class="host">
  <FnPropChild :on_confirm="other('x')" />
</div>
</template>
"#;

const CHILD_FILES: [(&str, &str); 2] = [
    ("string_child", STRING_CHILD),
    ("fn_prop_child", FN_PROP_CHILD),
];

const ALIASES: &str = "\
use super::fn_prop_child as FnPropChild;
use super::string_child as StringChild;
";

/// Compile one `.vx` source into the module file a build would write — the same
/// pair of entry points a build uses, joined in the order the CLI joins them.
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

/// Write the `.vx` tree the resolvers read, so a source-level test can resolve a
/// child's declared `Props` without building anything.
fn vx_tree(name: &str) -> PathBuf {
    let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/emit-invokes");
    std::fs::create_dir_all(&scratch).expect("create scratch dir");
    let root = scratch.join(format!("vx-{name}"));
    std::fs::create_dir_all(&root).expect("create vx dir");
    for (file, body) in CHILD_FILES {
        std::fs::write(root.join(format!("{file}.vx")), body).expect("write vx file");
    }
    root
}

/// Write the temporary crate: the `.vx` tree the resolvers read, a manifest
/// depending on `velox-core` and `velox-dom` only (generated code names neither
/// `velox-style` nor `velox-renderer`, so this builds in seconds and needs no
/// skia), and a `main` that renders, fires, and prints the log.
///
/// The crate lands inside THIS repository's `target/`, not `std::env::temp_dir()`:
/// the generated manifest carries `[workspace]`, so it must not be read as part of
/// the velox workspace, and a repo-local scratch dir keeps every byte this test
/// writes inside the checkout it belongs to.
fn scaffold(parent_name: &str, parent_body: &str) -> (PathBuf, PathBuf) {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/emit-invokes");
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
name = "emit_invokes_parent"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[[bin]]
name = "emit_invokes_parent"
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
    (root, src)
}

/// `main` for the running test: render once so the parent installs the handler,
/// then fire each child and print what the parent's method collected.
///
/// The two children are fired over their own channels, which is why they need
/// different setup:
///
/// - `StringChild` fires through `emit("confirm", ..)`. `State::new()` is all it
///   takes: the child looks the event up in the registry the PARENT'S rendered
///   call site filled in, so the payload arrives with no help from here.
/// - `FnPropChild` fires through the closure it was handed. `State::new()`
///   installs `on_confirm: None`, so a child constructed that way has nothing to
///   call and its `emit` fallback reaches an event the parent never bound on
///   this tag — it would fall through to nothing and the test would blame the
///   generator for a prop that was never in the child's hands.
///
/// The closure below is the value the parent GENERATED for
/// `:on_confirm="on_confirm"`, reproduced so the child can be fired at all. That
/// makes this the assertion that the parent's half of the binding is right: the
/// handler the child's `dispatch_emit` looks up has to be one the parent's
/// render registered, or the payload goes nowhere. The generator's half — that
/// the literal it writes is this closure, keyed on the handler the parent bound
/// rather than on the event — is asserted on the generated source by
/// `a_function_prop_is_a_closure_over_the_child_dispatcher` in
/// `emit_system_tests.rs`.
fn running_main(parent_name: &str) -> String {
    format!(
        r#"use std::sync::Arc;

mod fn_prop_child;
mod string_child;
mod {parent_name};

fn main() {{
    let state = Arc::new({parent_name}::script_rs::State::new());
    // The render is the half that matters: it is where the parent registers the
    // handler the child is about to emit to.
    let _vnode = {parent_name}::render_with_state(
        Arc::clone(&state),
        {parent_name}::make_resolve(Arc::clone(&state)),
    );

    string_child::script_rs::State::new().fire();

    fn_prop_child::script_rs::State {{
        props: fn_prop_child::script_rs::Props {{
            on_confirm: Some(Box::new(|p| {{
                fn_prop_child::dispatch_emit("on_confirm", p);
            }})),
        }},
    }}
    .fire();

    println!("LOG:{{}}", state.log.get().join(","));
}}
"#
    )
}

/// `main` for the refusal test: the generated modules are DECLARED and nothing
/// calls into them.
///
/// The `mod` lines are the whole test. A bare `fn main() {}` is not a smaller
/// version of the same thing — it never names the generated parent, so rustc
/// never type-checks the module the `compile_error!` was generated into, the
/// build succeeds, and the test then asserts a diagnostic it was never compiled
/// to produce is absent.
fn modules_only_main(parent_name: &str) -> String {
    format!("mod fn_prop_child;\nmod string_child;\nmod {parent_name};\n\nfn main() {{}}\n")
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

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

/// The parent handler RUNS, for a string handler name and for a function prop
/// alike, and it runs with the payload the child emitted.
///
/// The two payloads are asserted separately, not just "the log is non-empty": an
/// emit that reached the wrong handler, or reached it with the other child's
/// payload, would still leave a non-empty log. Order is the template's, so it is
/// asserted too — a registry that overwrote itself would fire the second child
/// through the first child's binding and put the lines in the other order.
#[test]
fn a_parent_handler_runs_for_a_string_name_and_for_a_function_prop() {
    let (root, src) = scaffold("parent", PARENT);
    std::fs::write(src.join("main.rs"), running_main("parent")).expect("write main");

    let out = cargo_in(&root, "run");
    assert!(
        out.status.success(),
        "the generated module did not build or run\n{}",
        report(&out)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let log = stdout
        .lines()
        .find_map(|l| l.strip_prefix("LOG:"))
        .unwrap_or_else(|| {
            panic!(
                "main did not print the parent's log, so nothing was observed\n{}",
                report(&out)
            )
        });

    assert_eq!(
        log,
        "from-string-child,from-fn-prop-child",
        "the parent handler did not run for both bindings.\n\
         A child that emitted found no handler to call: `emit()` looked the name up \
         and dropped it, so `@confirm` and `:on_confirm` both compiled and both did \
         nothing. If only one payload is present, that one binding is wired and the \
         other is not — the log order is the template's, so a swap means a later \
         registration overwrote an earlier one in the shared registry.\n{}",
        report(&out)
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A function prop bound to a CALL is refused, and the refusal names the prop,
/// the component and the rule.
///
/// A call is not a refusal the compiler would make on its own: a generated
/// closure over `other('x')` would either not compile, deep inside a props
/// literal the author never wrote, or — worse — capture a value that is gone by
/// the time the child calls it. Generated `compile_error!` is the only place a
/// diagnostic can point at.
#[test]
fn a_function_prop_bound_to_a_call_is_refused_by_name() {
    let (root, src) = scaffold("call_bound_parent", CALL_BOUND_PARENT);
    std::fs::write(src.join("main.rs"), modules_only_main("call_bound_parent"))
        .expect("write main");

    let out = cargo_in(&root, "build");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "a function prop bound to a CALL compiled. The closure the parent hands the \
         child would have to re-evaluate `other('x')` when the child fires, and its \
         captures would not survive the move into the child — so the handler would be \
         either a borrow of something dead or a silent no-op.\n{}",
        report(&out)
    );
    assert!(
        stderr.contains("on_confirm") && stderr.contains("FnPropChild"),
        "the bad binding failed to compile but the diagnostic does not name the prop and \
         the component together, so the author cannot tell which of their bindings it is \
         complaining about.\n{}",
        report(&out)
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The generated half of a function-prop binding: the value is a closure over the
/// CHILD's dispatcher, keyed on the handler the parent bound.
///
/// This exists because
/// `a_parent_handler_runs_for_a_string_name_and_for_a_function_prop` cannot cover
/// it. That test's `main` fires the function-prop child through a closure it
/// writes out by hand — it has to, because `State::new()` installs
/// `on_confirm: None` and the parent's props literal is built inside a render
/// expression `main` cannot name. A hand-written `dispatch_emit("on_confirm", ..)`
/// proves the parent's registration ran; it cannot prove the generator WROTE that
/// name, because the name would be right in `main` either way. So the generator's
/// half is asserted here, on the generated source, where a regression in the name
/// it emits cannot hide behind `main`.
///
/// The two facts have to agree, and the reason is the registry: the value the
/// parent hands over looks the dispatcher up in the CHILD's module by handler
/// name, so a literal naming a different name than `set_emit_dispatch` registered
/// is a child holding a working-looking closure that resolves to nothing. Both
/// halves are on the handler the author bound — the EVENT (`confirm`) is what the
/// string-name spelling goes through `emit` with, and it is not what either of
/// these two names is.
#[test]
fn a_function_prop_is_a_closure_over_the_child_dispatcher() {
    let base = vx_tree("fn-prop-literal");
    let module = generate(PARENT, "parent", &base);

    assert!(
        module.contains(
            r#"on_confirm: Some(Box::new(move |__vx_payload: &str| { FnPropChild::dispatch_emit("on_confirm", __vx_payload); }))"#
        ),
        "the function prop was not generated as a closure over the child's dispatcher, \
         keyed on the bound handler name `on_confirm`.\n{module}"
    );
    assert!(
        module.contains(r#"FnPropChild::set_emit_dispatch("on_confirm","#),
        "the parent registered no dispatcher under the name the closure it generated \
         looks up, so the closure resolves to nothing at run time.\n{module}"
    );
    // `@confirm="on_confirm"` is the OTHER spelling, and it reaches the same parent
    // method through the event map rather than the dispatch registry. Its dispatcher
    // is registered against the same handler name, because `make_on_event` matches on
    // method names — asking it for the event (`"confirm"`) hits its wildcard arm and
    // drops the payload with the closure having run, which is a silent no-op that no
    // string assertion on this file can see.
    assert!(
        module.contains(r#"StringChild::set_emit_dispatch("on_confirm","#)
            && module.contains(r#"__vx_dispatch("on_confirm", Some(__vx_payload))"#),
        "the `@event` spelling registered its dispatcher under the EVENT name instead \
         of the handler name its own dispatcher matches on.\n{module}"
    );
    // The registered handler must not keep the parent's `State` alive. It is held
    // behind a `Weak`, because the registry is a `thread_local!`: a strong `Arc`
    // there moves the destruction of every `Signal` in the state into that
    // thread-local's destructor, where the `EffectHandle` drop reaches for a
    // `velox-core` thread-local that is already gone and the process aborts. See
    // the note on `emit_dispatch_registrations`.
    assert!(
        module.contains("std::sync::Arc::downgrade(&state)")
            && !module.contains("make_on_event(std::sync::Arc::clone(&state))"),
        "the dispatch registry captured a strong `Arc<State>`, so the state is \
         destroyed by the thread-local's destructor rather than by the app.\n{module}"
    );
    let _ = std::fs::remove_dir_all(&base);
}
