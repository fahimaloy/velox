use velox_sfc::parse_sfc;

#[test]
fn splits_basic_sfc() {
    let src = r#"
<template id="root">
  <div class="app">{{ count }}</div>
</template>
<script setup lang="rs">
  // rust setup
</script>
<script>
  // regular script
</script>
<style scoped>
  .app { color: red; }
</style>
"#;

    let sfc = parse_sfc(src).expect("parse ok");
    let tpl = sfc.template.expect("template");
    assert!(tpl.content.contains("{{ count }}"));
    assert!(
        tpl.attrs
            .iter()
            .any(|a| a.name == "id" && a.value.as_deref() == Some("root"))
    );

    let ss = sfc.script_setup.expect("script_setup");
    assert!(ss.setup);
    assert!(ss.content.contains("rust setup"));

    let sc = sfc.script.expect("script");
    assert!(!sc.setup);
    assert!(sc.content.contains("regular script"));

    let st = sfc.style.expect("style");
    assert!(st.content.contains(".app"));
    assert!(st.attrs.iter().any(|a| a.name == "scoped"));
}
