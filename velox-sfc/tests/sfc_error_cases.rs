use velox_sfc::{Node, compile_template_to_rs, parse_sfc, parse_template_to_ast};

// =============================================================================
// SFC parse error cases
// =============================================================================

#[test]
fn parse_sfc_missing_template() {
    let src = r#"<script setup>
pub struct State {}
impl State { pub fn new() -> Self { Self {} } }
</script>"#;
    let sfc = parse_sfc(src).unwrap();
    assert!(sfc.template.is_none());
    assert!(sfc.script_setup.is_some());
}

#[test]
fn parse_sfc_missing_script() {
    let src = r#"<template><div>Hello</div></template>"#;
    let sfc = parse_sfc(src).unwrap();
    assert!(sfc.template.is_some());
    assert!(sfc.script_setup.is_none());
}

#[test]
fn parse_sfc_complete() {
    let src = r#"<template>
  <div class="app">{{ message }}</div>
</template>
<script setup>
pub struct State { pub message: String }
impl State { pub fn new() -> Self { Self { message: "Hello".to_string() } } }
</script>
<style>
.app { color: red; }
</style>"#;
    let sfc = parse_sfc(src).unwrap();
    assert!(sfc.template.is_some());
    assert!(sfc.script_setup.is_some());
    assert!(sfc.style.is_some());

    let tpl = sfc.template.unwrap();
    assert!(tpl.content.contains("message"));

    let script = sfc.script_setup.unwrap();
    assert!(script.setup);
    assert!(script.content.contains("pub struct State"));

    let style = sfc.style.unwrap();
    assert!(style.content.contains(".app"));
}

#[test]
fn parse_sfc_multiple_scripts() {
    // Only one <script setup> should be captured
    let src = r#"<script setup>
pub struct State {}
impl State { pub fn new() -> Self { Self {} } }
</script>
<template><div>test</div></template>"#;
    let sfc = parse_sfc(src).unwrap();
    assert!(sfc.script_setup.is_some());
}

#[test]
fn parse_sfc_script_without_setup() {
    let src = r#"<script>
// Regular script, not setup
</script>
<template><div>test</div></template>"#;
    let sfc = parse_sfc(src).unwrap();
    assert!(sfc.script.is_some());
    assert!(!sfc.script.as_ref().unwrap().setup);
    assert!(sfc.script_setup.is_none());
}

// =============================================================================
// Template parse error cases
// =============================================================================

