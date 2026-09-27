use std::fs;
use std::path::PathBuf;
use std::process::Command;

// This integration test writes generated SFC Rust to a temporary Cargo project and
// invokes `cargo build` to ensure the generated code compiles against workspace crates.
// It is ignored by default because it runs an external `cargo` build and is slow.

#[test]
#[ignore = "slow: runs external cargo build against full workspace"]
fn compile_generated_app_crate() {
    // Sample SFC with template and script setup
    let sfc_src = r#"<template>
  <div>
    <button @click="inc">Inc</button>
    <div class="count">{{ count }}</div>
  </div>
</template>
<script setup>
use std::cell::{Cell};
pub struct State { pub count: Cell<i32> }
impl State { pub fn new() -> Self { Self { count: Cell::new(0) } } pub fn inc(&self) { let v = self.count.get()+1; self.count.set(v); } }
</script>
"#;

    // Parse SFC and produce module code (stub + render functions)
    let sfc = velox_sfc::parse_sfc(sfc_src).expect("parse sfc");
    let name = "app";
    let mut module_code = velox_sfc::to_stub_rs(&sfc, name);
    let tpl_src = sfc
        .template
        .as_ref()
        .map(|t| t.content.as_str())
        .unwrap_or("");
    let render_fn = velox_sfc::compile_template_to_rs(tpl_src, name, None).expect("compile tpl");
    if let Some(pos) = module_code.rfind('}') {
        module_code.insert_str(pos, &format!("\n{}\n", render_fn));
    } else {
        module_code.push('\n');
        module_code.push_str(&render_fn);
    }

    // Create a temporary project directory
    let crate_base: PathBuf = {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // workspace root is parent of velox-sfc
        manifest_dir.parent().unwrap().to_path_buf()
    };

    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp = std::env::temp_dir().join(format!("velox_integration_{}", unique));
    let proj = tmp.join("app_crate");
    let src = proj.join("src");
    fs::create_dir_all(&src).expect("create tmp project");

    // Write Cargo.toml pointing to workspace crates by absolute path
    let cargo_toml = format!(
        r#"[package]
name = "velox_integration_test"
version = "0.1.0"
edition = "2021"

[dependencies]
velox-core = {{ path = "{}" }}
velox-dom = {{ path = "{}" }}
velox-style = {{ path = "{}" }}
velox-renderer = {{ path = "{}" }}
"#,
        crate_base.join("velox-core").display(),
        crate_base.join("velox-dom").display(),
        crate_base.join("velox-style").display(),
        crate_base.join("velox-renderer").display()
    );
    fs::write(proj.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");

    // Write generated module and main
    fs::write(src.join("app.rs"), module_code).expect("write app.rs");
    let main = r#"include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/app.rs"));
fn main() { let _ = app::render(); }
"#;
    fs::write(src.join("main.rs"), main).expect("write main.rs");

    // Run cargo build in the temp project
    let out = Command::new("cargo")
        .arg("build")
        .current_dir(&proj)
        .output()
        .expect("cargo build failed to spawn");
    if !out.status.success() {
        panic!(
            "cargo build failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
#[ignore = "slow: runs external cargo build with component imports"]
fn compile_component_import_test() {
    // Create temporary directory for component test
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp = std::env::temp_dir().join(format!("velox_component_test_{}", unique));
    fs::create_dir_all(&tmp).expect("create temp dir");

    // Create Button.vx component
    let button_src = r#"<template>
  <button :class="buttonClass" @click="onClick">
    {{ label }}
  </button>
</template>
<script setup>
pub struct State { pub label: String }
impl State { pub fn new(label: String) -> Self { Self { label } } pub fn onClick(&self) { println!("Button clicked!"); } }
</script>
<style>
button { padding: 8px 16px; background: blue; color: white; border: none; }
</style>"#;
    fs::write(tmp.join("Button.vx"), button_src).expect("write Button.vx");

    // Create App.vx that imports Button
    let app_src = r#"<template>
  <div class="app">
    <h1>My App</h1>
    <Button label="Click me!" />
  </div>
</template>
<script setup>
import Button from './Button.vx';
pub struct State { pub count: u32 }
impl State { pub fn new() -> Self { Self { count: 0 } } }
</script>
<style>
.app { padding: 20px; }
</style>"#;
    fs::write(tmp.join("App.vx"), app_src).expect("write App.vx");

    // Parse and compile App.vx with component resolver
    let sfc = velox_sfc::parse_sfc(app_src).expect("parse App SFC");
    let name = "app";

    // Create resolver with correct base path
    let mut resolver = velox_sfc::ComponentResolver::new(tmp.clone());
    if let Some(script_setup) = &sfc.script_setup {
        resolver.parse_imports(&script_setup.content);
    }

    // Compile template with component support
    let tpl_src = sfc
        .template
        .as_ref()
        .map(|t| t.content.as_str())
        .unwrap_or("");
    let render_fn = velox_sfc::compile_template_to_rs(tpl_src, name, Some(&mut resolver))
        .expect("compile template");

    // Generate stub with correct base path for import resolution
    let mut stub = velox_sfc::to_stub_rs_with_base(&sfc, name, Some(&tmp));

    // Add component module import
    if !resolver.component_names().is_empty() {
        for comp_name in resolver.component_names() {
            // Load and compile the component
            let _ = resolver.load_component(&comp_name); // This will compile Button.vx

            // Add mod declaration to stub
            let mod_line = format!("    pub mod {};\n", comp_name);
            if let Some(pos) = stub.find("pub const STYLE") {
                stub.insert_str(pos, &mod_line);
            }
        }
    }

    // Combine stub and render function
    let mut module_code = String::new();
    if let Some(pos) = stub.rfind('}') {
        module_code.push_str(&stub[..pos]);
        module_code.push('\n');
        module_code.push_str(&render_fn);
        module_code.push('\n');
        module_code.push_str(&stub[pos..]);
    } else {
        module_code.push_str(&stub);
        module_code.push('\n');
        module_code.push_str(&render_fn);
    }

    // Create temp cargo project for testing
    let proj = tmp.join("test_project");
    let src = proj.join("src");
    fs::create_dir_all(&src).expect("create test project");

    // Create Cargo.toml
    let crate_base: PathBuf = {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.parent().unwrap().to_path_buf()
    };

    let cargo_toml = format!(
        r#"[package]
name = "component_test"
version = "0.1.0"
edition = "2021"

[dependencies]
velox-core = {{ path = "{}" }}
velox-dom = {{ path = "{}" }}
velox-style = {{ path = "{}" }}
"#,
        crate_base.join("velox-core").display(),
        crate_base.join("velox-dom").display(),
        crate_base.join("velox-style").display()
    );

    fs::write(proj.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");

    // Write generated code
    fs::write(src.join("app.rs"), module_code).expect("write app.rs");
    let main_rs = r#"mod app;
use velox_dom::VNode;

fn main() {
    let vnode = app::render();
    println!("Component compiled successfully!");
}
"#;
    fs::write(src.join("main.rs"), main_rs).expect("write main.rs");

    // Run cargo build
    let out = Command::new("cargo")
        .arg("build")
        .current_dir(&proj)
        .output()
        .expect("cargo build failed");
    if !out.status.success() {
        panic!(
            "Component build failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    println!("✅ Component integration test passed!");
}

/// Requirement 3 of the R-1e brief, in the one place it can be witnessed: code
/// this crate generated, compiled, run, and observed from outside.
///
/// Every other test in `velox-sfc/tests/` reads emitted source. Reading the source
/// cannot tell you whether a `v-if` rendered its element, only whether the words
/// for a `v-if` are in the file — and two defects in this area reached review
/// because the text was pinned and the frame never was. So the harness below
/// builds a component, runs it, and reports what came out; the expectations live
/// in the test, not in the generated crate, and they come from Vue's rules.
///
/// The rows, and why each is here:
///
/// * a `String` of `"0"` under `v-if` must RENDER — in JS a non-empty string is
///   truthy whatever it spells, so `"0"` is truthy. This is the known limitation
///   R-1e records, and the row is here to make it a measurement rather than a
///   claim: `resolver_truthiness` parses the string as a number first, so `"0"`
///   is falsy here. Fixing it needs a typed channel, not a predicate.
/// * a `bool` getter under `v-if` must render for `true` and not for `false`,
///   which is the row that would catch a truthiness rule losing the distinction.
/// * a two-element `Vec` under `v-for` must produce EXACTLY two children. The
///   count, not a non-zero count: finding 1 is a Resolve-mode loop that renders
///   nothing at all, and "at least one" would pass by accident both now and after
///   R-1e-1 makes an unreachable key loud.
/// * a key with no getter must not leave a silently-empty text node. This row does
///   not pass today, and the assert below pins the measurement rather than the
///   verdict; see the comment on `nope_empty_text_node`.
///
/// The four are one fixture, because the point is that they are decided together:
/// one predicate, four routes, one frame.
const ANSWERABLE_FIXTURE: &str = r#"<template>
  <div class="app">
    <p class="zero" v-if="zero_text">zero</p>
    <p class="on" v-if="flag_on">on</p>
    <p class="off" v-if="flag_off">off</p>
    <ul class="rows">
      <li v-for="row in rows">{{ row }}</li>
    </ul>
    <p class="nope">{{ nope }}</p>
  </div>
</template>
<script setup>
use velox_core::signal::Signal;
pub struct State { pub rows: std::rc::Rc<Signal<Vec<String>>> }
impl State {
    pub fn new() -> Self {
        Self { rows: std::rc::Rc::new(Signal::new(vec![String::from("r0"), String::from("r1")])) }
    }
    pub fn zero_text(&self) -> String { String::from("0") }
    pub fn flag_on(&self) -> bool { true }
    pub fn flag_off(&self) -> bool { false }
}
</script>
"#;

/// The harness's half: it measures and prints `key=value` lines, and decides
/// nothing. Every expectation lives in the test that reads them, so the numbers
/// being compared are stated in reviewable source rather than inside a string
/// that is written to a temporary crate.
const HARNESS_MAIN: &str = r#"mod app;
use std::collections::BTreeMap;
use velox_dom::VNode;

fn walk<'a>(v: &'a VNode, out: &mut Vec<&'a VNode>) {
    out.push(v);
    if let VNode::Element { children, .. } = v {
        for child in children {
            walk(child, out);
        }
    }
}

fn has_class(v: &VNode, class: &str) -> bool {
    match v {
        VNode::Element { props, .. } => props.attrs.get("class").map(String::as_str) == Some(class),
        VNode::Text(_) => false,
    }
}

fn measure(vnode: &VNode) -> BTreeMap<String, String> {
    let mut all = Vec::new();
    walk(vnode, &mut all);
    let mut out = BTreeMap::new();
    let present = |class: &str| all.iter().any(|v| has_class(v, class));
    out.insert("zero_present".to_string(), present("zero").to_string());
    out.insert("on_present".to_string(), present("on").to_string());
    out.insert("off_present".to_string(), present("off").to_string());
    let rows = all
        .iter()
        .filter(|v| matches!(v, VNode::Element { tag, .. } if tag == "li"))
        .count();
    out.insert("li_count".to_string(), rows.to_string());
    let empty = all.iter().filter(|v| has_class(v, "nope")).any(|v| {
        matches!(v, VNode::Element { children, .. }
            if children.iter().any(|c| matches!(c, VNode::Text(t) if t.is_empty())))
    });
    out.insert("nope_empty_text_node".to_string(), empty.to_string());
    out
}

fn main() {
    // `render_with_state(state, make_resolve(state))` is the entry point the
    // renderer itself uses for a `State`-mode component, and the only one that
    // reads the component's own state: `render()` and `render_with_props()` both
    // go through the string-keyed resolver, which knows nothing about `rows`.
    let state = std::sync::Arc::new(app::script_rs::State::new());
    let vnode = app::render_with_state(
        std::sync::Arc::clone(&state),
        app::make_resolve(std::sync::Arc::clone(&state)),
    );
    for (key, value) in measure(&vnode) {
        println!("{key}={value}");
    }
    println!("tree={vnode:?}");
}
"#;

/// Compile `sfc_src`, run it, and read the measurements back. The crate is built
/// and executed in a temporary directory with `cargo run`, because the generated
/// code is a module that only exists once it has been written out; the only
/// dependencies it needs are `velox-core` and `velox-dom`, which the workspace
/// already provides by path.
fn run_generated_sfc(sfc_src: &str) -> std::collections::BTreeMap<String, String> {
    let sfc = velox_sfc::parse_sfc(sfc_src).expect("parse sfc");
    let name = "app";
    // The UNWRAPPED stub, with the render functions APPENDED rather than spliced
    // in. `to_stub_rs` emits its own `set_slots` / `set_emit_callbacks` helpers at
    // file top level and only then opens `pub mod app { … }`, so the component's
    // own `render_with_callbacks` calls those helpers with nothing in scope to
    // find them by, and the component's own functions end up at `app::app`. One
    // module from top to bottom is what the generated code assumes.
    let mut module_code = velox_sfc::to_stub_rs_unwrapped(&sfc, name, None);
    let tpl_src = sfc
        .template
        .as_ref()
        .map(|t| t.content.as_str())
        .unwrap_or("");
    let render_fn = velox_sfc::compile_template_to_rs_full_with_mode(
        tpl_src,
        name,
        None,
        sfc.script_setup
            .as_ref()
            .map(|block| block.content.as_str()),
        None,
        velox_sfc::RenderMode::State,
    )
    .expect("compile template");
    module_code.push('\n');
    module_code.push_str(&render_fn);
    module_code.push('\n');

    let crate_base: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let tmp = std::env::temp_dir().join(format!("velox_run_generated_{unique}"));
    let proj = tmp.join("app_crate");
    let src = proj.join("src");
    fs::create_dir_all(&src).expect("create tmp project");
    let cargo_toml = format!(
        r#"[package]
name = "velox_run_generated"
version = "0.1.0"
edition = "2021"

[dependencies]
velox-core = {{ path = "{}" }}
velox-dom = {{ path = "{}" }}
"#,
        crate_base.join("velox-core").display(),
        crate_base.join("velox-dom").display()
    );
    fs::write(proj.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");
    fs::write(src.join("app.rs"), &module_code).expect("write app.rs");
    fs::write(src.join("main.rs"), HARNESS_MAIN).expect("write main.rs");

    // One shared target directory: the two path dependencies are the bulk of the
    // build, and a fresh one per test would rebuild all of them every time.
    let target = std::env::temp_dir().join("velox_run_generated_target");
    fs::create_dir_all(&target).expect("create shared target dir");
    let out = Command::new("cargo")
        .args(["run", "--quiet", "--offline"])
        .env("CARGO_TARGET_DIR", &target)
        .current_dir(&proj)
        .output()
        .expect("cargo run failed to spawn");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        panic!(
            "the generated crate did not run\nstdout:\n{stdout}\nstderr:\n{}\ngenerated module:\n{module_code}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let _ = fs::remove_dir_all(&tmp);

    let mut report = std::collections::BTreeMap::new();
    for line in stdout.lines() {
        if let Some((key, value)) = line.split_once('=') {
            report.insert(key.to_string(), value.to_string());
        }
    }
    assert!(
        report.contains_key("li_count"),
        "the harness printed no measurements, so nothing was observed:\n{stdout}"
    );
    report
}

#[test]
#[ignore = "slow: compiles and runs a generated crate with an external cargo"]
fn a_generated_component_renders_what_vue_says_it_renders() {
    let report = run_generated_sfc(ANSWERABLE_FIXTURE);
    let tree = report
        .get("tree")
        .cloned()
        .unwrap_or_else(|| "<not printed>".to_string());
    let row = |key: &str| {
        report
            .get(key)
            .cloned()
            .unwrap_or_else(|| format!("<no row {key}>"))
    };

    // Each expectation comes from Vue's specification, written here and NOT read
    // back out of the crate under test, so the crate cannot agree with itself.
    //
    // ONE ROW MEASURED DIVERGENCE, named rather than papered over: Vue treats
    // every non-empty string as truthy, so `v-if` on a `String` of `"0"` renders
    // the element. `resolver_truthiness` reads a numeric string as a number
    // first, so `"0"` is falsy here. The expected value below is the one this
    // crate produces, NOT the one Vue says, and the test says so where it is
    // asserted. Unifying the answerability predicates cannot reach it: the
    // resolver's answer is a type-blind `String`, and only a typed channel fixes
    // it. Leaving the predicate as the one place the decision lives is what makes
    // that fact nameable at all.
    let rows: [(&str, &str, &str); 5] = [
        (
            "zero_present",
            "false",
            "DIVERGENCE, not a spec: Vue says `true` (a non-empty string is truthy \
             whatever it spells), and R-1e's recorded limitation says `false`, so \
             the element is absent.",
        ),
        (
            "on_present",
            "true",
            "a `bool` getter of `true` under `v-if` renders the element.",
        ),
        (
            "off_present",
            "false",
            "a `bool` getter of `false` under `v-if` does not render the element, \
             and this is the row a truthiness rule that lost the bool/string \
             distinction would fail.",
        ),
        (
            "li_count",
            "2",
            "a two-element `Vec` under `v-for` produces exactly two children. Not \
             \"at least one\": a loop that produced nothing would pass a non-zero \
             check, which is finding 1.",
        ),
        (
            "nope_empty_text_node",
            "true",
            "the unresolvable interpolation leaves a silently-empty text node. Vue \
             leaves nothing, so the spec for this row is `false` and the code does \
             not meet it yet; pinning `true` means the day it stops doing it, this \
             fails and names the change. It is NOT a verdict that `Text(\"\")` is \
             acceptable — the catch-all that answers it is out of scope here.",
        ),
    ];
    for (key, expected, why) in rows {
        assert_eq!(
            &row(key),
            expected,
            "{key} is `{expected}`, and the row below says why. rendered tree: {tree}\n{why}"
        );
    }
}
