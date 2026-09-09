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

/// A parent may attach `@event` callbacks to any component tag, so the emit
/// infrastructure (thread-local callback registry) is generated unconditionally.
#[test]
fn codegen_always_generates_emit_infrastructure() {
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

    // Emit infrastructure + render_with_callbacks are always available so a
    // parent can pass event callbacks to this component.
    assert!(
        rs.contains("EMIT_CALLBACKS"),
        "Should generate EMIT_CALLBACKS unconditionally"
    );
    assert!(
        rs.contains("render_with_callbacks"),
        "Should generate render_with_callbacks unconditionally"
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

/// Test that slot content from a parent is passed to a child component
/// via render_with_slots (no callbacks, with slot children).
#[test]
fn template_codegen_passes_slot_content_to_component() {
    let rs = compile_template_to_rs(
        r#"<div data-velox-component="MyComponent"><p>Hello Slot</p></div>"#,
        "Parent",
        None,
    )
    .unwrap();

    println!("-- SLOT CONTENT RS --\n{}\n-- END --", rs);

    // Should call render_with_slots (the 2-arg variant: props + slots)
    assert!(
        rs.contains("render_with_slots"),
        "Should call render_with_slots when component has slot children"
    );
    // Should build a slots HashMap with "default" key
    assert!(
        rs.contains("\"default\""),
        "Should pass default slot in HashMap"
    );
    // Fallback should contain the slot content
    assert!(
        rs.contains("Hello Slot"),
        "Should include slot fallback content"
    );
}

/// Test that a component with both slot children and event callbacks
/// uses render_with_slots with all 4 arguments.
#[test]
fn template_codegen_slot_content_with_callbacks() {
    let rs = compile_template_to_rs(
        r#"<div data-velox-component="MyComponent" @click="handle_click"><p>Slot content</p></div>"#,
        "Parent",
        None,
    )
    .unwrap();

    println!("-- SLOT + CB RS --\n{}\n-- END --", rs);

    // Should use render_with_slots with callbacks + slots
    assert!(
        rs.contains("render_with_slots"),
        "Should call render_with_slots when component has both callbacks and slots"
    );
    // Should contain the callback
    assert!(rs.contains("handle_click"));
    // Should contain slot content
    assert!(rs.contains("Slot content"));
}

/// Test that a component with events but no slot children still uses
/// render_with_callbacks (not the slots variant).
#[test]
fn template_codegen_callbacks_without_slots_uses_render_with_callbacks() {
    let rs = compile_template_to_rs(
        r#"<div data-velox-component="MyCounter" @change="handle_change" :count="5"></div>"#,
        "Parent",
        None,
    )
    .unwrap();

    println!("-- CB NO SLOT RS --\n{}\n-- END --", rs);

    // With callbacks but no children, should use render_with_callbacks
    assert!(
        rs.contains("render_with_callbacks"),
        "Should use render_with_callbacks when callbacks exist but no slot children"
    );
    // Should NOT generate the 4-argument render_with_slots
    assert!(
        !rs.contains("render_with_slots"),
        "Should NOT call render_with_slots when no slot children"
    );
}

/// Test that to_stub_rs generates render_with_slots in component code
/// when the template contains <slot> elements.
#[test]
fn codegen_generates_render_with_slots_for_slot_template() {
    let source = r#"
<template>
  <div class="card">
    <slot />
  </div>
</template>

<script setup>
</script>
"#;

    let sfc = parse_sfc(source).expect("should parse");
    let rs = to_stub_rs(&sfc, "Card");

    println!("-- CARD RS --\n{}\n-- END --", rs);

    // Should have SLOTS thread-local
    assert!(
        rs.contains("SLOTS"),
        "Should generate SLOTS thread-local for slot support"
    );
    // Should have set_slots function
    assert!(
        rs.contains("set_slots"),
        "Should generate set_slots function"
    );
    // Should have render_slot function
    assert!(
        rs.contains("render_slot"),
        "Should generate render_slot function"
    );
    // Should generate render_with_slots (slots-only variant)
    assert!(
        rs.contains("render_with_slots"),
        "Should generate render_with_slots function for component with <slot>"
    );
}

/// Test that a component without <slot> elements does NOT generate
/// the render_with_slots (slots-only variant) inside the component module,
/// but still has render_with_callbacks. Note that the SLOTS thread-local
/// is always generated as part of the shared emit infrastructure.
#[test]
fn codegen_no_render_with_slots_when_no_slot_elements() {
    let source = r#"
<template>
  <div>Hello World</div>
</template>

<script setup>
</script>
"#;

    let sfc = parse_sfc(source).expect("should parse");
    let rs = to_stub_rs(&sfc, "SimpleComponent");

    println!("-- SIMPLE RS --\n{}\n-- END --", rs);

    // Should still have emit infrastructure
    assert!(rs.contains("EMIT_CALLBACKS"), "Should have EMIT_CALLBACKS");
    // Should have render_with_callbacks
    assert!(
        rs.contains("render_with_callbacks"),
        "Should have render_with_callbacks"
    );
    // Should NOT generate the 2-arg render_with_slots (slots-only variant).
    // The slots-only variant body is: set_slots(slots); render_with_props(props)
    // without set_emit_callbacks. Count occurrences of render_with_slots —
    // there should be only ONE (the 4-arg variant from emit infrastructure).
    // The slots-only variant would add a SECOND one.
    let slots_count = rs.matches("fn render_with_slots").count();
    assert_eq!(
        slots_count, 1,
        "Should have exactly 1 render_with_slots (4-arg variant only), found {}",
        slots_count
    );
}

/// Test that <slot> elements in a component template generate render_slot() calls
/// with the correct slot name and fallback content.
#[test]
fn codegen_slot_elements_emit_render_slot_calls() {
    // compile_template_to_rs generates the actual render code that calls render_slot
    let rs = compile_template_to_rs(
        r#"<div><slot name="header" /><slot /><slot name="footer">Default Footer</slot></div>"#,
        "Layout",
        None,
    )
    .unwrap();

    println!("-- LAYOUT RS --\n{}\n-- END --", rs);

    // render_slot should be called with each slot name
    assert!(
        rs.contains("render_slot(\"header\""),
        "Should emit render_slot for named 'header' slot"
    );
    // The default slot should use "default" name
    assert!(
        rs.contains("render_slot(\"default\""),
        "Should emit render_slot for default slot"
    );
    // Named footer slot with fallback
    assert!(
        rs.contains("render_slot(\"footer\""),
        "Should emit render_slot for named 'footer' slot"
    );
    // Fallback content for footer slot should be present
    assert!(
        rs.contains("Default Footer"),
        "Should include fallback content for footer slot"
    );
}

/// Test that a parent component passing slot content generates
/// render_with_slots with a slots HashMap containing "default".
#[test]
fn codegen_parent_passes_slot_to_child() {
    let rs = compile_template_to_rs(
        r#"<div data-velox-component="Card"><h2>Title</h2><p>Content here</p></div>"#,
        "ParentApp",
        None,
    )
    .unwrap();

    println!("-- PARENT RS --\n{}\n-- END --", rs);

    // Should use render_with_slots (2-arg variant: props + slots)
    assert!(
        rs.contains("render_with_slots"),
        "Should call render_with_slots when child has slot content"
    );
    // Should build a HashMap with "default" key
    assert!(
        rs.contains("\"default\""),
        "Should pass slot content under 'default' key in HashMap"
    );
    // Both children should appear as slot content
    assert!(rs.contains("Title"));
    assert!(rs.contains("Content here"));
}

/// Test that v-show uses display: none CSS instead of rendering empty text.
#[test]
fn template_codegen_v_show_uses_display_none() {
    let rs = compile_template_to_rs(
        r#"<div v-show="false">Hidden</div>"#,
        "Parent",
        None,
    )
    .unwrap();

    println!("-- V-SHOW RS --\n{}\n-- END --", rs);

    // Should use display: none, NOT text("")
    assert!(
        rs.contains("display: none"),
        "v-show false should set display: none"
    );
    assert!(
        !rs.contains("text(\"\")"),
        "v-show should NOT use empty text like v-if does"
    );
    // The element should still render
    assert!(
        rs.contains("Hidden"),
        "v-show should always render content (unlike v-if)"
    );
}

/// Test that v-show with a style attribute merges display:none into existing styles.
#[test]
fn template_codegen_v_show_with_existing_style() {
    let rs = compile_template_to_rs(
        r#"<div v-show="false" style="color: red;">Content</div>"#,
        "Parent",
        None,
    )
    .unwrap();

    println!("-- V-SHOW STYLE RS --\n{}\n-- END --", rs);

    assert!(
        rs.contains("display: none"),
        "Should merge display: none into existing style"
    );
    assert!(
        rs.contains("color: red"),
        "Should preserve existing style"
    );
}

/// Test that v-show inside v-for context also uses display: none.
#[test]
fn template_codegen_v_show_in_v_for_context() {
    let rs = compile_template_to_rs(
        r#"<div v-for="item in items" :key="item.id"><span v-show="item.visible">{item.name}</span></div>"#,
        "List",
        None,
    )
    .unwrap();

    println!("-- V-SHOW VFOR RS --\n{}\n-- END --", rs);

    assert!(
        rs.contains("display: none"),
        "v-show in v-for context should use display: none"
    );
    assert!(
        !rs.contains(r#"text("")"#),
        "v-show in v-for should NOT use empty text"
    );
}
