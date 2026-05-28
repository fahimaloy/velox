use velox_sfc::compile_template_to_rs;

#[test]
fn codegen_v_if_else_pairing() {
    let tpl = "<div><p v-if=\"show\">A</p><p v-else>B</p></div>";
    let rs = compile_template_to_rs(tpl, "App", None).unwrap();
    // should emit a conditional `if (show) { ... } else { ... }` inside children
    // rewrite_if_expr may generate various formats: simple var, is_empty check, or truthy check
    assert!(
        rs.contains("if (show)")
            || rs.contains("if(show)")
            || rs.contains("resolve(\"show\")")
    );
    assert!(rs.contains("else"));
    assert!(rs.contains("text(\"A\")") || rs.contains("text(\"A\" )"));
    assert!(rs.contains("text(\"B\")") || rs.contains("text(\"B\" )"));
}
