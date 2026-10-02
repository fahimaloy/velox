//! T3 — a wrapped text `VNode` must be painted once per line, in full.
//!
//! ## The defect
//!
//! `velox-dom` emits one `LayoutNode` per line box, and every one of them
//! carries the same `source_index` (`inline_leaf_node`, `layout.rs:370`, is the
//! only thing in the crate that stamps one, and `inline_slots_to_nodes` is
//! called once per line). The painter resolved that index to the same
//! `&VNode::Text` N times and, at each node, re-wrapped the WHOLE string and
//! stopped after line 0 (`text_bottom` is one line below that node's own `y`).
//!
//! Net effect, for any text that wraps to N lines: **line 0 was painted N times
//! and lines 1..N-1 were never painted at all.** For `p.tagline` that is a
//! duplicated first line and a silently missing `"natively with Skia."`.
//!
//! ## What these tests assert, and why it is not a checksum
//!
//! `velox-renderer/tests/skia_text_wrap_render.rs` pinned this output with a
//! checksum recorded FROM THE BUG, which is how it got pinned as correct. So
//! every assertion here is geometric and absolute: each painted band has to sit
//! inside the line box layout gave that line, and the bands have to be
//! *different from each other*. Duplication fails the second; loss fails the
//! first (an empty line box has no band). Both are properties of the output
//! rather than of a golden file, and both were verified to go RED with the fix
//! reverted — see the file's last test.
//!
//! ## Tolerance
//!
//! A band's columns are compared against its line's `rect.w`, which is the
//! measured ADVANCE of that line's own text, so the ink is always inside it —
//! glyph ink stops short of the advance by the last glyph's right side bearing.
//! The rows are compared against the line box, which is the strut box the line
//! was placed with. The only slack allowed anywhere is `SLACK` px, for
//! antialiasing on a glyph edge; no ratio or golden width is used.

#![cfg(all(feature = "skia-native", unix))]

use velox_dom::{Props, VNode, h, layout::LayoutNode, text};
use velox_renderer::render_vnode_to_rgba;
use velox_style::Stylesheet;

/// Wide enough for the longest reference paragraph, so an "unwrapped" render
/// really is unwrapped.
const W: i32 = 420;
const H: i32 = 300;

/// Blank rows allowed inside one band.
///
/// The line advance here is 22px and a line of 14px text inks 11 to 15 of those
/// rows, so the smallest gap between two lines of one paragraph measured on this
/// crate's font is 7 rows and this cannot bridge a line break. It has to be
/// three rather than one: the dot of an `i` floats 2 rows clear of its
/// x-height body, and bridging that is what keeps one word from being counted
/// as two bands. Measured on the shapes below, the inter-line gaps are 7, 9 and
/// 11 rows.
const ROW_GAP: i32 = 3;

/// Page black, ink white, so a lit pixel is a glyph and nothing else.
const PAGE: [u8; 4] = [0, 0, 0, 255];

/// Antialiasing slack, in px, on a band's extent.
const SLACK: i32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Band {
    top: i32,
    bottom: i32,
    left: i32,
    right: i32,
    /// Lit pixels in the band.
    ink: usize,
}

impl Band {
    fn width(&self) -> i32 {
        self.right - self.left + 1
    }

    /// The band's shape, ignoring where it sits vertically. Two bands with the
    /// same profile are the same glyphs in the same place, i.e. one line drawn
    /// more than once.
    fn profile(&self) -> (i32, i32, usize) {
        (self.left, self.right, self.ink)
    }
}

fn render(v: &VNode) -> Vec<u8> {
    render_vnode_to_rgba(v, &Stylesheet::default(), W, H).expect("render to rgba")
}

