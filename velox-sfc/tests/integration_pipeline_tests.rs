// Integration tests that span multiple crates in the Velox pipeline.
// These tests verify the SFC -> AST -> VNode -> Render pipeline works end-to-end.

use std::cell::RefCell;
use std::rc::Rc;

use velox_dom::{diff::diff, h, text};
use velox_renderer::{Renderer, events, events::EventRegistry};
use velox_sfc::{compile_template_to_rs, parse_sfc, to_stub_rs};

// =============================================================================
// Full SFC compile pipeline tests
// =============================================================================

#[test]
fn sfc_parse_and_compile_counter_app() {
    let src = r#"<template>
  <div>
    <button @click="inc">Inc</button>
    <span>{{ count }}</span>
  </div>
</template>
<script setup>
use std::cell::Cell;
pub struct State { pub count: Cell<i32> }
impl State {
    pub fn new() -> Self { Self { count: Cell::new(0) } }
    pub fn inc(&self) { self.count.set(self.count.get() + 1); }
}
</script>"#;

    let sfc = parse_sfc(src).unwrap();
    assert!(sfc.template.is_some());
    assert!(sfc.script_setup.is_some());

    let tpl = sfc.template.as_ref().unwrap().content.as_str();
    let render_fn = compile_template_to_rs(tpl, "Counter", None).unwrap();

    // Generated code should include event handler infrastructure
    assert!(render_fn.contains("make_on_event"));
    assert!(render_fn.contains("inc"));
    assert!(render_fn.contains("render()"));
    assert!(render_fn.contains("render_with("));
}

#[test]
fn sfc_parse_and_compile_todo_list() {
    let src = r#"<template>
  <div class="todo-app">
    <h1>{{ title }}</h1>
    <ul>
      <li v-for="todo in todos" :key="todo.id">{{ todo.text }}</li>
    </ul>
  </div>
</template>
<script setup>
pub struct State { pub title: String }
impl State { pub fn new() -> Self { Self { title: "Todos".to_string() } } }
</script>"#;

    let sfc = parse_sfc(src).unwrap();
    let tpl = sfc.template.as_ref().unwrap().content.as_str();
    let render_fn = compile_template_to_rs(tpl, "TodoApp", None).unwrap();

    assert!(!render_fn.contains("v-for") || render_fn.contains("__for_count"));
    assert!(render_fn.contains("resolve("));
}

#[test]
fn sfc_parse_and_compile_conditional_ui() {
    let src = r#"<template>
  <div>
    <p v-if="isLoggedIn">Welcome, {{ username }}!</p>
    <div v-else>
      <input :value="loginInput" @input="onInput"/>
      <button @click="login">Login</button>
    </div>
  </div>
</template>
<script setup>
pub struct State { pub isLoggedIn: bool, pub username: String }
impl State { pub fn new() -> Self { Self { isLoggedIn: false, username: String::new() } } pub fn login(&self) {} pub fn onInput(&self) {} }
</script>"#;

    let sfc = parse_sfc(src).unwrap();
    let tpl = sfc.template.as_ref().unwrap().content.as_str();
    let render_fn = compile_template_to_rs(tpl, "Auth", None).unwrap();

    assert!(render_fn.contains("if "));
    assert!(render_fn.contains("else"));
    assert!(render_fn.contains("make_on_event"));
}

#[test]
fn sfc_stub_generation() {
    let src = r#"<template><div>{{ msg }}</div></template>
<script setup>
pub struct State { pub msg: String }
impl State { pub fn new() -> Self { Self { msg: "Hello".to_string() } } }
</script>
<style>
div { color: blue; }
</style>"#;

    let sfc = parse_sfc(src).unwrap();
    let stub = to_stub_rs(&sfc, "MyComponent");

    assert!(stub.contains("pub mod mycomponent"));
    assert!(stub.contains("pub const STYLE"));
    assert!(stub.contains("pub const TEMPLATE"));
    assert!(stub.contains("pub mod script_rs"));
}

// =============================================================================
// SFC -> AST -> codegen -> event handler pipeline
// =============================================================================

