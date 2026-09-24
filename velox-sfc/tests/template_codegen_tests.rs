use velox_sfc::{
    RenderMode, compile_template_to_rs, compile_template_to_rs_full,
    compile_template_to_rs_full_with_mode,
};

const SCRIPT_WITH_GETTERS: &str = r#"
pub struct State {
    pub count: i32,
}

impl State {
    pub fn new() -> Self {
        Self { count: 0 }
    }

    pub fn draft(&self) -> String {
        String::from("Buy milk")
    }

    pub fn input_placeholder(&self) -> String {
        String::from("What needs to be done?")
    }
}
"#;

/// Getter-backed State plus one payload-taking event handler, so tests can tell
/// genuine zero-argument getters apart from handlers that share their name.
/// The import makes `<TodoItem>` a real component tag rather than an element.
const SCRIPT_WITH_GETTERS_AND_HANDLER: &str = r#"
import TodoItem from './components/TodoItem.vx';

pub struct State {
    pub count: i32,
}

impl State {
    pub fn new() -> Self {
        Self { count: 0 }
    }

    pub fn draft(&self) -> String {
        String::from("Buy milk")
    }

    pub fn input_placeholder(&self) -> String {
        String::from("What needs to be done?")
    }

    pub fn active(&self) -> String {
        String::from("active")
    }

    pub fn completed(&self) -> String {
        String::from("true")
    }

    pub fn on_input(&self, payload: &str) {
        let _ = payload;
    }

    /// Zero-argument, but nothing comes back, so there is no text to render.
    pub fn reset(&self) {}

    /// Zero-argument, but a collection has no `Display` form.
    pub fn items(&self) -> Vec<String> {
        Vec::new()
    }
}
"#;

/// Extract the `make_resolve` function body so assertions only look at resolver arms.
fn make_resolve_body(rs: &str) -> &str {
    rs.split_once("pub fn make_resolve")
        .expect("make_resolve should be generated")
        .1
}

#[test]
fn codegen_v_for_dot_notation() {
    // Test that v-for with dot notation interpolation works
    // e.g., {{ item.name }} inside v-for="item in items"
    let rs = compile_template_to_rs(
        r#"<div v-for="item in items"><p>{{ item.name }}</p></div>"#,
        "App",
        None,
    )
    .unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);
    // Should contain the loop iteration over items
    assert!(rs.contains("__for_count"));
    // The index variable is now __idx (from parse_v_for default)
    assert!(rs.contains("for __idx in 0..__for_count"));
    // Should handle dot notation properly using indexed resolve
    assert!(rs.contains("items[{}].name"));
}

#[test]
fn codegen_v_for_with_index() {
    // Test v-for with (item, index) destructuring
    let rs = compile_template_to_rs(
        r#"<div v-for="(item, index) in items"><p>{{ item.name }} #{{ index }}</p></div>"#,
        "App",
        None,
    )
    .unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);
    assert!(rs.contains("__for_count"));
    assert!(rs.contains("for index in 0..__for_count"));
    // Should use the item name and index correctly
    assert!(rs.contains("items[{}].name"));
}

#[test]
fn codegen_v_for_with_key() {
    // Test v-for with :key attribute
    let rs = compile_template_to_rs(
        r#"<div v-for="item in items" :key="item.id"><p>{{ item.name }}</p></div>"#,
        "App",
        None,
    )
    .unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);
    assert!(rs.contains("__for_count"));
    // :key should be removed from attrs (not emitted as a prop)
    assert!(!rs.contains(r#".set("key""#));
}

#[test]
fn codegen_v_for_numeric_count() {
    // Test v-for with numeric count (legacy behavior)
    let rs = compile_template_to_rs(
        r#"<div v-for="i in 5"><span>{{ i }}</span></div>"#,
        "App",
        None,
    )
    .unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);
    assert!(rs.contains("__for_count"));
}

