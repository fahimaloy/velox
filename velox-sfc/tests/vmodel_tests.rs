// Integration tests for v-model compile pipeline.
// Verifies that v-model directives are desugared into :value + @input bindings,
// setter methods are generated on State, and make_on_event dispatches correctly.

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
}

// =============================================================================
// v-model make_on_event dispatch — uses compile_template_to_rs
// =============================================================================

#[test]
fn vmodel_make_on_event_dispatches_with_payload() {
    let tpl = r#"<input v-model="counter"/>"#;
    let code = velox_sfc::compile_template_to_rs(tpl, "app", None).unwrap();

    // make_on_event should exist and dispatch to __vmodel_set_counter
    assert!(
        code.contains("make_on_event"),
        "missing make_on_event helper in compiled template"
    );
    assert!(
        code.contains(r#""__vmodel_set_counter""#),
        "make_on_event does not dispatch to __vmodel_set_counter"
    );
    // v-model handlers receive the payload
    assert!(
        code.contains("state.__vmodel_set_counter(p)"),
        "dispatch arm should call state.__vmodel_set_counter(p) with payload"
    );
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

    // Handler name uses underscores for dots: __vmodel_set_form_name
    assert!(
        code.contains("__vmodel_set_form_name"),
        "dot notation not converted to underscored handler name"
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
