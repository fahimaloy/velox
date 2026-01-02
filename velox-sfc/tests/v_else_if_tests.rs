use velox_sfc::compile_template_to_rs;

#[test]
fn codegen_v_else_if_chain() {
    let tpl = "<div><p v-if=\"a\">A</p><p v-else-if=\"b\">B</p><p v-else-if=\"c\">C</p><p v-else>D</p></div>";
    let rs = compile_template_to_rs(tpl, "App").unwrap();
    assert!(rs.contains("if (a)" ) || rs.contains("if(a)"));
    assert!(rs.contains("else if (b)") || rs.contains("else if(b)"));
    assert!(rs.contains("else if (c)") || rs.contains("else if(c)"));
    assert!(rs.contains("else {"));
    assert!(rs.contains("text(\"A\")"));
    assert!(rs.contains("text(\"B\")"));
    assert!(rs.contains("text(\"C\")"));
    assert!(rs.contains("text(\"D\")"));
}