/// Render, and lay out — IN THAT ORDER, and of the CASCADED tree. Both halves of
/// that are load-bearing.
///
/// The renderer lays out `apply_with_cascade(vnode, sheet)`
/// (`prepare_frame`, `skia_render.rs:1433-1436`), so the line boxes the paint
/// followed are the ones the cascade produced. The UA sheet matters: it puts
/// `box-sizing: border-box` on every element, which on the app's own `.tagline`
/// moves the brand column from 562px to 514px and moves the wrap point with it —
/// 556/120 uncascaded against 509/167 cascaded. Laying out the raw tree therefore
/// compares painted ink against line boxes the paint never saw, and every
/// band-inside-its-box assertion in this file would be measuring a coincidence.
///
/// The order matters for a different reason: `prepare_frame` also installs a
/// measurer that reads real font metrics, globally, so a `compute_layout` that
/// runs BEFORE the first render measures with the crate's own fallback face
/// instead. The two faces disagree by enough to matter: for `alpha beta gamma` in
/// a 120px box the fallback reports line advances of 88 and 40px, Skia reports 84
/// and 58px.
fn render_and_layout(v: &VNode) -> (Vec<u8>, LayoutNode) {
    let buf = render(v);
    let styled = velox_style::apply_with_cascade(v, &Stylesheet::default());
    let laid = velox_dom::layout::compute_layout(&styled, W, H);
    (buf, laid)
}

/// A lit pixel: opaque, and not the page background.
///
/// The alpha test matters as much as the colour one. The root has no background,
/// so everything below the box is transparent black — `[0,0,0,0]` — which is
/// not `PAGE` and would otherwise read as a band of ink.
fn is_ink(p: [u8; 4]) -> bool {
    p[3] == 255 && p != PAGE
}
fn is_red(p: [u8; 4]) -> bool {
    p[3] > 200 && p[0] as i32 > p[1] as i32 + 40 && p[0] as i32 > p[2] as i32 + 40
}

/// Ink of the white text, excluding the page and any element with a colour of
/// its own. Used where a sibling element would otherwise count as a text band.
fn is_white(p: [u8; 4]) -> bool {
    is_ink(p) && p[0] as i32 > 200 && p[1] as i32 > 200 && p[2] as i32 > 200
}

fn is_green(p: [u8; 4]) -> bool {
    p[3] > 200 && p[1] as i32 > p[0] as i32 + 40 && p[1] as i32 > p[2] as i32 + 40
}

/// The bands of lit pixels in columns `0..x_max`, top to bottom.
///
/// Rows are grouped across `ROW_GAP` blank rows so an antialiased edge, or the
/// gap under the dot of an `i`, cannot split one line of text into two bands. A
/// wrapped paragraph leaves 7 blank rows between line boxes, so a real line
/// break is never absorbed.
fn ink_bands(buf: &[u8], x_max: i32) -> Vec<Band> {
    bands_where(buf, x_max, is_ink)
}

fn bands_where(buf: &[u8], x_max: i32, pred: fn([u8; 4]) -> bool) -> Vec<Band> {
    let mut rows: Vec<(i32, i32, i32, usize)> = Vec::new();
    for y in 0..H {
        let mut left = None;
        let mut right = None;
        let mut ink = 0;
        for x in 0..x_max {
            let i = ((y * W + x) * 4) as usize;
            if pred([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]) {
                left.get_or_insert(x);
                right = Some(x);
                ink += 1;
            }
        }
        if let (Some(l), Some(r)) = (left, right) {
            rows.push((y, l, r, ink));
        }
    }
    let mut out: Vec<Band> = Vec::new();
    for (y, l, r, ink) in rows {
        match out.last_mut() {
            Some(last) if y <= last.bottom + ROW_GAP => {
                last.bottom = y;
                last.left = last.left.min(l);
                last.right = last.right.max(r);
                last.ink += ink;
            }
            _ => out.push(Band {
                top: y,
                bottom: y,
                left: l,
                right: r,
                ink,
            }),
        }
    }
    out
}

/// The layout line boxes of a single text `VNode`: the root's children that
/// carry a `source_index`, in the order layout emitted them (which is line
/// order — the inline run appends one node set per line).
fn lines_of(laid: &LayoutNode) -> Vec<LayoutNode> {
    laid.children
        .iter()
        .filter(|c| c.source_index.is_some())
        .cloned()
        .collect()
}

