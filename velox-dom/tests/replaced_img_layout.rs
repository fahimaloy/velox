//! `<img src=…>` is a REPLACED element: a box whose size comes from outside the
//! tree, not from content this engine could measure.
//!
//! The bug these pin: `img` is in `INLINE_BY_DEFAULT_TAGS`, so with no `display`
//! it entered the inline run as a text-less Fragment and the back-fill gave it
//! `w: 0, h: <line-height>`. The renderer drew into a zero-width rect and no
//! image appeared. The only recipe that worked was spelling out
//! `style="display:inline-block;width:Wpx;height:Hpx"`, and that recipe has to
//! keep working — one test here is exactly it.
//!
//! NO RASTERISATION: every assertion is about the box `compute_layout` produced.
//! That is the contract — the renderer draws into the rect layout hands it — and
//! it is the only half this crate owns.
//!
//! The intrinsic sizes come from a FAKE probe registered through the same
//! `set_intrinsic_size_probe` seam a real backend registers through, so no image
//! file is decoded and none is needed: `velox-dom` has no decoder and must not
//! grow one. The fake answers for ONE `src` and only that one, so a test can
//! observe "no intrinsic size known" without depending on registration order.

use std::sync::Once;
use velox_dom::layout::{Rect, compute_layout, is_replaced_element};
use velox_dom::{Props, VNode, h, text};

/// The only `src` the fake backend knows. Anything else is a source it cannot
/// size, which is the same answer a broken or not-yet-decoded file gets.
const KNOWN_SRC: &str = "fake:40x20";
const INTRINSIC_W: i32 = 40;
const INTRINSIC_H: i32 = 20;

/// The synthetic backend. A MODEL of a decoder, not one: it proves the layout
/// honours the seam's pixel sizes, not that any real image has those.
fn fake_intrinsic_sizes(src: &str) -> Option<(i32, i32)> {
    (src == KNOWN_SRC).then_some((INTRINSIC_W, INTRINSIC_H))
}

/// Install the fake backend. Idempotent, so every test can call it and no test
/// can be run before someone else's registration — the seam is a process global
/// with no unregister, which is the same constraint `common::register_synthetic`
/// exists to work around for the text measurer.
fn register_fake_backend() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| velox_dom::layout::set_intrinsic_size_probe(fake_intrinsic_sizes));
}

/// The one child of `root`, which every tree here is built so that it is.
fn only_child(root: &VNode) -> Rect {
    child_rect(root)
}

/// The child's box, with the child count asserted.
fn child_rect(root: &VNode) -> Rect {
    let laid = compute_layout(root, 800, 600);
    assert_eq!(
        laid.children.len(),
        1,
        "expected exactly one laid-out child, got {:?}",
        laid.children.iter().map(|c| c.rect).collect::<Vec<_>>()
    );
    laid.children[0].rect
}

/// The child's SIZE, and nothing else.
///
/// Preferred over the whole rect because the position of an INLINE box on its
/// line depends on its height relative to the line's strut: a box 40px tall
/// starts at the top of the line, and a zero-height box sits ON the baseline
/// instead, so a full-rect assertion would be pinning the line's metrics rather
/// than this job's subject. The tests that care about a position assert it.
fn only_child_size(root: &VNode) -> (i32, i32) {
    let rect = child_rect(root);
    (rect.w, rect.h)
}

/// A block container of a known width, so a percentage has something definite to
/// resolve against.
fn container(child: VNode) -> VNode {
    h(
        "div",
        Props::new().set("style", "width: 400px;"),
        vec![child],
    )
}

fn img(props: Props) -> VNode {
    h("img", props, vec![])
}

/// A declared CSS `width` and `height` are the used values: the box is exactly
/// those, whatever the source says.
#[test]
fn declared_css_width_and_height_are_the_box() {
    let tree = container(img(Props::new()
        .set("src", KNOWN_SRC)
        .set("style", "width: 120px; height: 40px;")));

    assert_eq!(
        only_child(&tree),
        Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 40
        }
    );
}

/// The HTML `width`/`height` attributes are presentational hints, and reading
/// them in layout is that mapping. The container is 400px wide, so a box that
/// came out 400 wide would be a block filling its parent rather than a hint
/// being honoured.
#[test]
fn html_width_and_height_attributes_are_the_box() {
    let tree = container(img(Props::new()
        .set("src", KNOWN_SRC)
        .set("width", "80")
        .set("height", "30")));

    assert_eq!(
        only_child(&tree),
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 30
        }
    );
}

