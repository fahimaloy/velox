//! `@keydown` in the SFC layer: the binding a `.vx` author writes has to reach
//! generated Rust, and it has to reach it on the SAME route `@click` and
//! `@input` already use.
//!
//! # Why this file exists when the SFC path is generic
//!
//! There is no `@keydown` case anywhere in this crate. `read_attribute` turns
//! `@keydown` into `AttrKind::On { name: "keydown" }` by the same two lines that
//! turn `@click` into `AttrKind::On { name: "click" }`, and both prop emitters
//! (`emit_props`, and the mode-aware chain builder) spell any `On` attribute as
//! `.set("on:<name>", "<handler>")`. So `@keydown` compiles today — which is
//! exactly why it needs pinning. Every one of those four lines could be
//! specialised to a known event (`match name { "click" => …, "input" => … }`)
//! and `@keydown` would keep compiling *as far as the author could tell*: no
//! error, no warning, a `data-focus-on="F2"` that quietly stops focusing
//! anything. A source assertion is the only thing that notices, because the
//! failure is an inert string attribute — the same shape a template written
//! before this feature existed has.
//!
//! # What is deliberately NOT here
//!
//! Nothing in this file runs the generated code. `keydown_focus_e2e.rs` does
//! that, from a real `.vx` through a real `cargo run`; these are the cheap
//! source-level pins that say *which* tokens the generator has to keep
//! emitting for that run to be possible.

use velox_sfc::{RenderMode, compile_template_to_rs_full_with_mode, parse_sfc};

/// The app the end-to-end test also compiles: a keydown binding on the host, a
/// decoy field above the real one, and the real one named and bound to a key.
///
/// The decoy exists so the assertions are about the NAME, not about "the only
/// input in the tree". `data-focus-id="decoy"` on a field that must never take
/// focus is what makes `data-focus-id="composer"` mean something.
const APP: &str = r#"
<script setup>
use std::cell::RefCell;
pub struct State {
    pub text: RefCell<String>,
    pub last_key: RefCell<String>,
}
impl State {
    pub fn new() -> Self {
        State { text: RefCell::new(String::from("draft")), last_key: RefCell::new(String::new()) }
    }
    pub fn text(&self) -> String { self.text.borrow().clone() }
    pub fn on_key(&self, key: &str) { *self.last_key.borrow_mut() = key.to_string(); }
    pub fn on_input(&self, payload: &str) { *self.text.borrow_mut() = payload.to_string(); }
}
</script>
<template>
  <div class="host" @keydown="on_key">
    <input type="text" :value="text" @input="on_input" data-focus-id="decoy" />
    <input type="text" :value="text" @input="on_input" data-focus-id="composer" data-focus-on="F2" />
  </div>
</template>
"#;

/// Compile `APP` through the real entry point a build uses.
fn generated() -> String {
    let sfc = parse_sfc(APP).expect("the fixture is a well-formed SFC");
    let tpl = sfc
        .template
        .as_ref()
        .expect("fixture has a template")
        .content
        .as_str();
    compile_template_to_rs_full_with_mode(
        tpl,
        "app",
        None,
        sfc.script_setup.as_ref().map(|s| s.content.as_str()),
        None,
        RenderMode::State,
    )
    .expect("the fixture compiles")
}

// ---------------------------------------------------------------------------
// The binding
// ---------------------------------------------------------------------------