/// A black box `w` px wide holding one text node.
fn box_with_text(w: i32, body: &str) -> VNode {
    h(
        "div",
        Props::new().set(
            "style",
            format!("background:#000000;color:#ffffff;width:{w}px;height:260px"),
        ),
        vec![text(body)],
    )
}

/// Assert the painted bands line up one-for-one with the layout line boxes.
///
/// This is the whole contract: every line box has a band (nothing lost), every
/// band is inside its own line box and no other (nothing borrowed from another
/// line, which is what duplication looks like once the boxes differ in width),
/// and no two bands are the same glyphs (duplication, stated directly).
fn assert_one_band_per_line(
    buf: &[u8],
    laid: &LayoutNode,
    x_max: i32,
) -> (Vec<Band>, Vec<LayoutNode>) {
    let lines = lines_of(laid);
    let bands = ink_bands(buf, x_max);
    assert!(
        lines.len() >= 2,
        "this test needs a paragraph that wraps; the layout produced {} line box(es)",
        lines.len()
    );
    assert_eq!(
        bands.len(),
        lines.len(),
        "expected one painted band per line box ({lines:?}); got {bands:?} — a line box \
         with no band is a line of text that was never drawn"
    );
    for (i, (b, l)) in bands.iter().zip(&lines).enumerate() {
        assert!(
            b.top >= l.rect.y - SLACK && b.bottom <= l.rect.y + l.rect.h - 1 + SLACK,
            "band {i} occupies rows {}..{} but line box {i} is rows {}..{}: the text is \
             painted in the wrong line's band",
            b.top,
            b.bottom,
            l.rect.y,
            l.rect.y + l.rect.h - 1
        );
        assert!(
            b.left >= l.rect.x - SLACK && b.right <= l.rect.x + l.rect.w - 1 + SLACK,
            "band {i} spans columns {}..{} but line box {i} is columns {}..{}: the ink on \
             this line belongs to a DIFFERENT line of the text",
            b.left,
            b.right,
            l.rect.x,
            l.rect.x + l.rect.w - 1
        );
        assert!(b.ink > 0, "band {i} is empty");
    }
    for i in 1..bands.len() {
        assert_ne!(
            bands[i].profile(),
            bands[0].profile(),
            "band {i} has the same glyphs at the same columns as band 0 \
             ({:?} vs {:?}): line 0 has been painted more than once",
            bands[i].profile(),
            bands[0].profile()
        );
    }
    (bands, lines)
}

// ── 1. no duplication ───────────────────────────────────────────────────

/// Two lines, the first the longer of the two, which is the shape of
/// `p.tagline`. Duplication here paints line 0 on line 1's rows, and line 0 is
/// wider than line 1's box, so the borrowed ink lands outside line 1.
#[test]
fn two_lines_each_painted_once_at_its_own_line() {
    let v = box_with_text(120, "one two three four five");
    let (buf, laid) = render_and_layout(&v);
    let lines = lines_of(&laid);
    assert_eq!(lines.len(), 2, "this test is about the two-line case");
    assert!(
        lines[0].rect.w > lines[1].rect.w,
        "line 0 ({}) must be the wider one for this test to detect a borrowed line",
        lines[0].rect.w
    );
    assert_one_band_per_line(&buf, &laid, 120);
}

// ── 2. no loss ──────────────────────────────────────────────────────────

