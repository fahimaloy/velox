//! `<img src>`: SVG rasterisation, the persistent decode cache, and the
//! intrinsic-size probe that lets an unsized image be sized at all.
//!
//! ## What is here, and what each test fails without
//!
//! 1. `an_svg_rasterises_and_draws_into_the_elements_rect` — an SVG becomes
//!    pixels and paints, through LAYOUT and not by hand-placed rects. It asserts
//!    two exact colours: the SVG's own field colour in the rect's interior and
//!    its disc colour at the rect's centre. A pipeline that rasterised SVG to a
//!    blank, or to a flat fill, passes neither — and neither does one that
//!    painted a bitmap that was never this file's.
//! 2. `one_svg_at_two_declared_sizes_fills_both_rects` — the same FILE, two
//!    declared sizes, both filled, each with the disc at its own centre. This is
//!    the scale-independence claim as an assertion rather than a comment: the
//!    cache holds one raster at the SVG's own size and Skia scales it at draw
//!    time, so the second rect cannot be a reuse of the first one's pixels.
//! 3. `a_png_still_decodes_through_the_unchanged_raster_path` — the regression
//!    guard for putting a dependency next to the only decode that existed. A PNG
//!    is read, recognised by Skia's magic, and drawn, with its exact colour
//!    intact; had the new SVG branch been ordered ahead of the old one, this
//!    would draw nothing at all.
//! 4. `an_unsized_img_takes_its_intrinsic_size_from_the_probe` — the
//!    layout-side half. Nothing declares a size anywhere, and the layout rect
//!    comes back at the SVG's own 37x19. Without a registered probe `velox-dom`
//!    has no way to know that and lays the element out at 0x0, which paints
//!    nothing — so this is a test of the REGISTRATION as much as of the size.
//! 5. `a_missing_or_garbage_src_renders_nothing_and_does_not_panic` — six
//!    unresolvable sources: a missing path, an empty file, non-image bytes, XML
//!    that is detected as SVG and then fails to parse, and two URL schemes
//!    (`https:`, `data:`) that are PINNED as unsupported rather than
//!    implemented. Every one must leave only the container's own background and
//!    return `Ok` — not `Err`, not a panic, and no network.
//! 6. `two_draws_of_one_src_decode_once_per_frame_and_not_once_per_frame` —
//!    the decode counter, which is the only honest way to assert a cache
//!    without timing anything (a timing assertion measures the machine).
//! 7. `an_svg_the_cache_cannot_size_degrades_exactly_as_a_png_does` — what was
//!    chosen for the unresolvable case, stated as a test rather than left in a
//!    comment.
//!
//! ## Fixtures
//!
//! Generated into a temp directory at run time. SVGs are text and the PNG is
//! written by Skia, so the repository gains no binary blob and nothing here
//! depends on a checked-in asset surviving.
//!
//! Colours are asserted as exact bytes. `render_vnode_to_rgba` returns
//! premultiplied RGBA over a transparent-cleared surface, so an opaque pixel
//! comes back unmodified and there is no tolerance to argue about.

#![cfg(all(feature = "skia-native", unix))]

use std::path::{Path, PathBuf};

use velox_dom::{VNode, h, layout::compute_layout};
use velox_renderer::{image_decode_count, render_vnode_to_rgba, reset_image_decode_count};
use velox_style::Stylesheet;

// ===== FIXTURES ========================================================

/// The SVG's field colour, `#3366cc`, as the four bytes it must read back as.
const FIELD: [u8; 4] = [51, 102, 204, 255];
/// The SVG's disc colour, `#ffcc00`.
const DISC: [u8; 4] = [255, 204, 0, 255];

/// A two-tone SVG of exactly `w` x `h`: an opaque `#3366cc` field covering the
/// whole viewBox, with a `#ffcc00` disc in the middle.
///
/// Two tones because one is not enough. A solid field proves the image reached
/// the canvas and nothing more — a file of any single colour would pass a
/// pipeline that blits a placeholder. The disc proves the vector was
/// RASTERISED: its colour and position come out of resvg from path geometry,
/// and nothing else in this renderer knows what a circle is.
fn two_tone_svg(w: u32, h: u32) -> String {
    format!(
        concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"##,
            r##"<rect x="0" y="0" width="{w}" height="{h}" fill="#3366cc"/>"##,
            r##"<circle cx="{cx}" cy="{cy}" r="{r}" fill="#ffcc00"/>"##,
            "</svg>"
        ),
        w = w,
        h = h,
        cx = f64::from(w) / 2.0,
        cy = f64::from(h) / 2.0,
        r = (f64::from(w.min(h)) / 4.0).max(1.0),
    )
}

