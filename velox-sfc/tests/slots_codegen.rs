//! The named-slot codegen: how a parent's children are grouped, flattened, and
//! keyed, and how a child's `<slot>` outlet looks its key up.
//!
//! # Why this file exists
//!
//! `slot_binding`, `strip_slot_binding`, `slot_content` and `slots_map_expr` are
//! ~150 lines that turn a `<template v-slot:x>` fragment into one `HashMap`
//! entry. The only test that mentions `v-slot` at all was
//! `tests/slot_grammar.rs`, and its own header says it "stop[s] at `parse_sfc`,
//! before any of the codegen, because […] everything downstream of that is
//! unreachable" — true of the failure it was written for, and the reason
//! nothing below `parse_sfc` was ever checked.
//!
//! The shape it needs is the one that is easy to get wrong and hard to notice:
//! a fragment has to be FLATTENED (a `<template>` is not an element the renderer
//! knows, so leaving the wrapper in renders an inert `<template>` node with the
//! caller's content inside it), the `v-slot:` attribute has to come OFF (it
//! names the slot, it is not an attribute of the content, and left on it is an
//! unknown directive the emitter has no handler for), and several children
//! naming one slot have to land in ONE entry rather than one entry each.
//!
//! # The two halves, and the asymmetry between them
//!
//! A slot name is written twice, on opposite sides of the boundary. The parent
//! half is a directive, so the parser folds it — `v-slot:footerBar` becomes
//! `slot:footer-bar` and the map key is `footer-bar`. The child half is a plain
//! `name="…"` attribute, so nothing folded it, and the two never met. The
//! symptom was not an error: `render_slot` found no entry and rendered the
//! outlet's FALLBACK, so a component that was handed a footer quietly showed its
//! own instead. `a_camel_case_slot_name_meets_the_camel_case_name_it_was_given`
//! and the running test at the bottom are the two ends of that.
//!
//! # Why the running test is here and not in a source assertion
//!
//! A `HashMap::from([("footer-bar", …)])` on one side and a `render_slot("…")`
//! on the other agree or they do not, and a string assertion on either side
//! alone cannot tell a key from a lookup. The running test renders a real parent
//! into a real child and reads back which of the two texts came out.

use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Output};

use velox_sfc::compile_template_to_rs;

