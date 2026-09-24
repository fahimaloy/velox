//! Task 9 (CX-11, F-13/F-18/F-19): child-combinator preservation in scoped
//! CSS, per-compound scoping, and the `unknown component` diagnostic.
//!
//! - `scope_css` must keep `>` between compounds and append `[data-v-*]` to
//!   EVERY compound, not just the descendant-most one.
//! - Template parsing must warn when a PascalCase tag is not a registered
//!   component import (known = the SFC's resolved `<script setup>` imports).

use velox_sfc::codegen::scope_css;
use velox_sfc::parse_template;

// ---------------------------------------------------------------------------
// scope_css: child combinator + per-compound scoping
// ---------------------------------------------------------------------------

#[test]
fn child_combinator_preserved_and_scoped() {
    let out = scope_css("div > .card{ color:red }", "data-v-abc");
    assert_eq!(out, "div[data-v-abc] > .card[data-v-abc]{ color:red }");
}

#[test]
fn unspaced_child_combinator_preserved_and_scoped() {
    // `div>.card` (no surrounding whitespace) must scope the same way.
    let out = scope_css("div>.card{ color:red }", "data-v-abc");
    assert_eq!(out, "div[data-v-abc] > .card[data-v-abc]{ color:red }");
}

#[test]
fn descendant_selector_scopes_every_compound() {
    let out = scope_css(".header h1{ color: red }", "data-v-abc");
    assert_eq!(out, ".header[data-v-abc] h1[data-v-abc]{ color: red }");
}

#[test]
fn sibling_combinators_preserved_in_scoped_output() {
    // Whitespace around compounds is canonicalized to single spaces and the
    // space after `,` is dropped (pre-existing behavior); the combinators
    // themselves must survive and every compound must carry the scope attr.
    let out = scope_css("h1 + p, li ~ ul{ color: red }", "data-v-abc");
    assert_eq!(
        out,
        "h1[data-v-abc] + p[data-v-abc],li[data-v-abc] ~ ul[data-v-abc]{ color: red }"
    );
}

#[test]
fn comma_lists_still_scope_every_member() {
    let out = scope_css("h1, .btn{ color: red }", "data-v-abc");
    assert_eq!(out, "h1[data-v-abc],.btn[data-v-abc]{ color: red }");
}

#[test]
fn keyframes_selectors_stay_exempt() {
    let css = "@keyframes fade { from { opacity: 0 } to { opacity: 1 } } .b { color: blue }";
    let out = scope_css(css, "data-v-abc");
    assert!(
        !out.contains("from[data-v-abc]") && !out.contains("to[data-v-abc]"),
        "keyframe selectors must not be scoped: {out}"
    );
    assert!(
        out.contains(".b[data-v-abc]{ color: blue }"),
        "rule after @keyframes must still be scoped: {out}"
    );
}

#[test]
fn media_prelude_stay_exempt_inner_selectors_scoped() {
    let css = "@media (max-width: 600px) { .a { color: red } }";
    let out = scope_css(css, "data-v-abc");
    assert!(
        out.contains("@media (max-width: 600px) {"),
        "media prelude must not be scoped: {out}"
    );
    assert!(
        out.contains(".a[data-v-abc]{ color: red }"),
        "media inner selector must be scoped: {out}"
    );
}

// ---------------------------------------------------------------------------
// Template parsing: unknown-component diagnostic
// ---------------------------------------------------------------------------

#[test]
fn unknown_component_warns() {
    let diag = parse_template("<FooBar/>", &[]).expect("template should parse");
    assert!(
        diag.warnings
            .iter()
            .any(|w| w.contains("unknown component")),
        "expected an unknown-component warning, got: {:?}",
        diag.warnings
    );
}

#[test]
fn unknown_component_warning_names_the_tag_and_position() {
    let src = "<div>\n  <FooBar/>\n</div>";
    let diag = parse_template(src, &[]).expect("template should parse");
    let warning = diag
        .warnings
        .iter()
        .find(|w| w.contains("unknown component"))
        .expect("expected an unknown-component warning");
    assert!(
        warning.contains("<FooBar>"),
        "warning should name the tag: {warning}"
    );
    assert!(
        warning.contains("at 2,"),
        "warning should carry a line/col position: {warning}"
    );
}

#[test]
fn known_component_does_not_warn() {
    let diag = parse_template("<FooBar/>", &["FooBar"]).expect("template should parse");
    assert!(
        !diag
            .warnings
            .iter()
            .any(|w| w.contains("unknown component")),
        "registered component must not warn, got: {:?}",
        diag.warnings
    );
}

#[test]
fn lowercase_tags_never_trigger_component_warnings() {
    let diag =
        parse_template(r#"<div class="Card"><input/><span>hi</span></div>"#, &[]).expect("parse");
    assert!(
        diag.warnings
            .iter()
            .all(|w| !w.contains("unknown component")),
        "lowercase tags are elements, not components: {:?}",
        diag.warnings
    );
}

#[test]
fn structural_parse_warnings_surface_through_the_channel() {
    // Lenient parse warnings (unclosed tags at EOF) must be returned by the
    // diagnostics channel, not only printed to stderr.
    let diag = parse_template("<div><span>text", &[]).expect("lenient parse");
    assert!(
        diag.warnings
            .iter()
            .any(|w| w.contains("unclosed tag <span>")),
        "expected the unclosed-tag warning in the channel, got: {:?}",
        diag.warnings
    );
}
