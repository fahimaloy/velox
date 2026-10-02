//! The `@keydown` chain, end to end, from a `.vx` file to focused text input.
//!
//! # What this proves, and what it cannot
//!
//! Everything between the author's template and a focused text field runs here,
//! in one process, with nothing stubbed:
//!
//! ```text
//!   .vx source
//!     -> parse_sfc + compile_template_to_rs_full_with_mode   (the real pipeline)
//!     -> generated Rust, WRITTEN TO A CRATE AND COMPILED BY rustc
//!     -> render_with_state()          the real generated render
//!     -> a real VNode carrying on:keydown / data-focus-id / data-focus-on
//!     -> compute_layout + collect_input_targets   the real target vector
//!     -> plan_keydown("F2") + apply_keydown()     the real dispatch
//!     -> the GENERATED make_on_event closure      the generated handler runs
//!     -> InputTarget.focused on the field named data-focus-id="composer"
//! ```
//!
//! The generated code is executed, not pattern-matched, for the reason
//! `emit_invokes_parent.rs` gives: no string the generator emits can tell a wired
//! handler from an unwired one, because both are the same tokens. The proof is
//! the generated `State`'s own `RefCell`s — the handler ran, with the key name.
//!
//! The one link NOT exercised here is `VirtualKeyCode -> &str`. Constructing a
//! winit `F2` needs the `skia-native` feature, and a crate that enables it drags
//! the native backend into a from-scratch build — which is why this program
//! takes the key as a name. The seam is pinned from the other side, against the
//! real enum and the real winit-facing `dispatch_keydown`, by the `winit_keys`
//! module in `velox-renderer/tests/keydown_focus.rs`, which asserts
//! `key_name(VirtualKeyCode::F2) == "F2"`. The two halves meet at that string
//! and nothing else is assumed about it.
//!
//! # Why the fixture has TWO inputs
//!
//! `focus_input` takes a `usize` into a vector the author cannot see, so "F2
//! focuses input 0" would be a silent bug the moment a field is added above it.
//! The decoy is there to make the NAME do the work: `data-focus-id="composer"` is
//! the second field in the tree, so an implementation that resolved the name to
//! a paint-order index, or to the first input, would focus the decoy and fail
//! here. `data-focus-id="decoy"` is asserted to be unfocused at the end.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use velox_sfc::{
    RenderMode, compile_template_to_rs_full_with_mode, parse_sfc, to_stub_rs_unwrapped,
};

/// The app under test. Every attribute in the template is load bearing:
///
/// - `@keydown="on_key"` on the host — the binding under test. It is on the
///   wrapper, not the input, because a key press has no pointer to hit-test:
///   the runtime walks the whole tree for `on:keydown`, so a binding on any
///   ancestor sees every key.
/// - `data-focus-id` on both inputs — the stable names.
/// - `data-focus-on="F2"` on the composer only — which key focuses which field.
/// - `@input="on_input"` + `:value="text"` — the ordinary editing path, which
///   has to keep working with a keydown handler bound.
const APP: &str = r#"
<script setup>
use std::cell::RefCell;

pub struct State {
    pub text: RefCell<String>,
    pub last_key: RefCell<String>,
    pub presses: RefCell<usize>,
    pub inputs: RefCell<usize>,
}

impl State {
    pub fn new() -> Self {
        State {
            text: RefCell::new(String::from("draft")),
            last_key: RefCell::new(String::new()),
            presses: RefCell::new(0),
            inputs: RefCell::new(0),
        }
    }

    pub fn text(&self) -> String { self.text.borrow().clone() }

    pub fn on_key(&self, key: &str) {
        *self.last_key.borrow_mut() = key.to_string();
        *self.presses.borrow_mut() += 1;
    }

    pub fn on_input(&self, payload: &str) {
        *self.text.borrow_mut() = payload.to_string();
        *self.inputs.borrow_mut() += 1;
    }
}
</script>

