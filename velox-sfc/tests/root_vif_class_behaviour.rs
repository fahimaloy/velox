//! Behavioural proof for R-3: a `v-if` and a `:class` re-evaluate, and a static
//! `class` merges with a dynamic one.
//!
//! Both placements are proved, because they are different code paths: a
//! directive on the template's single root element (`ROOT_IF_TEMPLATE`,
//! `ROOT_SHOW_TEMPLATE`) and a directive on a nested element inside it
//! (`IF_TEMPLATE`). A `v-if` on an element inside a `v-for` body is a third
//! path again, and is proved too.
//!
//! These are VALUE-level claims — does the element appear, does the class list
//! change, does the colour the cascade produces change — and they are proved by
//! driving the real generated code against the real renderer. A test that only
//! reads the generated TEXT cannot see any of it, which is how the defect this
//! task fixes survived a passing suite: the code it generated was well-formed
//! and simply did nothing.
//!
//! How the proof works, and what it can and cannot see:
//!
//! * The codegen output is written to a crate under `/tmp` together with a probe
//!   program that builds the tree, rasterizes it with `render_vnode_to_rgba` —
//!   which runs `apply_with_cascade` and `compute_layout`, so the pixels are the
//!   cascade's output, not the template's — and prints one `key=value`
//!   measurement per line. The assertions live here in the test, against those
//!   numbers, so a failure names the claim that broke.
//! * Every count is over the whole frame, so a count of zero only means something
//!   if the colour belongs to exactly one element. Each colour below does.
//! * The crate is compiled once per run and every test reads the same
//!   measurements, so two tests cannot disagree about one number.
//!
//! What it cannot see: this crate is `velox-sfc`, so the proof covers what
//! codegen emits. The four goldens in `tests/testdata/` contain no `v-if` and no
//! `:class`, so a green golden run says nothing about this task.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use velox_sfc::RenderMode;
use velox_sfc::template_codegen::compile_template_to_rs_full_with_mode;

/// A `v-if` on a NESTED element whose condition is a `State` getter: the element
/// appears and disappears with the condition. `v-show` is on the same code path
/// and is proved here too, because leaving it out would be a single-case patch.
/// The third element is a `v-if` on an element inside a `v-for` body, where the
/// condition reads the loop item as a field.
const IF_TEMPLATE: &str = r#"<template>
  <div class="root">
    <div class="if-box" v-if="visible">IF</div>
    <div class="show-box" v-show="wide">SHOW</div>
    <div class="user-box" v-if="user.name">UN</div>
    <ul class="list">
      <li class="li-box" v-for="(todo, idx) in todos" :key="todo.id">
        <span class="li-inner" v-if="todo.keep">{{ todo.text }}</span>
      </li>
    </ul>
  </div>
</template>"#;

