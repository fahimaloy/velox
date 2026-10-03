//! PIXEL PROOF: `img-filter` is an IMAGE filter, it is not the CSS `filter`
//! property, and it applies all of a declaration or none of it.
//!
//! ## Why this file exists
//!
//! `filter` was honoured under the CSS name while doing something else
//! entirely. `apply_img_filter` has exactly two call sites
//! (`velox-renderer/src/skia_render.rs`), and both sit inside
//! `if let Some(src) = props.attrs.get("src")` — so `filter: blur(4px)` on a div,
//! on text, on a background or on a border was a total silent no-op. CSS
//! `filter` is a composited post-pass over the element's whole subtree (Filter
//! Effects 1 §2.1), so the name over-promised exactly as much as the behaviour
//! under-delivered.
//!
//! Worse, the value was applied by halves. The old loop `split(')')`, matched
//! `blur(` and `brightness(`, and discarded everything else with no diagnostic,
//! so `filter: blur(4px) grayscale(1)` blurred the image and dropped the
//! grayscale — real CSS invalidates the whole declaration on an unknown
//! function. Before the declaration-parser fix a class rule rendered unfiltered;
//! after it, the same class rule rendered half-filtered, which is the worst of
//! the three outcomes because it looks intentional.
//!
//! So the property is now `img-filter`, it says what it is, it is honoured only
//! where an image is drawn, and `parse_img_filter` is all-or-nothing.
//!
//! ## What was done to `skia_image_filter_render.rs`
//!
//! Replaced, not kept and not merely narrowed. That file asserted a golden
//! FNV-1a checksum (`0xc2e1032a`) over a PNG produced with
//! `render_vnode_to_raster_png`, was `#[ignore]`d, and carried the escape hatch
//! "Update this checksum after regenerating the raster output". It proved
//! DETERMINISM, not correctness — a wrong-but-stable frame passes it — and
//! `render_vnode_to_raster_png` skips `compute_layout`, so the layout it checked
//! was not the layout a frame paints. It also used the old property name, so it
//! had to change either way.
//!
//! ## Invariants
//!
//! 1. `render_vnode_to_rgba` with a real `Stylesheet`, so the class-rule arm
//!    goes through the declaration parser.
//! 2. Every equality is paired with a `!= control`, so no test can green on the
//!    image not being drawn at all.
//! 3. Nothing is `#[ignore]`d.
//!
//! Run with:
//!   cargo test -p velox-renderer --features skia-native --test img_filter_render

#![cfg(all(feature = "skia-native", unix))]

use std::path::{Path, PathBuf};

use velox_dom::{Props, VNode, h};
use velox_renderer::render_vnode_to_rgba;
use velox_style::Stylesheet;

const W: i32 = 64;
const H: i32 = 64;

/// The fixture's field colour, `#3366cc`.
const FIELD: [u8; 4] = [51, 102, 204, 255];
/// The fixture's disc, `#ffcc00`.
const DISC: [u8; 4] = [255, 204, 0, 255];

/// A temp directory unique to this process, outside the repository, so the
/// binary fixture is never a tracked file.
fn fixture_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("velox-img-filter-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    dir
}

/// A 16x16 PNG holding a hard-edged `#ffcc00` square in a `#3366cc` field.
///
/// Hard edges are the point: a blur only changes pixels where an edge is, so a
/// soft-edged fixture could make "the filter applied" and "the filter did
/// nothing" differ by nothing. Written with the same library that reads it back.
fn write_fixture(path: &Path) {
    use skia_safe as sk;
    let mut surface = sk::surfaces::raster_n32_premul((16, 16)).expect("fixture surface");
    let canvas = surface.canvas();
    canvas.clear(sk::Color::from_argb(255, 51, 102, 204));
    let mut paint = sk::Paint::default();
    paint.set_color(sk::Color::from_argb(255, 255, 204, 0));
    canvas.draw_rect(sk::Rect::from_xywh(4.0, 4.0, 8.0, 8.0), &paint);
    let image = surface.image_snapshot();
    #[allow(deprecated)]
    let data = image
        .encode_to_data(skia_safe::EncodedImageFormat::PNG)
        .expect("encode png fixture");
    std::fs::write(path, data.as_bytes()).expect("write png fixture");
}