<template>
  <div class="host" @keydown="on_key">
    <input type="text" style="width:200px;height:24px;" :value="text" @input="on_input" data-focus-id="decoy" />
    <input type="text" style="width:200px;height:24px;" :value="text" @input="on_input" data-focus-id="composer" data-focus-on="F2" />
  </div>
</template>
"#;

/// `main` for the generated crate.
///
/// It drives the same calls the plain event loop's key arm makes, in the same
/// order, and prints what it observed as `KEY:value` lines. The observations are
/// printed rather than asserted here so a failure is a fact the outer test can
/// quote: a program that panics mid-way reports which step died, and one that
/// runs to the end reports every step's answer.
const MAIN: &str = r#"
mod app;

use std::sync::Arc;
use velox_dom::VNode;
use velox_renderer::events::{
    EditAction, InputTarget, StackCtx, any_input_focused, apply_edit, apply_keydown,
    collect_input_targets, find_focus_id_path, focused_input_index, plan_keydown,
};

/// Char length of the value at `path` — what `apply_keydown` needs to put the
/// caret at the end of a freshly focused field.
fn value_len(vnode: &VNode) -> impl Fn(&[usize]) -> usize {
    let v = vnode.clone();
    move |p: &[usize]| match velox_renderer::find_node_at_path(&v, p) {
        Some(VNode::Element { props, .. }) => {
            props.attrs.get("value").map(|s| s.chars().count()).unwrap_or(0)
        }
        _ => 0,
    }
}

/// The whole current value of the field at `path`.
fn value_at(vnode: &VNode, path: &[usize]) -> String {
    match velox_renderer::find_node_at_path(vnode, path) {
        Some(VNode::Element { props, .. }) => {
            props.attrs.get("value").cloned().unwrap_or_default()
        }
        _ => String::new(),
    }
}

/// The loop's own `on:input` dispatch, reproduced because the loop's copy is
/// private: the generated dispatcher is keyed on the HANDLER NAME the `on:input`
/// prop carries, so that name is read off the node and asked for by name.
fn dispatch_input_value(
    vnode: &VNode,
    path: &[usize],
    new_value: &str,
    on_event: &mut impl FnMut(&str, Option<&str>),
) {
    if let Some(VNode::Element { props, .. }) = velox_renderer::find_node_at_path(vnode, path)
        && let Some(handler) = props.attrs.get("on:input").cloned()
    {
        on_event(&handler, Some(new_value));
    }
}

/// The real target vector, from a real layout of the real tree.
fn collect(vnode: &VNode) -> Vec<InputTarget> {
    let layout = velox_dom::layout::compute_layout(vnode, 800, 600);
    let mut targets = Vec::new();
    let mut order = 0;
    let mut path = Vec::new();
    collect_input_targets(
        vnode,
        &layout,
        None,
        StackCtx::ROOT,
        &mut path,
        &mut order,
        &mut targets,
    );
    targets
}

fn path_str(p: &Option<Vec<usize>>) -> String {
    match p {
        Some(v) => format!("[{}]", v.iter().map(usize::to_string).collect::<Vec<_>>().join(".")),
        None => "none".to_string(),
    }
}

/// `none` for an absent value, otherwise the number — so a missing fact is
/// never mistaken for a `0`.
fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "none".to_string(), |x| x.to_string())
}


