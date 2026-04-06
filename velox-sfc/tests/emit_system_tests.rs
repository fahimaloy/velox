use velox_sfc::codegen::to_stub_rs;
use velox_sfc::compile_template_to_rs;
use velox_sfc::parse_sfc;

/// Test that compile_template_to_rs generates callback map for component @event attrs.
#[test]
fn template_codegen_generates_callback_map_for_component_events() {
    // Template with a component tag that has @event listeners
    let rs = compile_template_to_rs(
        r#"<div data-velox-component="MyCounter" @change="handle_change" @submit="handle_submit"></div>"#,
        "Parent",
        None,
    )
    .unwrap();

    println!("-- GENERATED TEMPLATE RS --\n{}\n-- END --", rs);

    // Should generate a HashMap of callbacks
    assert!(
        rs.contains("std::collections::HashMap::from"),
        "Should generate HashMap::from for callbacks"
    );
    assert!(
        rs.contains("render_with_callbacks"),
        "Should call render_with_callbacks"
    );
    // Should contain the event names
    assert!(rs.contains("change"));
    assert!(rs.contains("submit"));
    // Should contain the handler names
    assert!(rs.contains("handle_change"));
    assert!(rs.contains("handle_submit"));
}

/// Test that a component without @event listeners still uses render_with_props.
#[test]
fn template_codegen_uses_render_with_props_when_no_events() {
    let rs = compile_template_to_rs(
        r#"<div data-velox-component="MyCounter" :count="value"></div>"#,
        "Parent",
        None,
    )
    .unwrap();

    // Should use render_with_props, not render_with_callbacks
    assert!(rs.contains("render_with_props"));
    assert!(
        !rs.contains("render_with_callbacks"),
        "Should NOT call render_with_callbacks when no @events"
    );
}

/// Test that to_stub_rs generates emit infrastructure when template has @event on components.
#[test]
fn codegen_generates_emit_infrastructure() {
    let source = r#"
<template>
  <MyCounter @change="handle_change" />
</template>

<script setup>
import MyCounter from './MyCounter.vx';
</script>
"#;

    let sfc = parse_sfc(source).expect("should parse");
    let rs = to_stub_rs(&sfc, "ParentApp");

    println!("-- GENERATED STUB RS --\n{}\n-- END --", rs);

    // Should have emit infrastructure at the module level
    assert!(
        rs.contains("EMIT_CALLBACKS"),
        "Should generate thread-local EMIT_CALLBACKS"
    );
    assert!(
        rs.contains("set_emit_callbacks"),
        "Should generate set_emit_callbacks function"
    );
    // The emit function is at the parent module level (super::emit)
    assert!(
        rs.contains("pub fn emit"),
        "Should generate emit function at parent module level"
    );
}

/// Test that to_stub_rs generates render_with_callbacks when component uses define_emits.
#[test]
fn codegen_generates_render_with_callbacks_for_emit_component() {
    let source = r#"
<template>
  <button @click="handle_click">Click me</button>
</template>

<script setup>
fn handle_click() {
    emit("change", "clicked");
}
</script>
"#;

    let sfc = parse_sfc(source).expect("should parse");
    let rs = to_stub_rs(&sfc, "MyCounter");

    println!("-- GENERATED COUNTER RS --\n{}\n-- END --", rs);

    // Should have render_with_callbacks
    assert!(
        rs.contains("render_with_callbacks"),
        "Should generate render_with_callbacks function"
    );
    // Should have the emit helper in script_rs
    assert!(
        rs.contains("pub fn emit"),
        "Should generate emit helper in script_rs"
    );
}

/// Test that codegen without @events does NOT generate emit infrastructure.
#[test]
fn codegen_skips_emit_infrastructure_without_events() {
    let source = r#"
<template>
  <div>Hello World</div>
</template>

<script setup>
fn greet() {}
</script>
"#;

    let sfc = parse_sfc(source).expect("should parse");
    let rs = to_stub_rs(&sfc, "SimpleComponent");

    // Should NOT have emit infrastructure when no component @events exist
    assert!(
        !rs.contains("EMIT_CALLBACKS"),
        "Should NOT generate EMIT_CALLBACKS when no component @events"
    );
}

/// Test callback map format in generated code.
#[test]
fn callback_map_format_is_correct() {
    let rs = compile_template_to_rs(
        r#"<div data-velox-component="Child" @update="on_update" @delete="on_delete"></div>"#,
        "Parent",
        None,
    )
    .unwrap();

    // The callback map should be a valid Rust HashMap expression
    assert!(rs.contains(r#""update""#));
    assert!(rs.contains(r#""on_update""#));
    assert!(rs.contains(r#""delete""#));
    assert!(rs.contains(r#""on_delete""#));
}
