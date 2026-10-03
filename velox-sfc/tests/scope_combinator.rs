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
// T1: a CSS comment must never contribute tokens to a selector prelude
// ---------------------------------------------------------------------------
//
// `scope_css` walks a stylesheet character by character and appends the scope
// attribute to every whitespace-separated token of a rule's prelude. A comment
// sitting between two rules used to be glued onto the front of the NEXT
// prelude, so `.dark .app` was emitted as
//
//     [data-v-x] .dark[data-v-x] .app[data-v-x]
//
// — a three-part descendant selector needing a scope-tagged ancestor strictly
// above the `.dark` carrier. The carrier is the tree root, whose ancestor list
// is empty, so the rule could never match in any context.
//
// Every assertion below is an EXACT emission, never a `contains("data-v-x")`:
// the weak form passes under the bug, which is how it shipped.

// Case 1 — the reported bug: a comment between two rules.
#[test]
fn comment_between_two_rules_does_not_pollute_the_next_prelude() {
    let css = ".app { color: red }\n/* Dark overrides. */\n.dark .app { color: black }\n";
    let out = scope_css(css, "data-v-x");
    assert_eq!(
        out,
        ".app[data-v-x]{ color: red }.dark[data-v-x] .app[data-v-x]{ color: black }\n"
    );
    // The bug's signature: a stray, scope-tagged, attribute-only leading
    // compound in front of `.dark`.
    assert!(
        !out.contains("[data-v-x] .dark"),
        "comment text produced a scope-tagged leading compound: {out}"
    );
}

// Case 1b — the same shape inside an at-rule container, where the comment sits
// between an `@media` prelude and the rule nested in it.
#[test]
fn comment_before_a_rule_nested_in_a_media_query_does_not_pollute_it() {
    let css = "@media (max-width: 600px) {\n/* why */\n.dark .app { color: black }\n}\n";
    let out = scope_css(css, "data-v-x");
    assert_eq!(
        out,
        "@media (max-width: 600px) {.dark[data-v-x] .app[data-v-x]{ color: black }}\n\n"
    );
}

// Case 2 — a comment INSIDE a selector prelude.
#[test]
fn comment_inside_a_selector_prelude_is_not_a_compound() {
    let out = scope_css(".foo /* x */ .bar { color: red }", "data-v-x");
    assert_eq!(out, ".foo[data-v-x] .bar[data-v-x]{ color: red }");
    // Two compounds authored, two compounds emitted — not three.
    assert!(
        !out.contains("*/"),
        "the comment's own `*/` became a scoped token: {out}"
    );
}

// Case 3 — a comment INSIDE a declaration block must not leak into the next
// rule. (The block's own contents are copied verbatim; the comment is replaced
// by a space so it can never close the block either — see case 6.)
#[test]
fn comment_inside_a_declaration_block_does_not_leak_into_the_next_rule() {
    let css = ".a { color: red; /* note */ background: blue }\n.b { color: green }\n";
    let out = scope_css(css, "data-v-x");
    assert_eq!(
        out,
        ".a[data-v-x]{ color: red;   background: blue }.b[data-v-x]{ color: green }\n"
    );
    assert!(
        !out.contains("blue[data-v-x]"),
        "a declaration was re-emitted in selector position: {out}"
    );
}

// Case 4 — a comment at the very start of the input.
#[test]
fn comment_at_the_start_of_the_input_does_not_pollute_the_first_prelude() {
    let out = scope_css("/* lead */ .a { color: red }", "data-v-x");
    assert_eq!(out, ".a[data-v-x]{ color: red }");
}

// Case 5 — an UNTERMINATED comment runs to end of input (CSS Syntax 3, "consume
// the remnants of the bad comment"). It must not hang, panic, or leak.
#[test]
fn unterminated_comment_does_not_hang_or_panic() {
    // The comment swallows `.drop`, so `.drop` must NOT become a scoped rule …
    let out = scope_css(
        ".keep { color: red }\n/* never closed\n.drop { color: blue }",
        "data-v-x",
    );
    assert!(
        out.contains(".keep[data-v-x]{ color: red }"),
        "the rule before the unterminated comment must still be scoped: {out}"
    );
    assert!(
        !out.contains(".drop[data-v-x]"),
        "text inside an unterminated comment must not be scoped: {out}"
    );
    // … and a bare, comment-only input must terminate rather than spin.
    let out = scope_css("/* only a comment, never closed", "data-v-x");
    assert!(
        !out.contains("data-v-x"),
        "a comment-only input must emit no scope attribute: {out:?}"
    );
    // A comment that swallows a closing brace must not resurrect the block.
    let out = scope_css(".a { color: red } /* never closed }", "data-v-x");
    assert_eq!(out, ".a[data-v-x]{ color: red }  ");
}

