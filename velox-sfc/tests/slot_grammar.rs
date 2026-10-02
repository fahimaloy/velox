//! The SFC block grammar has to survive a `<template>` nested inside a
//! `<template>`, because that is the only way a named slot is written.
//!
//! These are grammar-level tests: they stop at `parse_sfc`, before any of the
//! codegen, because the failure they guard is a file that does not parse at all.
//! Everything downstream of that is unreachable.

use velox_sfc::parse_sfc;

/// A named slot in the long form. The SFC-level `<template>` block has to end at
/// the close tag that matches ITS open tag, not at the first `</template>` in
/// the file.
const NAMED_SLOT_TEMPLATE: &str = r#"<template>
<Modal :open="open">
  <template v-slot:header>
    <h1>Title</h1>
  </template>
  <p>Body</p>
</Modal>
</template>

<script setup>
pub struct State {}
</script>
"#;

#[test]
fn nested_template_does_not_end_the_sfc_block() {
    let sfc = parse_sfc(NAMED_SLOT_TEMPLATE).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let template = sfc.template.expect("a <template> block");
    assert!(
        template.content.contains("</Modal>"),
        "the template block was cut short before the component closed: {:?}",
        template.content
    );
    assert!(
        template.content.contains("v-slot:header"),
        "the nested slot fragment is missing from the block: {:?}",
        template.content
    );
}

/// Two levels deep, because one level of nesting only proves the recursion works
/// once — and `template_body` is the rule that has to recurse for every level.
#[test]
fn two_levels_of_nesting_parse() {
    let source = r#"<template>
<Outer>
  <template v-slot:outer>
    <Inner>
      <template v-slot:inner>
        <span>deep</span>
      </template>
    </Inner>
  </template>
</Outer>
</template>
"#;
    let sfc = parse_sfc(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let content = &sfc.template.expect("a <template> block").content;
    assert!(
        content.contains("</Outer>"),
        "outer close lost: {content:?}"
    );
    assert!(
        content.contains("</Inner>"),
        "inner close lost: {content:?}"
    );
}

/// A `<template>` with no attributes is still a nested block, not the end of the
/// SFC one: the `>` in the negative lookahead has to be part of what is
/// distinguished, or `<templatex` would be read as a nested open tag.
#[test]
fn a_bare_nested_template_parses() {
    let source =
        "<template>\n<Outer>\n<template>\n<span>x</span>\n</template>\n</Outer>\n</template>\n";
    let sfc = parse_sfc(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let content = &sfc.template.expect("a <template> block").content;
    assert!(
        content.contains("</Outer>"),
        "outer close lost: {content:?}"
    );
}

/// A tag that merely STARTS with the same letters is not a nested block. If the
/// lookahead only tested `<template`, `<templates>` would be read as a nested
/// open tag and the whole body after it would be consumed as one.
#[test]
fn a_tag_prefixed_with_template_is_not_a_nested_block() {
    let source = "<template>\n<templates>x</templates>\n</template>\n";
    let sfc = parse_sfc(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let content = &sfc.template.expect("a <template> block").content;
    assert!(
        content.contains("<templates>"),
        "the body should be intact: {content:?}"
    );
}

/// The `#` shorthand on a nested `<template>`, including the `= "slotProps"`
/// value. The value has to be consumed by the grammar, or the `>` this rule ends
/// on lands inside the author's tag and the block does not close.
#[test]
fn slot_shorthand_with_a_value_parses() {
    let source = r#"<template>
<Modal>
  <template #footer="slotProps">
    <button>Close</button>
  </template>
</Modal>
</template>
"#;
    let sfc = parse_sfc(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let content = &sfc.template.expect("a <template> block").content;
    assert!(
        content.contains("</Modal>"),
        "the block was cut short: {content:?}"
    );
}

/// A `v-slot` binding on the SFC-level `<template>` is legal markup as far as the
/// grammar is concerned; nothing binds it there, and rejecting it would be a
/// parse error for something the template parser handles.
#[test]
fn slot_attr_on_a_nested_template_with_extra_attributes_parses() {
    let source = r#"<template>
<Modal>
  <template v-slot:header class="ignored">
    <h1>Title</h1>
  </template>
</Modal>
</template>
"#;
    let sfc = parse_sfc(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let content = &sfc.template.expect("a <template> block").content;
    assert!(
        content.contains("</Modal>"),
        "the block was cut short: {content:?}"
    );
}
