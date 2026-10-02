//! PIXEL PROOF: `clip-path: inset(...)` clips the element's OWN box, and a
//! class rule now reaches paint with the value intact.
//!
//! ## Why this file exists
//!
//! Two things were true of `clip-path` and neither was "unsupported".
//!
//! 1. `DeclarationParser::parse_value` (`velox-style/src/lib.rs`) truncated
//!    every CSS function value at its opening parenthesis, so
//!    `.card { clip-path: inset(10px) }` reached the renderer as `inset(` and
//!    no class rule could ever clip. The INLINE form always worked, because
//!    `parse_style_attr` is a plain `split(';')` that never goes through the
//!    stylesheet parser — an inline/class inconsistency, not a missing
//!    feature. `merge_styles` (velox-style/src/lib.rs) flattens class rules
//!    INTO the inline `style` attribute, so the two are now the same path and
//!    test 1 asserts they are byte-identical.
//! 2. With the value arriving intact, the real defect showed: `apply_clips`
//!    was reached only AFTER the element's own background and border had been
//!    drawn, so `clip-path: inset(20px)` on an element with a fill painted a
//!    full-bleed background and clipped only the text inside it. Masking 1 §3.1
//!    makes the reference box for `inset()` the BORDER box, so the fill and the
//!    border belong inside the clip; test 4 is the one that fails without the
//!    fix.
//!
//! ## Invariants
//!
//! 1. `render_vnode_to_rgba`, never `render_vnode_to_raster_png`: it runs
//!    `prepare_frame`, which applies the cascade before paint. A PNG render
//!    skips `compute_layout`, so the class rule would never be applied at all
//!    and the equality in test 1 would be vacuous.
//! 2. Every "nothing happened" claim is a byte-equality against a rendered
//!    control, never an absence of a panic. And every equality is paired with a
//!    `!= control` somewhere in the file, so a test cannot green on a second
//!    copy of the "clip painted nothing" bug.
//! 3. Nothing here is `#[ignore]`d. The cfg-gated renderer tests do run in the
//!    workspace job.
//!
//! ## What is deliberately NOT supported
//!
//! `circle()`, `ellipse()`, `polygon()` and `url(#svg-clip)` are other basic
//! shapes with their own reference boxes, and a percentage has no px reading
//! here. They are rejected (no clip), which test 6 pins — cleanly, as a full
//! painting, not as a crash.
//!
//! `INSET(10px)` does NOT clip (test 8). CSS function names are
//! case-insensitive, so this is a deviation from the spec; it is pinned rather
//! than fixed because `parse_style_attr`'s whole property dispatch
//! (`background-color`, `border-radius`, `line-height`, ...) is
//! case-sensitive too, and being case-insensitive for the value while the
//! property name is case-sensitive would be a worse, half-CSS answer.
//!
//! Run with:
//!   cargo test -p velox-renderer --features skia-native --test clip_path_render

#![cfg(all(feature = "skia-native", unix))]

use velox_dom::{Props, VNode, h, layout::compute_layout};
use velox_renderer::render_vnode_to_rgba;
use velox_style::Stylesheet;

const W: i32 = 200;
const H: i32 = 120;

/// The page's own opaque white fill, so "the clip cut this away" is readable as
/// white rather than as the transparent clear colour.
const WHITE: [u8; 4] = [255, 255, 255, 255];
/// The child that overhangs the inset on every side.
const BLUE: [u8; 4] = [0, 0, 255, 255];
/// The clipped element's own background (test 4).
const GREEN: [u8; 4] = [0, 255, 0, 255];

