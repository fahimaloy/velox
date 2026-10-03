//! SFC-level diagnostics: warnings surfaced by `parse_sfc`.
//!
//! `parse_sfc` splits an SFC into blocks and — when a `<template>` block is
//! present — runs the shared template-diagnostics channel
//! (`template_parse::parse_template`), storing any non-fatal warnings
//! (structural leniency notes and `unknown component` diagnostics for
//! PascalCase tags that are not registered component imports) on the returned
//! [`velox_sfc::Sfc`].
//!
//! These tests pin that user-facing surface. Warnings are non-fatal: an SFC
//! with diagnostics still parses, and the message text is single-sourced from
//! the same channel the compile path prints to stderr.

use velox_sfc::{parse_sfc, parse_template};

/// Core case: a PascalCase tag with no matching component import produces an
/// `unknown component` warning, and the SFC still parses (valid input is
/// never rejected — diagnostics only add warnings).
#[test]
fn unknown_pascal_component_emits_warning() {
    let src = "<template><UnknownComp/></template><script></script>";
    let sfc = parse_sfc(src).unwrap();
    assert!(
        sfc.warnings.iter().any(|w| w.contains("UnknownComp")),
        "expected an unknown-component warning for <UnknownComp>, got: {:?}",
        sfc.warnings
    );
}

/// The warning text is the shared template-diagnostics message, not a
/// parse_sfc-specific variant.
#[test]
fn warning_text_matches_template_diagnostics_channel() {
    let src = "<template><UnknownComp/></template><script></script>";
    let sfc = parse_sfc(src).unwrap();
    assert!(
        sfc.warnings
            .iter()
            .any(|w| w.contains("unknown component <UnknownComp>")),
        "warning text should come from the shared channel, got: {:?}",
        sfc.warnings
    );
}

/// Negative case: a component registered via a `<script setup>` import must
/// NOT produce an `unknown component` warning.
#[test]
fn known_component_import_does_not_warn() {
    let src = concat!(
        "<template><MyButton/></template>",
        "<script setup>import MyButton from './MyButton.vx';</script>",
    );
    let sfc = parse_sfc(src).unwrap();
    assert!(
        !sfc.warnings.iter().any(|w| w.contains("unknown component")),
        "imported component <MyButton> must not warn, got: {:?}",
        sfc.warnings
    );
    assert!(
        !sfc.warnings.iter().any(|w| w.contains("MyButton")),
        "no warning should mention the registered component, got: {:?}",
        sfc.warnings
    );
}

/// Lowercase tags are plain HTML elements and never produce component
/// warnings, whatever the script block contains.
#[test]
fn lowercase_tags_never_warn() {
    let src = "<template><div><span>hi</span></div></template><script setup>let x = 1;</script>";
    let sfc = parse_sfc(src).unwrap();
    assert!(
        sfc.warnings.is_empty(),
        "plain HTML markup must not warn, got: {:?}",
        sfc.warnings
    );
}

/// An SFC without a `<template>` block has no template diagnostics to
/// report.
#[test]
fn sfc_without_template_has_no_warnings() {
    let src = "<script setup>let count = 0;</script>";
    let sfc = parse_sfc(src).unwrap();
    assert!(
        sfc.warnings.is_empty(),
        "script-only SFC must not warn, got: {:?}",
        sfc.warnings
    );
}

/// Single-sourcing contract: `parse_sfc` warnings are exactly the warnings
/// the template-level channel produces for the same template content with
/// the same known-component list — the two surfaces cannot diverge.
#[test]
fn parse_sfc_warnings_match_template_channel() {
    let template = "<div><UnknownComp/></div>";
    let src = format!("<template>{template}</template><script></script>");
    let sfc = parse_sfc(&src).unwrap();
    let known: Vec<&str> = Vec::new();
    let diag = parse_template(template, &known).unwrap();
    assert_eq!(
        sfc.warnings, diag.warnings,
        "parse_sfc warnings must equal the shared channel's warnings"
    );
}
