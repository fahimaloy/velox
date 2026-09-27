//! Tests for the typed `Props` channel a parent hands a child.
//!
//! Every fixture module in this file is GENERATED, committed, and re-checked
//! against the compiler at the top of [`fixtures_are_current`]. The point of
//! committing generated code rather than shelling out to a second `cargo` is
//! that these tests are not `#[ignore]`d: they run in-process, in under a
//! second, on every `cargo test`. The generated halves are the output of
//! `to_stub_rs_unwrapped` and `compile_template_to_rs_full_with_mode`, joined in
//! the same order the CLI joins them (stub, then render functions), so they are
//! byte-for-byte what an app build would `include!`.
//!
//! The assertions are on the returned `velox_dom::VNode` — never on the text of
//! the generated source. Checking the source would only prove the compiler
//! emitted the string it was told to emit.

use velox_sfc::{
    ComponentResolver, RenderMode, compile_template_to_rs_full_with_mode, parse_sfc,
    to_stub_rs_unwrapped,
};

// ---------------------------------------------------------------------------
// The `.vx` inputs these fixtures are compiled from.
//
// These are inputs, not expectations: nothing here states what the rendered
// output should be. Every expected value in the tests below is a literal that
// a human read off the input, or a value the test itself put in.
// ---------------------------------------------------------------------------

const ROWS_PROPS_CHILD: &str = r#"
<script setup>
use std::rc::Rc;
use velox_core::signal::Signal;

#[derive(Clone)]
pub struct Row {
    pub label: String,
}

pub struct Props {
    pub rows: Vec<Row>,
}

pub struct State {
    pub props: Props,
    // Every component emits a `render_with_state` body whether or not a parent
    // ever calls it, and that body reads its collection out of `State`, so a
    // component that loops over a `Props` collection still has to declare a
    // `State` field of that name. Only `render_with_props` is driven by the
    // tests below, and there the collection comes from `Props`.
    pub rows: Rc<Signal<Vec<Row>>>,
}

impl State {
    pub fn new() -> Self {
        State {
            props: Props { rows: Vec::new() },
            rows: Rc::new(Signal::new(Vec::new())),
        }
    }

    pub fn heading(&self) -> String {
        String::from("rows")
    }
}
</script>

<template>
<section class="rows-view">
<p class="heading">{{ heading }}</p>
<ul class="rows">
  <li v-for="(row, idx) in rows" :key="row.label">{{ row.label }}@{{ idx }}</li>
</ul>
</section>
</template>
"#;

const ROWS_STATE_CHILD: &str = r#"
<script setup>
use std::rc::Rc;
use velox_core::signal::Signal;

#[derive(Clone)]
pub struct Row {
    pub label: String,
}

pub struct State {
    pub rows: Rc<Signal<Vec<Row>>>,
}

impl State {
    pub fn new() -> Self {
        State { rows: Rc::new(Signal::new(Vec::new())) }
    }

    pub fn heading(&self) -> String {
        String::from("rows")
    }
}
</script>

<template>
<section class="rows-view">
<p class="heading">{{ heading }}</p>
<ul class="rows">
  <li v-for="(row, idx) in rows" :key="row.label">{{ row.label }}@{{ idx }}</li>
</ul>
</section>
</template>
"#;

const PROPS_CHILD: &str = r#"
<script setup>
pub struct Props {
    pub greeting: String,
    pub count: usize,
}

pub struct State {
    pub props: Props,
}

impl State {
    pub fn new() -> Self {
        State { props: Props { greeting: String::new(), count: 0 } }
    }

    pub fn echoed(&self) -> String {
        self.props.greeting.clone()
    }

    pub fn counted(&self) -> String {
        self.props.count.to_string()
    }
}
</script>

<template>
<p class="greeting">{{ echoed }}|{{ counted }}</p>
</template>
"#;

const LIST_PARENT: &str = r#"
<script setup>
import PropsChild from './props_child.vx'

pub struct State {
    pub who_value: String,
    pub count_value: String,
}

impl State {
    pub fn new() -> Self {
        State { who_value: String::new(), count_value: String::new() }
    }

    pub fn who(&self) -> String {
        self.who_value.clone()
    }