/// The `HashMap` a parent hands a child, as generated, for a component tag
/// whose children name slots.
fn slots_map_for(template: &str) -> String {
    let rs = compile_template_to_rs(template, "ParentApp", None)
        .unwrap_or_else(|e| panic!("template did not compile: {e}\nfor:\n{template}"));
    // The map is the argument named `__slots`. Everything the grouping and keying
    // decisions affect is inside it, and nothing else in the render is.
    let start = rs
        .find("__slots = ")
        .unwrap_or_else(|| panic!("no slots map was emitted for:\n{template}\ngenerated:\n{rs}"))
        + "__slots = ".len();
    // Not "up to the next `;`": the map's VALUE is a block of statements ending
    // in `__children`, so the first `;` is inside it. Bracket-matched instead,
    // which is what makes this the map rather than its first line.
    let open = rs[start..]
        .find('[')
        .unwrap_or_else(|| panic!("no `HashMap::from([…])` in the slots map for:\n{template}"))
        + start;
    let bytes = rs.as_bytes();
    let mut depth = 0usize;
    let mut end = None;
    for (i, &byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end.expect("the slots map literal is closed");
    rs[start..=end].to_string()
}

// ---------------------------------------------------------------------------
// Grouping
// ---------------------------------------------------------------------------

/// Two children naming the SAME slot land in ONE map entry.
///
/// One entry, and the value is a `<slot>` wrapper around both — which is the
/// shape a multi-child default slot already has. Two entries would mean the
/// second `HashMap::from` pair overwrote the first (a `collect` into a map does),
/// and the caller would silently lose content it was told to provide.
#[test]
fn several_children_naming_one_slot_become_one_entry() {
    let map = slots_map_for(
        r#"<div data-velox-component="Card">
  <p v-slot:footer>first</p>
  <p v-slot:footer>second</p>
</div>"#,
    );
    assert_eq!(
        map.matches("\"footer\"").count(),
        1,
        "two children named `footer`, so the map must have ONE `footer` key — \
         a second one would overwrite the first and drop content the caller \
         provided. The map was:\n{map}"
    );
    assert!(
        map.contains("first") && map.contains("second"),
        "both children must be in the entry, not just the first. The map was:\n{map}"
    );
    assert!(
        map.contains("h(\"slot\""),
        "two VNodes under one name are wrapped in a `<slot>` element, which is the \
         shape a multi-child default slot already has. The map was:\n{map}"
    );
}

/// A child that names NO slot is the default slot, under the key `"default"` —
/// and it is a real entry in the same map, not a separate code path.
///
/// "Not in the map" would render nothing at all for the most common case in the
/// framework: a child with a `<slot>` and a parent that passes it some content
/// without naming a slot.
#[test]
fn a_child_naming_no_slot_yields_the_default_key() {
    let map = slots_map_for(r#"<div data-velox-component="Card"><p>body</p></div>"#);
    assert!(
        map.contains("\"default\""),
        "a child that names no slot is the default slot, and the default slot is an \
         entry in this map like any other. The map was:\n{map}"
    );
    assert!(
        map.contains("body"),
        "the content still has to be there. The map was:\n{map}"
    );
}

/// Two DIFFERENT names are two entries. Grouping that collapsed everything into
/// one would satisfy the one-slot test above and lose the whole feature.
#[test]
fn two_different_slot_names_are_two_entries() {
    let map = slots_map_for(
        r#"<div data-velox-component="Card">
  <h1 v-slot:header>title</h1>
  <p v-slot:footer>foot</p>
</div>"#,
    );
    assert!(
        map.contains("\"header\""),
        "the `header` entry is missing. The map was:\n{map}"
    );
    assert!(
        map.contains("\"footer\""),
        "the `footer` entry is missing. The map was:\n{map}"
    );
}

// ---------------------------------------------------------------------------
// Flattening, and the attribute coming off
// ---------------------------------------------------------------------------

/// `<template v-slot:x>` contributes its CHILDREN, not itself.
///
/// `template` is not an element the renderer knows. A fragment bound through one
/// has to be flattened or it renders as an inert `<template>` node with the
/// caller's content buried inside it — the child would see an empty-looking
/// outlet, and the caller's markup would be in the tree but not where the child's
/// styles reach it.
#[test]
fn a_template_v_slot_flattens_to_its_children() {
    let map = slots_map_for(
        r#"<div data-velox-component="Card">
  <template v-slot:header><h1>title</h1><p>sub</p></template>
</div>"#,
    );
    assert!(
        !map.contains("h(\"template\""),
        "the `<template>` wrapper must not be emitted: the renderer has no \
         `template` element, so it would be an inert node around the caller's \
         content. The map was:\n{map}"
    );
    for content in ["title", "sub"] {
        assert!(
            map.contains(content),
            "`{content}` is a child of the fragment and must survive the flatten. \
             The map was:\n{map}"
        );
    }
    // Two children of one fragment, so the wrapper assertion above has a case
    // that would look the same if the fragment were simply dropped.
    assert!(
        map.contains("h(\"slot\""),
        "two children of one fragment still group under the `<slot>` wrapper. \
         The map was:\n{map}"
    );
}

/// The content's OWN attributes survive the slot pass.
///
/// `v-slot:footer` names the slot; it is not an attribute of the content, and
/// `strip_slot_binding` takes it off. Everything else on the element is markup
/// the CALLER wrote, and the child is not entitled to drop it — a pass that
/// removed the whole attribute list would still put the right text in the right
/// slot, and the caller's `class` would be gone with nothing said.
///
/// This is the half of the strip that is visible here. Whether the `slot:`
/// attribute ITSELF came off is not: `emit_props_in_loop` emits only
/// `AttrKind::Static` and `AttrKind::Bind` and ignores `AttrKind::Directive`, so
/// that half holds in the output whether or not the strip happens. It is
/// asserted on the NODE instead, by `the_slot_binding_is_stripped_and_nothing_else_is`
/// in `src/codegen_unit_tests.rs`, which is the only place it can fail.
#[test]
fn the_contents_own_attributes_survive_the_slot_pass() {
    let map = slots_map_for(
        r#"<div data-velox-component="Card">
  <p v-slot:footer class="foot" data-role="coda">text</p>
  <template v-slot:header><span>h</span></template>
</div>"#,
    );
    for attr in ["foot", "coda"] {
        assert!(
            map.contains(attr),
            "`{attr}` is an attribute the caller wrote, and the slot pass must not \
             drop it. The map was:\n{map}"
        );
    }
    assert!(
        !map.contains("\"slot:footer\""),
        "the `v-slot:footer` binding must not reach the content element as an \
         attribute. The map was:\n{map}"
    );
}

// ---------------------------------------------------------------------------
// The name both sides have to agree on
// ---------------------------------------------------------------------------

/// The PARENT half: a camelCase slot name is stored kebab-cased, because the
/// parser folds directive names and both spellings go through the same fold.
///
/// This half was already correct. It is asserted here because the fix was on the
/// other side, and a test that only pins the lookup would pass just as happily
/// if the stored key changed shape — leaving the two halves disagreeing in the
/// other direction.
#[test]
fn a_camel_case_slot_name_is_stored_kebab_cased() {
    let map = slots_map_for(
        r#"<div data-velox-component="Card">
  <template v-slot:footerBar>foot</template>
</div>"#,
    );
    assert!(
        map.contains("\"footer-bar\""),
        "a directive name is folded to kebab-case by the parser, so the key is \
         `footer-bar`. The map was:\n{map}"
    );
    assert!(
        !map.contains("\"footerBar\""),
        "the raw camelCase spelling must not be the key — the `#` shorthand and \
         `v-slot:` have to reach the same string. The map was:\n{map}"
    );
}

/// The `#` shorthand and `v-slot:` are the SAME slot, not two.
///
/// They are folded onto one name by the parser, so both spellings of one slot
/// land in one entry. If they drifted apart, a component whose author used `#`
/// for the header and `v-slot:` for the footer would be fine while one that used
/// both for the same slot would silently lose one of them.
#[test]
fn the_hash_shorthand_and_v_slot_reach_the_same_entry() {
    let map = slots_map_for(
        r#"<div data-velox-component="Card">
  <p v-slot:footer>long form</p>
  <p #footer>short form</p>
</div>"#,
    );
    assert_eq!(
        map.matches("\"footer\"").count(),
        1,
        "both spellings name the same slot, so there is one key. The map was:\n{map}"
    );
    assert!(
        map.contains("long form") && map.contains("short form"),
        "both fragments have to be in it. The map was:\n{map}"
    );
}

/// The CHILD half, and the asymmetry this file is mostly about: a camelCase
/// `name` on the `<slot>` outlet is folded the way the parent's key was.
///
/// Before this, the parent stored `footer-bar` and the child looked up
/// `footerBar`, so the lookup missed and the outlet rendered its FALLBACK. No
/// diagnostic, no error — a component that looked like it had decided to ignore
/// its caller. The two names are written by different people in different files,
/// so neither can be required to match the other's spelling; both are folded.
#[test]
fn a_camel_case_slot_name_meets_the_camel_case_name_it_was_given() {
    let rs =
        compile_template_to_rs(r#"<div><slot name="footerBar" /></div>"#, "Card", None).unwrap();
    assert!(
        rs.contains("render_slot(\"footer-bar\""),
        "the outlet must look itself up under the folded name, so a parent writing \
         `v-slot:footerBar` and a child writing `name=\"footerBar\"` are the same \
         slot. The generated body was:\n{rs}"
    );
}

/// An already-kebab-cased name is untouched by the fold.
///
/// Without this, a fold that lowercased or mangled a name that needed no
/// folding would still pass every test above — every one of them uses a
/// camelCase name, so none of them can see a broken fold applied to a plain one.
#[test]
fn a_kebab_case_slot_name_is_left_alone() {
    let rs =
        compile_template_to_rs(r#"<div><slot name="footer-bar" /></div>"#, "Card", None).unwrap();
    assert!(
        rs.contains("render_slot(\"footer-bar\""),
        "`footer-bar` is what the fold produces and what it must leave alone. \
         The generated body was:\n{rs}"
    );
}

/// An outlet naming nothing is `"default"`, and the default is what the parent's
/// unnamed children are stored under.
#[test]
fn an_outlet_naming_nothing_looks_up_default() {
    let rs = compile_template_to_rs(r#"<div><slot /></div>"#, "Card", None).unwrap();
    assert!(
        rs.contains("render_slot(\"default\""),
        "the unnamed outlet and the parent's unnamed children are the same slot. \
         The generated body was:\n{rs}"
    );
}

// ---------------------------------------------------------------------------
// Running: the two halves actually meeting
// ---------------------------------------------------------------------------

/// A child with a named outlet and a fallback for it.
const MODAL: &str = r#"
<script setup>
pub struct Props {
    pub title: String,
}

pub struct State {
    pub props: Props,
}

impl State {
    pub fn new() -> Self {
        State { props: Props { title: String::new() } }
    }
}
</script>

<template>
<div class="modal">
  <header><slot name="headerBar">FALLBACK_HEADER</slot></header>
  <footer><slot name="footerBar">FALLBACK_FOOTER</slot></footer>
</div>
</template>
"#;

/// A parent that passes both slots, in the camelCase spelling on the parent side
/// and the camelCase spelling on the child side.
const APP: &str = r#"
<script setup>
import Modal from './modal.vx'
pub struct State {
    pub open: bool,
}

impl State {
    pub fn new() -> Self {
        State { open: true }
    }

    pub fn t(&self) -> String {
        String::from("t")
    }
}
</script>

<template>
<Modal :title="t">
  <template v-slot:headerBar><h1>CALLER_HEADER</h1></template>
  <template v-slot:footerBar><button>CALLER_FOOTER</button></template>
</Modal>
</template>
"#;

fn generate(source: &str, name: &str, base: &Path) -> String {
    let sfc = velox_sfc::parse_sfc(source).unwrap_or_else(|e| panic!("{name}: parse failed: {e}"));
    let script_setup = sfc.script_setup.as_ref().map(|s| s.content.as_str());
    let tpl = sfc
        .template
        .as_ref()
        .map(|t| t.content.as_str())
        .unwrap_or("");
    let mut resolver = velox_sfc::ComponentResolver::new(base.to_path_buf());
    if let Some(setup) = script_setup {
        resolver.parse_imports(setup);
    }
    let render = velox_sfc::compile_template_to_rs_full_with_mode(
        tpl,
        name,
        Some(&mut resolver),
        script_setup,
        None,
        velox_sfc::RenderMode::State,
    )
    .unwrap_or_else(|e| panic!("{name}: template compilation failed: {e}"));
    let mut module = velox_sfc::to_stub_rs_unwrapped(&sfc, name, Some(base));
    module.push('\n');
    module.push_str("use super::modal as Modal;\n");
    module.push_str("\n\n");
    module.push_str(&render);
    module
}

/// A run directory nobody else is using. Tests in one binary run on parallel
/// threads, so a fixed path has two of them rewriting the same `Cargo.toml` and
/// `src/main.rs` underneath each other.
fn unique_run_dir() -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/slots-codegen")
        .join(format!("run-{unique}"))
}

/// `cargo run`, not `cargo build`: a build proves the wiring type-checks and
/// cannot say which of the two texts came out, and which text came out IS the
/// assertion.
fn cargo_run(scratch: &ScratchCrate) -> Output {
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .arg("run")
        .arg("--quiet")
        .current_dir(&scratch.root)
        .env("CARGO_TARGET_DIR", scratch.root.join("target"))
        .output()
        .expect("cargo failed to spawn")
}

/// A scratch crate that deletes itself, so a FAILED assertion does not leave a
/// few hundred megabytes of build output behind. A plain `remove_dir_all` at the
/// end of the test body runs only when every assertion above it passed — and the
/// runs that matter most are exactly the ones that fail.
struct ScratchCrate {
    root: PathBuf,
}

impl Drop for ScratchCrate {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A caller's slotted content reaches the outlet the caller named.
///
/// This is the assertion the string checks above cannot make. They pin a key on
/// one side and a lookup on the other; only rendering can say the two are the
/// same slot. The FALLBACK text is the sharp end: a lookup that misses renders
/// it, so its ABSENCE is what proves the map was found — and the caller's text
/// being present is what proves the content was not merely dropped somewhere
/// else in the tree.
#[test]
fn a_callers_slotted_content_reaches_the_outlet_it_named() {
    let scratch = ScratchCrate {
        root: unique_run_dir(),
    };
    let root = scratch.root.clone();
    let src = root.join("src");
    std::fs::create_dir_all(&src).expect("create scratch src");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("velox-sfc has a parent")
        .to_path_buf();
    let manifest = format!(
        r#"[package]
name = "slots_codegen"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[[bin]]
name = "slots_codegen"
path = "src/main.rs"

[dependencies]
velox-core = {{ path = "{}" }}
velox-dom = {{ path = "{}" }}
"#,
        workspace.join("velox-core").display(),
        workspace.join("velox-dom").display(),
    );
    std::fs::write(root.join("Cargo.toml"), manifest).expect("write manifest");
    std::fs::write(root.join("modal.vx"), MODAL).expect("write modal.vx");
    std::fs::write(root.join("app.vx"), APP).expect("write app.vx");
    std::fs::write(src.join("modal.rs"), generate(MODAL, "modal", &root))
        .expect("write generated modal module");
    std::fs::write(src.join("app.rs"), generate(APP, "app", &root))
        .expect("write generated app module");
    std::fs::write(
        src.join("main.rs"),
        r#"
mod app;
mod modal;

fn main() {
    let state = std::sync::Arc::new(app::script_rs::State::new());
    let vnode = app::render_with_state(
        std::sync::Arc::clone(&state),
        app::make_resolve(std::sync::Arc::clone(&state)),
    );
    let mut text = Vec::new();
    slots_codegen_text(&vnode, &mut text);
    println!("TEXT:{}", text.join("|"));
}

fn slots_codegen_text(node: &velox_dom::VNode, out: &mut Vec<String>) {
    match node {
        velox_dom::VNode::Text(t) => out.push(t.clone()),
        velox_dom::VNode::Element { children, .. } => {
            for c in children {
                slots_codegen_text(c, out);
            }
        }
    }
}
"#,
    )
    .expect("write main");

    let out = cargo_run(&scratch);
    let rendered = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "the generated modules did not build\n--- stdout ---\n{rendered}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !rendered.is_empty(),
        "the program printed nothing, so nothing was asserted. stdout was:\n{rendered}"
    );

    for expected in ["CALLER_HEADER", "CALLER_FOOTER"] {
        assert!(
            rendered.contains(expected),
            "the caller's `{expected}` never reached the outlet it named. stdout was:\n{rendered}"
        );
    }
    for fallback in ["FALLBACK_HEADER", "FALLBACK_FOOTER"] {
        assert!(
            !rendered.contains(fallback),
            "the outlet rendered its FALLBACK (`{fallback}`), which means the lookup \
             missed: the parent stored one spelling of the name and the child looked \
             up another, and nothing said so. stdout was:\n{rendered}"
        );
    }
}