// Case 6 — `{`, `}` and `[data-v-` inside a comment are comment TEXT, not
// structure. `data-v-z` is deliberately not the scope id, so its presence in the
// output can only mean comment text leaked.
#[test]
fn comment_containing_structural_characters_is_not_structure() {
    let css = ".a { color: red; /* } { [data-v-z] */ background: blue }\n.b { color: blue }\n";
    let out = scope_css(css, "data-v-x");
    assert_eq!(
        out,
        ".a[data-v-x]{ color: red;   background: blue }.b[data-v-x]{ color: blue }\n"
    );
    assert!(
        !out.contains("data-v-z"),
        "comment text reached the output: {out}"
    );
    // The same, between rules rather than inside a block — and terminated, so
    // the rule after it survives. Under the bug the braces in the comment
    // rebalanced the block stack and `.b` landed inside `.a`'s block.
    let css = ".a { color: red }\n/* } .ghost[data-v-z] { */\n.b { color: blue }\n";
    let out = scope_css(css, "data-v-x");
    assert_eq!(
        out,
        ".a[data-v-x]{ color: red }.b[data-v-x]{ color: blue }\n"
    );
}

// Case 7 — a `/*` that is NOT a comment opener. A naive `split("/*")` fix
// truncates every one of these inputs; a real CSS tokenizer does not.
#[test]
fn comment_opener_inside_a_string_is_not_a_comment() {
    // Note the missing newline before `.b`: pre-existing behaviour, a style
    // rule's prelude whitespace is dropped by the per-compound tokenizer.
    let out = scope_css(".a { content: \"/*\"; }\n.b { color: blue }\n", "data-v-x");
    assert_eq!(
        out,
        ".a[data-v-x]{ content: \"/*\"; }.b[data-v-x]{ color: blue }\n"
    );

    let out = scope_css(".a { content: '/* not a comment */'; }\n", "data-v-x");
    assert_eq!(out, ".a[data-v-x]{ content: '/* not a comment */'; }\n");

    // `*/` inside a string must not end a comment that never started either.
    let out = scope_css(".a { content: \"*/\"; }\n", "data-v-x");
    assert_eq!(out, ".a[data-v-x]{ content: \"*/\"; }\n");
}

#[test]
fn comment_opener_inside_a_url_or_quoted_attribute_is_not_a_comment() {
    let out = scope_css(".b { background: url(/*) }\n", "data-v-x");
    assert_eq!(out, ".b[data-v-x]{ background: url(/*) }\n");

    let out = scope_css(".b { background: url(\"/*\") no-repeat }\n", "data-v-x");
    assert_eq!(out, ".b[data-v-x]{ background: url(\"/*\") no-repeat }\n");

    // A quoted attribute selector is string context too, and the scope
    // attribute must still land AFTER the closing bracket.
    let out = scope_css("a[href=\"/*\"] { color: red }\n", "data-v-x");
    assert_eq!(out, "a[href=\"/*\"][data-v-x]{ color: red }\n");

    let out = scope_css("a[href='/*'] { color: red }\n", "data-v-x");
    assert_eq!(out, "a[href='/*'][data-v-x]{ color: red }\n");
}

// Case 7b — a real comment still works in the same stylesheet, so the string
// handling did not simply disable comment stripping.
#[test]
fn comments_and_string_literals_coexist_in_one_stylesheet() {
    let css = "/* head */\n.a[href=\"/*\"] { content: \"/*\" }\n/* between */\n.dark .a { color: black }\n";
    let out = scope_css(css, "data-v-x");
    assert_eq!(
        out,
        ".a[href=\"/*\"][data-v-x]{ content: \"/*\" }.dark[data-v-x] .a[data-v-x]{ color: black }\n"
    );
}

// Comment-free CSS must pass through byte-identically: the fix is a no-op when
// there is nothing to strip.
#[test]
fn comment_free_stylesheets_are_untouched() {
    for css in [
        "div > .card{ color:red }",
        ".header h1{ color: red }",
        "h1, .btn{ color: red }",
        "@media (max-width: 600px) { .a { color: red } }",
        "@keyframes fade { from { opacity: 0 } to { opacity: 1 } } .b { color: blue }",
        "",
    ] {
        let expected = match css {
            "div > .card{ color:red }" => "div[data-v-x] > .card[data-v-x]{ color:red }",
            ".header h1{ color: red }" => ".header[data-v-x] h1[data-v-x]{ color: red }",
            "h1, .btn{ color: red }" => "h1[data-v-x],.btn[data-v-x]{ color: red }",
            "@media (max-width: 600px) { .a { color: red } }" => {
                "@media (max-width: 600px) {.a[data-v-x]{ color: red }} "
            }
            "@keyframes fade { from { opacity: 0 } to { opacity: 1 } } .b { color: blue }" => {
                "@keyframes fade { from { opacity: 0 } to { opacity: 1 }}.b[data-v-x]{ color: blue }"
            }
            _ => "",
        };
        assert_eq!(
            scope_css(css, "data-v-x"),
            expected,
            "changed output for {css:?}"
        );
    }
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