/// An attribute is a `<dimension>` or a `<percentage>`, and `120px` is the same
/// hint as `120`. The percentage resolves against the containing block, which is
/// the 400px container — not against the viewport.
#[test]
fn html_attributes_take_the_dimension_and_percentage_grammar() {
    let px_form = container(img(Props::new()
        .set("src", KNOWN_SRC)
        .set("width", "120px")
        .set("height", "60px")));
    assert_eq!(
        only_child(&px_form),
        Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 60
        },
        "a px suffix is the same <dimension> as a bare number"
    );

    let pct_form = container(img(Props::new()
        .set("src", KNOWN_SRC)
        .set("width", "25%")
        .set("height", "50px")));
    assert_eq!(
        only_child(&pct_form),
        Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 50
        },
        "25% of the 400px containing block"
    );
}

/// A value outside HTML's grammar is not a hint at all, so the box falls
/// through to the intrinsic size rather than resolving `2em` against a font size
/// the HTML spec never consults here. A negative value is excluded the same way.
#[test]
fn attributes_outside_the_html_grammar_fall_through_to_the_intrinsic_size() {
    register_fake_backend();

    for bad in ["2em", "-40", "auto", "", "40pt"] {
        let tree = container(img(Props::new().set("src", KNOWN_SRC).set("width", bad)));

        assert_eq!(
            only_child(&tree).w,
            INTRINSIC_W,
            "width={bad:?} is not a presentational dimension, so the intrinsic \
             width should have been used instead"
        );
    }
}

/// Nothing declared and nothing hinted: the source's own pixels are the box.
/// This is the case that was impossible before — there was no way for layout to
/// learn an image's size at all.
#[test]
fn intrinsic_size_is_the_box_when_nothing_else_says() {
    register_fake_backend();

    let tree = container(img(Props::new().set("src", KNOWN_SRC)));

    assert_eq!(
        only_child(&tree),
        Rect {
            x: 0,
            y: 0,
            w: INTRINSIC_W,
            h: INTRINSIC_H
        }
    );
}

/// A source the backend cannot size is a zero-size box, NOT a box that fills its
/// container. Filling is what `content_size_for` does for a block with no
/// declared width, and it is the one behaviour a replaced element must not
/// inherit: an image with no known size has nothing to paint, and a
/// container-wide box around nothing is worse than a zero one — it is a
/// clickable region covering the page.
///
/// Size only, because a zero-height INLINE box sits on the baseline and so has a
/// non-zero `y` — `height: 0` on an `<img>` does the same in a browser. The line
/// metrics are not this file's subject.
#[test]
fn an_unsizable_source_lays_out_as_zero_rather_than_filling() {
    register_fake_backend();

    let tree = container(img(Props::new().set("src", "no-such-image.png")));

    assert_eq!(only_child_size(&tree), (0, 0));
}

/// CSS beats the presentational hint, per AXIS: HTML maps the attributes to
/// `width`/`height` declarations, and a declaration overrides a hint. One axis
/// declared in CSS and the other only hinted is the case that shows the order is
/// per axis rather than "attributes never apply".
#[test]
fn css_overrides_the_attribute_on_the_axis_it_names() {
    register_fake_backend();

    let tree = container(img(Props::new()
        .set("src", KNOWN_SRC)
        .set("width", "80")
        .set("height", "30")
        .set("style", "width: 120px;")));

    assert_eq!(
        only_child(&tree),
        Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 30
        },
        "width from CSS, height from the attribute, intrinsic size for neither"
    );
}

/// The attribute beats the intrinsic size on its own axis, and the intrinsic
/// size still supplies the axis nothing else named. This is the whole
/// three-step order of CSS 2.1 §10.3.2 in one box.
#[test]
fn the_attribute_beats_the_intrinsic_size() {
    register_fake_backend();

    let tree = container(img(Props::new().set("src", KNOWN_SRC).set("height", "33")));

    assert_eq!(
        only_child(&tree),
        Rect {
            x: 0,
            y: 0,
            w: INTRINSIC_W,
            h: 33
        }
    );
}

/// THE OLD RECIPE. `display: inline-block` with a declared size, no `src`: the
/// route that already worked and must keep working, byte for byte.
///
/// It is also the reason the `src` test is not redundant with this one: an
/// `inline-block` and a replaced element now reach `lay_out_atomic` by different
/// predicates, and only this one has no `src` to size it.
#[test]
fn the_inline_block_recipe_is_unchanged() {
    let tree = h(
        "div",
        Props::new().set("style", "width: 400px;"),
        vec![img(Props::new().set(
            "style",
            "display:inline-block;width:200px;height:40px",
        ))],
    );

    assert_eq!(
        only_child(&tree),
        Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 40
        }
    );
}

/// ...and an `inline-block` that DOES carry a `src` is still sized by its
/// declaration, not demoted to its intrinsic size. The two routes must not
/// interfere.
#[test]
fn inline_block_with_a_src_still_takes_its_declared_size() {
    register_fake_backend();

    let tree = container(img(Props::new()
        .set("src", KNOWN_SRC)
        .set("style", "display:inline-block;width:200px;height:40px")));

    assert_eq!(
        only_child(&tree),
        Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 40
        }
    );
}