/// The `clip-path` value under test, and whether it is declared inline or as a
/// class rule. Geometry and colour never vary, so a difference between two
/// renderings can only be the clip.
#[derive(Clone, Copy, PartialEq)]
enum Clip {
    /// No `clip-path` at all — the control every other arm is compared to.
    None,
    /// Written into the `style` attribute, which never goes through the
    /// stylesheet parser.
    Inline(&'static str),
    /// Written as a class rule, which does.
    Class(&'static str),
}

impl Clip {
    fn decl(self) -> &'static str {
        match self {
            Clip::None => "",
            Clip::Inline(v) | Clip::Class(v) => v,
        }
    }

    fn is_none(self) -> bool {
        matches!(self, Clip::None)
    }
}

/// How the clipped element is decorated: with the blue child that overhangs the
/// inset on all four sides, and/or with a background of its own.
#[derive(Clone, Copy)]
struct Box3 {
    own_background: bool,
    child: bool,
}

impl Box3 {
    /// A full-bleed blue child filling the whole 100x100 element, so any
    /// missing clip is visible as blue where white is required.
    const CHILD_ONLY: Box3 = Box3 {
        own_background: false,
        child: true,
    };
    /// A background and nothing else: this is the arrangement whose fill was
    /// never clipped.
    const OWN_BACKGROUND: Box3 = Box3 {
        own_background: true,
        child: false,
    };
}

/// The page, the 100x100 element under test, and optionally a child that fills
/// it. Both are static blocks, so layout puts the subject at (0,0) and the
/// child at (0,0) inside it — asserted below rather than assumed, so a layout
/// change fails here with a clear message instead of quietly moving every
/// sample point.
fn tree(clip: Clip, deco: Box3) -> VNode {
    let mut subject = String::from("width: 100px; height: 100px;");
    if !clip.is_none() {
        match clip {
            Clip::Inline(_) => subject.push_str(&format!(" clip-path: {};", clip.decl())),
            Clip::Class(_) => subject.push_str(&format!("clip-path: {};", clip.decl())),
            Clip::None => {}
        }
    }
    if deco.own_background {
        subject.push_str(" background: #00ff00;");
    }

    let mut props = Props::new();
    if let Clip::Class(_) = clip {
        props = props.set("class", "clip");
    }
    props = props.set("style", subject);

    let children = if deco.child {
        vec![h(
            "div",
            Props::new().set("style", "width: 100px; height: 100px; background: #0000ff;"),
            vec![],
        )]
    } else {
        vec![]
    };

    h(
        "div",
        Props::new().set("style", "width: 200px; height: 120px; background: #ffffff;"),
        vec![h("div", props, children)],
    )
}

fn render(clip: Clip, deco: Box3) -> Vec<u8> {
    let sheet = match clip {
        // The class-rule form is the whole point: it must survive the
        // declaration parser intact, function arguments and all.
        Clip::Class(v) => Stylesheet::parse(&format!(".clip {{ clip-path: {v}; }}")),
        _ => Stylesheet::default(),
    };
    render_vnode_to_rgba(&tree(clip, deco), &sheet, W, H).expect("render to rgba")
}

fn px(buf: &[u8], x: i32, y: i32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    buf[i..i + 4].try_into().expect("four bytes per pixel")
}

fn hex(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3])
}

/// Byte-compare two frames, reporting the FIRST differing pixel. `assert_eq!` on
/// two 96KB frames prints every byte of both, which buries the one number worth
/// reading.
fn assert_same(actual: &[u8], expected: &[u8], what: &str) {
    if actual == expected {
        return;
    }
    if actual.len() != expected.len() {
        panic!(
            "{what}: the frames differ in size ({} vs {} bytes)",
            actual.len(),
            expected.len()
        );
    }
    let at = actual
        .iter()
        .zip(expected.iter())
        .position(|(a, b)| a != b)
        .expect("equal length");
    let p = (at / 4) as i32;
    panic!(
        "{what}: first difference at pixel ({}, {}) — painted {}, expected {}",
        p % W,
        p / W,
        hex(px(actual, p % W, p / W)),
        hex(px(expected, p % W, p / W))
    );
}

/// The `!= control` half of every equality above: two frames must not be
/// byte-identical. Asserted rather than assumed, so that a clip which clips
/// nothing fails here instead of quietly satisfying the equality beside it.
fn assert_differs(actual: &[u8], other: &[u8], what: &str) {
    if actual == other {
        panic!("{what}: the two frames are byte-identical");
    }
}

