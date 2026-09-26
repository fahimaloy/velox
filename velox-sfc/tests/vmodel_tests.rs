// Integration tests for v-model compile pipeline.
// Verifies that v-model directives are desugared into :value + @input bindings,
// setter methods are generated on State, and make_on_event dispatches correctly.

use velox_sfc::codegen::to_stub_rs;

/// The nearest non-blank line above the line containing `needle`, trimmed.
///
/// A generated setter must be preceded by `impl State {`: before this was fixed the
/// setters were appended after the closing brace of the script's own `impl State`, so
/// they were free functions and the dispatcher arm's `state.<setter>(p)` had nothing to
/// call. Looking at the previous non-blank line rather than counting braces keeps the
/// check immune to the `{` and `}` inside the generated code's format strings.
fn line_before(module: &str, needle: &str) -> String {
    let mut previous = String::new();
    for line in module.lines() {
        if line.contains(needle) {
            return previous.trim().to_string();
        }
        if !line.trim().is_empty() {
            previous = line.to_string();
        }
    }
    panic!("no line contains {needle:?}");
}

/// Every `__vmodel_set_*` dispatch arm in a generated module, as
/// `(arm key, the setter the arm calls)`.
///
/// The two come from different places on purpose, and that is the point: the arm key
/// is what `extract_vmodel` put in the `on:input` prop, and the callee is the same
/// string spliced into `state.{name}(p)`. A dotted expression used to keep its dot in
/// the key and produce `state.__vmodel_set_form.name(p)`, which is not a method call.
fn vmodel_dispatch_arms(code: &str) -> Vec<(String, String)> {
    let mut arms = Vec::new();
    for line in code.lines() {
        let line = line.trim();
        if !line.starts_with("\"__vmodel_set_") {
            continue;
        }
        let key = line
            .split('"')
            .nth(1)
            .expect("an arm line starts with a quoted key")
            .to_string();
        let callee = line
            .split("state.")
            .nth(1)
            .and_then(|rest| rest.split('(').next())
            .expect("an arm line calls something on state")
            .to_string();
        arms.push((key, callee));
    }
    arms
}

// =============================================================================
// v-model basic compilation — uses to_stub_rs for setter verification
// =============================================================================

#[test]
fn vmodel_generates_setter() {
    let src = r#"<template>
  <input v-model="counter"/>
</template>
<script setup>
use std::cell::Cell;
pub struct State { pub counter: Cell<i32> }
impl State {
    pub fn new() -> Self { Self { counter: Cell::new(0) } }
}
</script>"#;

    let sfc = velox_sfc::parse_sfc(src).unwrap();
    let stub = velox_sfc::codegen::to_stub_rs(&sfc, "VModelApp");

    // The __vmodel_set_counter setter method should exist on State
    assert!(
        stub.contains("__vmodel_set_counter"),
        "missing __vmodel_set_counter setter in generated code"
    );

    // The setter should call VModel::vmodel_set
    assert!(
        stub.contains("VModel::vmodel_set"),
        "missing VModel::vmodel_set call in setter"
    );

    // The setter must be an inherent method on State, not a free function. These used
    // to be appended after the closing brace of the script's own `impl State`, so the
    // module did not compile at all: `self` is only allowed in an associated function,
    // and the dispatch arm calls `state.__vmodel_set_counter(p)`.
    assert_eq!(
        line_before(&stub, "pub fn __vmodel_set_counter"),
        "impl State {",
        "the generated setter must open an `impl State` block, not trail the script's own"
    );
}

