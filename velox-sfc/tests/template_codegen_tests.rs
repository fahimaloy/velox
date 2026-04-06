use velox_sfc::compile_template_to_rs;

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