fn fixture_src() -> String {
    let path = fixture_dir().join("field-with-disc.png");
    if !path.exists() {
        write_fixture(&path);
    }
    path.to_string_lossy().into_owned()
}

/// The `img-filter` declaration under test, and whether it is inline or a class
/// rule. Only the declaration and the element type vary.
#[derive(Clone, Copy, PartialEq)]
enum Decl {
    /// No declaration at all — the control.
    None,
    Inline(&'static str),
    Class(&'static str),
}

impl Decl {
    fn is_none(self) -> bool {
        matches!(self, Decl::None)
    }

    fn value(self) -> &'static str {
        match self {
            Decl::None => "",
            Decl::Inline(v) | Decl::Class(v) => v,
        }
    }
}

/// An `<img src>` stretched over the whole surface — the one element the filter
/// is honoured on — optionally carrying the declaration.
fn image_tree(decl: Decl) -> VNode {
    let mut props = Props::new().set("style", "width: 64px; height: 64px;");
    if !decl.is_none() {
        props = props.set(
            "style",
            format!("width: 64px; height: 64px; img-filter: {};", decl.value()),
        );
    }
    if let Decl::Class(_) = decl {
        props = props.set("class", "f");
    }
    h("img", props.set("src", fixture_src()), vec![])
}

/// A `<div>` with a background and the same declaration — the element the
/// property does NOT touch.
fn div_tree(decl: Decl) -> VNode {
    let mut props = Props::new().set("style", "width: 64px; height: 64px; background: #3366cc;");
    if !decl.is_none() {
        props = props.set(
            "style",
            format!(
                "width: 64px; height: 64px; background: #3366cc; img-filter: {}",
                decl.value()
            ),
        );
    }
    h("div", props, vec![])
}

fn render_img(decl: Decl) -> Vec<u8> {
    let sheet = match decl {
        Decl::Class(v) => Stylesheet::parse(&format!(".f {{ img-filter: {v}; }}")),
        _ => Stylesheet::default(),
    };
    render_vnode_to_rgba(&image_tree(decl), &sheet, W, H).expect("render img to rgba")
}

fn render_div(decl: Decl) -> Vec<u8> {
    let sheet = match decl {
        Decl::Class(v) => Stylesheet::parse(&format!(".f {{ img-filter: {v}; }}")),
        _ => Stylesheet::default(),
    };
    render_vnode_to_rgba(&div_tree(decl), &sheet, W, H).expect("render div to rgba")
}

fn px(buf: &[u8], x: i32, y: i32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    buf[i..i + 4].try_into().expect("four bytes per pixel")
}

fn hex(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3])
}

/// Byte-compare two frames, reporting the FIRST differing pixel. `assert_eq!` on
/// two whole frames prints every byte of both, which buries the one number worth
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
/// byte-identical. Asserted rather than assumed, so a no-op filter fails here
/// instead of quietly satisfying the equality beside it.
fn assert_differs(actual: &[u8], other: &[u8], what: &str) {
    if actual == other {
        panic!("{what}: the two frames are byte-identical");
    }
}

/// Every pixel at least `MARGIN` from the edge is fully opaque.
///
/// A blur pulls in the transparent backdrop along the image's own edge, so the
/// outermost ring is legitimately translucent and cannot be asserted on. A NaN
/// colour channel, by contrast, punches a hole wherever it lands — and the
/// interior is where an infinite brightness would show it.
const MARGIN: i32 = 8;

fn opaque_away_from_the_edge(buf: &[u8]) -> bool {
    for y in MARGIN..H - MARGIN {
        for x in MARGIN..W - MARGIN {
            if px(buf, x, y)[3] != 255 {
                return false;
            }
        }
    }
    true
}

// ===== 10: class rule == inline, and both do something =====================