/// The `p.tagline` sentence: three lines, and the last one is a short tail
/// ("natively with Skia." split off). Under the defect lines 1 and 2 are never
/// drawn at all and line 0 is drawn on all three rows; the last band's ink has
/// to be its own, and it has to be there.
#[test]
fn the_last_line_of_a_wrapped_paragraph_is_painted() {
    let v = box_with_text(140, "Velox draws the interface natively with Skia.");
    let (buf, laid) = render_and_layout(&v);
    let (bands, lines) = assert_one_band_per_line(&buf, &laid, 140);
    let last = bands.len() - 1;
    assert!(
        bands[last].ink > 0 && bands[last].width() > 5,
        "the last line has no glyphs of its own: {bands:?}"
    );
    // The tail is short, and the defect painted a full-width line in its place.
    // The shared helper already rules a literal copy of line 0 out; this says
    // the last line is a tail rather than merely a different line.
    assert!(
        bands[last].width() * 3 < bands[0].width() * 2,
        "the last line's band is {}px against line 0's {}px: the tail is as wide as \
         the first line, which is what a duplicated line 0 looks like",
        bands[last].width(),
        bands[0].width()
    );
    assert!(
        lines[last].rect.w < lines[0].rect.w,
        "this test's shape needs the last line box to be the narrowest"
    );
}

// ── 3. a multi-line inline run ──────────────────────────────────────────

/// A wrapped inline element has to be painted the same as the same paragraph
/// without it.
///
/// This is stated as a DIFFERENTIAL rather than as geometry on purpose. The
/// bug's fingerprint here is not a missing band — the span's two line boxes both
/// get a band, because the painter draws line 0 at each of them. It is that both
/// bands hold the WRONG WORDS: the painter broke the whole string at each
/// node's own box width, so line 0 came out as "alpha beta" and line 1 as
/// "alpha" instead of "alpha beta" / "gamma". A band-count or band-position
/// assertion passes on that, which is why this test compares the two renders
/// byte for byte: an inline box with no padding, border or background of its own
/// cannot change a single pixel of its text, so any difference is a lie about
/// which line was drawn.
#[test]
fn a_wrapped_inline_element_paints_the_same_as_the_bare_paragraph() {
    let bare = box_with_text(120, "alpha beta gamma");
    let wrapped = h(
        "div",
        Props::new().set(
            "style",
            "background:#000000;color:#ffffff;width:120px;height:260px",
        ),
        vec![h(
            "span",
            Props::new().set("style", "color:#ffffff"),
            vec![text("alpha beta gamma")],
        )],
    );
    let (span_buf, laid) = render_and_layout(&wrapped);
    assert_eq!(
        lines_of(&laid).len(),
        2,
        "the span must wrap onto two line boxes, got {:?}",
        lines_of(&laid)
    );
    let (bare_buf, _) = render_and_layout(&bare);
    // Columns are excluded from the comparison, with one reason and one
    // citation: `skia_render.rs` nudges a line whose box fills its container
    // right by 2px, and an inline fragment box is exactly the width of its own
    // text, so the span's lines are drawn 2px right of the bare paragraph's.
    // That nudge is pre-existing and identical before and after this fix. Rows,
    // widths and lit-pixel counts are what say WHICH words were drawn, and a 2px
    // slide changes none of them.
    let shape = |buf: &[u8]| -> Vec<(i32, i32, i32, usize)> {
        ink_bands(buf, 120)
            .iter()
            .map(|b| (b.top, b.bottom, b.width(), b.ink))
            .collect()
    };
    assert_eq!(
        shape(&bare_buf),
        shape(&span_buf),
        "the span and the bare paragraph have to paint the same words on the same \
         rows: the painter broke the text at the wrong width for one of them. \
         bare = {:?}, span = {:?}",
        ink_bands(&bare_buf, 120),
        ink_bands(&span_buf, 120)
    );
    // Band geometry, too, so a failure says which line moved.
    assert_one_band_per_line(&span_buf, &laid, 120);
}