/// The geometry every test above depends on, checked once per tree shape so a
/// layout regression cannot make the pixel assertions lie.
#[test]
fn the_subject_and_its_child_are_where_these_tests_assume() {
    let laid = compute_layout(&tree(Clip::None, Box3::CHILD_ONLY), 800, 600);
    assert_eq!(
        (laid.rect.x, laid.rect.y, laid.rect.w, laid.rect.h),
        (0, 0, 200, 120),
        "the page must fill the surface from the origin"
    );
    let subject = &laid.children[0];
    assert_eq!(
        (
            subject.rect.x,
            subject.rect.y,
            subject.rect.w,
            subject.rect.h
        ),
        (0, 0, 100, 100),
        "the element under test must be 100x100 at the origin"
    );
    let child = &subject.children[0];
    assert_eq!(
        (child.rect.x, child.rect.y, child.rect.w, child.rect.h),
        (0, 0, 100, 100),
        "the child must fill the element exactly, so every inset edge cuts it"
    );
}

// ===== 1 + 2: class rule vs inline, and both vs unclipped =================

/// Test 1 and test 2 in one place, because the `!=` is what makes the
/// equality meaningful: without it, a renderer that clipped nothing in BOTH
/// arms would pass.
#[test]
fn a_class_rule_inset_paints_exactly_like_an_inline_one_and_neither_like_no_clip() {
    let unclipped = render(Clip::None, Box3::CHILD_ONLY);
    let inline = render(Clip::Inline("inset(10px)"), Box3::CHILD_ONLY);
    let class_rule = render(Clip::Class("inset(10px)"), Box3::CHILD_ONLY);

    assert_eq!(
        hex(px(&unclipped, 50, 50)),
        hex(BLUE),
        "the control must paint the child at all, or nothing below means anything"
    );

    assert_differs(
        &class_rule,
        &unclipped,
        "clip-path clipped nothing: the declaration never reached paint",
    );
    assert_differs(&inline, &unclipped, "the inline clip-path clipped nothing");

    assert_same(
        &class_rule,
        &inline,
        "class-rule and inline `clip-path` render differently",
    );

    // And the clip is at the inset, not merely somewhere: 10px in from every
    // edge is clear, 50px in is the child's own colour.
    assert_eq!(hex(px(&class_rule, 50, 50)), hex(BLUE));
    assert_eq!(hex(px(&class_rule, 5, 50)), hex(WHITE));
    assert_eq!(hex(px(&class_rule, 50, 5)), hex(WHITE));
    assert_eq!(hex(px(&class_rule, 95, 95)), hex(WHITE));
}

/// Test 2 on its own, so the "it does something" claim is readable without the
/// class-rule machinery in the way.
#[test]
fn a_clipped_element_differs_from_an_unclipped_one() {
    assert_differs(
        &render(Clip::Inline("inset(10px)"), Box3::CHILD_ONLY),
        &render(Clip::None, Box3::CHILD_ONLY),
        "an inset clip and no clip at all produced the same pixels",
    );
}

// ===== 3: a child painted past the inset edge ==============================

/// The child is 100x100 and the element is 100x100, so it overhangs the inset
/// edge on all four sides. Every sample is inside the child's rect, which is
/// what makes "white here" mean "clipped" rather than "never drawn".
#[test]
fn a_child_painted_past_the_inset_edge_is_cut_at_the_edge() {
    let clipped = render(Clip::Inline("inset(20px)"), Box3::CHILD_ONLY);

    assert_eq!(
        hex(px(&clipped, 50, 50)),
        hex(BLUE),
        "the middle of the element is inside a 20px inset and must show the child"
    );
    for (x, y, edge) in [
        (5, 50, "left"),
        (95, 50, "right"),
        (50, 5, "top"),
        (50, 95, "bottom"),
        (5, 5, "top-left corner"),
        (95, 95, "bottom-right corner"),
    ] {
        assert_eq!(
            hex(px(&clipped, x, y)),
            hex(WHITE),
            "the {edge} of a 20px inset must be clear, and the child overhangs it"
        );
    }
    // Just inside each edge the child is back, so the cut is AT the edge.
    assert_eq!(hex(px(&clipped, 21, 50)), hex(BLUE));
    assert_eq!(hex(px(&clipped, 78, 50)), hex(BLUE));
    assert_eq!(hex(px(&clipped, 50, 21)), hex(BLUE));
    assert_eq!(hex(px(&clipped, 50, 78)), hex(BLUE));
}

// ===== 4: the element's OWN background =====================================