/// A `w` x `h` PNG holding one flat `#3366cc` field, written with Skia.
///
/// Written rather than checked in so the bytes are produced by the same library
/// that reads them back: a hand-made fixture would be testing whether some
/// other encoder's output happens to be one Skia accepts.
fn write_png(path: &Path, w: i32, h: i32) {
    use skia_safe as sk;
    let mut surface = sk::surfaces::raster_n32_premul((w, h)).expect("png fixture surface");
    surface
        .canvas()
        .clear(sk::Color::from_argb(255, 51, 102, 204));
    let image = surface.image_snapshot();
    #[allow(deprecated)]
    let data = image
        .encode_to_data(skia_safe::EncodedImageFormat::PNG)
        .expect("encode png fixture");
    std::fs::write(path, data.as_bytes()).expect("write png fixture");
}

/// A temp directory unique to this process, created on demand.
///
/// Per-process rather than per-test because a few of these tests deliberately
/// use the same fixture NAME; per-process keeps them from colliding whatever
/// the harness does with threads. Outside the repository, so a binary fixture is
/// never a tracked file.
fn fixture_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("velox-image-render-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    dir
}

/// Write `contents` to `name` in the fixture dir, returning the path as the
/// string an author would have written into `src` — the tests are about what
/// that string resolves to.
fn write_fixture(name: &str, contents: &[u8]) -> String {
    let path = fixture_dir().join(name);
    std::fs::write(&path, contents).expect("write fixture");
    path.to_string_lossy().into_owned()
}

// ===== PIXEL ASSERTIONS ================================================

fn px(rgba: &[u8], w: i32, x: i32, y: i32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    rgba[i..i + 4].try_into().expect("four bytes per pixel")
}

fn all_transparent(rgba: &[u8]) -> bool {
    rgba.chunks_exact(4).all(|p| p == [0, 0, 0, 0])
}

/// The tight bounding box of every pixel with any alpha, as `(l, t, r, b)`,
/// inclusive of the last lit column and row.
///
/// A bounding box rather than a pixel count, because a count cannot tell "drawn
/// at the right place at the right size" from "drawn twice in one corner".
fn ink_bbox(rgba: &[u8], w: i32, h: i32) -> Option<(i32, i32, i32, i32)> {
    let mut acc: Option<(i32, i32, i32, i32)> = None;
    for y in 0..h {
        for x in 0..w {
            if px(rgba, w, x, y)[3] == 0 {
                continue;
            }
            acc = Some(match acc {
                None => (x, y, x, y),
                Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x), b.max(y)),
            });
        }
    }
    acc
}

fn render(vnode: &VNode, w: i32, h: i32) -> Vec<u8> {
    render_vnode_to_rgba(vnode, &Stylesheet::default(), w, h).expect("render to rgba")
}

/// The rect of the `n`th `<img>` in `vnode`, as `(x, y, w, h)`, read back from
/// a real layout pass.
///
/// `LayoutNode` carries no tag, so the images are found by document position
/// instead: every fixture here is a `<div>` whose only children are `<img>`
/// elements, so the `n`th child of the laid-out root IS the `n`th image. Read
/// back rather than assumed, because the point of these assertions is that the
/// pixels and the box they claim to fill are the same box.
///
/// A zero-size box is returned as it is. Every fixture image is a DIRECT child
/// of the root, so "the first child with an extent" is not a safe way to skip
/// the degenerate cases -- and the degenerate cases are the ones where the
/// answer must be `(0, 0, 0, 0)` rather than the next box along.
fn img_rect(vnode: &VNode, w: i32, h: i32, n: usize) -> (i32, i32, i32, i32) {
    let rect = compute_layout(vnode, w, h).children[n].rect;
    (rect.x, rect.y, rect.w, rect.h)
}