fn app_main() {
    // --- the generated render, from the generated State ---------------------
    let state = Arc::new(app::script_rs::State::new());
    let vnode = app::render_with_state(Arc::clone(&state), app::make_resolve(Arc::clone(&state)));
    let mut targets = collect(&vnode);
    let mut focused: Option<Vec<usize>> = None;
    let mut on_event = app::make_on_event(Arc::clone(&state));

    println!("INPUTS_COLLECTED:{}", targets.len());
    println!("COMPOSER_PATH:{}", path_str(&find_focus_id_path(&vnode, &[], "composer")));
    println!("DECOY_PATH:{}", path_str(&find_focus_id_path(&vnode, &[], "decoy")));

    // --- F2, planned then applied, exactly as the loop's key arm does --------
    let plan = plan_keydown(&vnode, "F2");
    println!("PLAN_HANDLERS:{}", plan.handlers.join(","));
    println!("PLAN_FOCUS_PATHS:{}", plan.focus_paths.len());
    let repaint = apply_keydown(
        &plan,
        "F2",
        &mut targets,
        &mut focused,
        &value_len(&vnode),
        &mut on_event,
    );
    println!("REPAINT:{repaint}");
    println!("LAST_KEY:{}", state.last_key.borrow());
    println!("PRESSES:{}", state.presses.borrow());

    // --- where focus landed --------------------------------------------------
    println!("FOCUSED_PATH:{}", path_str(&focused));
    println!("FOCUSED_INDEX:{}", opt(focused_input_index(&targets)));
    println!("ANY_FOCUSED:{}", any_input_focused(&targets));
    println!("COUNT_FOCUSED:{}", targets.iter().filter(|t| t.focused).count());
    let composer = find_focus_id_path(&vnode, &[], "composer");
    let composer_idx = composer
        .as_ref()
        .and_then(|p| velox_renderer::events::input_index_by_path(&targets, p));
    let decoy = find_focus_id_path(&vnode, &[], "decoy");
    let decoy_idx =
        decoy.as_ref().and_then(|p| velox_renderer::events::input_index_by_path(&targets, p));
    println!("COMPOSER_IDX:{}", opt(composer_idx));
    println!("DECOY_IDX:{}", opt(decoy_idx));
    println!("CURSOR:{}", opt(composer_idx.map(|i| targets[i].cursor)));
    println!("ANCHOR:{}", opt(composer_idx.and_then(|i| targets[i].anchor)));

    // --- ordinary typing, with the keydown handler bound -------------------
    let Some(idx) = focused_input_index(&targets) else {
        panic!("F2 did not focus anything, so the rest of the chain has no subject");
    };
    let path = targets[idx].path.clone();
    let before = value_at(&vnode, &path);
    let res = apply_edit(&mut targets[idx], &before, EditAction::Insert('!'));
    if let Some(v) = &res.value {
        dispatch_input_value(&vnode, &path, v, &mut on_event);
    }
    println!("TYPED_INTO:{}", path_str(&Some(path.clone())));
    println!("TEXT_AFTER_TYPE:{}", state.text.borrow());
    println!("INPUTS_DISPATCHED:{}", state.inputs.borrow());
    println!("CURSOR_AFTER_TYPE:{}", targets[idx].cursor);

    // --- Esc, with the keydown handler bound -------------------------------
    let current = value_at(&vnode, &path);
    let res = apply_edit(&mut targets[idx], &current, EditAction::Blur);
    println!("ESC_REPAINT:{}", res.needs_repaint());
    println!("ESC_CHANGED_TEXT:{}", res.value.is_some());
    println!("ANY_FOCUSED_AFTER_ESC:{}", any_input_focused(&targets));
    println!("TEXT_AFTER_ESC:{}", state.text.borrow());
}

"#;

/// The stability case: a `v-for` list of fields whose ORDER changes, with the
/// composer in it.
///
/// This is the fixture that a numeric index cannot survive. `data-focus-id` is
/// the same string before and after, its tree path changes from `[0.2]` to
/// `[0.0]`, and its position in the target vector changes with it — so the
/// composer is the LAST field before the reorder and the FIRST after, which is
/// precisely the change that makes "focus input 0" quietly focus the wrong one.
const REORDER: &str = r#"
<script setup>
use std::cell::RefCell;
use velox_core::ergonomics::Ref;

#[derive(Clone)]
pub struct Field {
    pub id: String,
    pub hot: String,
}