/// An `<img>` with NO `src` names no source, so it has no intrinsic size to be
/// sized by and is not replaced. It keeps the box it had: a text-less inline
/// member of the line, zero wide. `tests/layout_tests.rs` pins the rest of that
/// element's behaviour; this pins that adding the replaced rule did not take it
/// over.
#[test]
fn an_img_without_a_src_is_not_replaced() {
    let tree = container(h("img", Props::new(), vec![]));

    assert!(
        !is_replaced_element(&h("img", Props::new(), vec![])),
        "no `src`, no replaced element"
    );
    assert!(
        is_replaced_element(&img(Props::new().set("src", KNOWN_SRC))),
        "a `src` attribute is what makes a box replaced"
    );
    assert_eq!(
        only_child(&tree).w,
        0,
        "an <img> with no source is still the zero-width line member it was"
    );
}

/// THE ACTUAL BUG. The image has to RESERVE ITS SPACE on the line, or the text
/// beside it is laid out as though nothing were there and the whole point of
/// sizing it is lost. Before this, the image was 0 wide and the text started at
/// x = 0 underneath it.
#[test]
fn an_inline_image_reserves_its_width_on_the_line() {
    register_fake_backend();

    let tree = h(
        "div",
        Props::new().set("style", "width: 400px;"),
        vec![img(Props::new().set("src", KNOWN_SRC)), text("after")],
    );

    let laid = compute_layout(&tree, 800, 600);
    let image = laid
        .children
        .iter()
        .find(|c| c.source_index == Some(0))
        .expect("the image has a box on the line");
    let word = laid
        .children
        .iter()
        .find(|c| c.source_index == Some(1))
        .expect("the text beside it has a box");

    assert_eq!((image.rect.w, image.rect.h), (INTRINSIC_W, INTRINSIC_H));
    assert_eq!(
        word.rect.x, INTRINSIC_W,
        "the text must start after the image, not underneath it"
    );
}

/// DECIDED AND PINNED: a `src` is what makes a box replaced, not a tag. Every
/// element that can name an external source is replaced in CSS — `img`, `video`,
/// `iframe`, `embed`, `object`, `input type=image` — and a tag list would be a
/// list to extend by hand with a silent zero-size box as the failure mode.
///
/// So a `<div src=…>` is a replaced BLOCK: sized by its source, and NOT filled
/// to its container the way a `<div>` is. The negative half matters as much: a
/// `<div>` with no `src` is still an ordinary block that fills.
#[test]
fn a_non_img_element_with_a_src_is_replaced_too() {
    register_fake_backend();

    let block = container(h("div", Props::new().set("src", KNOWN_SRC), vec![]));
    assert_eq!(
        only_child(&block),
        Rect {
            x: 0,
            y: 0,
            w: INTRINSIC_W,
            h: INTRINSIC_H
        },
        "a replaced block is sized by its source, not by the containing block"
    );

    let plain = container(h("div", Props::new(), vec![]));
    assert_eq!(
        only_child(&plain).w,
        400,
        "a block with no `src` still fills, which is what the previous assertion \
         has to be read against"
    );
}

/// The same rule on an INLINE-LEVEL non-`img`: a `<span src=…>` is an atomic
/// inline box. Being replaced makes an element atomic whatever its `display`
/// says, so the run places it whole instead of flattening a box that has no text
/// to flatten.
#[test]
fn an_inline_non_img_with_a_src_is_an_atomic_box() {
    register_fake_backend();

    let tree = container(h("span", Props::new().set("src", KNOWN_SRC), vec![]));

    assert_eq!(
        only_child(&tree),
        Rect {
            x: 0,
            y: 0,
            w: INTRINSIC_W,
            h: INTRINSIC_H
        }
    );
}

/// Padding and border sit OUTSIDE the intrinsic size under the default
/// `box-sizing: content-box`, exactly as they do around a declared width: the
/// source's pixels are the CONTENT box, so `40x20` plus 5px of padding on each
/// side is a 50x30 border box.
#[test]
fn padding_and_border_add_outside_the_intrinsic_size() {
    register_fake_backend();

    let tree = container(img(Props::new()
        .set("src", KNOWN_SRC)
        .set("style", "padding: 5px; border: 5px solid #000;")));

    let rect = only_child(&tree);
    assert_eq!(
        (rect.w, rect.h),
        (INTRINSIC_W + 20, INTRINSIC_H + 20),
        "content 40x20 plus 5px padding and 5px border on every side"
    );
}

/// `min-width` is a constraint on the used value, so it still applies to a box
/// whose width came from a hint or from the source. The order is
/// floor-to-min last, and a replaced element does not get to opt out of it.
#[test]
fn min_width_still_floors_a_replaced_box() {
    register_fake_backend();

    let tree = container(img(Props::new()
        .set("src", KNOWN_SRC)
        .set("style", "min-width: 200px;")));

    assert_eq!(only_child(&tree).w, 200);
}