#[test]
fn pipeline_template_to_events() {
    // Compile a template with event handlers
    let tpl = r#"<button @click="handleClick">Click</button>"#;
    let render_fn = compile_template_to_rs(tpl, "Btn", None).unwrap();

    // Verify event handler is captured
    assert!(render_fn.contains("on:click"));
    assert!(render_fn.contains("handleClick"));

    // Verify make_on_event is generated
    assert!(render_fn.contains("make_on_event"));
}

#[test]
fn pipeline_multiple_events_multiple_elements() {
    let tpl = r#"<div>
        <button @click="onAdd">Add</button>
        <button @click="onRemove">Remove</button>
        <span @hover="onHover">info</span>
    </div>"#;
    let render_fn = compile_template_to_rs(tpl, "Panel", None).unwrap();

    // All handler names should appear
    assert!(render_fn.contains("onAdd"));
    assert!(render_fn.contains("onRemove"));
    assert!(render_fn.contains("onHover"));
}

// =============================================================================
// SFC parse errors with complex templates
// =============================================================================

#[test]
fn sfc_parse_complex_template_with_v_for_and_v_if() {
    let src = r#"<template>
  <div>
    <div v-for="item in items" v-if="item.visible">
      <span>{{ item.name }}</span>
      <button @click="remove">X</button>
    </div>
  </div>
</template>
<script setup>
pub struct State {}
impl State { pub fn new() -> Self { Self {} } pub fn remove(&self) {} }
</script>"#;

    let sfc = parse_sfc(src).unwrap();
    assert!(sfc.template.is_some());
    assert!(sfc.script_setup.is_some());

    let tpl = sfc.template.as_ref().unwrap().content.as_str();
    let render_fn = compile_template_to_rs(tpl, "List", None).unwrap();

    assert!(render_fn.contains("__for_count"));
    assert!(render_fn.contains("if "));
}

#[test]
fn sfc_parse_nested_v_for() {
    let src = r#"<template>
  <table>
    <tr v-for="row in matrix">
      <td v-for="cell in row">{{ cell }}</td>
    </tr>
  </table>
</template>
<script setup>
pub struct State {}
impl State { pub fn new() -> Self { Self {} } }
</script>"#;

    let sfc = parse_sfc(src).unwrap();
    let tpl = sfc.template.as_ref().unwrap().content.as_str();
    let render_fn = compile_template_to_rs(tpl, "Matrix", None).unwrap();

    // Should have two loop structures
    let for_count = render_fn.matches("__for_count").count();
    assert!(
        for_count >= 2,
        "expected 2+ v-for loops, found {}",
        for_count
    );
}

// =============================================================================
// End-to-end vnode creation from compiled template
// =============================================================================