// ===== 1. AN SVG RASTERISES AND DRAWS ===================================

#[test]
fn an_svg_rasterises_and_draws_into_the_elements_rect() {
    let src = write_fixture("one-svg.svg", two_tone_svg(40, 24).as_bytes());
    // 100x60 from a 40x24 source: a 2.5x scale. If the bitmap had been baked at
    // the destination size instead of the source's this rect would be the wrong
    // thing; if the image were drawn 1:1 the ink box would be 40x24 and both
    // assertions below would fail.
    let vnode = h(
        "div",
        vec![("style", "width:120px;height:80px")],
        vec![h(
            "img",
            vec![("src", src.as_str()), ("style", "width:100px;height:60px")],
            vec![],
        )],
    );

    let rgba = render(&vnode, 120, 80);

    assert_eq!(
        img_rect(&vnode, 120, 80, 0),
        (0, 0, 100, 60),
        "precondition: layout puts the image in the declared 100x60 rect, since \
         the pixel assertions below are about that box and no other"
    );
    assert_eq!(
        ink_bbox(&rgba, 120, 80),
        Some((0, 0, 99, 59)),
        "the SVG must fill the element's declared rect and stop there, so the \
         declared size is what positioned and sized the draw"
    );
    assert_eq!(
        px(&rgba, 120, 1, 1),
        FIELD,
        "the rect's interior carries the SVG's own field colour, so what was \
         drawn is a raster of THIS file and not a placeholder"
    );
    assert_eq!(
        px(&rgba, 120, 50, 30),
        DISC,
        "the centre of the rect carries the disc, so the vector geometry was \
         rasterised rather than the file being drawn as a flat fill"
    );
}

// ===== 2. ONE SVG, TWO DECLARED SIZES ==================================

#[test]
fn one_svg_at_two_declared_sizes_fills_both_rects() {
    // ONE file, two reads of the same path, two different declared sizes.
    let src = write_fixture("two-sizes-svg.svg", two_tone_svg(37, 19).as_bytes());
    let small = |style: &str| h("img", vec![("src", src.as_str()), ("style", style)], vec![]);
    let vnode = h(
        "div",
        vec![("style", "width:120px;height:100px")],
        vec![
            small("display:block;width:20px;height:10px"),
            small("display:block;width:60px;height:40px"),
        ],
    );

    let rgba = render(&vnode, 120, 100);

    // Boxes stacked: the first at the origin, the second below it.
    let first = (0, 0, 20, 10);
    let second = (0, 10, 60, 40);
    for (name, (x, y, w, h)) in [("first", first), ("second", second)] {
        assert_eq!(
            (
                px(&rgba, 120, x + 1, y + 1),
                px(&rgba, 120, x + w / 2, y + h / 2)
            ),
            (FIELD, DISC),
            "{name} rect {first:?} must carry the SVG's field colour in its \
             interior and its disc at its own centre, at whatever size it was \
             declared — the cached raster is at the SOURCE's 37x19 and Skia \
             scales it per draw"
        );
    }
    assert_eq!(
        ink_bbox(&rgba, 120, 100),
        Some((0, 0, 59, 49)),
        "the larger rect is 60x40 and starts at y=10, so the union of both \
         draws is exactly that — a second draw at the first rect's size would \
         leave the bottom 10 rows empty, and one draw at the larger size would \
         overflow the first"
    );
    assert_eq!(
        image_decode_count(),
        1,
        "one file, one raster: the two draws share a single decode. This is the \
         assertion that would fail if the cache were rebuilt per draw."
    );
}

// ===== 3. A PNG IS UNCHANGED ===========================================

