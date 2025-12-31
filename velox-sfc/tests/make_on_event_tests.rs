use velox_sfc::template_codegen;

#[test]
fn emits_make_on_event_helper() {
    // Minimal fragment with two handlers (no outer <template> wrapper)
    let src = r#"<div><button @click="inc">Inc</button><button @click="dec">Dec</button></div>"#;
    let out = template_codegen::compile_template_to_rs(src, "App").expect("compile");
    println!("GENERATED:\n{}", out);
    assert!(out.contains("pub fn make_on_event("), "make_on_event helper missing");
    assert!(out.contains("\"inc\""), "handler 'inc' not emitted");
    assert!(out.contains("\"dec\""), "handler 'dec' not emitted");
}
