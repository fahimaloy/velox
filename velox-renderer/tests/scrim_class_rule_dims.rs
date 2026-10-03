//! PIXEL PROOF: an `rgba()` scrim written as a CLASS RULE dims the page.
//!
//! ## Why this file exists
//!
//! `DeclarationParser::parse_value` (`velox-style/src/lib.rs`) collected tokens
//! with `next_including_whitespace()` and never entered the nested block that
//! holds a function's arguments, so every function value was truncated at its
//! first `(`. `.scrim { background: rgba(0, 0, 0, 0.5) }` reached the renderer
//! as `rgba(` — unparseable — and the box painted NOTHING. A dialog therefore
//! opened with no dim behind it at all.
//!
//! `velox-dom/tests/composer_pixels.rs:33` documents this defect and works
//! around it by using an INLINE `style` (its invariant #2), because an inline
//! attribute never goes through the stylesheet parser. That workaround is what
//! hid the bug. This file removes the need for it: the class-rule form and the
//! inline form are asserted to be byte-identical renderings.
//!
//! ## Invariants
//!
//! 1. `render_vnode_to_rgba` is used, never `render_vnode_to_raster_png`: it
//!    runs `prepare_frame`, which is what applies the cascade before paint.
//! 2. The comparison is against the INLINE rendering of the same value, so the
//!    test cannot depend on knowing where the box landed or what the clear
//!    colour is. `class_scrim == inline_scrim` is the whole claim.
//! 3. `no_scrim != class_scrim` is asserted too. Without it the equality in (2)
//!    would be trivially true if the scrim painted nothing in BOTH renderings
//!    — i.e. it would green on a second copy of the same bug.
//!
//! Run with:
//!   cargo test -p velox-renderer --features skia-native --test scrim_class_rule_dims

#![cfg(all(feature = "skia-native", unix))]

use velox_dom::{Props, VNode, h};
use velox_renderer::render_vnode_to_rgba;
use velox_style::Stylesheet;

const W: i32 = 200;
const H: i32 = 40;

/// How the scrim's COLOUR is declared. This is the only thing that varies
/// between the three renderings; the geometry is always the same.
#[derive(Clone, Copy, PartialEq)]
enum Scrim {
    /// No scrim element at all — the control.
    None,
    /// The colour is an INLINE `style` attribute, which never goes through the
    /// stylesheet parser. This is the form that has always worked.
    Inline,
    /// The colour is a CLASS RULE, which does go through the parser.
    Class,
}

/// The page under the scrim: a white box with a left-hand half available to be
/// covered.
fn tree(s: Scrim) -> VNode {
    let scrim = match s {
        Scrim::None => vec![],
        Scrim::Inline => vec![h(
            "div",
            Props::new()
                .set("class", "scrim")
                .set("style", "background: rgba(0, 0, 0, 0.5);"),
            vec![],
        )],
        Scrim::Class => vec![h("div", Props::new().set("class", "scrim"), vec![])],
    };
    h(
        "div",
        Props::new().set(
            "style",
            "display: flex; flex-direction: row; width: 200px; height: 40px; background: #ffffff;",
        ),
        scrim,
    )
}

/// `.scrim` supplies the scrim's geometry and (in the `Class` case) its colour.
fn render(s: Scrim) -> Vec<u8> {
    let sheet =
        Stylesheet::parse(".scrim { width: 100px; height: 40px; background: rgba(0, 0, 0, 0.5); }");
    render_vnode_to_rgba(&tree(s), &sheet, W, H).expect("render")
}

/// Sample one pixel. The buffer is premultiplied with opaque alpha, so the RGB
/// triple is the colour directly.
fn px(buf: &[u8], x: i32, y: i32) -> [u8; 3] {
    let i = ((y * W + x) * 4) as usize;
    [buf[i], buf[i + 1], buf[i + 2]]
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

#[test]
fn an_rgba_scrim_from_a_class_rule_paints_exactly_like_an_inline_one() {
    let no_scrim = render(Scrim::None);
    let inline_scrim = render(Scrim::Inline);
    let class_scrim = render(Scrim::Class);

    // Invariant 3 first: the scrim must actually change something, otherwise
    // the byte-equality below would be vacuous.
    assert_ne!(
        class_scrim, no_scrim,
        "the class-rule scrim painted nothing at all — the rgba() value never reached paint"
    );

    assert_eq!(
        class_scrim,
        inline_scrim,
        "class-rule and inline rgba() scrims render differently.\n\
         class: {}\n  inline: {}",
        hex(px(&class_scrim, 50, 20)),
        hex(px(&inline_scrim, 50, 20))
    );

    // And the dim is the correct one: black at 50% over white is 0x80 per
    // channel (alpha 0.5*255 = 127, so 255*128/255 = 128).
    assert_eq!(
        hex(px(&class_scrim, 50, 20)),
        "#808080",
        "the covered half should be black-at-50% over white"
    );
    assert_eq!(
        hex(px(&class_scrim, 150, 20)),
        "#ffffff",
        "the uncovered half should still be white"
    );
}