pub struct State {
    pub fields: Ref<Vec<Field>>,
    pub text: RefCell<String>,
    pub last_key: RefCell<String>,
}

impl State {
    pub fn new() -> Self {
        State {
            fields: velox_core::r#ref!(vec![
                Field { id: String::from("a"), hot: String::new() },
                Field { id: String::from("b"), hot: String::new() },
                Field { id: String::from("composer"), hot: String::from("F2") },
            ]),
            text: RefCell::new(String::from("draft")),
            last_key: RefCell::new(String::new()),
        }
    }

    pub fn fields(&self) -> Vec<Field> { self.fields.get() }
    pub fn text(&self) -> String { self.text.borrow().clone() }
    pub fn on_key(&self, key: &str) { *self.last_key.borrow_mut() = key.to_string(); }
    pub fn on_input(&self, payload: &str) { *self.text.borrow_mut() = payload.to_string(); }

    pub fn reverse(&self) {
        self.fields.update(|mut f| { f.reverse(); f });
    }
}
</script>

<template>
  <div class="host" @keydown="on_key">
    <input type="text" style="width:200px;height:24px;" v-for="field in fields" :key="field.id"
      :value="text" @input="on_input" :data-focus-id="field.id" :data-focus-on="field.hot" />
  </div>
</template>
"#;

fn reorder_main_src() -> &'static str {
    r#"
fn reorder_main() {
    let state = Arc::new(reorder::script_rs::State::new());
    let render = |state: &Arc<reorder::script_rs::State>| {
        reorder::render_with_state(Arc::clone(state), reorder::make_resolve(Arc::clone(state)))
    };

    for (label, state) in [("before", Arc::clone(&state)), ("after", Arc::clone(&state))] {
        if label == "after" {
            state.reverse();
        }
        let vnode = render(&state);
        let mut targets = collect(&vnode);
        let mut focused: Option<Vec<usize>> = None;
        let mut on_event = reorder::make_on_event(Arc::clone(&state));

        // Which target index is the composer, in THIS order?
        let composer = find_focus_id_path(&vnode, &[], "composer")
            .unwrap_or_else(|| panic!("{label}: `composer` is in the list"));
        let idx = velox_renderer::events::input_index_by_path(&targets, &composer)
            .unwrap_or_else(|| panic!("{label}: the composer has no input target"));
        println!("{label}.COMPOSER_PATH:{}", path_str(&Some(composer.clone())));
        println!("{label}.COMPOSER_IDX:{idx}");
        println!("{label}.TOTAL:{}", targets.len());

        let plan = plan_keydown(&vnode, "F2");
        println!("{label}.GRANTS:{}", plan.focus_paths.len());
        println!("{label}.HANDLERS:{}", plan.handlers.join(","));
        apply_keydown(
            &plan,
            "F2",
            &mut targets,
            &mut focused,
            &value_len(&vnode),
            &mut on_event,
        );

        println!("{label}.FOCUSED_PATH:{}", path_str(&focused));
        println!("{label}.FOCUSED_IDX:{}", opt(focused_input_index(&targets)));
        println!("{label}.LAST_KEY:{}", state.last_key.borrow());
        // And the stale index this order makes wrong, for the record.
        println!("{label}.IDX0_ID:{}", id_of(&vnode, &targets[0].path));
    }
}

/// The `data-focus-id` of the element at `path`, or `none`.
fn id_of(vnode: &VNode, path: &[usize]) -> String {
    match velox_renderer::find_node_at_path(vnode, path) {
        Some(VNode::Element { props, .. }) => props
            .attrs
            .get("data-focus-id")
            .cloned()
            .unwrap_or_else(|| String::from("none")),
        _ => String::from("none"),
    }
}
"#
}