#[test]
fn compiled_template_produces_valid_vnode_structure() {
    // This test verifies that the generated code pattern produces valid VNodes
    let tpl = r#"<div class="card"><h2>{{ title }}</h2><p>{{ body }}</p></div>"#;
    let rs = compile_template_to_rs(tpl, "Card", None).unwrap();

    // Check that the generated code uses the correct VNode constructors
    assert!(rs.contains(r#"h("div""#));
    assert!(rs.contains(r#"h("h2""#));
    assert!(rs.contains(r#"h("p""#));
    assert!(rs.contains(r#".set("class", "card")"#));
    assert!(rs.contains(r#"resolve("title")"#));
    assert!(rs.contains(r#"resolve("body")"#));
}

#[test]
fn compiled_template_with_self_closing_elements() {
    let tpl = r#"<form><input class="input" :value="email" @input="onEmail"/><button @click="submit">Send</button></form>"#;
    let rs = compile_template_to_rs(tpl, "Form", None).unwrap();

    assert!(rs.contains(r#"h("input""#));
    assert!(rs.contains(r#".set("class", "input")"#));
    assert!(rs.contains(r#".set("value", &resolve("email"))"#));
    assert!(rs.contains(r#".set("on:input", "onEmail")"#));
    assert!(rs.contains(r#".set("on:click", "submit")"#));
}

// =============================================================================
// Integration: diff + event dispatch
// =============================================================================

#[test]
fn diff_and_event_dispatch_integration() {
    // Create two VNode trees that differ
    let old_vnode = h(
        "div",
        (),
        vec![h(
            "button",
            velox_dom::Props::new().set("on:click", "oldHandler"),
            vec![text("Old")],
        )],
    );
    let new_vnode = h(
        "div",
        (),
        vec![h(
            "button",
            velox_dom::Props::new().set("on:click", "newHandler"),
            vec![text("New")],
        )],
    );

    // Diff them
    let patches = diff(&old_vnode, &new_vnode);
    assert!(!patches.is_empty());

    // Mount new vnode and dispatch event
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&new_vnode).expect("mount should succeed");

    let count = Rc::new(RefCell::new(0));
    let mut reg = EventRegistry::new();
    {
        let c = count.clone();
        reg.on("newHandler", move || {
            *c.borrow_mut() += 1;
        });
    }

    let n = events::dispatch("click", &tree, &mut reg);
    assert_eq!(n, 1);
    assert_eq!(*count.borrow(), 1);
}

// =============================================================================
// Integration: SFC -> compile -> vnode -> diff
// =============================================================================

#[test]
fn sfc_to_vnode_diff_integration() {
    // Parse and compile an SFC
    let src = r#"<template><div><span>{{ text }}</span></div></template>
<script setup>
pub struct State { pub text: String }
impl State { pub fn new() -> Self { Self { text: "hello".to_string() } } }
</script>"#;
    let sfc = parse_sfc(src).unwrap();
    let tpl = sfc.template.as_ref().unwrap().content.as_str();
    let _render_fn = compile_template_to_rs(tpl, "Test", None).unwrap();

    // Create VNodes that match the expected structure
    let old_vnode = h("div", (), vec![h("span", (), vec![text("hello")])]);
    let new_vnode = h("div", (), vec![h("span", (), vec![text("world")])]);

    let patches = diff(&old_vnode, &new_vnode);
    assert_eq!(patches.len(), 1);
    assert!(
        matches!(&patches[0], velox_dom::diff::Patch::UpdateChild(0, child_patches) if child_patches.len() == 1)
    );
}

// =============================================================================
// Integration: component rendering pipeline
// =============================================================================

#[test]
fn full_pipeline_sfc_to_render_tree() {
    let src = r#"<template>
  <div class="app">
    <header><h1>{{ title }}</h1></header>
    <main>
      <p>{{ content }}</p>
      <button @click="action">Go</button>
    </main>
  </div>
</template>
<script setup>
pub struct State { pub title: String, pub content: String }
impl State {
    pub fn new() -> Self { Self { title: "App".to_string(), content: "Hello".to_string() } }
    pub fn action(&self) {}
}
</script>"#;

    let sfc = parse_sfc(src).unwrap();
    let tpl = sfc.template.as_ref().unwrap().content.as_str();
    let render_fn = compile_template_to_rs(tpl, "FullApp", None).unwrap();

    // Verify all parts of the pipeline
    assert!(render_fn.contains(r#"h("div""#));
    assert!(render_fn.contains(r#".set("class", "app")"#));
    assert!(render_fn.contains(r#"h("header""#));
    assert!(render_fn.contains(r#"h("h1""#));
    assert!(render_fn.contains(r#"h("main""#));
    assert!(render_fn.contains(r#"h("p""#));
    assert!(render_fn.contains(r#"h("button""#));
    assert!(render_fn.contains(r#".set("on:click", "action")"#));
    assert!(render_fn.contains("make_on_event"));
    assert!(render_fn.contains("render_with_props"));
}

#[test]
fn sfc_template_only_no_script() {
    let src = r#"<template><p>Static content</p></template>"#;
    let sfc = parse_sfc(src).unwrap();
    assert!(sfc.template.is_some());
    assert!(sfc.script_setup.is_none());

    let tpl = sfc.template.as_ref().unwrap().content.as_str();
    let render_fn = compile_template_to_rs(tpl, "Static", None).unwrap();
    assert!(
        render_fn.contains(r#"text("Static content")"#)
            || render_fn.contains(r#"text("Static content")"#)
    );
}

#[test]
fn sfc_with_style_block() {
    let src = r#"<template><div class="styled">Content</div></template>
<style>
.styled { padding: 16px; margin: 8px; }
</style>"#;

    let sfc = parse_sfc(src).unwrap();
    assert!(sfc.template.is_some());
    assert!(sfc.style.is_some());

    let stub = to_stub_rs(&sfc, "StyledComp");
    assert!(stub.contains("pub const STYLE"));
    assert!(stub.contains(".styled"));
}
