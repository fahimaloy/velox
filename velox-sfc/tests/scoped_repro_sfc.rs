use velox_sfc::{generate_scope_id, parse_sfc, to_stub_rs};

#[test]
fn scope_css_media_not_corrupted() {
    let src = r#"<template><div class="a">hi</div></template>
<style scoped>
@media (max-width: 600px) { .a { color: red; } }
.b { color: blue; }
@keyframes fade { from { opacity:0; } to { opacity:1; } }
</style>"#;
    let sfc = parse_sfc(src).unwrap();
    let stub = to_stub_rs(&sfc, "TestComp");
    // Extract STYLE const content
    let start = stub.find("pub const STYLE").expect("STYLE");
    let slice = &stub[start..std::cmp::min(stub.len(), start + 2000)];
    eprintln!("STYLE slice: {}", slice);
    // @media prelude must NOT have [data-v-
    assert!(!slice.contains("@media (max-width: 600px) [data-v-"), "media prelude corrupted: {}", slice);
    assert!(!slice.contains("@media (max-width: 600px)[data-v-"), "media prelude corrupted: {}", slice);
    assert!(slice.contains("@media (max-width: 600px)"), "media prelude missing");
    // inner selectors must be scoped
    assert!(slice.contains(".a[data-v-"), "inner .a not scoped: {}", slice);
    assert!(slice.contains(".b[data-v-"), "outer .b not scoped: {}", slice);
    // keyframes inner must NOT be scoped
    assert!(!slice.contains("from[data-v-"), "keyframes from incorrectly scoped: {}", slice);
    assert!(!slice.contains("to[data-v-"), "keyframes to incorrectly scoped: {}", slice);
}

#[test]
fn scope_id_deterministic() {
    let id1 = generate_scope_id("MyComp");
    let id2 = generate_scope_id("MyComp");
    assert_eq!(id1, id2);
    assert!(id1.starts_with("data-v-"));
}

#[test]
fn scoped_template_gets_data_attr() {
    // Compile template with scope_id should add data-v attr to each element
    let tpl = r#"<div class="btn"><span>hi</span></div>"#;
    let scope = "data-v-12345678";
    let rs = velox_sfc::compile_template_to_rs_full(tpl, "App", None, None, Some(scope)).unwrap();
    eprintln!("rs: {}", rs);
    // Every h("div" ...) and h("span" ...) should have .set("data-v-12345678", "")
    assert!(rs.contains("data-v-12345678"), "scope attr not injected: {}", rs);
    assert!(rs.matches("data-v-12345678").count() >= 2, "both div and span should be scoped, found {}", rs.matches("data-v-12345678").count());
}

#[test]
fn scope_comma_list_all_scoped() {
    let src = r#"<template><div>hi</div></template><style scoped>h1, .btn { color: red; }</style>"#;
    let sfc = parse_sfc(src).unwrap();
    let stub = to_stub_rs(&sfc, "C");
    let slice = &stub[stub.find("STYLE").unwrap()..];
    assert!(slice.contains("h1[data-v-"), "h1 not scoped: {}", slice);
    assert!(slice.contains(".btn[data-v-"), ".btn not scoped: {}", slice);
    // Ensure we didn't leave unscoped comma members
    assert!(!slice.contains("h1 {"), "h1 unscoped left: {}", slice);
}