/// Two inline elements on ONE line: the sibling that follows must keep its
/// place. The line tally is keyed by the resolved `&VNode`, so a second text
/// node on the same line starts at ordinal 0 of its OWN run rather than
/// inheriting the first one's.
#[test]
fn two_inline_elements_on_one_line_keep_their_places() {
    let v = h(
        "div",
        Props::new().set(
            "style",
            "background:#000000;color:#ffffff;width:280px;height:80px",
        ),
        vec![
            h(
                "span",
                Props::new().set("style", "color:#ff0000"),
                vec![text("left half")],
            ),
            h(
                "span",
                Props::new().set("style", "color:#00ff00"),
                vec![text("right half")],
            ),
        ],
    );
    let (buf, laid) = render_and_layout(&v);
    let lines = lines_of(&laid);
    assert_eq!(lines.len(), 2, "two spans, two line boxes: {lines:?}");

    let red = bands_where(&buf, W, is_red);
    let green = bands_where(&buf, W, is_green);
    assert_eq!(
        red.len(),
        1,
        "the red span must ink exactly one band: {red:?}"
    );
    assert_eq!(
        green.len(),
        1,
        "the green span must ink exactly one band: {green:?}"
    );
    assert!(
        green[0].left > red[0].right,
        "the second span's ink ({}..{}) must start after the first's ({}..{}): the two \
         inline elements are not side by side",
        green[0].left,
        green[0].right,
        red[0].left,
        red[0].right
    );
    for (b, l) in [(red[0], &lines[0]), (green[0], &lines[1])] {
        assert!(
            b.left >= l.rect.x - SLACK && b.right <= l.rect.x + l.rect.w - 1 + SLACK,
            "ink at columns {}..{} escapes its own line box {}..{}: an inline element's \
             text is painted at another element's position",
            b.left,
            b.right,
            l.rect.x,
            l.rect.x + l.rect.w - 1
        );
    }
}

/// Row/column bounds and lit-pixel count of a band's bounding box.
#[derive(Debug, PartialEq, Eq)]
struct Rect {
    y0: i32,
    y1: i32,
    x0: i32,
    x1: i32,
    ink: usize,
}

/// The same words, on the same rows, split by hand instead of by wrapping.
///
/// The reference cannot be "the same string, unwrapped": that is one 331px
/// `draw_str` where the paragraph is three 125/134/72px ones, and glyph origins
/// land on different subpixel phases, so the antialiased lit-pixel totals differ
/// by 58px (1468 wrapped against 1410 unwrapped) with nothing wrong anywhere. So
/// the reference is built the way the painter sees the real thing: the same
/// substrings, each its own single-line box at the same y, each drawn by the
/// same `draw_str` from the same x. Every lit pixel then has to match exactly.
fn reference_lines(words: &[&str]) -> Vec<Rect> {
    let boxes: Vec<VNode> = words
        .iter()
        .map(|w| {
            h(
                "div",
                // 380px, not 400: a line box that fills its container is nudged
                // 2px right by the painter (see the comment above `render`), and
                // a nudged reference would not be comparable.
                Props::new().set("style", "width:380px;height:22px;color:#ffffff"),
                vec![text(*w)],
            )
        })
        .collect();
    let v = h(
        "div",
        // Opaque black, like every other box in this file: `is_ink` needs an
        // alpha of 255, and a glyph drawn onto a transparent surface keeps the
        // antialiasing in its alpha channel, so a transparent reference would
        // ink 22 pixels where the real thing inks 545.
        Props::new().set("style", "width:400px;height:300px;background:#000000"),
        boxes,
    );
    let (buf, laid) = render_and_layout(&v);
    let _ = &laid;
    ink_bands(&buf, 380)
        .iter()
        .map(|b| Rect {
            y0: b.top,
            y1: b.bottom,
            x0: b.left,
            x1: b.right,
            ink: b.ink,
        })
        .collect()
}