#[test]
fn a_png_still_decodes_through_the_unchanged_raster_path() {
    let path = fixture_dir().join("plain.png");
    write_png(&path, 16, 16);
    let src = path.to_string_lossy().into_owned();

    let vnode = h(
        "div",
        vec![("style", "width:40px;height:40px")],
        vec![h(
            "img",
            vec![("src", src.as_str()), ("style", "width:32px;height:32px")],
            vec![],
        )],
    );
    reset_image_decode_count();
    let rgba = render(&vnode, 40, 40);

    assert_eq!(
        ink_bbox(&rgba, 40, 40),
        Some((0, 0, 31, 31)),
        "precondition: the PNG filled its declared 32x32 rect"
    );
    assert_eq!(
        px(&rgba, 40, 16, 16),
        FIELD,
        "a PNG must still decode to its own pixels — a branch ordered ahead of \
         `Image::from_encoded`, or a format dispatch that sniffed the wrong \
         thing, would draw nothing at all here"
    );
    assert_eq!(
        image_decode_count(),
        1,
        "and through exactly one decode, by the same route as an SVG"
    );
}

// ===== 4. THE INTRINSIC-SIZE PROBE =====================================

#[test]
fn an_unsized_img_takes_its_intrinsic_size_from_the_probe() {
    // 37x19: coprime-ish and both odd, so a rect of this size cannot be a
    // rounding of anything else on offer. No width, no height, no style on the
    // image at all — the ONLY thing that can size this box is the probe.
    let src = write_fixture("unsized-svg.svg", two_tone_svg(37, 19).as_bytes());
    let vnode = h(
        "div",
        vec![("style", "width:80px;height:60px")],
        vec![h("img", vec![("src", src.as_str())], vec![])],
    );

    // Render first: that is what installs the probe. Then ask layout directly so
    // the rect is read rather than inferred from pixels.
    reset_image_decode_count();
    let rgba = render(&vnode, 80, 60);

    assert_eq!(
        img_rect(&vnode, 80, 60, 0),
        (0, 0, 37, 19),
        "an unsized `<img>` is a replaced element sized from its source's \
         intrinsic size (CSS 2.1 §10.3.2 step 2). With no probe registered \
         `velox-dom` has no way to know the source's size and lays it out at \
         0x0, which paints nothing — so this asserts the REGISTRATION too."
    );
    assert_eq!(
        ink_bbox(&rgba, 80, 60),
        Some((0, 0, 36, 18)),
        "and the paint agrees with that rect: the intrinsic size is not just \
         in the layout, it is what the image was drawn into"
    );
    assert_eq!(px(&rgba, 80, 18, 9), DISC, "with the disc at its centre");
    assert_eq!(
        image_decode_count(),
        1,
        "layout asks the probe for a width pass and again for a height pass, \
         and the paint walk asks again -- four questions about one src, one \
         decode. A probe that re-read the file per question, or a cache the \
         probe did not share with the painter, would report 4."
    );
}

// ===== 5. NOTHING RESOLVES =============================================

#[test]
fn a_missing_or_garbage_src_renders_nothing_and_does_not_panic() {
    let cases: Vec<(&str, String)> = vec![
        (
            "a path that does not exist",
            fixture_dir()
                .join("does-not-exist.png")
                .to_string_lossy()
                .into_owned(),
        ),
        ("an empty file", write_fixture("empty.svg", b"")),
        (
            "random bytes wearing a .png extension",
            write_fixture("garbage.png", &[0xde, 0xad, 0xbe, 0xef, 0x00, 0x11, 0x22]),
        ),
        (
            "text that IS detected as SVG and then fails to parse",
            write_fixture(
                "malformed.svg",
                b"<svg width=\"10\" height=\"10\"><rect <//></svg>",
            ),
        ),
        // A URL scheme. PINNED, not implemented: `load` resolves `src` with
        // `std::fs::read` and nothing else, so `http(s)://` and `data:` reach
        // the filesystem, find no such file, and paint nothing. There is no
        // network fetch path and no URL decoder anywhere in this workspace, and
        // adding one is a separate decision with its own size — a synchronous
        // socket read inside a paint walk is not a thing to smuggle in with
        // rasterisation support. The two cases are here so that adding scheme
        // support later has to UPDATE this test rather than quietly make it
        // pass for the wrong reason.
        ("an https: URL", "https://example.invalid/logo.svg".to_string()),
        (
            "a data: URL carrying a real SVG",
            "data:image/svg+xml,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%2210%22%20height%3D%2210%22%3E%3C%2Fsvg%3E"
                .to_string(),
        ),
    ];

    for (what, src) in cases {
        let vnode = h(
            "div",
            vec![("style", "width:60px;height:40px;background:#ff0000")],
            vec![h("img", vec![("src", src.as_str())], vec![])],
        );
        // A red container, so "the image drew nothing" is distinguishable from
        // "nothing drew at all" — and so the `<img>` is a real box either way.
        let rgba = render(&vnode, 60, 40);
        assert!(
            !all_transparent(&rgba),
            "{what}: the container's own background must still paint, or this \
             test proves nothing about the image"
        );
        // The image must not have drawn OVER the container. Nothing in the
        // renderer clips an `<img>` to nothing, so the check is that the only
        // ink is the container's own red.
        for y in 0..40 {
            for x in 0..60 {
                assert_eq!(
                    px(&rgba, 60, x, y),
                    [255, 0, 0, 255],
                    "{what}: pixel ({x}, {y}) is not the container's own \
                     background, so an unresolvable src drew something"
                );
            }
        }
    }
}

