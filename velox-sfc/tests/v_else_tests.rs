use velox_sfc::compile_template_to_rs;

#[test]
fn codegen_v_if_else_pairing() {
    let tpl = "<div><p v-if=\"show\">A</p><p v-else>B</p></div>";
    let rs = compile_template_to_rs(tpl, "App", None).unwrap();
    // should emit a conditional `if (show) { ... } else { ... }` inside children
    // rewrite_if_expr may generate various formats: simple var, is_empty check, or truthy check
    assert!(rs.contains(r#"resolve("show")"#));
    assert!(rs.contains("else"));
    assert!(rs.contains("text(\"A\")") || rs.contains("text(\"A\" )"));
    assert!(rs.contains("text(\"B\")") || rs.contains("text(\"B\" )"));
}

#[test]
fn codegen_v_if_else_with_classes() {
    let tpl = r#"<div>
      <p class="count">{{ counter }}</p>
      <p v-if="positive" class="positive">positive</p>
      <p v-else class="neutral">not positive</p>
    </div>"#;
    let rs = compile_template_to_rs(tpl, "App", None).unwrap();
    println!("Generated code for v-if/else with classes:\n{}", rs);
    
    // Basic checks
    assert!(rs.contains(r#"resolve("positive")"#));
    assert!(rs.contains("else"));
    assert!(rs.contains("text(\"positive\")") || rs.contains("text(\"positive\" )"));
    assert!(rs.contains("text(\"not positive\")") || rs.contains("text(\"not positive\" )"));
}

#[test]
fn debug_our_specific_case() {
    // This matches exactly what's in our App.vx template
    let tpl = r#"<div class="card">
      <p class="count">{{ counter }}</p>
      <p v-if="positive" class="positive">positive</p>
      <p v-else class="neutral">not positive</p>
    </div>"#;
    
    let rs = compile_template_to_rs(tpl, "test", None).unwrap();
    println!("Generated code:");
    println!("{}", rs);
    
    // The key thing to check is whether we get a proper if/else structure
    // Looking at the counter example, it should generate something like:
    // { if condition { ... } else { ... } }
    
    // Count the braces to see structure
    let open_braces: usize = rs.matches("{").count();
    let close_braces: usize = rs.matches("}").count();
    
    println!("{{ count: {}, }} count: {}", open_braces, close_braces);
    
    // Should have the if/else pattern
    assert!(rs.contains("if"), "Should contain 'if'");
    assert!(rs.contains("else"), "Should contain 'else'");
    
    // Check for the specific text outputs
    assert!(rs.contains(r#"text("positive")"#) || rs.contains(r#"text("positive" )"#), "Should have positive text");
    assert!(rs.contains(r#"text("not positive")"#) || rs.contains(r#"text("not positive" )"#), "Should have not positive text");
}

#[test]
fn debug_with_header_and_buttons() {
    // Full template like ours
    let tpl = r#"<div class="app">
      <header class="header">
        <h1>{{ title }}</h1>
      </header>
      <div class="card">
        <p class="count">{{ counter }}</p>
        <p v-if="positive" class="positive">positive</p>
        <p v-else class="neutral">not positive</p>
        <button class="btn" @click="increment">+1</button>
        <button class="btn" @click="decrement">-1</button>
        <button class="btn" @click="reset">Reset</button>
      </div>
    </div>"#;
    
    let rs = compile_template_to_rs(tpl, "test", None).unwrap();
    println!("\nFull template generated code:");
    println!("{}", rs);
    
    // Should still have proper if/else
    assert!(rs.contains("if"), "Should contain 'if'");
    assert!(rs.contains("else"), "Should contain 'else'");
}