#[test]
fn a_class_rule_blur_paints_exactly_like_an_inline_one_on_an_img() {
    let unfiltered = render_img(Decl::None);
    let inline = render_img(Decl::Inline("blur(2px)"));
    let class_rule = render_img(Decl::Class("blur(2px)"));

    assert_eq!(
        hex(px(&unfiltered, 2, 2)),
        hex(FIELD),
        "the fixture's field must reach the canvas, or nothing below means anything"
    );
    assert_eq!(hex(px(&unfiltered, 32, 32)), hex(DISC));

    assert_differs(
        &inline,
        &unfiltered,
        "the inline blur changed nothing: the filter never reached the paint",
    );
    assert_differs(
        &class_rule,
        &unfiltered,
        "the class-rule blur changed nothing: the declaration was eaten by the parser",
    );
    assert_same(
        &class_rule,
        &inline,
        "class-rule and inline `img-filter` render differently",
    );
    assert!(
        opaque_away_from_the_edge(&class_rule),
        "a blur punched a hole through the middle of the image"
    );
}

/// The fixture's disc/field boundary, asserted directly rather than only through
/// `!= control`: the blur has to move that edge. No claim is made about either
/// pixel's value — `draw_image_rect` upscales the 16px fixture with Skia's
/// default sampling, so the unfiltered edge is already a ramp.
#[test]
fn a_blur_moves_the_fixture_edge() {
    let unfiltered = render_img(Decl::None);
    let blurred = render_img(Decl::Inline("blur(2px)"));
    // The disc spans destination columns 16..48; column 16 is its left edge.
    assert_eq!(
        hex(px(&unfiltered, 32, 32)),
        hex(DISC),
        "the fixture's disc must be inside the image for this test to mean anything"
    );
    assert_ne!(
        hex(px(&blurred, 16, 32)),
        hex(px(&unfiltered, 16, 32)),
        "the blur left the disc's left edge exactly where it was"
    );
}

// ===== 11: all of a filter list, or none of it =============================

/// The half-application defect, pinned in its new state: an unknown function
/// invalidates the WHOLE declaration, so `blur(2px) grayscale(1)` applies NO
/// blur at all. `grayscale()` is not implemented here, so there is no honest way
/// to apply the first half.
#[test]
fn an_unknown_function_invalidates_the_whole_declaration_and_applies_no_blur() {
    let unfiltered = render_img(Decl::None);
    let half = render_img(Decl::Inline("blur(2px) grayscale(1)"));
    assert_same(
        &half,
        &unfiltered,
        "`blur(2px) grayscale(1)` applied part of its value; an invalid filter list must \
         be dropped whole",
    );
    assert_differs(
        &half,
        &render_img(Decl::Inline("blur(2px)")),
        "the control is wrong: `blur(2px)` alone must blur",
    );

    // Every other unimplemented function, with and without a real one beside it.
    for value in [
        "grayscale(1)",
        "blur(2px) contrast(2)",
        "blur(2px) blur(4px)",
        "blur(2px) brightness(1.2) opacity(0.5)",
        "sepia(1)",
        "blur(2px) nonsense(1)",
        "blur(2px",
        "blur",
    ] {
        assert_same(
            &render_img(Decl::Inline(value)),
            &unfiltered,
            "`img-filter: {value}` applied something; it must be dropped whole",
        );
    }
}

/// A list this renderer CAN read is applied whole, and the two functions
/// compose onto one paint rather than one overwriting the other.
#[test]
fn a_readable_two_function_list_is_applied_whole() {
    let unfiltered = render_img(Decl::None);
    let blur_only = render_img(Decl::Inline("blur(2px)"));
    let brightness_only = render_img(Decl::Inline("brightness(1.5)"));
    let both = render_img(Decl::Inline("blur(2px) brightness(1.5)"));

    assert_differs(&blur_only, &unfiltered, "the blur half applied nothing");
    assert_differs(
        &brightness_only,
        &unfiltered,
        "the brightness half applied nothing",
    );
    assert_differs(
        &both,
        &unfiltered,
        "a readable two-function list applied nothing",
    );
    assert_differs(&both, &blur_only, "the brightness half was dropped");
    assert_differs(&both, &brightness_only, "the blur half was dropped");
    assert!(opaque_away_from_the_edge(&both));
}

/// `none` is the CSS spelling of "no filter" and means exactly that here.
#[test]
fn img_filter_none_paints_the_unfiltered_image() {
    assert_same(
        &render_img(Decl::Inline("none")),
        &render_img(Decl::None),
        "`img-filter: none` must be the unfiltered image",
    );
}