const IF_STYLE: &str = r#"
.if-box { background: #ff0000; width: 120px; height: 40px; }
.show-box { background: #00ff00; width: 120px; height: 40px; }
.li-inner { background: #0000ff; width: 120px; height: 20px; }
.user-box { background: #ffff00; width: 60px; height: 20px; }
"#;

const IF_SCRIPT: &str = r#"
use velox_core::ergonomics::Ref;

#[derive(Clone)]
pub struct Item {
    pub id: i32,
    pub text: String,
    pub keep: bool,
}

#[derive(Clone)]
pub struct User {
    name: String,
}

impl User {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
    pub fn name(&self) -> String {
        self.name.clone()
    }
}

pub struct State {
    visible: Ref<bool>,
    wide: Ref<bool>,
    user: Ref<User>,
    pub todos: Ref<Vec<Item>>,
}

impl State {
    pub fn new() -> Self {
        Self {
            visible: velox_core::r#ref!(false),
            wide: velox_core::r#ref!(false),
            user: velox_core::r#ref!(User::new("")),
            todos: velox_core::r#ref!(vec![Item {
                id: 1,
                text: "a".to_string(),
                keep: false,
            }]),
        }
    }

    pub fn visible(&self) -> bool {
        self.visible.get()
    }
    pub fn wide(&self) -> bool {
        self.wide.get()
    }
    pub fn todos(&self) -> Vec<Item> {
        self.todos.get()
    }
    pub fn user(&self) -> User {
        self.user.get()
    }

    pub fn set_visible(&self, value: bool) {
        self.visible.set(value);
    }
    pub fn set_wide(&self, value: bool) {
        self.wide.set(value);
    }
    pub fn set_user_name(&self, value: &str) {
        self.user.set(User::new(value));
    }
    pub fn set_keep(&self, value: bool) {
        self.todos.update(|mut items| {
            for item in items.iter_mut() {
                item.keep = value;
            }
            items
        });
    }
}
"#;

/// A static `class` and a dynamic `:class` on the same element, in both the
/// object form and the string form. The two elements are told apart by the word
/// in their class list, not by their position, so the probe does not depend on
/// document order.
const CLASS_TEMPLATE: &str = r#"<template>
  <div class="root">
    <div class="card" :class="{ active: visible, 'is-wide': wide }">MERGED</div>
    <p class="card2" :class="extra">STRING</p>
  </div>
</template>"#;

const CLASS_STYLE: &str = r#"
.card { background: #00ff00; width: 200px; height: 60px; }
.active { background: #0000ff; }
.is-wide { background: #ffff00; }
.card2 { background: #00ffff; width: 200px; height: 60px; }
.on { background: #ff00ff; }
"#;

const CLASS_SCRIPT: &str = r#"
use velox_core::ergonomics::Ref;

pub struct State {
    visible: Ref<bool>,
    wide: Ref<bool>,
    extra: Ref<String>,
}

impl State {
    pub fn new() -> Self {
        Self {
            visible: velox_core::r#ref!(false),
            wide: velox_core::r#ref!(false),
            extra: velox_core::r#ref!(String::new()),
        }
    }

    pub fn visible(&self) -> bool {
        self.visible.get()
    }
    pub fn wide(&self) -> bool {
        self.wide.get()
    }
    pub fn extra(&self) -> String {
        self.extra.get()
    }

    pub fn set_visible(&self, value: bool) {
        self.visible.set(value);
    }
    pub fn set_wide(&self, value: bool) {
        self.wide.set(value);
    }
    pub fn set_extra(&self, value: &str) {
        self.extra.set(value.to_string());
    }
}
"#;

/// The probe program. It measures and prints; it asserts nothing, so every
/// assertion is in the test and every failure names a claim.
/// A `v-if` on the template's single ROOT element — the placement the audit
/// called out. The element is the whole render, so it is also the whole frame:
/// when the condition is false the frame has nothing of it on it.
const ROOT_IF_TEMPLATE: &str = r#"<template>
  <div class="root-if-box" v-if="visible">RIF</div>
</template>"#;

const ROOT_IF_STYLE: &str = r#"
.root-if-box { background: #ff00ff; width: 140px; height: 50px; }
"#;

const ROOT_IF_SCRIPT: &str = r#"
use velox_core::ergonomics::Ref;

pub struct State {
    visible: Ref<bool>,
}

impl State {
    pub fn new() -> Self {
        Self {
            visible: velox_core::r#ref!(false),
        }
    }
    pub fn visible(&self) -> bool {
        self.visible.get()
    }
    pub fn set_visible(&self, v: bool) {
        self.visible.set(v);
    }
}
"#;

/// A `v-show` on the template's single ROOT element. It is the same
/// `rewrite_if_expr` path as `v-if`, and it was never compilable: all four
/// `v-show` emit sites bound the element to an immutable `__node` and then took
/// `ref mut props` from it, which is `error[E0596]`.
const ROOT_SHOW_TEMPLATE: &str = r#"<template>
  <div class="root-show-box" v-show="wide">RSW</div>
</template>"#;

const ROOT_SHOW_STYLE: &str = r#"
.root-show-box { background: #00ffff; width: 140px; height: 50px; }
"#;

const ROOT_SHOW_SCRIPT: &str = r#"
use velox_core::ergonomics::Ref;

pub struct State {
    wide: Ref<bool>,
}

impl State {
    pub fn new() -> Self {
        Self {
            wide: velox_core::r#ref!(false),
        }
    }
    pub fn wide(&self) -> bool {
        self.wide.get()
    }
    pub fn set_wide(&self, v: bool) {
        self.wide.set(v);
    }
}
"#;

/// A `v-if` / `v-else-if` chain, and a compound condition whose second operand is
/// a boolean.
///
/// Both are here for the same reason: each was measured as a place a condition
/// is read as something it is not. `v-else-if` goes through the same
/// `rewrite_if_expr` path a `v-if` does, so its operand is a resolver read like
/// any other and an unregistered one answers `""` — always falsy, so the
/// `v-else-if` element never renders. The compound condition puts a comparison
/// and a boolean in the same expression: only the comparison's operand may be
/// read as a number, and the boolean has to decide the render on its own. If it
/// were read as a number the expression would not compile (`f64 && f64`), and if
/// it were read as a truthiness the element would appear whenever the first
/// operand alone holds — so the pixels distinguish the three cases.
const CHAIN_TEMPLATE: &str = r#"<template>
  <div class="chain-wrap">
    <p class="first-box" v-if="first">FIRST</p>
    <p class="alt-box" v-else-if="alt">ALT</p>
    <p class="both-box" v-if="count > 0 && both">BOTH</p>
  </div>
</template>"#;

const CHAIN_STYLE: &str = r#"
.chain-wrap { display: flex; flex-direction: column; }
.first-box { background: #ff0000; width: 140px; height: 20px; }
.alt-box { background: #0000ff; width: 140px; height: 20px; }
.both-box { background: #00ff00; width: 140px; height: 20px; }
"#;

const CHAIN_SCRIPT: &str = r#"
use velox_core::ergonomics::Ref;

pub struct State {
    first: Ref<bool>,
    alt: Ref<bool>,
    both: Ref<bool>,
    count: Ref<i32>,
}

impl State {
    pub fn new() -> Self {
        Self {
            first: velox_core::r#ref!(false),
            alt: velox_core::r#ref!(false),
            both: velox_core::r#ref!(false),
            count: velox_core::r#ref!(0),
        }
    }
    pub fn first(&self) -> bool {
        self.first.get()
    }
    pub fn set_first(&self, v: bool) {
        self.first.set(v);
    }
    pub fn alt(&self) -> bool {
        self.alt.get()
    }
    pub fn set_alt(&self, v: bool) {
        self.alt.set(v);
    }
    pub fn both(&self) -> bool {
        self.both.get()
    }
    pub fn set_both(&self, v: bool) {
        self.both.set(v);
    }
    pub fn count(&self) -> i32 {
        self.count.get()
    }
    pub fn set_count(&self, v: i32) {
        self.count.set(v);
    }
}
"#;

const PROBE_MAIN: &str = r#"
mod chain_case;
mod class_case;
mod if_case;
mod root_if_case;
mod root_show_case;

use velox_dom::VNode;

/// The pixels of one exact colour in the whole frame.
fn count(px: &[u8], want: [u8; 3]) -> usize {
    px.chunks_exact(4)
        .filter(|p| p[0] == want[0] && p[1] == want[1] && p[2] == want[2])
        .count()
}

/// The class attribute of the first element whose class list contains `word`,
/// matched a whole word at a time so `card` never matches `card2`.
fn class_with_word(node: &VNode, word: &str) -> String {
    if let VNode::Element { props, .. } = node
        && let Some(class) = props.attrs.get("class")
        && class.split_whitespace().any(|c| c == word)
    {
        return class.clone();
    }
    if let VNode::Element { children, .. } = node {
        for child in children {
            let found = class_with_word(child, word);
            if !found.is_empty() {
                return found;
            }
        }
    }
    String::new()
}

fn render_if(state: &std::sync::Arc<if_case::app::script_rs::State>) -> Vec<u8> {
    let vnode = if_case::app::render_with_state(
        std::sync::Arc::clone(state),
        if_case::app::make_resolve(std::sync::Arc::clone(state)),
    );
    velox_renderer::render_vnode_to_rgba(
        &vnode,
        &velox_style::Stylesheet::parse(if_case::app::STYLE),
        400,
        300,
    )
    .expect("raster the if_case tree")
}

fn render_root_if(state: &std::sync::Arc<root_if_case::app::script_rs::State>) -> Vec<u8> {
    let vnode = root_if_case::app::render_with_state(
        std::sync::Arc::clone(state),
        root_if_case::app::make_resolve(std::sync::Arc::clone(state)),
    );
    velox_renderer::render_vnode_to_rgba(
        &vnode,
        &velox_style::Stylesheet::parse(root_if_case::app::STYLE),
        400,
        300,
    )
    .expect("raster the root_if_case tree")
}

fn render_root_show(state: &std::sync::Arc<root_show_case::app::script_rs::State>) -> Vec<u8> {
    let vnode = root_show_case::app::render_with_state(
        std::sync::Arc::clone(state),
        root_show_case::app::make_resolve(std::sync::Arc::clone(state)),
    );
    velox_renderer::render_vnode_to_rgba(
        &vnode,
        &velox_style::Stylesheet::parse(root_show_case::app::STYLE),
        400,
        300,
    )
    .expect("raster the root_show_case tree")
}

fn render_class(state: &std::sync::Arc<class_case::app::script_rs::State>) -> (Vec<u8>, String, String) {
    let vnode = class_case::app::render_with_state(
        std::sync::Arc::clone(state),
        class_case::app::make_resolve(std::sync::Arc::clone(state)),
    );
    let px = velox_renderer::render_vnode_to_rgba(
        &vnode,
        &velox_style::Stylesheet::parse(class_case::app::STYLE),
        400,
        300,
    )
    .expect("raster the class_case tree");
    (
        px,
        class_with_word(&vnode, "card"),
        class_with_word(&vnode, "card2"),
    )
}

fn render_chain(state: &std::sync::Arc<chain_case::app::script_rs::State>) -> Vec<u8> {
    let vnode = chain_case::app::render_with_state(
        std::sync::Arc::clone(state),
        chain_case::app::make_resolve(std::sync::Arc::clone(state)),
    );
    velox_renderer::render_vnode_to_rgba(
        &vnode,
        &velox_style::Stylesheet::parse(chain_case::app::STYLE),
        400,
        300,
    )
    .expect("raster the chain_case tree")
}

fn main() {
    const RED: [u8; 3] = [255, 0, 0];
    const GREEN: [u8; 3] = [0, 255, 0];
    const BLUE: [u8; 3] = [0, 0, 255];
    const YELLOW: [u8; 3] = [255, 255, 0];
    const CYAN: [u8; 3] = [0, 255, 255];
    const MAGENTA: [u8; 3] = [255, 0, 255];

    // ---- v-if, v-show, and a v-if inside a v-for body ----
    let state = std::sync::Arc::new(if_case::app::script_rs::State::new());
    let px = render_if(&state);
    println!("if.off.red={}", count(&px, RED));
    println!("if.off.blue={}", count(&px, BLUE));

    state.set_visible(true);
    println!("if.on.red={}", count(&render_if(&state), RED));

    state.set_visible(false);
    state.set_wide(false);
    println!("show.off.green={}", count(&render_if(&state), GREEN));
    state.set_wide(true);
    println!("show.on.green={}", count(&render_if(&state), GREEN));

    state.set_keep(true);
    println!("loop.on.blue={}", count(&render_if(&state), BLUE));
    state.set_keep(false);
    println!("loop.off.blue={}", count(&render_if(&state), BLUE));

    // ---- a DOTTED condition: one key for the whole path ----
    let state = std::sync::Arc::clone(&state);
    state.set_user_name("");
    println!("user.off.yellow={}", count(&render_if(&state), YELLOW));
    state.set_user_name("ada");
    println!("user.on.yellow={}", count(&render_if(&state), YELLOW));
    state.set_user_name("");

    // ---- the same two directives on the template's single root element ----
    let ifstate = std::sync::Arc::new(root_if_case::app::script_rs::State::new());
    println!(
        "rootif.off.magenta={}",
        count(&render_root_if(&ifstate), MAGENTA)
    );
    ifstate.set_visible(true);
    println!(
        "rootif.on.magenta={}",
        count(&render_root_if(&ifstate), MAGENTA)
    );

    let showstate = std::sync::Arc::new(root_show_case::app::script_rs::State::new());
    println!(
        "rootshow.off.cyan={}",
        count(&render_root_show(&showstate), CYAN)
    );
    showstate.set_wide(true);
    println!(
        "rootshow.on.cyan={}",
        count(&render_root_show(&showstate), CYAN)
    );

    // ---- a v-if / v-else-if chain, and a compound condition ----
    let chstate = std::sync::Arc::new(chain_case::app::script_rs::State::new());
    // first=false, alt=false: neither branch of the chain is painted.
    let px = render_chain(&chstate);
    println!("chain.off.first.red={}", count(&px, RED));
    println!("chain.off.alt.blue={}", count(&px, BLUE));

    // alt=true with first=false: the v-else-if element is the one that appears.
    chstate.set_alt(true);
    let px = render_chain(&chstate);
    println!("chain.alt.alt.blue={}", count(&px, BLUE));
    println!("chain.alt.first.red={}", count(&px, RED));

    // first=true: the v-if branch wins and the v-else-if element is not painted,
    // which is the chain's semantics and also says it is really a chain.
    chstate.set_first(true);
    let px = render_chain(&chstate);
    println!("chain.firstfirst.alt.blue={}", count(&px, BLUE));
    println!("chain.firstfirst.first.red={}", count(&px, RED));

    // The compound condition, one operand at a time. `both` is a boolean in a
    // logic position with a comparison elsewhere in the expression: the green box
    // appears only when BOTH operands hold, so the boolean operand decides the
    // render itself rather than riding on the comparison's operand.
    chstate.set_first(false);
    chstate.set_alt(false);
    chstate.set_both(true);
    chstate.set_count(0);
    println!(
        "chain.count0.bothtrue.green={}",
        count(&render_chain(&chstate), GREEN)
    );
    chstate.set_count(2);
    println!(
        "chain.count2.bothtrue.green={}",
        count(&render_chain(&chstate), GREEN)
    );
    chstate.set_both(false);
    println!(
        "chain.count2.bothfalse.green={}",
        count(&render_chain(&chstate), GREEN)
    );

    // ---- a static class and a dynamic one on the same element ----
    let cstate = std::sync::Arc::new(class_case::app::script_rs::State::new());
    let (px, card, card2) = render_class(&cstate);
    println!("class.off.card={card}");
    println!("class.off.card2={card2}");
    println!("class.off.green={}", count(&px, GREEN));
    println!("class.off.blue={}", count(&px, BLUE));
    println!("class.off.cyan={}", count(&px, CYAN));

    cstate.set_visible(true);
    let (px, card, _) = render_class(&cstate);
    println!("class.active.card={card}");
    println!("class.active.green={}", count(&px, GREEN));
    println!("class.active.blue={}", count(&px, BLUE));

    cstate.set_visible(false);
    cstate.set_wide(true);
    let (px, card, _) = render_class(&cstate);
    println!("class.wide.card={card}");
    println!("class.wide.green={}", count(&px, GREEN));
    println!("class.wide.yellow={}", count(&px, YELLOW));

    cstate.set_wide(false);
    cstate.set_extra("on off");
    let (px, _, card2) = render_class(&cstate);
    println!("class.string.card2={card2}");
    println!("class.string.cyan={}", count(&px, CYAN));
    println!("class.string.magenta={}", count(&px, MAGENTA));
}
"#;

fn proof_root() -> PathBuf {
    std::env::var("VELOX_R3_PROOF_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("velox-r3-proof"))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("velox-sfc lives inside the workspace")
        .to_path_buf()
}

fn generated_app(template: &str, script: &str) -> String {
    compile_template_to_rs_full_with_mode(
        template,
        "App",
        None,
        Some(script),
        None,
        RenderMode::State,
    )
    .expect("the template compiles")
}

fn write_case(root: &Path, name: &str, template: &str, style: &str, script: &str) {
    let dir = root.join("src").join(name);
    std::fs::create_dir_all(&dir).expect("create the case directory");
    std::fs::write(dir.join("app.rs"), generated_app(template, script))
        .expect("write the generated app");
    std::fs::write(dir.join("script_rs.rs"), script).expect("write the script");
    // The shape `velox-sfc::codegen` produces for a component, which is what
    // `compile_template_to_rs_full_with_mode` is one half of: the `app` module,
    // with the script inline inside it under `app::script_rs`. Every `include!`
    // is relative to this file, so `app.rs` and `script_rs.rs` are its siblings.
    std::fs::write(
        dir.join("mod.rs"),
        format!(
            "pub mod app {{\n    pub mod script_rs {{ include!(\"script_rs.rs\"); }}\n    \
             include!(\"app.rs\");\n    pub const STYLE: &str = r#\"{style}\"#;\n}}\n"
        ),
    )
    .expect("write the case module");
}

fn write_crate(root: &Path) {
    let workspace = workspace_root();
    let manifest = format!(
        r#"[package]
name = "r3-proof"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[[bin]]
name = "r3-proof"
path = "src/main.rs"

[dependencies]
velox-core = {{ path = "{core}" }}
velox-dom = {{ path = "{dom}" }}
velox-style = {{ path = "{style}" }}
velox-renderer = {{ path = "{renderer}", features = ["skia-native"] }}
"#,
        core = workspace.join("velox-core").display(),
        dom = workspace.join("velox-dom").display(),
        style = workspace.join("velox-style").display(),
        renderer = workspace.join("velox-renderer").display(),
    );
    std::fs::write(root.join("Cargo.toml"), manifest).expect("write the manifest");
    std::fs::write(root.join("src").join("main.rs"), PROBE_MAIN).expect("write the probe");
}

/// The probe's stdout, after compiling it. A compile failure is a test failure:
/// the generated code not building is the first thing this task has to rule out.
fn probe() -> &'static str {
    static ONCE: OnceLock<String> = OnceLock::new();
    ONCE.get_or_init(|| {
        let root = proof_root();
        // Only the sources are rewritten. The target directory is kept, because
        // it holds the Skia backend and rebuilding it dominates the run.
        let _ = std::fs::remove_dir_all(root.join("src"));
        std::fs::create_dir_all(root.join("src")).expect("create the proof crate");
        write_crate(&root);
        write_case(&root, "if_case", IF_TEMPLATE, IF_STYLE, IF_SCRIPT);
        write_case(
            &root,
            "root_if_case",
            ROOT_IF_TEMPLATE,
            ROOT_IF_STYLE,
            ROOT_IF_SCRIPT,
        );
        write_case(
            &root,
            "root_show_case",
            ROOT_SHOW_TEMPLATE,
            ROOT_SHOW_STYLE,
            ROOT_SHOW_SCRIPT,
        );
        write_case(
            &root,
            "class_case",
            CLASS_TEMPLATE,
            CLASS_STYLE,
            CLASS_SCRIPT,
        );
        write_case(
            &root,
            "chain_case",
            CHAIN_TEMPLATE,
            CHAIN_STYLE,
            CHAIN_SCRIPT,
        );
        let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .current_dir(&root)
            .args(["run", "--quiet", "--offline"])
            .env("CARGO_TARGET_DIR", root.join("target"))
            .output()
            .expect("run the probe crate");
        assert!(
            out.status.success(),
            "the probe crate did not run (status {:?})\n--- stdout ---\n{}\n--- stderr ---\n{}",
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    })
}

/// One `key=value` measurement from the probe.
fn measurement(key: &str) -> String {
    let out = probe();
    let prefix = format!("{key}=");
    for line in out.lines() {
        if let Some(value) = line.strip_prefix(&prefix) {
            return value.to_string();
        }
    }
    panic!("the probe printed no `{key}=` measurement\n--- its output ---\n{out}");
}

fn pixels(key: &str) -> usize {
    let raw = measurement(key);
    raw.parse()
        .unwrap_or_else(|e| panic!("`{key}` is not a pixel count: {e}"))
}

/// A count above this means the element really is painted: each colour below
/// fills a box of at least 120x20 logical pixels. Every assertion that wants
/// "painted" is paired with one that wants "not painted" for the same colour, so a
/// threshold this low cannot paper over an element that merely shrank.
const PAINTED: usize = 500;

#[test]
#[ignore = "slow: compiles a generated crate with the Skia backend"]
fn a_v_if_on_the_root_element_paints_it_only_while_its_condition_holds() {
    assert_eq!(
        pixels("rootif.off.magenta"),
        0,
        "a `v-if` on the root element whose condition is false must not render it: \
         the element is the whole render, so nothing of it may be painted"
    );
    assert!(
        pixels("rootif.on.magenta") > PAINTED,
        "a `v-if` on the root element must paint it as soon as its condition \
         becomes true: {} magenta pixels",
        pixels("rootif.on.magenta")
    );
}

#[test]
#[ignore = "slow: compiles a generated crate with the Skia backend"]
fn a_v_show_on_the_root_element_hides_and_reveals_it_with_its_condition() {
    assert_eq!(
        pixels("rootshow.off.cyan"),
        0,
        "a `v-show` on the root element whose condition is false must render it \
         hidden, so nothing of it is painted"
    );
    assert!(
        pixels("rootshow.on.cyan") > PAINTED,
        "a `v-show` on the root element must reveal it as soon as its condition \
         becomes true: {} cyan pixels",
        pixels("rootshow.on.cyan")
    );
}

#[test]
#[ignore = "slow: compiles a generated crate with the Skia backend"]
fn a_v_if_on_a_nested_element_paints_it_only_while_its_condition_holds() {
    assert_eq!(
        pixels("if.off.red"),
        0,
        "a `v-if` whose condition is false must not render its element at all"
    );
    assert!(
        pixels("if.on.red") > PAINTED,
        "a `v-if` whose condition became true must paint its element: {} red pixels",
        pixels("if.on.red")
    );
}

#[test]
#[ignore = "slow: compiles a generated crate with the Skia backend"]
fn a_v_show_on_a_nested_element_hides_and_reveals_it_with_its_condition() {
    assert_eq!(
        pixels("show.off.green"),
        0,
        "a `v-show` whose condition is false must render its element hidden, so \
         nothing of it is painted"
    );
    assert!(
        pixels("show.on.green") > PAINTED,
        "a `v-show` whose condition became true must paint its element: {} green pixels",
        pixels("show.on.green")
    );
}

/// A dotted condition is one lookup of one value. Before this task it was two
/// lookups with a `.` between them, which does not compile at all.
#[test]
#[ignore = "slow: compiles a generated crate with the Skia backend"]
fn a_dotted_condition_re_evaluates_as_one_value() {
    assert_eq!(
        pixels("user.off.yellow"),
        0,
        "a dotted condition over an empty value must not render its element: {} \
         yellow pixels",
        pixels("user.off.yellow")
    );
    assert!(
        pixels("user.on.yellow") > PAINTED / 10,
        "a dotted condition must render its element as soon as the path holds a \
         value: {} yellow pixels",
        pixels("user.on.yellow")
    );
}

#[test]
#[ignore = "slow: compiles a generated crate with the Skia backend"]
fn a_v_if_on_an_element_inside_a_v_for_body_follows_the_loop_item() {
    assert_eq!(
        pixels("loop.off.blue"),
        0,
        "a `v-if` inside a `v-for` body must not render while the loop item's \
         condition is false"
    );
    assert!(
        pixels("loop.on.blue") > PAINTED,
        "a `v-if` inside a `v-for` body must render when the loop item's condition \
         becomes true: {} blue pixels",
        pixels("loop.on.blue")
    );
}

#[test]
#[ignore = "slow: compiles a generated crate with the Skia backend"]
fn a_static_class_merges_with_an_object_syntax_class() {
    assert_eq!(
        measurement("class.off.card"),
        "card",
        "with no condition true the class list is the static class alone and \
         nothing else — an empty entry or a trailing space would break the \
         cascade's whole-word match"
    );
    assert_eq!(
        measurement("class.active.card"),
        "card active",
        "Vue merges a static class with a dynamic one: the class list is both, in \
         that order"
    );
    assert_eq!(
        measurement("class.wide.card"),
        "card is-wide",
        "a quoted object key names the class without its quotes"
    );
}

#[test]
#[ignore = "slow: compiles a generated crate with the Skia backend"]
fn the_cascade_paints_the_merged_class_list() {
    assert!(
        pixels("class.off.green") > PAINTED,
        "the static class must reach the cascade: {} green pixels",
        pixels("class.off.green")
    );
    assert_eq!(
        pixels("class.off.blue"),
        0,
        "a false `:class` condition must contribute no class name"
    );
    assert_eq!(
        pixels("class.active.green"),
        0,
        "when the dynamic class applies, the later `.active` rule must win over \
         `.card`, which is only true if the class list holds BOTH"
    );
    assert!(
        pixels("class.active.blue") > PAINTED,
        "a true `:class` condition must add a class the cascade applies: {} blue pixels",
        pixels("class.active.blue")
    );
    assert!(
        pixels("class.wide.yellow") > PAINTED,
        "a quoted object key must name a class the cascade can match: {} yellow pixels",
        pixels("class.wide.yellow")
    );
}

#[test]
#[ignore = "slow: compiles a generated crate with the Skia backend"]
fn a_string_syntax_class_joins_the_static_one() {
    assert_eq!(
        measurement("class.string.card2"),
        "card2 on off",
        "a `:class` holding class NAMES joins the static class the same way an \
         object form does, one entry per name"
    );
    assert_eq!(
        pixels("class.string.cyan"),
        0,
        "with a dynamic name present the later `.on` rule must win over `.card2`, \
         which is only true if the class list holds both"
    );
    assert!(
        pixels("class.string.magenta") > PAINTED,
        "a name from a string `:class` must reach the cascade: {} magenta pixels",
        pixels("class.string.magenta")
    );
}

/// A `v-else-if` element appears when its own condition holds and the `v-if`
/// branch does not, and it is not painted when the `v-if` branch does. The
/// operand is a resolver read on the same path a `v-if`'s is, so registering it
/// is what makes the element render at all.
#[test]
#[ignore = "slow: compiles a generated crate with the Skia backend"]
fn a_v_else_if_element_paints_only_while_the_chain_reaches_it() {
    assert_eq!(
        pixels("chain.off.first.red"),
        0,
        "with `first` false the `v-if` element must not be painted"
    );
    assert_eq!(
        pixels("chain.off.alt.blue"),
        0,
        "with `alt` false too, no branch of the chain may be painted: {} blue \
         pixels",
        pixels("chain.off.alt.blue")
    );
    assert!(
        pixels("chain.alt.alt.blue") > PAINTED,
        "`alt` alone must reach the `v-else-if` element: {} blue pixels",
        pixels("chain.alt.alt.blue")
    );
    assert_eq!(
        pixels("chain.alt.first.red"),
        0,
        "the `v-else-if` element must not appear while the `v-if` branch is the \
         one that holds"
    );
    assert_eq!(
        pixels("chain.firstfirst.alt.blue"),
        0,
        "a `v-if` that holds takes the chain, so the `v-else-if` element must not \
         also be painted: {} blue pixels",
        pixels("chain.firstfirst.alt.blue")
    );
    assert!(
        pixels("chain.firstfirst.first.red") > PAINTED,
        "the `v-if` element must be painted when its own condition holds: {} red \
         pixels",
        pixels("chain.firstfirst.first.red")
    );
}

/// `count > 0 && both` puts a comparison and a boolean in one expression, and
/// `both` decides the render on its own. A boolean that were read as a number
/// would either not compile (`f64 && f64`) or ride on the comparison's operand,
/// so these three measurements are what distinguishes the two.
#[test]
#[ignore = "slow: compiles a generated crate with the Skia backend"]
fn a_compound_condition_re_evaluates_both_of_its_operands() {
    assert_eq!(
        pixels("chain.count0.bothtrue.green"),
        0,
        "the comparison's operand is false, so the compound condition must be \
         false however `both` is set: {} green pixels",
        pixels("chain.count0.bothtrue.green")
    );
    assert!(
        pixels("chain.count2.bothtrue.green") > PAINTED,
        "with both operands true the compound condition must paint its element: \
         {} green pixels",
        pixels("chain.count2.bothtrue.green")
    );
    assert_eq!(
        pixels("chain.count2.bothfalse.green"),
        0,
        "the boolean operand decides the compound condition itself — it is not \
         read as a number and not carried by the comparison's operand: {} green \
         pixels",
        pixels("chain.count2.bothfalse.green")
    );
}