/// Compile `APP` into the module file a build would write, through the same
/// entry points the build uses and in the same order.
fn generate(source: &str) -> String {
    let sfc = parse_sfc(source).expect("the fixture is a well-formed SFC");
    let tpl = sfc
        .template
        .as_ref()
        .expect("fixture has a template")
        .content
        .as_str();
    let render = compile_template_to_rs_full_with_mode(
        tpl,
        "app",
        None,
        sfc.script_setup.as_ref().map(|s| s.content.as_str()),
        None,
        RenderMode::State,
    )
    .expect("the fixture compiles");
    let mut module = to_stub_rs_unwrapped(&sfc, "app", None);
    module.push_str("\n\n");
    module.push_str(&render);
    module
}

/// The scratch crate, inside THIS repository's `target/`.
///
/// The generated manifest carries `[workspace]`, so it must not be read as part
/// of the velox workspace, and a repo-local dir keeps every byte this test
/// writes inside the checkout it belongs to.
///
/// # The path is UNIQUE per run, not stable
///
/// This file has two `#[test]`s and they live in ONE integration-test binary, so
/// cargo runs them on parallel threads. A stable `target/keydown-focus-e2e` gave
/// both of them the same `Cargo.toml`, the same `src/main.rs`, the same
/// `src/app.rs` and the same `src/reorder.rs`, and the same `CARGO_TARGET_DIR` to
/// run two `cargo run`s in. One test rewrote a file the other was compiling
/// ("the generated sources are rewritten" is not atomic), and the two cargo
/// invocations contended for one target dir. The suffix is the same one
/// `emit_invokes_parent.rs` uses, and it is the whole fix: two tests, two
/// directories, two target dirs, no shared file.
///
/// The cost is that no dependency build is reused between runs, and this crate
/// pulls `velox-renderer`, so one run's `target/` is a few hundred megabytes.
/// [`ScratchCrate`] removes the run directory when the test ends, so the cost is
/// paid per run and not accumulated per run. A build cache shared between two
/// concurrently-running tests is a race with a non-deterministic failure mode,
/// which is worth more than the seconds it saved.
fn scaffold() -> ScratchCrate {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/keydown-focus-e2e")
        .join(format!("run-{unique}"));
    let src = root.join("src");
    std::fs::create_dir_all(&src).expect("create scratch src dir");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("velox-sfc has a parent")
        .to_path_buf();

    // No features: the whole point of the split between the winit-facing entry
    // point and the winit-free one is that this crate needs no native backend.
    let manifest = format!(
        r#"[package]
name = "keydown_focus_e2e"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[[bin]]
name = "keydown_focus_e2e"
path = "src/main.rs"

[dependencies]
velox-core = {{ path = "{}" }}
velox-dom = {{ path = "{}" }}
velox-renderer = {{ path = "{}" }}
"#,
        workspace.join("velox-core").display(),
        workspace.join("velox-dom").display(),
        workspace.join("velox-renderer").display(),
    );
    std::fs::write(root.join("Cargo.toml"), manifest).expect("write manifest");
    // The two generated components are SIBLING modules, not one bigger one, so
    // each keeps its own `script_rs::State` and the probe can hold both at once:
    // the stability case has to observe the same key press before and after a
    // reorder, which means the state that performs the reorder is still live.
    // `MAIN` already declares `mod app;`; the reorder module is the addition, and
    // it has to be in the same crate for `reorder_main` to reach the helpers MAIN
    // defines.
    let probe_main = format!(
        "mod reorder;\n{MAIN}\n{}\nfn main() {{\n    app_main();\n    reorder_main();\n}}\n",
        reorder_main_src()
    );
    std::fs::write(src.join("main.rs"), probe_main).expect("write main");
    std::fs::write(src.join("app.rs"), generate(APP)).expect("write generated app module");
    std::fs::write(src.join("reorder.rs"), generate(REORDER))
        .expect("write generated reorder module");
    ScratchCrate { root }
}

