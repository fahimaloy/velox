use velox_sfc::compile_template_to_rs;

#[test]
fn codegen_v_else_if_chain() {
    let tpl = "<div><p v-if=\"a\">A</p><p v-else-if=\"b\">B</p><p v-else-if=\"c\">C</p><p v-else>D</p></div>";
    let rs = compile_template_to_rs(tpl, "App", None).unwrap();

    // The rewrite_if_expr generates: (resolve("a") == "true" || (!resolve("a").is_empty() && resolve("a") != "false"))
    // or the simple form: !resolve("a").is_empty() or just: a
    assert!(rs.contains(r#"resolve("a")"#));
    assert!(rs.contains("else if") || rs.contains("else if"));
    assert!(rs.contains("resolve(\"b\")") || rs.contains("b)"));
    assert!(rs.contains("resolve(\"c\")") || rs.contains("c)"));
    assert!(rs.contains("else {"));
    assert!(rs.contains("text(\"A\")"));
    assert!(rs.contains("text(\"B\")"));
    assert!(rs.contains("text(\"C\")"));
    assert!(rs.contains("text(\"D\")"));
}