#[test]
fn codegen_div_with_text() {
    let rs = compile_template_to_rs("<div>hi</div>", "App", None).unwrap();
    assert!(rs.contains(r#"use velox_dom::*"#));
    assert!(rs.contains(r#"h("div""#));
    assert!(rs.contains(r#"text("hi")"#));
}

#[test]
fn codegen_interpolation() {
    let rs = compile_template_to_rs("<p>Hello {{name}}</p>", "App", None).unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);
    assert!(rs.contains(r#"h("p""#));
    assert!(rs.contains(r#"text("Hello")"#) || rs.contains(r#"text("Hello ")"#));
    assert!(rs.contains(r#"resolve("name")"#) || rs.contains(r#"resolve("name")"#));
}

#[test]
fn codegen_attrs() {
    let rs = compile_template_to_rs(
        r#"<input class="x" :value="count" @input="onInput"/>"#,
        "App",
        None,
    )
    .unwrap();
    assert!(rs.contains(r#".set("class", "x")"#));
    assert!(rs.contains(r#".set("value", &resolve("count"))"#));
    assert!(rs.contains(r#".set("on:input", "onInput")"#));
}

/// A component-bound attribute is emitted as `resolve("<expr>")`, so the
/// generated resolver has to register that key. Otherwise it falls through to
/// the `_ => String::new()` arm and the child component renders an empty prop.
#[test]
fn component_bound_attrs_are_registered_as_resolver_keys() {
    let rs = compile_template_to_rs_full(
        r#"<div data-velox-component="TodoInput" :value="draft" :placeholder="input_placeholder"></div>"#,
        "TodoApp",
        None,
        Some(SCRIPT_WITH_GETTERS),
        None,
    )
    .unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);

    // The props themselves are looked up through the resolver.
    assert!(
        rs.contains(r#"resolve("draft")"#),
        "`:value` must be read through the resolver:\n{rs}"
    );
    assert!(
        rs.contains(r#"resolve("input_placeholder")"#),
        "`:placeholder` must be read through the resolver:\n{rs}"
    );

    let resolve = make_resolve_body(&rs);
    assert!(
        resolve.contains(r#""draft" => state.draft().to_string()"#),
        "bound `:value=\"draft\"` must be a resolver key:\n{resolve}"
    );
    assert!(
        resolve.contains(r#""input_placeholder" => state.input_placeholder().to_string()"#),
        "bound `:placeholder=\"input_placeholder\"` must be a resolver key:\n{resolve}"
    );
}

/// Plain-element bindings use the same `resolve(...)` path, and so do the
/// conditions of an object-syntax `:class`.
#[test]
fn element_bound_attr_and_class_condition_are_registered_as_resolver_keys() {
    let rs = compile_template_to_rs_full(
        r#"<div><input :value="draft" /><p :class="{ done: input_placeholder }">x</p></div>"#,
        "TodoApp",
        None,
        Some(SCRIPT_WITH_GETTERS),
        None,
    )
    .unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);

    let resolve = make_resolve_body(&rs);
    assert!(
        resolve.contains(r#""draft" => state.draft().to_string()"#),
        "`:value=\"draft\"` must be a resolver key:\n{resolve}"
    );
    assert!(
        resolve.contains(r#""input_placeholder" => state.input_placeholder().to_string()"#),
        "`:class` condition must be a resolver key:\n{resolve}"
    );
}

/// Registering keys must not invent State getters: a bound attribute whose
/// expression is a state *field* (no getter) is left unregistered so the
/// generated code keeps compiling for `Signal`/`Ref` fields.
#[test]
fn bound_attr_without_state_getter_is_not_registered() {
    let script = r#"
pub struct State {
    pub count: std::rc::Rc<velox_core::signal::Signal<i32>>,
}
"#;
    let rs = compile_template_to_rs_full(
        r#"<input :value="count" />"#,
        "Counter",
        None,
        Some(script),
        None,
    )
    .unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);

    assert!(rs.contains(r#".set("value", &resolve("count"))"#));
    let resolve = make_resolve_body(&rs);
    assert!(
        !resolve.contains(r#""count" =>"#),
        "no getter means no resolver arm for a field-based binding:\n{resolve}"
    );
}

/// A bare `:class="active"` binding is emitted as `resolve("active")`, so that
/// exact key has to be registered or the class silently renders empty.
#[test]
fn bare_class_binding_is_registered_as_resolver_key() {
    let rs = compile_template_to_rs_full(
        r#"<div><p :class="active">x</p></div>"#,
        "TodoApp",
        None,
        Some(SCRIPT_WITH_GETTERS_AND_HANDLER),
        None,
    )
    .unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);

    assert!(
        rs.contains(r#"resolve("active")"#),
        "`:class=\"active\"` must be read through the resolver:\n{rs}"
    );
    let resolve = make_resolve_body(&rs);
    assert!(
        resolve.contains(r#""active" => state.active().to_string()"#),
        "`:class=\"active\"` must be a resolver key:\n{resolve}"
    );
}

/// A negated object condition is emitted as a lookup of the operand, so the
/// operand — not the `!`-prefixed text — is the key the resolver needs.
#[test]
fn negated_class_condition_registers_the_condition_key() {
    let rs = compile_template_to_rs_full(
        r#"<div><p :class="{ done: !completed }">x</p></div>"#,
        "TodoApp",
        None,
        Some(SCRIPT_WITH_GETTERS_AND_HANDLER),
        None,
    )
    .unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);

    assert!(
        rs.contains(r#"resolve("completed")"#),
        "`:class=\"{{ done: !completed }}\"` must look up `completed`:\n{rs}"
    );
    let resolve = make_resolve_body(&rs);
    assert!(
        resolve.contains(r#""completed" => state.completed().to_string()"#),
        "the condition operand must be a resolver key:\n{resolve}"
    );
    assert!(
        !resolve.contains(r#""!completed" =>"#),
        "the `!`-prefixed text is not a resolver key:\n{resolve}"
    );
}

/// A component-bound object `:class` looked the whole literal up before, so the
/// child never saw the condition. It must look up the condition key instead.
#[test]
fn component_bound_object_class_registers_the_condition_key() {
    let rs = compile_template_to_rs_full(
        r#"<div data-velox-component="TodoItem" :class="{ done: completed }"></div>"#,
        "TodoApp",
        None,
        Some(SCRIPT_WITH_GETTERS_AND_HANDLER),
        None,
    )
    .unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);

    assert!(
        !rs.contains(r#"resolve("{ done: completed }")"#),
        "the whole class literal must not be used as a resolver key:\n{rs}"
    );
    assert!(
        rs.contains(r#"resolve("completed")"#),
        "the class condition must be read through the resolver:\n{rs}"
    );
    let resolve = make_resolve_body(&rs);
    assert!(
        resolve.contains(r#""completed" => state.completed().to_string()"#),
        "the class condition must be a resolver key:\n{resolve}"
    );
}

/// A bound expression that is not a bare identifier cannot become a resolver
/// lookup: it is reported as a codegen warning (see the unit tests next to the
/// collector) and never registered as a key, so no arm claims to resolve it.
#[test]
fn unsupported_bound_expression_is_not_registered_as_a_resolver_key() {
    let rs = compile_template_to_rs_full(
        r#"<input :value="draft.trim()" />"#,
        "TodoApp",
        None,
        Some(SCRIPT_WITH_GETTERS_AND_HANDLER),
        None,
    )
    .unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);

    let resolve = make_resolve_body(&rs);
    assert!(
        !resolve.contains(r#""draft.trim()" =>"#),
        "a non-bare expression must not be registered as a resolver key:\n{resolve}"
    );
}

/// A payload-taking method is an event handler, not a getter. Registering its
/// name would emit `state.on_input().to_string()`, which does not compile.
#[test]
fn payload_taking_method_is_not_registered_as_a_getter() {
    let rs = compile_template_to_rs_full(
        r#"<input :value="on_input" />"#,
        "TodoApp",
        None,
        Some(SCRIPT_WITH_GETTERS_AND_HANDLER),
        None,
    )
    .unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);

    let resolve = make_resolve_body(&rs);
    assert!(
        !resolve.contains(r#""on_input" =>"#),
        "a payload-taking handler must not be registered as a getter:\n{resolve}"
    );
    assert!(
        !rs.contains("state.on_input()"),
        "generated code must never call a handler as a getter:\n{rs}"
    );
}

/// A zero-argument method with no declared return type returns `()`, which has
/// no `Display` form, so it must never become a resolver arm.
#[test]
fn zero_argument_method_without_return_type_is_not_registered_as_a_getter() {
    let rs = compile_template_to_rs_full(
        r#"<input :value="reset" />"#,
        "TodoApp",
        None,
        Some(SCRIPT_WITH_GETTERS_AND_HANDLER),
        None,
    )
    .unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);

    let resolve = make_resolve_body(&rs);
    assert!(
        !resolve.contains(r#""reset" =>"#),
        "a method with no declared return type is not a getter:\n{resolve}"
    );
    assert!(
        !rs.contains("state.reset()"),
        "generated code must never call a unit method as a getter:\n{rs}"
    );
}

/// A zero-argument method whose return type cannot be rendered as text is not a
/// getter either, for the same reason.
#[test]
fn zero_argument_method_with_unrenderable_return_type_is_not_registered_as_a_getter() {
    let rs = compile_template_to_rs_full(
        r#"<input :value="items" />"#,
        "TodoApp",
        None,
        Some(SCRIPT_WITH_GETTERS_AND_HANDLER),
        None,
    )
    .unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);

    let resolve = make_resolve_body(&rs);
    assert!(
        !resolve.contains(r#""items" =>"#),
        "a method returning `Vec<String>` is not a text getter:\n{resolve}"
    );
    assert!(
        !rs.contains("state.items().to_string()"),
        "generated code must never stringify an unrenderable value:\n{rs}"
    );
}

/// A binding rooted at a `v-for` loop variable is read directly by the State
/// renderer but goes through `resolve(...)` in the Resolve renderer, which has
/// no loop value to read. The mode split is pinned here: the State body keeps
/// the direct read, the Resolve body still looks the expression up, and no
/// resolver arm is invented for it.
#[test]
fn loop_rooted_binding_reads_the_loop_item_in_state_mode_only() {
    let script = r#"
pub struct State {
    pub todos: std::rc::Rc<velox_core::signal::Signal<Vec<String>>>,
}

impl State {
    pub fn todos(&self) -> Vec<String> {
        self.todos.get()
    }
}
"#;
    let rs = compile_template_to_rs_full(
        r#"<div v-for="(todo, idx) in todos"><p :value="todo.text" :index="idx">x</p></div>"#,
        "TodoApp",
        None,
        Some(script),
        None,
    )
    .unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);

    // Resolve mode: no loop context, so the expression is looked up verbatim.
    assert!(
        rs.contains(r#"resolve("todo.text")"#),
        "Resolve mode must still look the loop expression up:\n{rs}"
    );
    // State mode: the loop item is read directly.
    assert!(
        rs.contains(r#"format!("{}", todo.text)"#),
        "State mode must keep reading the loop item directly:\n{rs}"
    );

    let resolve = make_resolve_body(&rs);
    assert!(
        !resolve.contains(r#""todo.text" =>"#),
        "a loop-variable expression must not be registered as a key:\n{resolve}"
    );
    assert!(
        !resolve.contains(r#""idx" =>"#),
        "a loop index must not be registered as a key:\n{resolve}"
    );
}

/// `:key` generation is unchanged: the Resolve body still inserts the looked-up
/// key and the State body still reads the loop field. Only the collector's
/// understanding of it changed (it is reported for a Resolve-mode consumer, not
/// registered). Both statements are pinned in full, not by fragment.
#[test]
fn v_for_key_generation_is_unchanged() {
    let rs = compile_template_to_rs_full(
        r#"<div v-for="todo in todos" :key="todo.id">x</div>"#,
        "TodoApp",
        None,
        None,
        None,
    )
    .unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);

    // State mode: the whole insertion statement.
    assert!(
        rs.contains(
            r#"if let velox_dom::VNode::Element { ref mut props, .. } = __node { props.attrs.insert("key".to_string(), todo.id.to_string()); } __node }"#,
        ),
        "State mode keeps inserting the loop field:\n{rs}"
    );
    // Resolve mode: the whole insertion statement, double-interpolating form
    // included. That generation defect is queued separately, so today's output
    // is pinned on purpose.
    assert!(
        rs.contains(
            r#"if let velox_dom::VNode::Element { ref mut props, .. } = __node { props.attrs.insert("key".to_string(), resolve({ let __v = resolve("todo"); __v == "true" || (!__v.is_empty() && __v != "false") }.{ let __v = resolve("id"); __v == "true" || (!__v.is_empty() && __v != "false") }).to_string()); } __node }"#,
        ),
        "Resolve mode `:key` generation must stay unchanged:\n{rs}"
    );
}

/// The renderer's mode decides which diagnostics a component gets, never the
/// generated module: the two entry points must produce identical code, and the
/// State-mode wrapper must agree with passing [`RenderMode::State`] explicitly.
#[test]
fn render_mode_changes_diagnostics_not_generated_code() {
    let template =
        r#"<div v-for="(todo, idx) in todos"><TodoItem :todo="todo.text" :index="idx" /></div>"#;

    let state = compile_template_to_rs_full_with_mode(
        template,
        "TodoApp",
        None,
        Some(SCRIPT_WITH_GETTERS_AND_HANDLER),
        None,
        RenderMode::State,
    )
    .unwrap();
    let resolve = compile_template_to_rs_full_with_mode(
        template,
        "TodoApp",
        None,
        Some(SCRIPT_WITH_GETTERS_AND_HANDLER),
        None,
        RenderMode::Resolve,
    )
    .unwrap();
    assert_eq!(
        state, resolve,
        "the mode must only select diagnostics, not codegen output"
    );

    let defaulted = compile_template_to_rs_full(
        template,
        "TodoApp",
        None,
        Some(SCRIPT_WITH_GETTERS_AND_HANDLER),
        None,
    )
    .unwrap();
    assert_eq!(
        state, defaulted,
        "the default entry point must compile in State mode"
    );
}

/// A state field is not a getter, but a field that happens to share its name with
/// a real getter must still resolve through that getter.
#[test]
fn field_with_a_same_named_getter_still_resolves_through_the_getter() {
    let script = r#"
pub struct State {
    pub draft: std::rc::Rc<velox_core::signal::Signal<String>>,
}

impl State {
    pub fn draft(&self) -> String {
        self.draft.get().clone()
    }
}
"#;
    let rs = compile_template_to_rs_full(
        r#"<input :value="draft" />"#,
        "TodoApp",
        None,
        Some(script),
        None,
    )
    .unwrap();
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);

    let resolve = make_resolve_body(&rs);
    assert!(
        resolve.contains(r#""draft" => state.draft().to_string()"#),
        "a same-named getter must still register the key:\n{resolve}"
    );
}