/// Every glyph of the paragraph is inked exactly once, and it is inked in the
/// words the wrap put on that line.
///
/// The band-per-line assertions say each line has *a* band and that no two lines
/// share one. This says which words are in each band: a painter that redraws line
/// 0 on line 2 still puts a plausible band on line 2, inside line 2's box, of a
/// width no other line has. The only thing wrong with it is that the paragraph's
/// last two words were never drawn, and that shows up here and nowhere else.
#[test]
fn every_glyph_of_a_wrapped_paragraph_is_inked_exactly_once() {
    let body = "Velox draws the interface natively with Skia.";
    let (buf, laid) = render_and_layout(&box_with_text(140, body));
    let lines = lines_of(&laid);
    assert!(
        lines.len() >= 3,
        "this test needs a paragraph of at least three lines; got {lines:?}"
    );
    let bands = ink_bands(&buf, 140);
    assert_eq!(
        bands.len(),
        lines.len(),
        "bands {bands:?} vs lines {lines:?}"
    );

    // The wrap this paragraph is expected to take at 140px. Written out, not
    // computed, so that a change in either the wrap or the painter's break shows
    // up as a mismatch against this literal rather than being re-derived and
    // quietly agreeing with itself.
    let want = reference_lines(&["Velox draws the", "interface natively", "with Skia."]);
    let got: Vec<Rect> = bands
        .iter()
        .map(|b| Rect {
            y0: b.top,
            y1: b.bottom,
            x0: b.left,
            x1: b.right,
            ink: b.ink,
        })
        .collect();
    assert_eq!(
        got, want,
        "wrapped bands {got:?} vs hand-split reference {want:?}"
    );

    let wrapped_ink: usize = bands.iter().map(|b| b.ink).sum();
    let whole_ink: usize = want.iter().map(|r| r.ink).sum();
    assert_eq!(
        wrapped_ink, whole_ink,
        "the paragraph does not add up to itself"
    );
}

/// An atomic inline box in the middle of a paragraph forces a line break, so
/// one text `VNode`'s later lines are MUCH narrower than the box the text wraps
/// in — 39px of text in a 300px container.
///
/// This is the case that separates "break at this line's own advance" from
/// "break at the containing box's width", and no plain wrapped paragraph can,
/// because there the line limit and the container width are the same number. The
/// whole of `six seven eight` fits in 300px, so a painter that breaks at the
/// container puts it all on the forced line and leaves the last line box empty.
/// The atomic is drawn in a colour of its own so the text bands can be counted
/// without it.
#[test]
fn a_forced_line_break_narrower_than_its_container_still_breaks_per_line() {
    let atomic = h(
        "img",
        Props::new().set(
            "style",
            "display:inline-block;width:200px;height:40px;background:#0000ff",
        ),
        vec![],
    );
    let v = h(
        "div",
        Props::new().set(
            "style",
            "background:#000000;color:#ffffff;width:300px;height:180px",
        ),
        vec![
            text("one two three four five"),
            atomic,
            text("six seven eight"),
        ],
    );
    let (buf, laid) = render_and_layout(&v);
    // The atomic's own line box has the same height as the text's, so the two
    // runs are told apart by index into the children: 0 is the first run's
    // single line, 1 is the atomic, 2 and 3 are the second run's two lines.
    let text_lines: Vec<&LayoutNode> = laid
        .children
        .iter()
        .filter(|c| c.source_index.is_some() && c.rect.h <= 22 && c.rect.w < 200)
        .collect();
    assert_eq!(
        text_lines.len(),
        3,
        "three text line boxes expected (one before the atomic, two after): {text_lines:?}"
    );
    assert!(
        text_lines[2].rect.w < text_lines[1].rect.w,
        "the forced line is the narrow one: {:?} vs {:?}",
        text_lines[1].rect,
        text_lines[2].rect
    );
    assert!(
        text_lines[2].rect.w * 4 < laid.rect.w,
        "this test needs the last line to be far narrower than the container: \
         line {:?} in a {}px box",
        text_lines[2].rect,
        laid.rect.w
    );
    let bands = bands_where(&buf, W, is_white);
    assert_eq!(
        bands.len(),
        3,
        "one text band per text line box, the blue atomic excluded: {bands:?}"
    );
    for (i, b) in bands.iter().enumerate() {
        let l = text_lines[i];
        assert!(
            b.top >= l.rect.y - SLACK && b.bottom <= l.rect.y + l.rect.h - 1 + SLACK,
            "band {i} is on rows {}..{} but line box {i} is rows {}..{}",
            b.top,
            b.bottom,
            l.rect.y,
            l.rect.y + l.rect.h - 1
        );
        assert!(
            b.left >= l.rect.x - SLACK && b.right <= l.rect.x + l.rect.w - 1 + SLACK,
            "band {i} spans columns {}..{} but line box {i} is columns {}..{}: the \
             text was broken at the CONTAINER's width instead of this line's own \
             advance",
            b.left,
            b.right,
            l.rect.x,
            l.rect.x + l.rect.w - 1
        );
    }
}

