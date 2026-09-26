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

/// A binding rooted at a `v-for` loop variable is a real read in both renderers:
/// the State body reads the loop item, the Resolve body reads the same field
/// through the indexed resolver the loop body already uses for an interpolated
/// loop value, and no resolver arm is invented for it either way.
#[test]
fn loop_rooted_binding_reads_the_loop_item_in_both_renderers() {
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

    // Resolve mode: the loop item is read as the indexed resolver lookup the
    // same loop body uses for an interpolated `todo.text`.
    assert!(
        rs.contains(r#"resolve(&format!("todos[{}].text", idx))"#),
        "Resolve mode must read the loop field through the indexed resolver:\n{rs}"
    );
    assert!(
        !rs.contains(r#"resolve("todo.text")"#),
        "Resolve mode must not look a loop-rooted expression up verbatim:\n{rs}"
    );
    // The loop index is a real binding in the Resolve body too, so it is read
    // directly rather than through the resolver.
    assert!(
        rs.contains(r#".set("index", &format!("{}", idx))"#),
        "the loop index is a binding the Resolve body already has:\n{rs}"
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

/// `:key` generation: the State body reads the loop field and the Resolve body
/// reads the same field through the indexed resolver, exactly once. Both
/// statements are pinned in full, not by fragment. (This test originally pinned
/// the Resolve body's double-interpolating form as a known defect, and then
/// pinned its single-lookup form; the loop-rooted key is now read directly.)
#[test]
fn v_for_key_generation_is_single_interpolation_in_both_renderers() {
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
    // Resolve mode: the whole insertion statement, one indexed resolver read.
    assert!(
        rs.contains(
            r#"if let velox_dom::VNode::Element { ref mut props, .. } = __node { props.attrs.insert("key".to_string(), resolve(&format!("todos[{}].id", __idx)).to_string()); } __node }"#,
        ),
        "Resolve mode reads the loop-rooted key exactly once, through the loop:\n{rs}"
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

// --- `:key` on a `v-for` (audit finding G-1) -----------------------------------
//
// Both renderers are always generated, so a `:key` defect in the Resolve-mode
// `v-for` body breaks compilation of the whole generated module regardless of
// the mode the component was compiled in. The goldens below were captured from
// the pre-fix output (`git show e3a7ade:velox-sfc/src/template_codegen.rs`).

/// A `v-for` element with `:key`, in the documented `{{ … }}` spelling.
const V_FOR_KEY_MUSTACHE: &str =
    r#"<ul><li v-for="item in items" :key="{{ item.id }}">{{ item.name }}</li></ul>"#;
/// The same element with the bare-expression spelling.
const V_FOR_KEY_PLAIN: &str =
    r#"<ul><li v-for="item in items" :key="item.id">{{ item.name }}</li></ul>"#;
/// A `v-for` element with no `:key` at all.
const V_FOR_NO_KEY: &str = r#"<ul><li v-for="item in items">{{ item.name }}</li></ul>"#;
/// A `v-for` element with a valueless `:key` — "present but no value".
const V_FOR_KEY_VALUELESS: &str = r#"<ul><li v-for="item in items" :key>{{ item.name }}</li></ul>"#;

/// The generated key statement(s) in a module, one per renderer.
fn key_statements(rs: &str) -> Vec<&str> {
    rs.lines()
        .filter(|line| line.contains(r#"props.attrs.insert("key""#))
        .collect()
}

/// The State-mode renderer body from a generated module.
fn state_renderer(rs: &str) -> &str {
    let start = rs
        .find("pub fn render_with_state")
        .expect("the State-mode renderer is always generated");
    let rest = &rs[start..];
    let end = rest
        .find("pub fn make_resolve")
        .expect("make_resolve is always generated");
    &rest[..end]
}

/// The Resolve-mode renderer body from a generated module.
fn resolve_renderer(rs: &str) -> &str {
    let start = rs
        .find("pub fn render_with<F>")
        .expect("the Resolve-mode renderer is always generated");
    let rest = &rs[start..];
    let end = rest
        .find("pub fn render_with_state")
        .expect("the State-mode renderer is always generated");
    &rest[..end]
}

fn compile_in_mode(tpl: &str, mode: RenderMode) -> String {
    compile_template_to_rs_full_with_mode(tpl, "App", None, None, None, mode)
        .expect("template compiles")
}

/// [`compile_in_mode`] with a `<script setup>` block, so the generated arms are
/// resolved against the `State` methods the script declares.
fn compile_with_script(tpl: &str, script: &str, mode: RenderMode) -> String {
    compile_template_to_rs_full_with_mode(tpl, "App", None, Some(script), None, mode)
        .expect("template compiles")
}

/// The key expression must be interpolated exactly once, through a single
/// resolver read of the loop item — the same shape the sibling `v-for` branch
/// uses for an interpolated loop value (`text(resolve(&format!("items[{}].name",
/// __idx)))`) and for its collection (`let __for_expr = resolve("items");`).
#[test]
fn resolve_mode_v_for_key_interpolates_the_expression_exactly_once() {
    let rs = compile_in_mode(V_FOR_KEY_MUSTACHE, RenderMode::Resolve);
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);

    assert!(
        rs.contains(
            r#"props.attrs.insert("key".to_string(), resolve(&format!("items[{}].id", __idx)).to_string());"#
        ),
        "the key must be read once, through the loop it is rooted in:\n{rs}"
    );
    // Each renderer interpolates the expression exactly once, and never rewrites
    // it into a nested lookup or a field access on a block.
    assert_eq!(
        resolve_renderer(&rs).matches("items[{}].id").count(),
        1,
        "the Resolve-mode renderer must interpolate the key exactly once:\n{rs}"
    );
    assert_eq!(
        state_renderer(&rs).matches("item.id").count(),
        1,
        "the State-mode renderer must read the key exactly once:\n{rs}"
    );
    // The pre-fix defects: the mustache pair was carried through into the
    // generated `resolve(…)`, and the dotted expression was rewritten into a
    // field access on a block expression.
    assert!(
        !rs.contains("resolve({{"),
        "`{{` must not reach the generated resolver call:\n{rs}"
    );
    assert!(
        !rs.contains(r#"resolve({ let __v = resolve("#),
        "a dotted key must not be rewritten into a block field access:\n{rs}"
    );
}

/// Both spellings of the same key expression must generate the same code.
#[test]
fn resolve_mode_v_for_key_spellings_generate_the_same_lookup() {
    let mustache = compile_in_mode(V_FOR_KEY_MUSTACHE, RenderMode::Resolve);
    let plain = compile_in_mode(V_FOR_KEY_PLAIN, RenderMode::Resolve);

    let of =
        |rs: &str| -> Vec<String> { key_statements(rs).into_iter().map(str::to_string).collect() };
    assert_eq!(
        of(&mustache),
        of(&plain),
        "`:key=\"{{{{ item.id }}}}\"` and `:key=\"item.id\"` are the same expression:\n{mustache}\n---\n{plain}"
    );
}

/// The State-mode renderer must be byte-for-byte what it was before the fix:
/// it reads the loop field directly and must not change at all.
#[test]
fn state_mode_v_for_key_output_matches_the_pre_fix_golden() {
    for (tpl, golden) in [
        (
            V_FOR_KEY_MUSTACHE,
            include_str!("testdata/v_for_key_mustache.state_renderer.golden"),
        ),
        (
            V_FOR_KEY_PLAIN,
            include_str!("testdata/v_for_key_plain.state_renderer.golden"),
        ),
    ] {
        let rs = compile_in_mode(tpl, RenderMode::State);
        assert_eq!(
            state_renderer(&rs),
            golden,
            "the State-mode renderer must be unchanged for {tpl}"
        );
    }
}

/// A `v-for` with no `:key` must generate exactly what it generated before the
/// fix, in both renderers and in both modes.
#[test]
fn v_for_without_a_key_output_matches_the_pre_fix_golden() {
    let module_golden = include_str!("testdata/v_for_without_key.module.golden");
    for mode in [RenderMode::Resolve, RenderMode::State] {
        let rs = compile_in_mode(V_FOR_NO_KEY, mode);
        assert_eq!(
            rs, module_golden,
            "a keyless `v-for` module must be unchanged in {mode:?}"
        );
        assert_eq!(
            resolve_renderer(&rs),
            include_str!("testdata/v_for_without_key.resolve_renderer.golden"),
            "the keyless Resolve-mode renderer must be unchanged in {mode:?}"
        );
    }
}

/// A valueless `:key` keeps its own path: nothing is inserted, and the module is
/// identical to the keyless one (so it keeps compiling).
#[test]
fn v_for_with_a_valueless_key_generates_the_keyless_module() {
    for mode in [RenderMode::Resolve, RenderMode::State] {
        let rs = compile_in_mode(V_FOR_KEY_VALUELESS, mode);
        assert!(
            key_statements(&rs).is_empty(),
            "a valueless `:key` must not insert a key in {mode:?}:\n{rs}"
        );
        assert_eq!(
            rs,
            include_str!("testdata/v_for_without_key.module.golden"),
            "a valueless `:key` must generate the keyless module in {mode:?}"
        );
    }
}

/// R-1c. `make_resolve` for this module, as generated text.
fn make_resolve_fn(rs: &str) -> &str {
    let start = rs
        .find("pub fn make_resolve")
        .expect("make_resolve is always generated");
    let rest = &rs[start..];
    let end = rest
        .find("pub fn make_on_event")
        .expect("make_on_event is always generated");
    &rest[..end]
}

/// Every arm `make_resolve` matches on, trimmed — the whole set, so an arm
/// reappearing under any name fails the assertion.
fn make_resolve_arms(rs: &str) -> Vec<&str> {
    let body = make_resolve_fn(rs);
    let start = body
        .find("match name {")
        .expect("make_resolve matches on a name");
    let rest = &body[start..];
    let end = rest
        .find(
            "
    }",
        )
        .expect("the match closes");
    rest[..end]
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("match name"))
        .collect()
}

/// A `{{ item.name }}` inside `v-for="item in items"` must not produce a
/// resolver arm. The loop item is not a `State` field: the arm would read
/// `state.item.name()` while the field is `items` and there is no `item()`
/// getter, which is E0609 — the generated module does not compile. The whole
/// `make_resolve` is pinned here, not just the absence of one string, so an arm
/// reappearing under another name fails the test.
#[test]
fn a_loop_rooted_interpolation_produces_no_resolver_arm() {
    for mode in [RenderMode::State, RenderMode::Resolve] {
        let rs = compile_in_mode(V_FOR_NO_KEY, mode);
        assert_eq!(
            make_resolve_arms(&rs),
            vec!["_ => String::new(),"],
            "a loop-rooted interpolation is not a `State` field, so it cannot \
             be an arm in {mode:?}"
        );
        assert!(
            !rs.contains("state.item."),
            "nothing may read the loop item as a state field path in {mode:?} \
             (`state.items.get()` is the collection, and is expected):\n{rs}"
        );
    }
}

/// The Resolve-mode body is untouched by that: it reads the loop item through
/// the index the loop binds, which is the only way that body can read a loop
/// value, and it did so before this change.
#[test]
fn a_loop_rooted_interpolation_still_renders_through_the_loop_index() {
    for mode in [RenderMode::State, RenderMode::Resolve] {
        let module = compile_in_mode(V_FOR_NO_KEY, mode);
        let resolve_r = resolve_renderer(&module);
        assert!(
            resolve_r.contains(r#"text(resolve(&format!("items[{}].name", __idx)))"#),
            "the Resolve body reads the loop item through the index in {mode:?}:\n{resolve_r}"
        );
    }
    // The State renderer reads the same value off the loop binding, so it never
    // needed an arm for it.
    let state_module = compile_in_mode(V_FOR_NO_KEY, RenderMode::State);
    let state_r = state_renderer(&state_module);
    assert!(
        state_r.contains("__obj.name.to_string()"),
        "the State body reads the loop binding directly:\n{state_r}"
    );
}

/// R-1c pinned this shape to make the scope boundary visible: a dotted
/// interpolation at the top level of a component was collected and emitted
/// exactly as it was, arm included, even though the arm it produced
/// (`state.user.name()`) cannot compile for a hand-written `State` — that shape
/// needed a `user` *field* holding something with a `name` method, and no
/// template can produce that. R-1d is the task that fixes it, so the pin is now
/// the corrected shape: the root is a call on the accessor, and every later
/// segment is a call on what came before.
const SCRIPT_WITH_A_USER_ACCESSOR: &str = r#"
pub struct User { name: String }
impl User { pub fn name(&self) -> String { self.name.clone() } }

pub struct State { user: User }
impl State {
    pub fn new() -> Self { Self { user: User { name: String::new() } } }
    pub fn user(&self) -> User { self.user.clone() }
    pub fn title(&self) -> String { String::new() }
}
"#;

/// A root-level member path is a chain of calls on the root accessor. The old
/// shape — `state.user.name()` — was a field access on `State` and could not
/// compile; this one compiles for a `State` that declares the accessor the
/// framework's convention already looks up, and it is the same text in both
/// modes.
#[test]
fn a_root_level_member_path_emits_a_chain_on_the_root_accessor() {
    for mode in [RenderMode::State, RenderMode::Resolve] {
        let rs = compile_with_script(
            r#"<p>{{ user.name }}</p>"#,
            SCRIPT_WITH_A_USER_ACCESSOR,
            mode,
        );
        assert!(
            make_resolve_fn(&rs).contains(r#""user.name" => state.user().name().to_string(),"#),
            "the root is called, and so is every segment after it, in {mode:?}:\n{rs}"
        );
    }
}

/// The chain is rewritten uniformly, including an index segment: `items()[0]`
/// indexes what the accessor returned rather than calling it, and the segments
/// after the index are still calls. Codegen cannot know what a user's element
/// type looks like, so it does not guess — it emits the shape the accessor
/// convention implies and lets the compiler name a missing inner method.
#[test]
fn a_root_level_indexed_member_path_indexes_the_root_accessor() {
    let script = r#"
pub struct Item { name: String }
impl Item { pub fn name(&self) -> String { self.name.clone() } }

pub struct State { items: Vec<Item> }
impl State {
    pub fn new() -> Self { Self { items: Vec::new() } }
    pub fn items(&self) -> Vec<Item> { self.items.clone() }
}
"#;
    for mode in [RenderMode::State, RenderMode::Resolve] {
        let rs = compile_with_script(r#"<p>{{ items[0].name }}</p>"#, script, mode);
        assert!(
            make_resolve_fn(&rs)
                .contains(r#""items[0].name" => state.items()[0].name().to_string(),"#),
            "an index is a place, not a method, and the segments around it are calls, in {mode:?}:\n{rs}"
        );
    }
}

/// Without an accessor for the root there is nothing to call, so the key is not
/// registered and no arm is emitted — the arm that cannot compile is gone from
/// the generated module. The diagnostic that says so is asserted in the unit
/// test of the same name, which is where the warnings are observable.
#[test]
fn a_root_level_member_path_without_an_accessor_emits_no_arm() {
    for mode in [RenderMode::State, RenderMode::Resolve] {
        // No script at all: there is no `user` accessor, let alone a `user` field.
        let rs = compile_in_mode(r#"<p>{{ user.name }}</p>"#, mode);
        assert!(
            !make_resolve_fn(&rs).contains("user.name"),
            "an unanswerable member path must not be registered, so it cannot emit the \
             arm `state.user.name()` that does not compile, in {mode:?}:\n{rs}"
        );
    }
}

/// A call expression is not a member path and this task does not touch it. The
/// arm it generates is `state.user.name()()`, which is still a compile break —
/// codegen cannot know that a user writing `user.name()` means "pass `name()`'s
/// value to `user()`", and inventing that is the wrong-render trade R-1d's
/// ruling rejected. So the shape is pinned exactly as it was, and the report
/// records the discrepancy: the dispatch described this path as emitting
/// `state.user(state.name())`, and nothing in this workspace emits that for an
/// interpolation.
#[test]
fn a_call_interpolation_key_is_left_exactly_as_it_was() {
    for mode in [RenderMode::State, RenderMode::Resolve] {
        let rs = compile_in_mode(r#"<p>{{ user.name() }}</p>"#, mode);
        assert!(
            make_resolve_fn(&rs).contains(r#""user.name()" => state.user.name()().to_string(),"#),
            "a call key keeps the fallback arm it always had, in {mode:?}:\n{rs}"
        );
    }
}

/// A bare name is gated on a getter, and a getter-backed one is emitted exactly
/// as it always was. The first half is the byte-identity the goldens and the
/// examples rest on: `{{ title }}` with a `title()` still registers and still
/// emits `"title" => state.title().to_string(),`. The second is the F-1 fix:
/// without a `title()`, that arm is `state.title()` against a `State` that has no
/// such method, so the key is not registered and no arm is emitted at all.
#[test]
fn a_bare_interpolation_is_registered_with_a_getter_and_dropped_without_one() {
    const GETTER: &str = r#"
impl State {
    pub fn title(&self) -> String { String::new() }
}
"#;
    for mode in [RenderMode::State, RenderMode::Resolve] {
        let with = compile_with_script(r#"<p>{{ title }}</p>"#, GETTER, mode);
        assert_eq!(
            make_resolve_arms(&with),
            vec![
                r#""title" => state.title().to_string(),"#.to_string(),
                "_ => String::new(),".to_string(),
            ],
            "a getter-backed bare name is emitted byte-identically, in {mode:?}:\n{with}"
        );

        let without = compile_in_mode(r#"<p>{{ title }}</p>"#, mode);
        assert_eq!(
            make_resolve_arms(&without),
            vec!["_ => String::new(),".to_string()],
            "with no getter the arm would not compile, so the key is not \
             registered, in {mode:?}:\n{without}"
        );
    }
}

/// A `v-for` element with loop-rooted bindings on the element itself: a `:key`,
/// a plain element binding, and (on a component) the index binding.
const LOOP_ROOTED_BINDS: &str =
    r#"<li v-for="todo in todos" :key="todo.id" :data-id="todo.id">{{ todo.text }}</li>"#;
const LOOP_ROOTED_COMPONENT_BINDS: &str = r#"<TodoItem v-for="(todo, index) in todos" :key="todo.id" :todo="todo.text" :index="index" />"#;

/// Inside a `v-for` body a loop-rooted binding must be read from the loop, not
/// looked up in the resolver: `make_resolve` is built once, outside every loop,
/// so a `resolve("todo.id")` call there can never succeed because the loop value
/// never reaches the resolver. The Resolve body iterates an index over the
/// collection string, so the loop item is read the way the same body already
/// reads an interpolated one — `resolve(&format!("todos[{}].id", __idx))` — while
/// the loop index is a real binding and is read directly.
#[test]
fn resolve_mode_loop_rooted_binding_is_read_from_the_loop_item() {
    let rs = compile_in_mode(LOOP_ROOTED_BINDS, RenderMode::Resolve);
    let resolve_r = resolve_renderer(&rs);

    assert!(
        resolve_r.contains(r#"for __idx in 0..__for_count {"#),
        "the Resolve loop body binds the index:\n{resolve_r}"
    );
    assert!(
        resolve_r.contains(
            r#".set("data-id", &format!("{}", resolve(&format!("todos[{}].id", __idx))))"#
        ),
        "an element binding rooted at the loop item must read the loop item:\n{resolve_r}"
    );
    assert!(
        resolve_r.contains(
            r#"props.attrs.insert("key".to_string(), resolve(&format!("todos[{}].id", __idx)).to_string());"#
        ),
        "the `:key` must be read from the loop item:\n{resolve_r}"
    );
    assert!(
        !resolve_r.contains(r#"resolve("todo.id")"#),
        "a loop-rooted binding must not be looked up in the resolver:\n{resolve_r}"
    );
    assert!(
        !make_resolve_body(&rs).contains(r#""todo.id" =>"#),
        "a loop-rooted binding must not be registered as a resolver key either:\n{rs}"
    );

    let rs = compile_in_mode(LOOP_ROOTED_COMPONENT_BINDS, RenderMode::Resolve);
    let resolve_r = resolve_renderer(&rs);
    assert!(
        resolve_r.contains(
            r#".set("todo", &format!("{}", resolve(&format!("todos[{}].text", index))))"#
        ),
        "a component prop rooted at the loop item must read the loop item:\n{resolve_r}"
    );
    assert!(
        resolve_r.contains(r#".set("index", &format!("{}", index))"#),
        "a component prop rooted at the loop index must read the index:\n{resolve_r}"
    );
    assert!(
        !resolve_r.contains(r#"resolve("todo.text")"#)
            && !resolve_r.contains(r#"resolve("index")"#),
        "neither the item nor the index is a resolver key:\n{resolve_r}"
    );

    // Boundary: this task does not touch the loop's collection expression, which
    // is still a resolver lookup (the queued collection-key work owns it).
    assert!(
        resolve_r.contains(r#"let __for_expr = resolve("todos");"#),
        "the collection lookup must stay as it is:\n{resolve_r}"
    );
}

/// A loop-rooted expression the loop cannot read as a field path is not silently
/// left empty: it keeps the resolver lookup the diagnostic can report.
#[test]
fn resolve_mode_unreadable_loop_rooted_binding_keeps_a_reported_lookup() {
    let rs = compile_in_mode(
        r#"<li v-for="todo in todos" :data-id="todo.a + todo.b">x</li>"#,
        RenderMode::Resolve,
    );
    let resolve_r = resolve_renderer(&rs);

    assert!(
        resolve_r.contains(r#".set("data-id", &resolve("todo.a + todo.b"))"#),
        "an expression that is not a field path must stay a plain lookup:\n{resolve_r}"
    );
    assert!(
        !resolve_r.contains("todos[{}].a + todo.b"),
        "only a field path may be rewritten into an indexed read:\n{resolve_r}"
    );
}

/// The negative control. This test is deliberately loop-free: it must pass both
/// with and without the loop-rooted read, because it exists to show that reading
/// a loop item through its loop did not divert an ordinary binding away from the
/// registered resolver arm. It is a guard, not evidence for the fix.
#[test]
fn resolve_mode_non_loop_binding_still_resolves_through_the_resolver() {
    let rs = compile_template_to_rs_full_with_mode(
        r#"<div><input :value="draft" :placeholder="input_placeholder" /><p :data-active="active">x</p></div>"#,
        "App",
        None,
        Some(SCRIPT_WITH_GETTERS_AND_HANDLER),
        None,
        RenderMode::Resolve,
    )
    .expect("template compiles");

    let resolve = make_resolve_body(&rs);
    assert!(
        resolve.contains(r#""draft" =>"#)
            && resolve.contains(r#""input_placeholder" =>"#)
            && resolve.contains(r#""active" =>"#),
        "non-loop bindings must still be registered as resolver keys:\n{resolve}"
    );

    let resolve_r = resolve_renderer(&rs);
    assert!(
        resolve_r.contains(r#"resolve("draft")"#)
            && resolve_r.contains(r#"resolve("input_placeholder")"#)
            && resolve_r.contains(r#"resolve("active")"#),
        "non-loop bindings must still be read through the resolver:\n{resolve_r}"
    );
    assert!(
        !resolve_r.contains("todos[{}]"),
        "a loop-free template must gain no loop-rooted read:\n{resolve_r}"
    );
}

/// State mode already read loop-rooted bindings from the loop item. Its output
/// must be byte-for-byte what it was: `.set("data-id", &format!("{}", todo.id))`
/// and `props.attrs.insert("key".to_string(), todo.id.to_string());`.
#[test]
fn state_mode_loop_rooted_binding_output_is_unchanged() {
    let rs = compile_in_mode(LOOP_ROOTED_BINDS, RenderMode::State);
    let state_r = state_renderer(&rs);
    assert!(
        state_r.contains(
            r#"h("li", Props::new().set("data-id", &format!("{}", todo.id)), vec![text({ let __obj = &todo; __obj.text.to_string() })]); if let velox_dom::VNode::Element { ref mut props, .. } = __node { props.attrs.insert("key".to_string(), todo.id.to_string()); } __node"#
        ),
        "State-mode output must not change:\n{state_r}"
    );
    assert!(
        !state_r.contains(r#"resolve("todo.id")"#),
        "State mode must keep reading the loop item:\n{state_r}"
    );

    let rs = compile_in_mode(LOOP_ROOTED_COMPONENT_BINDS, RenderMode::State);
    let state_r = state_renderer(&rs);
    assert!(
        state_r.contains(
            r#"h("TodoItem", Props::new().set("todo", &format!("{}", todo.text)).set("index", &format!("{}", index)), vec![]); if let velox_dom::VNode::Element { ref mut props, .. } = __node { props.attrs.insert("key".to_string(), todo.id.to_string()); } __node"#
        ),
        "State-mode component output must not change:\n{state_r}"
    );
}

/// An object `:class` condition reads a `String` from the resolver, and a `String`
/// is not a condition. In a Resolve-mode loop the condition is rewritten to the
/// same indexed read the sibling `:key` binding uses, so it must be given the
/// truthiness test the rest of codegen already uses for such a read — otherwise
/// the condition sits in `if` position and does not compile (E0308).
#[test]
fn resolve_mode_class_condition_on_a_loop_item_is_a_truthiness_test() {
    for condition in [r#"todo.done"#, r#"todo.is_done"#] {
        let tpl = format!(r#"<li v-for="todo in todos" :class="{{ active: {condition} }}">x</li>"#);
        let rs = compile_in_mode(&tpl, RenderMode::Resolve);
        let resolve_r = resolve_renderer(&rs);

        assert!(
            resolve_r.contains(&format!(
                r#"if {{ let __v = resolve(&format!("todos[{{}}].{field}", __idx)); __v == "true" || (!__v.is_empty() && __v != "false") }} {{ __classes.push("active"); }}"#,
                field = condition.trim_start_matches("todo.")
            )),
            "a `String` condition must be tested for truthiness, not used as one \
             ({condition}):\n{resolve_r}"
        );
        assert!(
            !resolve_r.contains(&format!("if {condition} {{")),
            "the raw `String` read must not sit in `if` position ({condition}):\n{resolve_r}"
        );
    }
}

/// The object `:class` condition that is NOT a field read — `todo.a == todo.b` —
/// is still broken in a Resolve-mode loop, and this pins the fact instead of
/// hiding it: the generated condition names `todo`, which the Resolve renderer
/// never binds, so the module does not compile (E0425). This is unchanged from
/// before the task and identical in both modes; the loop diagnostic now reports
/// the loop it sits in, so the case is neither silent nor fixed.
#[test]
fn compound_class_condition_in_a_resolve_loop_is_still_broken() {
    let tpl = r#"<li v-for="todo in todos" :class="{ active: todo.a == todo.b }">x</li>"#;
    let rs = compile_in_mode(tpl, RenderMode::Resolve);
    let resolve_r = resolve_renderer(&rs);

    assert!(
        resolve_r.contains(r#"if todo.a == todo.b { __classes.push("active"); }"#),
        "the compound condition is still emitted raw, and it cannot compile in \
         this renderer:\n{resolve_r}"
    );
    assert!(
        resolve_r.contains("for __idx in 0..__for_count {"),
        "the body binds an index, not the item the condition names:\n{resolve_r}"
    );
    assert!(
        state_renderer(&rs).contains(r#"if todo.a == todo.b { __classes.push("active"); }"#),
        "State mode reads the same expression off the loop item, and is \
         unchanged:\n{}",
        state_renderer(&rs)
    );
}