/// One run's scratch directory, removed when the test that made it ends.
///
/// The unique-per-run path is what stops the two tests in this binary from
/// rewriting each other's `Cargo.toml` and `src/*.rs` while both cargo runs are
/// live, and the price is that nothing is reused between runs: this crate pulls
/// `velox-renderer`, so one run's `target/` is a few hundred megabytes, and a
/// directory that survives would add up on every `cargo test -p velox-sfc`.
///
/// `Drop` rather than a cleanup line at the end of each test, so the directory
/// also goes when a test PANICS — a failure that leaves 300MB behind is the last
/// thing anyone wants while working out why it failed. Removal is best-effort:
/// a directory another process still holds open on Windows is not this test's
/// to fail over.
struct ScratchCrate {
    root: PathBuf,
}

impl Drop for ScratchCrate {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn cargo_run(scratch: &ScratchCrate) -> Output {
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .arg("run")
        .arg("--quiet")
        .current_dir(&scratch.root)
        .env("CARGO_TARGET_DIR", scratch.root.join("target"))
        .output()
        .expect("cargo failed to spawn")
}

fn report(out: &Output) -> String {
    format!(
        "--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The one fact-reading helper: the value of a `KEY:value` line the program
/// printed, or a panic naming the lines that are missing.
fn fact<'a>(stdout: &'a str, key: &str) -> &'a str {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix(key))
        .map(str::trim)
        .unwrap_or_else(|| {
            panic!(
                "the generated program printed no `{key}` line, so it never got that far\n{stdout}"
            )
        })
}

// ---------------------------------------------------------------------------
// The test
// ---------------------------------------------------------------------------

/// A `.vx` file declaring `@keydown` focuses the input it named, and the field
/// it did not name is left alone.
///
/// Four things are asserted, and each one is a link that can break on its own
/// while the program still runs:
///
/// 1. **The generated handler ran, with the key name.** `LAST_KEY:F2` and
///    `PRESSES:1`. Zero presses means the `on:keydown` prop never reached the
///    tree or the dispatch arm was keyed on the wrong string — the inert case,
///    and the one that is invisible in every source-level assertion.
/// 2. **Focus landed on the NAMED field, not the first one.**
///    `FOCUSED_PATH` has to equal `COMPOSER_PATH`, `FOCUSED_INDEX` has to equal
///    `COMPOSER_IDX`, and the decoy's index has to be a different one. The decoy
///    is the FIRST input in the tree, so a name resolved to a paint-order index
///    focuses the wrong field and still leaves a focused input behind — which is
///    why the paths are compared and not just the flags.
/// 3. **Ordinary typing still edits.** `TEXT_AFTER_TYPE:draft!` with the
///    handler bound. A `@keydown` dispatch that consumed the key, or that ran
///    before the editor and disturbed focus, would break this and nothing else.
/// 4. **Esc still blurs.** `ANY_FOCUSED_AFTER_ESC:false` with
///    `ESC_CHANGED_TEXT:false` — Esc leaves the field AND leaves the text alone,
///    so it is not dispatched to `on:input` and a controlled input does not
///    fight the renderer.
#[test]
fn f2_in_a_vx_file_focuses_the_input_it_named_and_typing_and_esc_still_work() {
    let scratch = scaffold();
    let out = cargo_run(&scratch);
    assert!(
        out.status.success(),
        "the generated module did not build or run, so none of the links from \
         `.vx` to a focused field can be claimed to work\n{}",
        report(&out)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();

    // --- the fixture is what the assertions assume --------------------------
    assert_eq!(
        fact(&stdout, "INPUTS_COLLECTED:"),
        "2",
        "the fixture declares two text inputs, so a target count other than 2 \
         means the layout or the collection changed underneath this test and the \
         name-vs-index distinction it depends on is no longer being made.\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "PLAN_HANDLERS:"),
        "on_key",
        "the generated tree carries no `@keydown` binding, or carries one whose \
         handler name is not the one the fixture wrote.\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "PLAN_FOCUS_PATHS:"),
        "1",
        "exactly one element declares `data-focus-on=\"F2\"`, so the plan must \
         name one path. Zero means the attribute did not reach the tree; two \
         means `data-focus-on` is being read as a wildcard.\n{stdout}"
    );

    // --- link 1: the generated handler ran, with the key name ---------------
    assert_eq!(
        fact(&stdout, "LAST_KEY:"),
        "F2",
        "the `@keydown` handler in the generated `State` did not receive the key \
         name. The binding compiled, the prop reached the tree, and the dispatch \
         reached nothing — the inert case.\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "PRESSES:"),
        "1",
        "F2 was delivered to the handler other than once. A walk that revisits a \
         node, or a tree where the same binding is registered twice, would pass \
         the `LAST_KEY` assertion while firing an author handler per keypress.\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "REPAINT:"),
        "true",
        "F2 moved focus and called a handler, so the caller must repaint. \
         `false` would leave the focus ring and the caret one frame behind.\n{stdout}"
    );

    // --- link 2: the NAME chose the field -----------------------------------
    let composer_path = fact(&stdout, "COMPOSER_PATH:").to_string();
    let decoy_path = fact(&stdout, "DECOY_PATH:").to_string();
    assert_ne!(
        composer_path, decoy_path,
        "the two `data-focus-id`s resolved to the same path, so this fixture can \
         no longer tell the named field from the first one.\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "FOCUSED_PATH:"),
        composer_path,
        "focus did not land on the field `data-focus-id=\"composer\"` names. F2 \
         moved focus somewhere else — most likely to the FIRST input, which is \
         the decoy, which is what resolving the name to a paint-order index \
         looks like.\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "ANY_FOCUSED:"),
        "true",
        "F2 granted no focus at all.\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "COUNT_FOCUSED:"),
        "1",
        "focus is single-valued: the editor sends a keystroke to whichever \
         focused target comes first in the vector, so two focused fields means \
         typing goes into the one the author did not ask for.\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "FOCUSED_INDEX:"),
        fact(&stdout, "COMPOSER_IDX:"),
        "the focused INDEX and the index of the named field disagree, so the two \
         ways of asking the same question have drifted.\n{stdout}"
    );
    assert_ne!(
        fact(&stdout, "COMPOSER_IDX:"),
        fact(&stdout, "DECOY_IDX:"),
        "both fields resolved to the same target index.\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "CURSOR:"),
        "5",
        "a freshly focused field puts the caret at the end of its value, so the \
         next keystroke appends. \"draft\" is 5 chars.\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "ANCHOR:"),
        "none",
        "focus gain leaves no selection behind, or a stale highlight is painted \
         on a field nobody selected.\n{stdout}"
    );

    // --- link 3: ordinary typing still edits --------------------------------
    assert_eq!(
        fact(&stdout, "TYPED_INTO:"),
        composer_path,
        "the keystroke was applied to a different field than the one F2 focused, \
         so the caret and the focus flag disagree.\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "TEXT_AFTER_TYPE:"),
        "draft!",
        "typing did not reach the generated `State`. A `@keydown` binding is \
         additive — it may grant focus and call handlers, never consume the key \
         — so a `!` pressed with this handler present must still insert.\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "INPUTS_DISPATCHED:"),
        "1",
        "the typed value was not dispatched through the generated `on:input` \
         arm exactly once.\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "CURSOR_AFTER_TYPE:"),
        "6",
        "the caret did not advance past the inserted char, so the next keystroke \
         would overwrite it.\n{stdout}"
    );

    // --- link 4: Esc still blurs, and still is not an edit ------------------
    assert_eq!(
        fact(&stdout, "ESC_REPAINT:"),
        "true",
        "Esc took away the focus ring and the caret, which are painted, so the \
         loop must redraw.\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "ESC_CHANGED_TEXT:"),
        "false",
        "Esc produced a new value. Esc ends the editing session; it is not an \
         edit, and dispatching it to `on:input` would make a controlled input \
         fight the renderer over text the user did not change.\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "ANY_FOCUSED_AFTER_ESC:"),
        "false",
        "Esc did not release focus, so the field cannot be left with the \
         keyboard.\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "TEXT_AFTER_ESC:"),
        "draft!",
        "Esc changed the text.\n{stdout}"
    );
}

/// The same key, the same `data-focus-id`, in a `v-for` list whose order the app
/// itself changed — and it still finds the same field.
///
/// This is the case a numeric index cannot be trusted with, so it is the case the
/// naming scheme has to survive. The composer is the LAST of three fields before
/// the reorder and the FIRST after, so its target index goes `2` to `0` and its
/// tree path goes `[0.2]` to `[0.0]`. Both facts are asserted, because if either
/// stayed put the fixture would not be testing anything: an implementation that
/// remembered an index, or a path, would pass a test whose inputs never moved.
///
/// The fact that would fail such an implementation is the last one: in the
/// reordered document, `IDX0_ID` is `composer`, so "focus the field at index 0"
/// would have picked the right one *by accident*, and the assertion on the
/// document BEFORE the reorder is what rules that out.
#[test]
fn a_focus_id_survives_the_list_around_it_being_reordered() {
    let scratch = scaffold();
    let out = cargo_run(&scratch);
    assert!(
        out.status.success(),
        "the generated modules did not build or run, so the reorder case cannot \
         be claimed to work\n{}",
        report(&out)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();

    assert_eq!(fact(&stdout, "before.TOTAL:"), "3");
    assert_eq!(fact(&stdout, "after.TOTAL:"), "3");

    // The composer is the last of three before, the first after: the index really
    // did move, and by the whole length of the list.
    assert_eq!(
        fact(&stdout, "before.COMPOSER_IDX:"),
        "2",
        "the fixture has to start with the composer LAST for this to be a \
         reorder test at all\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "after.COMPOSER_IDX:"),
        "0",
        "reversing the list must move the composer to index 0\n{stdout}"
    );
    assert_ne!(
        fact(&stdout, "before.COMPOSER_PATH:"),
        fact(&stdout, "after.COMPOSER_PATH:"),
        "the tree path must have moved too, or the fixture did not reorder \
         anything\n{stdout}"
    );

    // So the index an author would have written is the wrong one, twice over.
    assert_eq!(
        fact(&stdout, "before.IDX0_ID:"),
        "a",
        "index 0 in the first document is the field named `a`\n{stdout}"
    );
    assert_eq!(
        fact(&stdout, "after.IDX0_ID:"),
        "composer",
        "index 0 in the reordered document is the composer — so an index would \
         have been right by accident AFTER the reorder and wrong BEFORE it, which \
         is the worse way to be wrong\n{stdout}"
    );

    // And the name found the composer both times, and only the composer.
    for label in ["before", "after"] {
        assert_eq!(
            fact(&stdout, &format!("{label}.GRANTS:")),
            "1",
            "{label}: exactly one field in the list claims F2, so the plan must \
             name exactly one path\n{stdout}"
        );
        assert_eq!(
            fact(&stdout, &format!("{label}.HANDLERS:")),
            "on_key",
            "{label}: the generated tree lost its `@keydown` binding\n{stdout}"
        );
        assert_eq!(
            fact(&stdout, &format!("{label}.FOCUSED_PATH:")),
            fact(&stdout, &format!("{label}.COMPOSER_PATH:")),
            "{label}: F2 did not focus the field `data-focus-id=\"composer\"` names"
        );
        assert_eq!(
            fact(&stdout, &format!("{label}.FOCUSED_IDX:")),
            fact(&stdout, &format!("{label}.COMPOSER_IDX:")),
            "{label}: the focused target and the named field disagree"
        );
        assert_eq!(
            fact(&stdout, &format!("{label}.LAST_KEY:")),
            "F2",
            "{label}: the generated handler did not receive the key name\n{stdout}"
        );
    }
}