#[test]
fn vmodel_make_on_event_dispatches_with_payload() {
    // A bare name and a dotted expression, because the two disagreeed: the arm key and
    // the callee come from the same string, and only the dotted one used to keep its
    // dot, which produced `state.__vmodel_set_form.name(p)`.
    for expr in ["counter", "form.name"] {
        let setter = format!("__vmodel_set_{}", expr.replace('.', "_"));
        let tpl = format!(r#"<input v-model="{expr}"/>"#);
        let code = velox_sfc::compile_template_to_rs(&tpl, "app", None).unwrap();

        // make_on_event should exist and dispatch to the setter
        assert!(
            code.contains("make_on_event"),
            "missing make_on_event helper in compiled template"
        );
        assert!(
            code.contains(&format!(r#""{setter}""#)),
            "make_on_event does not dispatch to {setter}"
        );

        // The one arm dispatches to the same name the on:input prop carries.
        let arms = vmodel_dispatch_arms(&code);
        assert_eq!(
            arms,
            vec![(setter.clone(), setter.clone())],
            "the dispatch arm key and the setter it calls must agree, for `{expr}`"
        );
    }
}

// =============================================================================
// v-model desugars to :value + @input in the rendered output
// =============================================================================

#[test]
fn vmodel_desugars_to_value_and_input_binding() {
    let tpl = r#"<input v-model="counter"/>"#;
    let code = velox_sfc::compile_template_to_rs(tpl, "app", None).unwrap();

    // v-model should be desugared to :value="counter"
    assert!(
        code.contains("resolve(\"counter\")"),
        "v-model not desugared to :value binding"
    );

    // v-model should be desugared to on:input="__vmodel_set_counter"
    assert!(
        code.contains("__vmodel_set_counter"),
        "v-model not desugared to @input binding"
    );
}

// =============================================================================
// v-model with dot notation (nested fields)
// =============================================================================

#[test]
fn vmodel_dot_notation_nested_field() {
    let tpl = r#"<input v-model="form.name"/>"#;
    let code = velox_sfc::compile_template_to_rs(tpl, "app", None).unwrap();

    // Handler name uses underscores for dots: __vmodel_set_form_name. The old assertion
    // was only that the string appears somewhere, which the desugaring in
    // `extract_vmodel` satisfied while `collect_handlers` still emitted the dotted
    // spelling and the dispatcher arm did not compile. So assert the agreement, and
    // assert the dotted spelling is gone.
    assert!(
        code.contains("__vmodel_set_form_name"),
        "dot notation not converted to underscored handler name"
    );
    assert!(
        !code.contains("__vmodel_set_form.name"),
        "the dotted spelling survived: the dispatcher arm would be a field access, not a \
         method call"
    );
    assert_eq!(
        vmodel_dispatch_arms(&code),
        vec![(
            "__vmodel_set_form_name".to_string(),
            "__vmodel_set_form_name".to_string()
        )],
        "one dotted expression must produce one arm that calls the setter extract_vmodel \
         named"
    );
}

// =============================================================================
// v-model collect_vmodel_expressions unit
// =============================================================================

#[test]
fn collect_vmodel_expressions_finds_directives() {
    let nodes = velox_sfc::parse_template_to_ast(
        r#"<div><input v-model="counter"/><input v-model="name"/></div>"#,
    )
    .unwrap();

    let vmodels = velox_sfc::collect_vmodel_expressions(&nodes);

    assert_eq!(
        vmodels.len(),
        2,
        "expected 2 v-model expressions, got {}",
        vmodels.len()
    );

    let names: Vec<&str> = vmodels.iter().map(|(expr, _)| expr.as_str()).collect();
    assert!(names.contains(&"counter"), "missing counter v-model");
    assert!(names.contains(&"name"), "missing name v-model");

    let handlers: Vec<&str> = vmodels.iter().map(|(_, h)| h.as_str()).collect();
    assert!(
        handlers.contains(&"__vmodel_set_counter"),
        "missing __vmodel_set_counter handler"
    );
    assert!(
        handlers.contains(&"__vmodel_set_name"),
        "missing __vmodel_set_name handler"
    );
}

// =============================================================================
// v-model generate_vmodel_setters unit
// =============================================================================

#[test]
fn generate_vmodel_setters_produces_methods() {
    let vmodels = vec![
        ("counter".to_string(), "__vmodel_set_counter".to_string()),
        (
            "form.name".to_string(),
            "__vmodel_set_form_name".to_string(),
        ),
    ];

    let setters = velox_sfc::generate_vmodel_setters(&vmodels);

    assert!(
        setters.contains("pub fn __vmodel_set_counter"),
        "missing counter setter fn"
    );
    assert!(
        setters.contains("pub fn __vmodel_set_form_name"),
        "missing form.name setter fn"
    );
    assert!(
        setters.contains("VModel::vmodel_set(&self.counter, payload)"),
        "counter setter does not call VModel::vmodel_set on self.counter"
    );
    assert!(
        setters.contains("VModel::vmodel_set(&self.form.name, payload)"),
        "form.name setter does not call VModel::vmodel_set on self.form.name"
    );
}

// =============================================================================
// Multiple v-model directives in same template
// =============================================================================

#[test]
fn multiple_vmodel_directives_in_same_template() {
    let tpl = r#"<div><input v-model="firstName"/><input v-model="lastName"/></div>"#;
    let code = velox_sfc::compile_template_to_rs(tpl, "app", None).unwrap();

    assert!(
        code.contains("__vmodel_set_firstName"),
        "missing __vmodel_set_firstName setter"
    );
    assert!(
        code.contains("__vmodel_set_lastName"),
        "missing __vmodel_set_lastName setter"
    );
    assert!(
        code.contains(r#""__vmodel_set_firstName""#),
        "make_on_event missing firstName dispatch arm"
    );
    assert!(
        code.contains(r#""__vmodel_set_lastName""#),
        "make_on_event missing lastName dispatch arm"
    );
}

// =============================================================================
// v-model with additional attributes preserved (to_stub_rs path)
// =============================================================================

#[test]
fn vmodel_preserves_other_attributes() {
    let src = r#"<template>
  <input class="field" v-model="counter"/>
</template>
<script setup>
use std::cell::Cell;
pub struct State { pub counter: Cell<i32> }
impl State {
    pub fn new() -> Self { Self { counter: Cell::new(0) } }
}
</script>"#;

    let sfc = velox_sfc::parse_sfc(src).unwrap();
    let stub = velox_sfc::codegen::to_stub_rs(&sfc, "VModelAttrs");

    // Other attributes should be preserved
    assert!(stub.contains("class"), "class attribute not preserved");

    // v-model setter should still be generated
    assert!(
        stub.contains("__vmodel_set_counter"),
        "v-model setter not generated alongside other attrs"
    );
}

// =============================================================================
// The live read: a root-level v-model used to route its value through `resolve` with
// no arm to answer it, so the input always rendered empty.
// =============================================================================

/// A script whose `State` exposes the accessors the template needs, so the assertions
/// are about a module that could actually compile. `form` is a user struct, which the
/// dotted case needs, and `note` deliberately has no accessor.
const DRAFT_SCRIPT: &str = r#"<script setup>
use std::cell::RefCell;
pub struct Form { pub name: RefCell<String> }
impl Clone for Form {
    fn clone(&self) -> Self { Form { name: RefCell::new(self.name.borrow().clone()) } }
}
pub struct State {
    pub draft: RefCell<String>,
    pub form: Form,
    pub note: RefCell<String>,
}
impl State {
    pub fn new() -> Self {
        Self {
            draft: RefCell::new(String::new()),
            form: Form { name: RefCell::new(String::new()) },
            note: RefCell::new(String::new()),
        }
    }
    pub fn draft(&self) -> String { self.draft.borrow().clone() }
    pub fn form(&self) -> Form { self.form.clone() }
}
</script>"#;

/// A `v-for` collection has to be a signal for the existing loop codegen to iterate it,
/// and the element type has to be cloneable, so `Todo` derives `Clone`.
const LIST_SCRIPT: &str = r#"<script setup>
use velox_core::signal::Signal;
use std::rc::Rc;
#[derive(Clone)]
pub struct Todo { pub text: String }
pub struct State { pub todos: Rc<Signal<Vec<Todo>>> }
impl State {
    pub fn new() -> Self {
        Self { todos: Rc::new(Signal::new(vec![Todo { text: String::new() }])) }
    }
    pub fn todos(&self) -> Vec<Todo> { self.todos.get().clone() }
}
</script>"#;

/// The two halves a component is built from, from one SFC: `to_stub_rs` emits the
/// `State` module with the generated setters in it, and the template generator emits
/// the render functions, the resolver and the dispatcher. An example build runs
/// exactly this pair, so a test that reads both is reading what an example runs.
struct Component {
    stub: String,
    template: String,
}

fn component(template: &str, script: &str) -> Component {
    let src = format!("<template>{template}</template>{script}");
    let sfc = velox_sfc::parse_sfc(&src).unwrap();
    let stub = to_stub_rs(&sfc, "App");
    let template_src = sfc
        .template
        .as_ref()
        .expect("the fixture has a template block")
        .content
        .clone();
    let script_src = sfc
        .script_setup
        .as_ref()
        .expect("the fixture has a script block")
        .content
        .clone();
    // The same call the build command makes, in the same mode: State, so a binding
    // only a resolver can satisfy is rendered from the State instead of reported.
    let template = velox_sfc::compile_template_to_rs_full_with_mode(
        &template_src,
        "App",
        None,
        Some(&script_src),
        None,
        velox_sfc::RenderMode::State,
    )
    .expect("the template compiles");
    Component { stub, template }
}

/// The arms `make_resolve` matches on, as `(key, the expression it resolves to)`, so a
/// test can assert the whole set instead of hunting a substring. A key that has no arm
/// silently resolves to the empty string, so the fallback arm being there alone is the
/// failure this catches.
fn resolver_arms(template: &str) -> Vec<(String, String)> {
    let start = template
        .find("pub fn make_resolve")
        .expect("make_resolve is always generated");
    let rest = &template[start..];
    let end = rest
        .find("pub fn make_on_event")
        .expect("make_on_event is always generated");
    let mut arms = Vec::new();
    for line in rest[..end].lines() {
        let line = line.trim();
        if !(line.starts_with('"') || line.starts_with('_')) {
            continue;
        }
        let (key, body) = match line.strip_prefix('"') {
            Some(rest) => {
                let (key, body) = rest.split_once("\" =>").expect("an arm has a body");
                (
                    key.to_string(),
                    body.trim().trim_end_matches(',').to_string(),
                )
            }
            None => {
                let (_, body) = line.split_once(" =>").expect("the fallback has a body");
                (
                    "_".to_string(),
                    body.trim().trim_end_matches(',').to_string(),
                )
            }
        };
        arms.push((key, body));
    }
    arms
}

#[test]
fn a_root_level_v_model_reads_a_live_resolver_arm() {
    let app = component(r#"<input v-model="draft"/>"#, DRAFT_SCRIPT);

    // The synthetic `:value` bind is created at emit time, so it used to be invisible
    // to key collection: the arm set was the fallback alone and the input rendered "".
    assert_eq!(
        resolver_arms(&app.template),
        vec![
            ("draft".to_string(), "state.draft().to_string()".to_string()),
            ("_".to_string(), "String::new()".to_string()),
        ],
        "a v-model on a State accessor must get a resolver arm of its own"
    );
    assert!(
        app.template.contains(r#".set("value", &resolve("draft"))"#),
        "the value is read through the resolver, so the arm above is what answers it"
    );
    assert_eq!(
        vmodel_dispatch_arms(&app.template),
        vec![(
            "__vmodel_set_draft".to_string(),
            "__vmodel_set_draft".to_string()
        )],
        "and the write is dispatchable under the name the prop carries"
    );
}

#[test]
fn a_dotted_v_model_reads_its_chain_and_writes_the_nested_field() {
    let app = component(r#"<input v-model="form.name"/>"#, DRAFT_SCRIPT);

    // R-1d's read path: `form.name` is answered by the method chain `form().name`.
    assert_eq!(
        resolver_arms(&app.template),
        vec![
            (
                "form.name".to_string(),
                "state.form().name().to_string()".to_string()
            ),
            ("_".to_string(), "String::new()".to_string()),
        ],
        "a dotted v-model reads the same chain an interpolation of the same path does"
    );
    assert_eq!(
        vmodel_dispatch_arms(&app.template),
        vec![(
            "__vmodel_set_form_name".to_string(),
            "__vmodel_set_form_name".to_string()
        )],
        "one dotted expression, one arm, one name"
    );
    assert!(
        app.stub
            .contains("VModel::vmodel_set(&self.form.name, payload)"),
        "the setter must still target the nested field"
    );
}

/// The open finding, pinned: without an accessor there is no arm, so the input renders
/// empty. The write arm survives, because the field really is there.
#[test]
fn a_v_model_without_an_accessor_writes_and_renders_an_empty_value() {
    let app = component(r#"<input v-model="note"/>"#, DRAFT_SCRIPT);

    assert_eq!(
        resolver_arms(&app.template),
        vec![("_".to_string(), "String::new()".to_string())],
        "a field with no accessor has no arm, so the read is the empty fallback"
    );
    assert_eq!(
        vmodel_dispatch_arms(&app.template),
        vec![(
            "__vmodel_set_note".to_string(),
            "__vmodel_set_note".to_string()
        )],
        "the write arm survives even when the read cannot be answered"
    );
}

#[test]
fn a_v_model_in_a_v_for_writes_nothing_and_reads_the_loop_item() {
    let app = component(
        r#"<div v-for="todo in todos"><input v-model="todo.text"/></div>"#,
        LIST_SCRIPT,
    );

    // A v-model in a loop body used to be dropped outright, because the loop-body
    // emitters never desugared it and dropped every directive, leaving a bare input.
    assert!(
        !app.stub.contains("__vmodel_set_") && !app.template.contains("__vmodel_set_"),
        "a loop-rooted v-model must not generate a setter the dispatcher cannot reach:\n{}",
        app.stub
    );
    assert_eq!(
        vmodel_dispatch_arms(&app.template),
        Vec::<(String, String)>::new(),
        "and no dispatch arm, which would be unreachable: the dispatcher holds a `State`, \
         not the loop item"
    );

    // The value is read from the loop item directly, the shape a loop-rooted `:value`
    // bind already used, because the resolver is built outside the loop and cannot see
    // `todo`.
    assert!(
        app.template
            .contains(r#".set("value", &format!("{}", todo.text))"#),
        "a loop-rooted v-model must read the loop item directly:\n{}",
        app.template
    );
    assert!(
        !app.template.contains(r#"resolve("todo.text")"#),
        "a loop-rooted v-model must not route its read through the resolver:\n{}",
        app.template
    );
}

#[test]
fn a_v_model_beside_a_loop_still_gets_a_working_write() {
    // The loop fix must not take the root-level path down with it.
    let app = component(
        r#"<div><input v-model="draft"/><div v-for="todo in todos"><input v-model="todo.text"/></div></div>"#,
        LIST_SCRIPT,
    );

    assert_eq!(
        vmodel_dispatch_arms(&app.template),
        vec![(
            "__vmodel_set_draft".to_string(),
            "__vmodel_set_draft".to_string()
        )],
        "only the root-level v-model is dispatchable"
    );
    assert!(
        app.template.contains(r#".set("value", &resolve("draft"))"#)
            && app
                .template
                .contains(r#".set("value", &format!("{}", todo.text))"#),
        "each v-model reads the value it can actually read:\n{}",
        app.template
    );
}