/// The defect this file was written for: `apply_clips` used to run only after
/// the fill and the border had been drawn, so an element with a background came
/// out full-bleed with its text clipped.
#[test]
fn the_elements_own_background_is_clipped_too() {
    let clipped = render(Clip::Inline("inset(20px)"), Box3::OWN_BACKGROUND);

    assert_eq!(
        hex(px(&clipped, 50, 50)),
        hex(GREEN),
        "the middle of the element is inside the inset and must show its own fill"
    );
    for (x, y, edge) in [
        (5, 50, "left"),
        (95, 50, "right"),
        (50, 5, "top"),
        (50, 95, "bottom"),
        (95, 95, "bottom-right corner"),
    ] {
        assert_eq!(
            hex(px(&clipped, x, y)),
            hex(WHITE),
            "the element's OWN background was not clipped at the {edge} of a 20px inset"
        );
    }

    // The unclipped control is what makes the four white samples above mean
    // something: without a clip this same tree is green edge to edge.
    let unclipped = render(Clip::None, Box3::OWN_BACKGROUND);
    assert_eq!(hex(px(&unclipped, 5, 50)), hex(GREEN));
    assert_differs(
        &clipped,
        &unclipped,
        "a 20px inset left the element's own background untouched",
    );
}

/// The same claim for the class-rule path, since that is the form that was
/// previously impossible and the one a stylesheet author actually writes.
#[test]
fn a_class_rule_background_is_clipped_too() {
    let class_rule = render(Clip::Class("inset(20px)"), Box3::OWN_BACKGROUND);
    assert_eq!(hex(px(&class_rule, 5, 50)), hex(WHITE));
    assert_eq!(hex(px(&class_rule, 50, 50)), hex(GREEN));
    assert_differs(
        &class_rule,
        &render(Clip::None, Box3::OWN_BACKGROUND),
        "a class-rule 20px inset left the element's own background untouched",
    );
}

// ===== 5: the 2-, 3- and 4-value shorthands ================================

/// Each form is asymmetric on purpose: a shorthand read as its 1-value form, or
/// with its sides transposed, would still clip SOMETHING and would still differ
/// from the control, so each sample below sits where the forms disagree.
#[test]
fn the_two_value_shorthand_clips_top_bottom_and_left_right_apart() {
    // top/bottom 10px, left/right 40px: inside is x in [40,60), y in [10,90).
    let clipped = render(Clip::Inline("inset(10px 40px)"), Box3::CHILD_ONLY);
    assert_differs(
        &clipped,
        &render(Clip::None, Box3::CHILD_ONLY),
        "this shorthand form clipped nothing at all",
    );
    assert_eq!(hex(px(&clipped, 50, 50)), hex(BLUE), "centre is inside");
    assert_eq!(
        hex(px(&clipped, 30, 50)),
        hex(WHITE),
        "x=30 is inside the 40px side"
    );
    assert_eq!(
        hex(px(&clipped, 50, 5)),
        hex(WHITE),
        "y=5 is inside the 10px side"
    );
    assert_eq!(
        hex(px(&clipped, 70, 50)),
        hex(WHITE),
        "x=70 is past the 40px side"
    );
    assert_eq!(
        hex(px(&clipped, 50, 10)),
        hex(BLUE),
        "y=10 is at the 10px side"
    );
    assert_eq!(
        hex(px(&clipped, 40, 50)),
        hex(BLUE),
        "x=40 is at the 40px side"
    );
}

#[test]
fn the_three_value_shorthand_clips_top_sides_bottom() {
    // top 10, left/right 40, bottom 60: inside is x in [40,60), y in [10,40).
    let clipped = render(Clip::Inline("inset(10px 40px 60px)"), Box3::CHILD_ONLY);
    assert_differs(
        &clipped,
        &render(Clip::None, Box3::CHILD_ONLY),
        "this shorthand form clipped nothing at all",
    );
    assert_eq!(hex(px(&clipped, 50, 20)), hex(BLUE), "centre is inside");
    assert_eq!(
        hex(px(&clipped, 50, 45)),
        hex(WHITE),
        "y=45 is past the 60px bottom"
    );
    assert_eq!(
        hex(px(&clipped, 50, 8)),
        hex(WHITE),
        "y=8 is inside the 10px top"
    );
    assert_eq!(
        hex(px(&clipped, 30, 20)),
        hex(WHITE),
        "x=30 is inside the 40px side"
    );
    assert_eq!(
        hex(px(&clipped, 50, 39)),
        hex(BLUE),
        "y=39 is inside the bottom edge"
    );
}

