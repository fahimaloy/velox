use velox_sfc::{compile_template_to_rs, compile_template_to_rs_full};

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
        r#"<TodoInput :value="draft" :placeholder="input_placeholder" />"#,
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