    pub fn how_many(&self) -> String {
        self.count_value.clone()
    }
}
</script>

<template>
<div class="parent">
  <PropsChild :greeting="who" :count="how_many" />
</div>
</template>
"#;

/// A component whose `State` declares a collection the template never names, so
/// the script is present and `State` is otherwise well-formed. The collection
/// these loops reach for is deliberately NOT declared anywhere: it is the
/// absence the refusals below are about, and the presence of a script is what
/// makes an absence a mistake rather than the caller's business.
const STATE_WITHOUT_THE_COLLECTION: &str = r#"
pub struct Todo {
    pub text: String,
}

pub struct State {
    pub count: i32,
}

impl State {
    pub fn new() -> Self {
        State { count: 0 }
    }

    pub fn count(&self) -> i32 {
        self.count
    }
}
"#;

/// A component that declares `rows` as a `Props` field of a type a `v-for`
/// cannot iterate. The field is REAL and declared, so this is the other
/// absence: not a name nothing supplies, but a name supplied in the wrong shape.
const PROPS_ROWS_OF_THE_WRONG_SHAPE: &str = r#"
pub struct Props {
    pub rows: String,
}

pub struct State {
    pub props: Props,
}

impl State {
    pub fn new() -> Self {
        State { props: Props { rows: String::new() } }
    }
}
"#;

/// A component that holds the collection as a `State` field, which is the
/// third way a loop can be given something to count and the one with no
/// `Props` and no getter involved.
const STATE_FIELD_COLLECTION: &str = r#"
pub struct Todo {
    pub text: String,
}

pub struct State {
    pub todos: std::rc::Rc<velox_core::signal::Signal<Vec<Todo>>>,
}

impl State {
    pub fn new() -> Self {
        State { todos: std::rc::Rc::new(velox_core::signal::Signal::new(Vec::new())) }
    }
}
"#;

/// Compile a bare template in `Resolve` mode and hand back the compiler's own
/// verdict, unchanged.
///
/// `Resolve` is the mode the refusals below are raised in, and it is the mode
/// where the defect is SILENT: the resolver is asked for the collection, answers
/// an empty string, the count is zero and the body never runs, with nothing in
/// the output to show for it. `State` mode emits `state.<name>.get()` for the
/// same template instead, and `rustc` rejects that outright, so a hole there is
/// already loud without help from here. See the loop-family gate in
/// `collect_resolver_keys`.
fn resolve_mode_verdict(template: &str, script_setup: &str) -> Result<String, String> {
    compile_template_to_rs_full_with_mode(
        template,
        "App",
        None,
        Some(script_setup),
        None,
        RenderMode::Resolve,
    )
}

// ---------------------------------------------------------------------------
// The generated modules, exactly as a build would emit them.
// ---------------------------------------------------------------------------

// `#[rustfmt::skip]` on each of these is load-bearing, not tidiness: the modules
// are compiler output, byte for byte, and `fixtures_are_current` below fails if
// anything reformats them. They cannot live outside the module tree either, so
// the one thing that could reformat them is turned off here.
#[rustfmt::skip]
#[path = "fixtures/list_parent.rs"]
pub mod list_parent;
#[rustfmt::skip]
#[path = "fixtures/props_child.rs"]
pub mod props_child;
#[rustfmt::skip]
#[path = "fixtures/rows_props_child.rs"]
pub mod rows_props_child;
#[rustfmt::skip]
#[path = "fixtures/rows_state_child.rs"]
pub mod rows_state_child;

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

/// What a build writes into a child's module file, in the order the CLI joins
/// it: the stub, then the render half. `velox-cli/src/commands/build.rs` does
/// exactly this concatenation for a child component, and the root is the same
/// two halves wrapped in a `pub mod` — the wrapper adds nothing the tests here
/// need, so the root is generated the same way.
///
/// The one line that is not generated is [`PARENT_ALIAS`]: the CLI declares
/// each imported component as a submodule of the component that imports it,
/// and a `pub mod` cannot be declared inside an `include!`d half, so the parent
/// reaches the child the short way round instead — the child is a module of
/// this test crate and the parent aliases it by path. That is the same alias
/// the generated code below resolves, so the parent is exercising a real
/// cross-component `Props` handoff rather than a test-local shortcut.
const PARENT_ALIAS: &str = "use super::props_child as PropsChild;";