/// Two paragraphs, each in its own parent, each its parent's first child.
///
/// `source_index` is a SIBLING index, so both of these text nodes are
/// `children[0]` of their own parent and both carry `source_index: 0`. Keying
/// the painter's line tally on it therefore hands the second paragraph the
/// first one's word count, and since both are a single line the second paints
/// nothing at all — a whole paragraph of text silently deleted by a key that
/// looks unique. Both paragraphs have to ink.
#[test]
fn two_paragraphs_whose_source_index_collides_both_paint() {
    let v = h(
        "div",
        Props::new().set(
            "style",
            "background:#000000;color:#ffffff;width:280px;height:90px",
        ),
        vec![
            h(
                "div",
                Props::new().set("style", "width:280px"),
                vec![h(
                    "span",
                    Props::new().set("style", "color:#ff0000"),
                    vec![text("alpha beta gamma delta")],
                )],
            ),
            h(
                "div",
                Props::new().set("style", "width:280px"),
                vec![h(
                    "span",
                    Props::new().set("style", "color:#00ff00"),
                    vec![text("epsilon zeta eta theta")],
                )],
            ),
        ],
    );
    let buf = render(&v);
    let red = bands_where(&buf, W, is_red);
    let green = bands_where(&buf, W, is_green);
    assert_eq!(
        red.len(),
        1,
        "the first paragraph must ink one band: {red:?}"
    );
    assert_eq!(
        green.len(),
        1,
        "the second paragraph must ink one band: {green:?} — its text node shares \
         `source_index` 0 with the first one's, and a line tally keyed on that \
         leaves it with no words left to draw"
    );
    assert!(
        green[0].top > red[0].bottom,
        "the second paragraph is below the first: red rows {}..{}, green rows {}..{}",
        red[0].top,
        red[0].bottom,
        green[0].top,
        green[0].bottom
    );
    assert!(
        red[0].width() != green[0].width(),
        "the two paragraphs are different words, so their bands cannot be the same \
         width: red {}..{} ({}px), green {}..{} ({}px) — this is what one line drawn \
         under the other looks like",
        red[0].left,
        red[0].right,
        red[0].width(),
        green[0].left,
        green[0].right,
        green[0].width()
    );
}

// ── 4. single-line regression ───────────────────────────────────────────

/// One line of text, no wrap: the common case must be untouched. The pin is
/// exact — the band's columns and its lit-pixel count — because the fix changes
/// which line of a wrap a node draws, and a node that owns line 0 of a
/// one-line wrap is every other text node in every app.
///
/// The pin moved once, in `0cc6982`, when the default face stopped being Noto Sans
/// and became DejaVu Sans: the same string is 7px narrower with 12 fewer lit
/// pixels under DejaVu's glyphs. What matters and did NOT move is everything
/// structural in this file — one band per line box, each band inside its own line
/// box, and every band's profile distinct from band 0's — which is why the other
/// seven tests in this file stayed green through the swap untouched.
#[test]
fn a_single_line_is_untouched() {
    let v = box_with_text(280, "one line of text");
    let (buf, laid) = render_and_layout(&v);
    let lines = lines_of(&laid);
    assert_eq!(
        lines.len(),
        1,
        "this test is about the one-line case: {lines:?}"
    );
    let bands = ink_bands(&buf, 280);
    assert_eq!(bands.len(), 1, "one line, one band: {bands:?}");
    assert_eq!(
        (bands[0].left, bands[0].right, bands[0].ink),
        (0, 106, 474),
        "the single-line band moved: {:?}",
        bands[0]
    );
    assert!(
        bands[0].right <= lines[0].rect.w - 1 + SLACK,
        "ink at column {} escapes its line box of width {}",
        bands[0].right,
        lines[0].rect.w
    );
}