#[test]
fn the_four_value_shorthand_clips_each_side_on_its_own() {
    // top 5, right 10, bottom 15, left 20: inside is x in [20,90), y in [5,85).
    let clipped = render(Clip::Inline("inset(5px 10px 15px 20px)"), Box3::CHILD_ONLY);
    assert_differs(
        &clipped,
        &render(Clip::None, Box3::CHILD_ONLY),
        "this shorthand form clipped nothing at all",
    );
    assert_eq!(hex(px(&clipped, 50, 50)), hex(BLUE), "centre is inside");
    assert_eq!(
        hex(px(&clipped, 10, 50)),
        hex(WHITE),
        "x=10 is inside the 20px left"
    );
    assert_eq!(
        hex(px(&clipped, 95, 50)),
        hex(WHITE),
        "x=95 is inside the 10px right"
    );
    assert_eq!(
        hex(px(&clipped, 50, 2)),
        hex(WHITE),
        "y=2 is inside the 5px top"
    );
    assert_eq!(
        hex(px(&clipped, 50, 90)),
        hex(WHITE),
        "y=90 is inside the 15px bottom"
    );
    assert_eq!(
        hex(px(&clipped, 20, 50)),
        hex(BLUE),
        "x=20 is the left edge itself"
    );
    assert_eq!(
        hex(px(&clipped, 89, 50)),
        hex(BLUE),
        "x=89 is the last right-edge column"
    );
    assert_eq!(
        hex(px(&clipped, 19, 50)),
        hex(WHITE),
        "x=19 is one column outside the left edge"
    );
}

/// A five-value `inset()` is not CSS (CSS Shapes 1 §2.1: one to four lengths).
/// Rejected like any other invalid value, and the whole declaration with it.
#[test]
fn a_five_value_inset_is_rejected_rather_than_guessed_at() {
    let rejected = render(
        Clip::Inline("inset(5px 10px 15px 20px 25px)"),
        Box3::CHILD_ONLY,
    );
    assert_same(
        &rejected,
        &render(Clip::None, Box3::CHILD_ONLY),
        "a five-value inset must be dropped, not truncated to its first four",
    );
}

// ===== 6: the shapes that are NOT supported ================================

/// `circle()`, `ellipse()`, `polygon()` and `url(#svg-clip)` are other basic
/// shapes, and a percentage has no px reading here. All four are rejected, so
/// all four paint the child in full — cleanly, and without a panic.
#[test]
fn every_shape_this_renderer_does_not_draw_is_rejected_and_paints_everything() {
    let unclipped = render(Clip::None, Box3::CHILD_ONLY);
    assert_eq!(hex(px(&unclipped, 50, 50)), hex(BLUE));
    assert_differs(
        &unclipped,
        &vec![0u8; (W * H * 4) as usize],
        "the control is blank: nothing below can prove a rejection",
    );

    for shape in [
        "circle(50%)",
        "circle(50px at 50px 50px)",
        "ellipse(40px 20px)",
        "polygon(0 0, 100px 0, 100px 100px)",
        "url(#svg-clip)",
        "inset(10% 20%)",
        "inset(10em)",
        "inset(10)",
        "none",
    ] {
        let painted = render(Clip::Inline(shape), Box3::CHILD_ONLY);
        assert_same(
            &painted,
            &unclipped,
            "`clip-path: {shape}` must be rejected outright: it painted something other \
             than the unclipped control, so it was half-honoured",
        );
        // "In full", stated directly: the corner the child covers is blue.
        assert_eq!(
            hex(px(&painted, 95, 95)),
            hex(BLUE),
            "`clip-path: {shape}` clipped the child instead of being rejected"
        );
    }
}

// ===== 7 + 8 + 9: the values that must not clip at all =====================

