use velox_sfc::compile_template_to_rs;

#[test]
fn codegen_div_with_text() {
    let rs = compile_template_to_rs("<div>hi</div>", "App").unwrap();
    assert!(rs.contains(r#"use velox_dom::*"#));
    assert!(rs.contains(r#"h("div""#));
    assert!(rs.contains(r#"text("hi")"#));
}

#[test]
fn codegen_interpolation() {
    let rs = compile_template_to_rs("<p>Hello {{name}}</p>", "App").unwrap();
    assert!(rs.contains(r#"h("p""#));
    assert!(rs.contains(r#"text("Hello")"#) || rs.contains(r#"text("Hello ")"#));
    assert!(rs.contains(r#"format!("{}", name)"#));
}

#[test]
fn codegen_attrs() {
    let rs = compile_template_to_rs(
        r#"<input class="x" :value="count" @input="onInput"/>"#,
        "App",
    )
    .unwrap();
    assert!(rs.contains(r#".set("class", "x")"#));
    assert!(rs.contains(r#".set("value", &format!("{}", count))"#));
    assert!(rs.contains(r#".set("on:input", "onInput")"#));
}