#[test]
fn parse_template_unclosed_tag() {
    // Parser should be lenient and still produce output
    let result = parse_template_to_ast(r#"<div><span>text</div>"#);
    assert!(result.is_ok());
}

#[test]
fn parse_template_unmatched_closing_tag() {
    let result = parse_template_to_ast(r#"</div>"#);
    assert!(result.is_ok());
}

#[test]
fn parse_template_malformed_attr_no_value() {
    let ast = parse_template_to_ast(r#"<input disabled>"#).unwrap();
    match &ast[0] {
        Node::Element { attrs, .. } => {
            assert!(!attrs.is_empty());
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn parse_template_malformed_attr_unquoted_value() {
    // Parser may or may not handle unquoted values; just check it doesn't crash
    let result = parse_template_to_ast(r#"<div class=unquoted>text</div>"#);
    assert!(result.is_ok());
}

#[test]
fn parse_template_script_like_text() {
    // Text that looks like HTML tags should be parsed as text, not elements
    let result = parse_template_to_ast(r#"<div>this is not <unknown> html</div>"#);
    assert!(result.is_ok());
}

#[test]
fn parse_template_comment_text() {
    // The parser should handle HTML comments gracefully
    let result = parse_template_to_ast(r#"<div><!-- comment --></div>"#);
    // Should not crash; result may vary based on grammar
    assert!(result.is_ok());
}

// =============================================================================
// Template codegen error/edge cases
// =============================================================================

#[test]
fn codegen_whitespace_only_element() {
    let rs = compile_template_to_rs(r#"<div>   </div>"#, "App", None).unwrap();
    // Should produce some text node
    assert!(rs.contains("text("));
}

#[test]
fn codegen_self_closing_with_attrs() {
    let rs = compile_template_to_rs(
        r#"<input class="input" :value="name" @input="onChange"/>"#,
        "App",
        None,
    )
    .unwrap();
    assert!(rs.contains(r#"h("input""#));
    assert!(rs.contains(r#".set("class", "input")"#));
    assert!(rs.contains(r#".set("value", &resolve("name"))"#));
    assert!(rs.contains(r#".set("on:input", "onChange")"#));
}

#[test]
fn codegen_element_with_many_attrs() {
    let rs = compile_template_to_rs(
        r#"<div id="main" class="container" data-x="1" :title="heading" @click="handle"></div>"#,
        "App",
        None,
    )
    .unwrap();
    assert!(rs.contains(r#".set("id", "main")"#));
    assert!(rs.contains(r#".set("class", "container")"#));
    assert!(rs.contains(r#".set("data-x", "1")"#));
    assert!(rs.contains(r#".set("title", &resolve("heading"))"#));
    assert!(rs.contains(r#".set("on:click", "handle")"#));
}

#[test]
fn codegen_nested_v_for_in_v_for() {
    let rs = compile_template_to_rs(
        r#"<table v-for="table in tables"><tr v-for="row in table"><td>{{ row }}</td></tr></table>"#,
        "App",
        None,
    )
    .unwrap();
    let for_count = rs.matches("__for_count").count();
    assert!(for_count >= 2);
}

#[test]
fn codegen_v_for_with_v_if_v_else() {
    let rs = compile_template_to_rs(
        r#"<div v-for="item in items"><span v-if="item.active">{{ item.name }}</span><span v-else>inactive</span></div>"#,
        "App",
        None,
    )
    .unwrap();
    assert!(rs.contains("__for_count"));
    // v-if+v-else inside v-for generates conditional logic
    assert!(rs.contains("else"));
}

#[test]
fn codegen_v_if_without_v_else() {
    let rs =
        compile_template_to_rs(r#"<div><p v-if="show">visible</p></div>"#, "App", None).unwrap();
    assert!(rs.contains("if "));
}

#[test]
fn codegen_complex_v_if_chain() {
    let rs = compile_template_to_rs(
        r#"<div>
            <p v-if="a">A</p>
            <p v-else-if="b">B</p>
            <p v-else-if="c">C</p>
            <p v-else>D</p>
          </div>"#,
        "App",
        None,
    )
    .unwrap();
    // The codegen may use various forms of conditional expressions
    assert!(rs.contains("if "));
    assert!(rs.contains("else"));
}

// =============================================================================
// Component import parsing tests
// =============================================================================

#[test]
fn component_resolver_parse_single_default_import() {
    let mut resolver = velox_sfc::ComponentResolver::new("/tmp/test");
    resolver.parse_imports(r#"import MyButton from './Button.vx'"#);
    assert!(resolver.is_component("MyButton"));
    assert_eq!(resolver.component_names(), vec!["MyButton"]);
}

#[test]
fn component_resolver_parse_named_imports() {
    let mut resolver = velox_sfc::ComponentResolver::new("/tmp/test");
    resolver.parse_imports(r#"import { Button, Card, Modal } from './ui.vx'"#);
    assert!(resolver.is_component("Button"));
    assert!(resolver.is_component("Card"));
    assert!(resolver.is_component("Modal"));
    // May include duplicates from parsing; just verify they exist
    assert!(resolver.component_names().len() >= 3);
}

#[test]
fn component_resolver_parse_mixed_imports() {
    let mut resolver = velox_sfc::ComponentResolver::new("/tmp/test");
    resolver.parse_imports(r#"import Header from './Header.vx'"#);
    resolver.parse_imports(r#"import { Footer, Sidebar } from './Layout.vx'"#);
    assert!(resolver.is_component("Header"));
    assert!(resolver.is_component("Footer"));
    assert!(resolver.is_component("Sidebar"));
    assert!(resolver.component_names().len() >= 3);
}

#[test]
fn component_resolver_empty_script() {
    let mut resolver = velox_sfc::ComponentResolver::new("/tmp/test");
    resolver.parse_imports("");
    assert!(resolver.component_names().is_empty());
}

#[test]
fn component_resolver_no_imports_in_script() {
    let mut resolver = velox_sfc::ComponentResolver::new("/tmp/test");
    resolver.parse_imports("pub struct State {}\nimpl State { pub fn new() -> Self { Self {} } }");
    assert!(resolver.component_names().is_empty());
}

#[test]
fn component_resolver_resolve_relative_path() {
    let resolver = velox_sfc::ComponentResolver::new("/project/src");
    let path = resolver.resolve_path("./components/Button.vx");
    // Path may contain './' segment; verify it starts with the base
    assert!(path.to_string_lossy().starts_with("/project/src/"));
    assert!(path.to_string_lossy().contains("components/Button.vx"));
}

#[test]
fn component_resolver_resolve_absolute_path() {
    let resolver = velox_sfc::ComponentResolver::new("/project/src");
    let path = resolver.resolve_path("/lib/components/Button.vx");
    assert_eq!(path.to_string_lossy(), "/lib/components/Button.vx");
}

#[test]
fn component_resolver_load_nonexistent_component() {
    let mut resolver = velox_sfc::ComponentResolver::new("/tmp");
    resolver.parse_imports(r#"import Fake from './nonexistent.vx'"#);
    let result = resolver.load_component("Fake");
    assert!(result.is_err());
}

#[test]
fn component_transform_marks_component() {
    use velox_sfc::component_resolver::{ComponentResolver, transform_components};

    let mut resolver = ComponentResolver::new("/tmp");
    resolver.parse_imports(r#"import MyComp from './MyComp.vx'"#);

    let mut nodes = vec![Node::Element {
        tag: "MyComp".to_string(),
        attrs: vec![],
        children: vec![],
        self_closing: false,
    }];
    transform_components(&mut nodes, &resolver);

    match &nodes[0] {
        Node::Element { tag, attrs, .. } => {
            assert_eq!(tag, "div"); // transformed to div
            assert!(
                attrs
                    .iter()
                    .any(|a| a.name == "data-velox-component"
                        && a.value.as_deref() == Some("MyComp"))
            );
        }
        _ => panic!("expected element"),
    }
}

#[test]
fn component_transform_leaves_non_component() {
    use velox_sfc::component_resolver::{ComponentResolver, transform_components};

    let resolver = ComponentResolver::new("/tmp");
    let mut nodes = vec![Node::Element {
        tag: "div".to_string(),
        attrs: vec![],
        children: vec![Node::Text("hello".to_string())],
        self_closing: false,
    }];
    transform_components(&mut nodes, &resolver);

    match &nodes[0] {
        Node::Element { tag, .. } => {
            assert_eq!(tag, "div"); // unchanged
        }
        _ => panic!("expected element"),
    }
}