/// Compile one `.vx` source into the module file a build would write.
fn generate(source: &str, name: &str, base: &std::path::Path) -> String {
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
    if name == "list_parent" {
        module.push('\n');
        module.push_str(PARENT_ALIAS);
        module.push('\n');
    }
    module.push_str("\n\n");
    module.push_str(&render);
    module
}

/// Write the `.vx` files the resolvers need to see into a temporary directory.
fn vx_tree() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("velox-props-collection-vx");
    std::fs::create_dir_all(&dir).expect("create temp vx dir");
    for (file, body) in [
        ("props_child.vx", PROPS_CHILD),
        ("rows_props_child.vx", ROWS_PROPS_CHILD),
        ("rows_state_child.vx", ROWS_STATE_CHILD),
        ("list_parent.vx", LIST_PARENT),
    ] {
        std::fs::write(dir.join(file), body).expect("write vx file");
    }
    dir
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(FIXTURE_DIR).join(format!("{name}.rs"))
}

fn fixture_sources() -> Vec<(&'static str, &'static str)> {
    vec![
        ("props_child", PROPS_CHILD),
        ("rows_props_child", ROWS_PROPS_CHILD),
        ("rows_state_child", ROWS_STATE_CHILD),
        ("list_parent", LIST_PARENT),
    ]
}

#[test]
fn fixtures_are_current() {
    let base = vx_tree();
    let mut stale = Vec::new();
    for (name, source) in fixture_sources() {
        let generated = generate(source, name, &base);
        let path = fixture_path(name);
        let Ok(committed) = std::fs::read_to_string(&path) else {
            panic!(
                "{} is missing, so the tests below cannot run at all",
                path.display()
            );
        };
        if committed != generated {
            stale.push(format!(
                "{} differs from what the compiler produces now",
                path.display()
            ));
        }
    }
    assert!(
        stale.is_empty(),
        "the committed generated fixtures are stale, so every test below would be \
         asserting on code the compiler no longer emits:\n  {}\n\
         (run the fixture writer to update them)",
        stale.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Reading a rendered tree back out.
// ---------------------------------------------------------------------------

/// Every text leaf under `node`, in document order.
fn text_leaves(node: &velox_dom::VNode, out: &mut Vec<String>) {
    match node {
        velox_dom::VNode::Text(t) => out.push(t.clone()),
        velox_dom::VNode::Element { children, .. } => {
            for child in children {
                text_leaves(child, out);
            }
        }
    }
}

/// The text of every `<li>` under `node`, in document order.
fn list_item_texts(node: &velox_dom::VNode) -> Vec<String> {
    let mut found = Vec::new();
    collect_li_texts(node, &mut found);
    found
}

/// The first element with `tag` under `node`.
fn find_element<'a>(node: &'a velox_dom::VNode, tag: &str) -> Option<&'a velox_dom::VNode> {
    if let velox_dom::VNode::Element {
        tag: t, children, ..
    } = node
    {
        if t == tag {
            return Some(node);
        }
        for child in children {
            if let Some(found) = find_element(child, tag) {
                return Some(found);
            }
        }
    }
    None
}

fn collect_li_texts(node: &velox_dom::VNode, out: &mut Vec<String>) {
    if let velox_dom::VNode::Element { tag, children, .. } = node
        && tag == "li"
    {
        let mut leaves = Vec::new();
        for child in children {
            text_leaves(child, &mut leaves);
        }
        out.push(leaves.concat());
        return;
    }
    if let velox_dom::VNode::Element { children, .. } = node {
        for child in children {
            collect_li_texts(child, out);
        }
    }
}

/// Every text leaf of `node` joined into one string.
fn all_text(node: &velox_dom::VNode) -> String {
    let mut leaves = Vec::new();
    text_leaves(node, &mut leaves);
    leaves.concat()
}