// ===== 6. THE CACHE ====================================================

#[test]
fn two_draws_of_one_src_decode_once_per_frame_and_not_once_per_frame() {
    let src = write_fixture("counted.svg", two_tone_svg(30, 30).as_bytes());
    let img = || {
        h(
            "img",
            vec![
                ("src", src.as_str()),
                ("style", "display:block;width:30px;height:30px"),
            ],
            vec![],
        )
    };
    let vnode = h(
        "div",
        vec![("style", "width:60px;height:60px")],
        vec![img(), img()],
    );

    reset_image_decode_count();
    render(&vnode, 60, 60);
    assert_eq!(
        image_decode_count(),
        1,
        "two draws of one src in ONE frame must decode once"
    );

    // And the frame after it. This is the part the per-frame cache could not
    // do: the tree is unchanged, so the second frame's picture is the same
    // picture, and a cache rebuilt per frame pays for it again.
    reset_image_decode_count();
    render(&vnode, 60, 60);
    render(&vnode, 60, 60);
    assert_eq!(
        image_decode_count(),
        0,
        "two further frames of the same tree decode NOTHING. This is the whole \
         point of hoisting the cache: a visible image used to cost a disk read \
         and a full decode on every single frame."
    );
}

// ===== 7. WHAT WAS CHOSEN FOR THE UNSIZABLE CASE =======================

#[test]
fn an_svg_the_cache_cannot_size_degrades_exactly_as_a_png_does() {
    // Two SVGs that rasterise to nothing: one with a zero intrinsic size, one
    // that is well-formed XML but carries no drawable content at all. Both are
    // detected as SVG, so both reach resvg — and both are invisible.
    let cases = [
        (
            "a zero-sized SVG",
            write_fixture("zero-size.svg", two_tone_svg(0, 0).as_bytes()),
        ),
        (
            "an SVG with nothing to draw",
            write_fixture(
                "empty-content.svg",
                two_tone_svg(24, 24)
                    .replace("<circle", "<g")
                    .replace("</svg>", "</g></svg>")
                    .as_bytes(),
            ),
        ),
    ];

    for (what, src) in cases {
        let vnode = h(
            "div",
            vec![("style", "width:60px;height:40px")],
            vec![h("img", vec![("src", src.as_str())], vec![])],
        );
        let rgba = render(&vnode, 60, 40);

        assert!(
            all_transparent(&rgba),
            "{what}: degrades to painting NOTHING, which is what a PNG whose \
             file is missing already does. What was NOT chosen, and why: \
             velox-dom states at layout.rs that CSS 2.1 step 3 (the 300x150 \
             default object size) is deliberately not modelled, and an \
             unsized replaced element with an unresolvable source is a \
             zero-size box there — the same as an empty inline-block. A \
             fallback invented here would be a second answer to that question, \
             in the wrong crate, where nothing could invalidate it."
        );
        let (_, _, w, h) = img_rect(&vnode, 60, 60, 0);
        assert_eq!(
            (w, h),
            (0, 0),
            "{what}: and it is a zero-SIZE box rather than an invented one, so \
             `velox-dom` still has exactly one rule for an unsized replaced \
             element with no answer to give. The zero-size box's own `y` is its \
             baseline position on the line, which is why only the extents are \
             asserted — a zero-height box at any `y` paints nothing."
        );
    }
}