/// `@keydown="on_key"` becomes the `on:keydown` prop the runtime reads, and an
/// arm in the dispatcher that calls the `State` method with the payload.
///
/// Both halves are load-bearing and neither implies the other. The prop alone is
/// what an inert string attribute looks like: the runtime finds a handler NAME
/// and a payload, and if `make_on_event` has no arm for that name the call
/// lands on `_ => {}` — the handler never runs and nothing says so. The arm
/// alone is a method nothing can reach. So both are asserted, and the arm is
/// asserted to be the PAYLOAD-TAKING shape, because the key name arrives as the
/// payload: a zero-arg arm would compile and receive nothing, which is the
/// whole reason `@keydown` needs a payload to be worth having.
#[test]
fn a_keydown_binding_compiles_to_an_on_keydown_prop_and_a_payload_arm() {
    let out = generated();

    assert!(
        out.contains(r#".set("on:keydown", "on_key")"#),
        "the binding did not reach the generated props as `on:keydown`. \
         `@keydown` and `@click` share one `AttrKind::On` path, so this is the \
         emission of every `on:<event>` prop collapsing to the events it already \
         knew about — which is invisible from the template, because the template \
         still parses.\n{out}"
    );

    assert!(
        out.contains(r#""on_key" => { if let Some(p) = payload { state.on_key(p);"#),
        "no dispatch arm calls `on_key` with the event payload. The key name IS \
         the payload, so a zero-argument arm would compile, match, and hand the \
         handler nothing — the binding would look wired and never fire.\n{out}"
    );
}

/// The arm is keyed on the HANDLER name, not on the event name.
///
/// This is the footgun `make_on_event` is built around: it matches on the string
/// the prop carries, and the prop carries whatever the author wrote after the
/// `@`. So `@keydown="on_key"` dispatches on `"on_key"`. An arm emitted under
/// `"keydown"` would match nothing the runtime ever asks for, and the generated
/// code would look correct in every string a source check can see.
#[test]
fn the_dispatch_arm_is_keyed_on_the_handler_name_not_the_event_name() {
    let out = generated();

    assert!(
        !out.contains(r#""keydown" =>"#),
        "an arm keyed on the EVENT name was emitted. `make_on_event` matches the \
         string the `on:keydown` prop carries, which is the handler the author \
         wrote, so `\"keydown\" => …` is an arm nothing can reach.\n{out}"
    );
    assert!(
        out.contains(r#""on_key" =>"#),
        "the arm is not keyed on the handler name either, so the keydown \
         binding is dispatched to nothing.\n{out}"
    );
}

/// `@keydown` is additive at the SFC layer: it takes no arm away from the
/// bindings that were already there.
///
/// A `@keydown` binding is not allowed to change which other keys the component
/// can react to, and the first way that would break is at the generator — a
/// handler collector or a dispatcher that kept only the newest binding kind, or
/// a map keyed on something that collided. Both remaining arms are asserted
/// with their payloads, because a zero-arg arm for `@input` would silently stop
/// delivering the typed text.
#[test]
fn a_keydown_binding_does_not_displace_click_or_input() {
    let src = APP.replace(
        r#"@keydown="on_key""#,
        r#"@click="on_key" @input="on_input" @keydown="on_key""#,
    );
    let sfc = parse_sfc(&src).expect("parses");
    let tpl = sfc
        .template
        .as_ref()
        .expect("has template")
        .content
        .as_str();
    let out = compile_template_to_rs_full_with_mode(
        tpl,
        "app",
        None,
        sfc.script_setup.as_ref().map(|s| s.content.as_str()),
        None,
        RenderMode::State,
    )
    .expect("compiles");

    for prop in [r#"on:click"#, r#"on:input"#, r#"on:keydown"#] {
        assert!(
            out.contains(&format!(r#".set("{prop}", "#)),
            "`{prop}` lost its prop once a `@keydown` binding was added to the \
             same tag. The three bindings are one `AttrKind::On` path, so a \
             collector or emitter that kept only one kind of them would show up \
             here and nowhere else.\n{out}"
        );
    }
    for arm in ["on_key", "on_input"] {
        assert!(
            out.contains(&format!(r#""{arm}" => {{ if let Some(p) = payload {{"#)),
            "the `{arm}` dispatch arm is gone. A keydown binding must not cost \
             the component a handler it already had.\n{out}"
        );
    }
}

// ---------------------------------------------------------------------------
// The focus pair
// ---------------------------------------------------------------------------

/// `data-focus-id` and `data-focus-on` reach the generated props with their
/// values byte-for-byte.
///
/// These two are the only handle an author has on focus, and they are load-
/// bearing in a way `class` is not: the runtime looks the element up by
/// COMPARING the value (`data-focus-id` against the name being focused,
/// `data-focus-on` against the key name), so anything that rewrote one — a
/// camelCase→kebab-case normaliser applied to a `data-` attribute, a lowercase
/// pass, a dropped value for a hyphenated name — would leave a well-formed
/// template whose focus binding silently matches nothing. The two values are
/// asserted exactly, including the `-` in the attribute names, which is also
/// what proves they were read as static attributes rather than as something the
/// directive parser claimed.
#[test]
fn the_focus_pair_survives_codegen_as_two_static_props() {
    let out = generated();

    for literal in [
        r#".set("data-focus-id", "decoy")"#,
        r#".set("data-focus-id", "composer")"#,
        r#".set("data-focus-on", "F2")"#,
    ] {
        assert!(
            out.contains(literal),
            "`{literal}` is not in the generated props, byte for byte. The value \
             is compared as a STRING at run time, so a re-spelling — kebab-casing, \
             a different case, a dropped hyphen — produces a template that still \
             compiles and a focus binding that matches nothing.\n{out}"
        );
    }
}

/// The focus pair is not a binding, and must not grow a dispatch arm.
///
/// `data-focus-on` reads like an event attribute and is not one: it grants
/// focus, it does not call a handler. If a future change routed it through the
/// `AttrKind::On` path — because it starts with `data-` and sits on an element
/// that also has `@input` — the visible damage would be a dispatcher arm keyed
/// on `"F2"` calling `state.F2()`, which does not compile; so this is really a
/// pin on "the two features stayed separate", asserted on the generated arm set.
#[test]
fn the_focus_pair_produces_no_dispatch_arm() {
    let out = generated();

    assert!(
        !out.contains(r#""F2" =>"#),
        "the focus grant was emitted as a dispatch arm. `data-focus-on` is a \
         declarative focus rule, not an event binding: the key name is the thing \
         it is COMPARED against, and turning it into a handler call would both \
         break the lookup and invent a `State` method named `F2`.\n{out}"
    );
}