fn rows(labels: &[&str]) -> Vec<rows_props_child::script_rs::Row> {
    labels
        .iter()
        .map(|l| rows_props_child::script_rs::Row {
            label: String::from(*l),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Requirement 3: the typed `v-for` collection, at 2, 3 and 0 rows.
// ---------------------------------------------------------------------------

#[test]
fn a_props_v_for_renders_one_child_per_row() {
    for count in [2usize, 3] {
        let node = rows_props_child::render_with_props(rows_props_child::PropsArg {
            rows: rows(&["a", "b", "c"][..count]),
        });
        let expected: Vec<String> = ["a", "b", "c"][..count]
            .iter()
            .enumerate()
            .map(|(i, l)| format!("{l}@{i}"))
            .collect();
        assert_eq!(
            list_item_texts(&node),
            expected,
            "with {count} rows bound, the loop must produce one <li> per row, in order, \
             each reading its OWN row's label"
        );
    }
}

#[test]
fn a_props_v_for_over_an_empty_collection_renders_no_children() {
    let node = rows_props_child::render_with_props(rows_props_child::PropsArg { rows: rows(&[]) });
    assert_eq!(
        list_item_texts(&node),
        Vec::<String>::new(),
        "an empty collection must produce an <ul> with no <li> in it"
    );
    // The list itself still has to be there: "no children" is not "no element".
    // A build that rendered nothing at all would satisfy the count above too.
    let list = find_element(&node, "ul").unwrap_or_else(|| {
        panic!("the <ul> must still be rendered, just with no <li> in it: {node:?}")
    });
    assert!(
        !list_item_texts(list).iter().any(|t| t.contains('@')),
        "with no rows bound the <ul> must hold no loop output: {list:?}"
    );
}

/// The same `.vx` template, compiled into a `State`-mode component, must agree
/// with the props-mode one row for row.
///
/// This is the differential half: the State arm is the one that already worked,
/// so it is the oracle. Nothing below re-implements its behaviour — it runs the
/// same compiled template through the other code path and compares the trees.
// A component's `State` holds `Rc<Signal<..>>`, so the `Arc` a generated entry
// point takes is never `Send`. The generated entry points allow that themselves;
// this test constructs the same `Arc` on its own, so it needs the same allowance.
#[allow(clippy::arc_with_non_send_sync)]
#[test]
fn the_state_mode_arm_renders_the_same_rows_as_the_props_channel() {
    for count in [0usize, 2, 3] {
        let labels = ["a", "b", "c"][..count].to_vec();
        let shared: Vec<String> = labels
            .iter()
            .enumerate()
            .map(|(i, l)| format!("{l}@{i}"))
            .collect();

        let props_node = rows_props_child::render_with_props(rows_props_child::PropsArg {
            rows: labels
                .iter()
                .map(|l| rows_props_child::script_rs::Row {
                    label: String::from(*l),
                })
                .collect(),
        });

        let state = std::sync::Arc::new(rows_state_child::script_rs::State::new());
        state.rows.set(
            labels
                .iter()
                .map(|l| rows_state_child::script_rs::Row {
                    label: String::from(*l),
                })
                .collect(),
        );
        let state_node = rows_state_child::render_with_state(
            std::sync::Arc::clone(&state),
            rows_state_child::make_resolve(std::sync::Arc::clone(&state)),
        );

        assert_eq!(
            list_item_texts(&props_node),
            shared,
            "with {count} rows bound, the props channel must render each row's own label"
        );
        assert_eq!(
            list_item_texts(&state_node),
            shared,
            "with {count} rows in State, the arm that already worked must render the same thing"
        );
        assert_eq!(
            list_item_texts(&props_node),
            list_item_texts(&state_node),
            "with {count} rows, the two ways of handing a collection across the \
             component boundary must render the same tree"
        );
    }
}

// ---------------------------------------------------------------------------
// Requirement 2: the child's OWN code can read the prop it was given.
// ---------------------------------------------------------------------------

#[test]
fn a_child_script_method_reads_the_prop_the_parent_bound() {
    let node = props_child::render_with_props(props_child::PropsArg {
        greeting: String::from("hello"),
        count: 3,
    });
    assert_eq!(
        all_text(&node).replace(' ', ""),
        "hello|3",
        "the child's `echoed()` and `counted()` read `self.props`, so this can only \
         come out right if the props reached the child's own State"
    );
}

#[test]
fn a_parent_binding_reaches_a_child_script_method() {
    let node = list_parent::render_with_props(list_parent::PropsArg {
        values: std::collections::HashMap::from([
            (String::from("who"), String::from("hey")),
            (String::from("how_many"), String::from("7")),
        ]),
    });
    assert_eq!(
        all_text(&node).replace(' ', ""),
        "hey|7",
        "the parent binds `who` and `how_many`; the child must receive them and read \
         them back through its own methods"
    );
}

// ---------------------------------------------------------------------------
// Requirement 1: a parent supplies every field the child declares.
// ---------------------------------------------------------------------------

/// The generated parent is compiled against the child's declared `Props`, so a
/// binding that missed a field would not compile. That is the whole contract —
/// this test pins which fields the contract covers, and that a child which
/// declares no props at all still takes a binding.
/// The generated parent is compiled against the child's declared `Props`, so a
/// binding that missed a field would not compile. That is the whole contract, and
/// it is not observable from a rendered tree: what it guarantees is a *build*
/// failure naming the missing field, which is checked by
/// `a_parent_binding_missing_a_declared_field_does_not_compile` in
/// `tests/compile_failures.rs`.
///
/// What this test adds is the obligation the contract carries — the parent's
/// binding has to cover every declared field, and the value that comes out the
/// other end has to be the one that was bound. The count is read off the child's
/// own `Props` declaration rather than written here twice, so adding a field to
/// the child without covering it in the parent fails this test.
#[test]
fn a_parent_binding_covers_every_declared_props_field() {
    let node = list_parent::render_with_props(list_parent::PropsArg {
        values: std::collections::HashMap::from([
            (String::from("who"), String::from("hey")),
            (String::from("how_many"), String::from("7")),
        ]),
    });
    assert_eq!(
        all_text(&node).replace(' ', ""),
        "hey|7",
        "`list_parent` binds `greeting` and `count`; the child must receive both, in the \
         order it declares them"
    );

    let declared: Vec<String> = velox_sfc::script_index::extract_props_fields(PROPS_CHILD)
        .into_iter()
        .map(|f| f.name)
        .collect();
    assert_eq!(
        declared,
        vec!["greeting".to_string(), "count".to_string()],
        "the child's declared Props fields, and the two the parent binds. If this \
         assertion fails the declarations and the bindings have drifted apart, and the \
         totals the contract rests on have to be re-checked."
    );
}

// ---------------------------------------------------------------------------
// Refusing to generate a `v-for` whose collection nothing can supply.
//
// These four tests are the negative half of the collection story the tests above
// tell the positive half of, and they observe the compiler's OWN `Err` from the
// real entry point. Nothing here re-derives the rule being checked: each one
// hands the compiler a component and reads back what it decided, so a test that
// reimplements the gate could not pass these.
// ---------------------------------------------------------------------------

/// A `v-for` over a collection this component's own declaration does not contain
/// must REFUSE, not warn.
///
/// The script is present, so it is the whole inventory of what the component
/// has, and `todos` is in none of it. Nothing can answer the resolver for it, the
/// count is zero, and the body never runs — a list-shaped hole in the rendered
/// tree with nothing in the output to show for it. A warning on stderr does not
/// stop that from shipping; the `Err` does.
#[test]
fn a_v_for_over_a_collection_the_component_does_not_declare_is_refused() {
    let verdict = resolve_mode_verdict(
        r#"<ul><li v-for="todo in todos">{{ todo.text }}</li></ul>"#,
        STATE_WITHOUT_THE_COLLECTION,
    );
    let Err(refusal) = verdict else {
        panic!(
            "a `v-for` over a collection this component does not declare must be refused, \
             and a component that renders no list at all must not compile. It compiled."
        );
    };
    assert!(
        refusal.contains("todos"),
        "the refusal must name the collection it cannot supply, so the author knows \
         which loop to fix. The message was:\n{refusal}"
    );
    for remedy in ["pub struct Props", "getter", "render_with_state"] {
        assert!(
            refusal.contains(remedy),
            "the refusal must name every way to fix it: declare `todos` in \
             `pub struct Props` as a `Vec<T>`, give it a zero-argument `State` \
             getter, or render through `render_with_state`. `{remedy}` is missing \
             from:\n{refusal}"
        );
    }
}

/// A collection that IS declared, in a shape a `v-for` cannot iterate, is a
/// different mistake from one that is not declared at all, and the refusal has
/// to tell them apart.
///
/// Here `rows` is a real `Props` field of type `String`. The author wrote a name
/// and got the shape wrong, so the message has to quote the type they wrote —
/// reporting this as "this component does not declare `rows`" would send them
/// looking for a missing declaration that is right there in their own file.
#[test]
fn a_refusal_for_a_declared_but_uniterable_field_names_the_type_that_was_written() {
    let verdict = resolve_mode_verdict(
        r#"<ul><li v-for="row in rows">{{ row }}</li></ul>"#,
        PROPS_ROWS_OF_THE_WRONG_SHAPE,
    );
    let Err(refusal) = verdict else {
        panic!(
            "a `v-for` over a `Props` field of type `String` must be refused; the \
             generated `for` cannot read it. It compiled."
        );
    };
    assert!(
        refusal.contains("String"),
        "the refusal must quote the declared type the author wrote, or they have no \
         way to see which of their own declarations is wrong. The message was:\n{refusal}"
    );
    assert!(
        !refusal.contains("does not declare"),
        "`rows` IS declared — as a `String`. Reporting it as undeclared would name \
         the wrong mistake. The message was:\n{refusal}"
    );
}

/// Holding the collection as a `State` field is the third way a loop gets
/// something to count, and a component that does it is NOT broken.
///
/// `render_with_state` reads `state.todos.get()` directly, so the loop runs. That
/// is a component that renders, so it must compile — the refusal is for names
/// nothing holds, and this name is held. What it earns instead is the warning
/// that names which renderer reads the field and which one is left with nothing.
#[test]
fn a_collection_held_as_a_state_field_is_not_refused() {
    let verdict = resolve_mode_verdict(
        r#"<ul><li v-for="todo in todos">{{ todo.text }}</li></ul>"#,
        STATE_FIELD_COLLECTION,
    );
    assert!(
        verdict.is_ok(),
        "a component that holds the collection as a `State` field renders its loop \
         through `render_with_state`, so it must still compile. It was refused with:\n{:?}",
        verdict.err()
    );
}

/// Every unrenderable collection is listed, not just the first.
///
/// Two loops, two independent mistakes in the same template, and the author who
/// is told about one and builds again meets the other a build later. The crate's
/// existing fatal path for template validation already reports its whole list for
/// this reason, so this is the crate's convention rather than a new one.
#[test]
fn every_unrenderable_collection_is_listed_not_only_the_first() {
    let verdict = resolve_mode_verdict(
        r#"<div><ul><li v-for="todo in todos">{{ todo.text }}</li></ul>\
           <ul><li v-for="task in tasks">{{ task.title }}</li></ul></div>"#,
        STATE_WITHOUT_THE_COLLECTION,
    );
    let Err(refusal) = verdict else {
        panic!("two collections this component does not declare must be refused together");
    };
    for collection in ["todos", "tasks"] {
        assert!(
            refusal.contains(collection),
            "the refusal must list EVERY unrenderable collection, not stop at the \
             first. `{collection}` is missing from:\n{refusal}"
        );
    }
    assert_eq!(
        refusal.matches("cannot render").count(),
        2,
        "exactly one refusal per unrenderable collection, so a count assertion can \
         tell 'both are named' from 'one is named twice'. The message was:\n{refusal}"
    );
}

/// A component with no `<script setup>` at all is never refused.
///
/// With no script there is no inventory, so there is nothing to hold an absence
/// against: every name in a script-less `render_with` template is the caller's
/// to bind through the `resolve` closure, which is what such a template is for.
/// Refusing one would refuse a legitimate component on a rule that cannot
/// distinguish it from a typo.
#[test]
fn a_script_less_component_is_never_refused_for_its_collection() {
    let verdict = compile_template_to_rs_full_with_mode(
        r#"<ul><li v-for="item in items">{{ item.name }}</li></ul>"#,
        "App",
        None,
        None,
        None,
        RenderMode::Resolve,
    );
    assert!(
        verdict.is_ok(),
        "a script-less component binds its collection through the caller's \
         `resolve` closure, so codegen has no inventory to call the name missing \
         and must not refuse. It was refused with:\n{:?}",
        verdict.err()
    );
}
