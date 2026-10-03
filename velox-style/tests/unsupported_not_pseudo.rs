//! Velox cannot express `:not(...)`, and it does not merely ignore it.
//!
//! # Why this test exists
//!
//! `ua.css` carries `input { padding: 6px 10px; min-height: 24px;
//! color: #000000; }` with no `type` condition, so a `checkbox` inherits the
//! padding and the forced black. Conditioning it is the obvious fix and it is
//! worse than the deviation, which is what makes the fact worth pinning.
//!
//! # What happens to `:not()`
//!
//! `split_pseudos` recognises exactly two pseudo-classes/pseudo-elements,
//! `hover` and `placeholder`, and drops anything else. `parse_selector_part`
//! then extracts the first `[...]` group as a REQUIREMENT. For
//! `input:not([type=checkbox])` the two steps compose into the opposite of
//! what was written:
//!
//! ```text
//!   written:   input:not([type=checkbox])
//!   parsed:    SelectorPart { tag: "input", attr_name: "type",
//!                             attr_value: Some("checkbox"), placeholder: false }
//!   i.e.      input[type=checkbox]
//! ```
//!
//! So a UA rule written `input:not([type=checkbox])` would apply to the
//! checkbox and NOT to the text field. Writing it is worse than not writing it,
//! and nothing in the selector tells you that.
//!
//! # What this test is for
//!
//! It is a fence, not an endorsement. It records that `:not()` is unavailable
//! so nobody reaches for it in a stylesheet, and it will FAIL — which is the
//! point — if `:not()` is implemented, because implementing it correctly means
//! the attribute group must stop being an unconditional requirement. At that
//! point `ua.css` can be conditioned on input type and
//! `ua_defaults_live::the_input_rule_is_unconditioned_on_input_type_and_that_deviation_is_pinned`
//! should be revisited.

use velox_style::Stylesheet;

/// The selector `:not()` is unavailable for, so nobody conditions a UA rule on
/// it and gets the inverse of what they wrote.
#[test]
fn the_not_pseudo_is_unsupported_and_inverts_the_condition() {
    let not = Stylesheet::parse("input:not([type=checkbox]) { color: #000000; }");
    let plain = Stylesheet::parse("input[type=checkbox] { color: #000000; }");
    let not_sel = &not.rules[0].selector;
    let plain_sel = &plain.rules[0].selector;

    assert_eq!(
        not_sel, plain_sel,
        "`input:not([type=checkbox])` no longer parses to `input[type=checkbox]`. \
         If this now fails because `:not()` is INVERTED no more, `:not()` is \
         supported: velox-style can now condition `ua.css` on input type, and \
         the deviation pinned in velox-dom/tests/ua_defaults_live.rs should be \
         fixed rather than documented."
    );
}

/// The specific field that makes the inversion dangerous: the bracket group is
/// kept as a REQUIREMENT rather than discarded along with the `not(...)`.
#[test]
fn a_not_bracket_group_is_kept_as_a_requirement() {
    let sheet = Stylesheet::parse("input:not([type=checkbox]) { color: #000000; }");
    let part = &sheet.rules[0].selector.parts[0];
    assert_eq!(part.tag, "input");
    assert_eq!(
        (part.attr_name.as_str(), part.attr_value.as_deref()),
        ("type", Some("checkbox")),
        "the bracketed group is a REQUIREMENT, so the selector matches ONLY a \
         checkbox — the inverse of the `:not()` that was written"
    );
    assert!(
        !part.placeholder,
        "sanity: nothing about this is a placeholder rule"
    );
}

/// A `:not()` with no bracket group is silently DROPPED, so it does not reject
/// the selector — it just becomes a broader rule than the author wrote, which
/// is the other way this misleads.
#[test]
fn a_not_without_a_bracket_group_broadens_to_the_bare_tag() {
    let sheet = Stylesheet::parse("input:not(checkbox) { color: #000000; }");
    let part = &sheet.rules[0].selector.parts[0];
    assert_eq!(part.tag, "input");
    assert_eq!(
        part.attr_name, "",
        "no bracket group to keep, so the selector is just `input` — every \
         input matches, which is not what `input:not(checkbox)` means"
    );
}