/// `f32::parse("1e999")` is `Ok(inf)`, so `inset(1e999px)` reaches
/// `parse_clip_inset` carrying an infinity. The pinned outcome is that the
/// DECLARATION is dropped, not that a degenerate rect is produced.
///
/// **These tests pin the intent, not the guard.** Removing the `is_finite`
/// check from `parse_px_value` leaves every one of them green: `inset_rect`
/// clamps with `.max(0.0)`, so an infinite inset becomes a zero-width rect and
/// `needs_clip` declines to apply it. The frames are identical either way. That
/// is a fact about `inset_rect`'s clamp, not evidence about the guard — see the
/// comment on `parse_px_value`, which says so explicitly. What these tests
/// genuinely lock down is the *contract*: an unrepresentable inset renders
/// exactly like no clip at all, on the subtree arrangement and on an
/// own-background arrangement. If a future change to the clamp path starts
/// erasing a subtree, this is the test that will notice.
#[test]
fn an_overflowing_inset_is_rejected_and_does_not_erase_the_subtree() {
    let overflowing = render(Clip::Inline("inset(1e999px)"), Box3::CHILD_ONLY);
    assert_same(
        &overflowing,
        &render(Clip::None, Box3::CHILD_ONLY),
        "`inset(1e999px)` must be rejected, not turned into an infinite clip rect that \
         empties the element",
    );
    assert_eq!(
        hex(px(&overflowing, 50, 50)),
        hex(BLUE),
        "the subtree vanished: an infinite inset clipped the whole element"
    );
}

/// The `NaN` spelling of the same thing, and a non-finite in only ONE of four
/// components: a per-call-site guard would have to remember every position, which
/// is why the guard lives in `parse_px_value` itself.
#[test]
fn a_nan_or_partially_overflowing_inset_is_rejected_too() {
    let unclipped = render(Clip::None, Box3::CHILD_ONLY);
    for value in [
        "inset(NaNpx)",
        "inset(1e999px 10px)",
        "inset(10px 10px 10px 1e999px)",
        "inset(-1e999px)",
    ] {
        assert_same(
            &render(Clip::Inline(value), Box3::CHILD_ONLY),
            &unclipped,
            "`clip-path: {value}` must be rejected outright",
        );
    }
}

/// Pinned: the function name is matched case-SENSITIVELY, so `INSET(10px)` is a
/// rejected declaration rather than a clipped one. See the module comment for
/// why this is pinned rather than fixed.
#[test]
fn an_uppercase_inset_is_not_a_clip() {
    let upper = render(Clip::Inline("INSET(10px)"), Box3::CHILD_ONLY);
    assert_same(
        &upper,
        &render(Clip::None, Box3::CHILD_ONLY),
        "the case-sensitivity decision changed: `INSET(10px)` now clips",
    );
    assert_differs(
        &upper,
        &render(Clip::Inline("inset(10px)"), Box3::CHILD_ONLY),
        "the control is wrong: lowercase `inset(10px)` must clip",
    );
}

/// Pinned: a negative inset is an INVALID declaration (CSS 2.1 §4.3, "negative
/// values are invalid"), so it is dropped and the element is not clipped. It
/// must not be allowed to EXPAND the clip rect, which is what a negative
/// component used to do.
#[test]
fn a_negative_inset_is_a_no_op() {
    let unclipped = render(Clip::None, Box3::CHILD_ONLY);
    for value in ["inset(-10px)", "inset(-10px 20px)", "inset(10px -20px)"] {
        assert_same(
            &render(Clip::Inline(value), Box3::CHILD_ONLY),
            &unclipped,
            "`clip-path: {value}` must be dropped as invalid, not applied as an expansion",
        );
    }
    // Sanity: the control really is unclipped, so the equality above is not a
    // no-op against a frame that clips everything.
    assert_differs(
        &unclipped,
        &render(Clip::Inline("inset(10px)"), Box3::CHILD_ONLY),
        "the control is wrong: `inset(10px)` must clip",
    );
}

/// `inset()` with no argument is `inset(0 0 0 0)` in CSS Shapes 1 §2.1 — a
/// clip identical to the element's own border box, i.e. a no-op. It must not be
/// read as an empty clip that erases everything.
#[test]
fn an_empty_inset_is_the_elements_own_box() {
    assert_same(
        &render(Clip::Inline("inset()"), Box3::CHILD_ONLY),
        &render(Clip::None, Box3::CHILD_ONLY),
        "`inset()` is a zero inset, which is the element's own box",
    );
}