// ===== 12: non-finite values ==============================================

/// `f32::parse("1e999")` is `Ok(inf)`, and an infinite brightness becomes a
/// colour matrix full of `inf`. The pinned outcome is that the declaration is
/// rejected: the image paints unchanged and nothing becomes NaN.
#[test]
fn an_overflowing_brightness_is_rejected_and_paints_no_nan() {
    let unfiltered = render_img(Decl::None);
    for value in [
        "brightness(1e999)",
        "blur(1e999px)",
        "blur(2px) brightness(1e999)",
        "brightness(1e999) blur(2px)",
        "brightness(NaN)",
        "blur(NaNpx)",
        "blur(-1e999px)",
    ] {
        let painted = render_img(Decl::Inline(value));
        assert_same(
            &painted,
            &unfiltered,
            "`img-filter: {value}` was applied; a non-finite argument must be rejected",
        );
        assert!(
            opaque_away_from_the_edge(&painted),
            "`img-filter: {value}` punched a hole in the middle of the image: a NaN channel"
        );
    }
}

// ===== 13: the element the property does not touch =========================

/// Pinned deliberately, so nobody later "fixes" the img-only limitation into a
/// real subtree filter assuming that was ever the intent. The CSS `filter`
/// property would need an offscreen layer and a filter chain — real work, out of
/// scope here — and half of it is worse than none of it, because a half-applied
/// composite looks intentional.
#[test]
fn an_img_filter_on_an_element_with_no_image_changes_nothing() {
    let plain = render_div(Decl::None);
    assert_eq!(
        hex(px(&plain, 32, 32)),
        hex(FIELD),
        "the control div must paint its background"
    );

    for decl in [
        Decl::Inline("blur(4px)"),
        Decl::Inline("brightness(2)"),
        Decl::Inline("blur(4px) brightness(2)"),
        Decl::Class("blur(4px)"),
    ] {
        assert_same(
            &render_div(decl),
            &plain,
            "`img-filter` reached an element with no image; it is honoured only where an \
             image is drawn, which is what the property name now says",
        );
    }
}

/// The CSS property NAME is gone, on every element. `filter: blur(4px)` is inert
/// — on an image, on a div, on text, everywhere — so nothing can half-apply it by
/// accident. Pinned so that re-honouring the name has to be a deliberate act
/// with a subtree compositor behind it.
#[test]
fn the_css_filter_property_name_is_inert() {
    let unfiltered_img = render_img(Decl::None);
    let unfiltered_div = render_div(Decl::None);

    // An `<img>` carrying the CSS spelling is drawn unfiltered.
    let vnode = h(
        "img",
        Props::new().set("src", fixture_src()).set(
            "style",
            "width: 64px; height: 64px; filter: blur(4px) brightness(2);",
        ),
        vec![],
    );
    let as_css_filter = render_vnode_to_rgba(&vnode, &Stylesheet::default(), W, H)
        .expect("render img with the css filter spelling");
    assert_same(
        &as_css_filter,
        &unfiltered_img,
        "`filter:` was honoured again on an <img>; that name belongs to a subtree \
         composite this renderer does not implement",
    );

    // And a class rule spelling it is equally inert.
    let sheet = Stylesheet::parse(".f { filter: blur(4px); } img-filter: blur(4px); }");
    let classed = h(
        "div",
        Props::new()
            .set("class", "f")
            .set("style", "width: 64px; height: 64px;"),
        vec![],
    );
    let rendered = render_vnode_to_rgba(&classed, &sheet, W, H).expect("render classed div");
    let plain_div = render_vnode_to_rgba(
        &h(
            "div",
            Props::new().set("style", "width: 64px; height: 64px;"),
            vec![],
        ),
        &Stylesheet::default(),
        W,
        H,
    )
    .expect("render plain div");
    assert_same(
        &rendered,
        &plain_div,
        "a class rule naming the css `filter` property still did something",
    );
    assert_same(
        &render_div(Decl::None),
        &unfiltered_div,
        "the div control must not depend on the tree above",
    );
}
