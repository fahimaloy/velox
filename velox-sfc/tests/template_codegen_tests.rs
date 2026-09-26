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

/// `:key` generation: the State body reads the loop field and the Resolve body
/// looks the expression up once. Both statements are pinned in full, not by
/// fragment. (This test originally pinned the Resolve body's double-interpolating
/// form as a known defect; that defect is fixed by the `:key` task.)
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
    // Resolve mode: the whole insertion statement, one literal resolver lookup.
    assert!(
        rs.contains(
            r#"if let velox_dom::VNode::Element { ref mut props, .. } = __node { props.attrs.insert("key".to_string(), resolve("todo.id").to_string()); } __node }"#,
        ),
        "Resolve mode looks the key up exactly once:\n{rs}"
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

/// The key expression must be interpolated exactly once, through a single
/// `resolve("…")` lookup of the literal expression — the same shape the sibling
/// `v-for` branch uses for its collection (`let __for_expr = resolve("items");`).
#[test]
fn resolve_mode_v_for_key_interpolates_the_expression_exactly_once() {
    let rs = compile_in_mode(V_FOR_KEY_MUSTACHE, RenderMode::Resolve);
    println!("-- GENERATED RS --\n{}\n-- END RS --", rs);

    assert!(
        rs.contains(r#"props.attrs.insert("key".to_string(), resolve("item.id").to_string());"#),
        "the key must be a single literal resolver lookup:\n{rs}"
    );
    // Each renderer interpolates the expression exactly once, and never rewrites
    // it into a nested lookup or a field access on a block.
    assert_eq!(
        resolve_renderer(&rs).matches("item.id").count(),
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
