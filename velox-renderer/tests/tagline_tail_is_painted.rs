//! T3's gate artefact: a rendered `.tagline` showing `"natively with Skia."`.
//!
//! The task file makes a screenshot of `.tagline` proving the tail survives part
//! of the gate, because a green suite is not sufficient evidence for a text-loss
//! bug. This is that screenshot, plus the assertions that make it more than a
//! picture.
//!
//! ## Why the markup is a hand-built replica and not the compiled template
//!
//! The real `.tagline` lives in `velox-cli/templates/project/src/App.vx`, which
//! is compiled at build time by `velox-cli`'s `build_cmd`. `velox-cli` depends on
//! `velox-renderer`, so a test in this crate cannot reach it without a dependency
//! cycle, and this crate's write scope is not `velox-cli`. So the element is
//! rebuilt here from the template's own declarations, copied verbatim:
//!
//! | what | from `velox-cli/templates/project/src/App.vx` |
//! |---|---|
//! | the string | `App.vx:139-141` `tagline()`, 110 chars |
//! | `.shell` | `App.vx:279-285` `width:100%; max-width:620px; padding:44px 24px 32px` |
//! | `.masthead` | `App.vx:286-291` flex row, `gap:20px`, `justify-content:space-between` |
//! | `.brand` | `App.vx:292-296` flex column, `min-width:0` |
//! | `.tagline` | `App.vx:312-317` `font-size:14px; line-height:1.45; color:#5a6469` |
//! | the toggle sibling | `App.vx:319-330` `flex:0 0 auto; width:38px; height:38px` |
//!
//! Those give the real geometry: the shell's 620px max-width less its own 24px
//! padding each side is a 572px content box, and the tagline's own text measures
//! ~676px at 14px, so it takes **two line boxes** — which is the case the bug
//! needed. The pre-fix painter drew line 0 on both nodes and never drew the tail,
//! so the rendered PNG was missing `"natively with Skia."` outright.
//!
//! ## What is asserted
//!
//! Not "the PNG is not empty", and not a checksum — a checksum recorded from
//! buggy output is how this defect got pinned as correct in the first place.
//! Three things, each of which the pre-fix painter fails:
//!
//! 1. Layout gave the tagline two line boxes, and there is one band of ink on
//!    each one's own rows. A duplicated line 0 puts both bands in line 0's rows;
//!    a dropped tail leaves line 1's rows empty.
//! 2. The two bands are not the same width, so they cannot be the same paint.
//! 3. The last band **is the tail**: every suffix of the tagline is rendered on
//!    its own in a box wide enough not to wrap, and the band's exact column
//!    extent and lit-pixel count must match one of them — the longest one, which
//!    must run to the last character of the string. A band that is a prefix of
//!    the true last line (a dropped final word) matches no suffix, and a band
//!    that is line 0's paint repeated matches a shorter one that does not reach
//!    the end. Then the string is checked to end with the five words the task
//!    names.
//!
//! The PNG is written to `gates/t3/`, the gate log directory.

#![cfg(all(feature = "skia-native", unix))]

use std::path::PathBuf;

use velox_dom::{Props, VNode, h, layout::LayoutNode, text};
use velox_renderer::render_vnode_to_rgba;
use velox_style::Stylesheet;

/// The template's `.shell` max-width. The window is exactly this wide so the
/// shell's own `max-width` binds, as it does in a wider window.
const WINDOW_W: i32 = 620;
const WINDOW_H: i32 = 300;

/// Wide and tall enough for one unwrapped line of the tagline and the widest
/// candidate suffix, so a reference render can never wrap.
const REF_W: i32 = 900;
const REF_H: i32 = 40;

/// `App.vx:139-141`, verbatim.
const TAGLINE: &str = "One file per component. A template, a state block and a scoped \
                       style block, drawn natively with Skia.";

/// The words the bug used to drop, and the end of `TAGLINE`.
const TAIL: &str = "natively with Skia.";

/// Blank rows allowed inside one band. The dot of an `i` floats clear of its
/// x-height body, so one row is not enough to keep a word from splitting in two.
/// The gap between two lines of 14px text is far larger than this (measured: 7
/// rows between the ink of two consecutive lines).
const ROW_GAP: i32 = 3;

/// Rows or columns the ink may sit outside a line box, for antialiasing on a
/// glyph edge.
const SLACK: i32 = 2;

/// The band of lit pixels on one run of rows, with the columns it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Band {
    top: i32,
    bottom: i32,
    left: i32,
    right: i32,
    ink: usize,
}

impl Band {
    fn width(self) -> i32 {
        self.right - self.left + 1
    }
}

/// The app shell, down to and including `.tagline`, with the masthead's toggle
/// beside it so the PNG is the app's own shape and not a bare paragraph.
fn scaffolded_app() -> VNode {
    h(
        "div",
        Props::new().set("style", "background:#ffffff"),
        vec![h(
            "div",
            // App.vx:279-285
            Props::new().set("style", "width:100%;max-width:620px;padding:44px 24px 32px"),
            vec![h(
                "header",
                // App.vx:286-291
                Props::new().set(
                    "style",
                    "display:flex;flex-direction:row;align-items:flex-start;\
                     justify-content:space-between;gap:20px",
                ),
                vec![
                    h(
                        "div",
                        // App.vx:292-296
                        Props::new().set("style", "display:flex;flex-direction:column;min-width:0"),
                        vec![
                            h(
                                "p",
                                // App.vx:298-304
                                Props::new().set(
                                    "style",
                                    "margin:0 0 10px;font-size:11px;line-height:1.2;\
                                     color:#0d6e66",
                                ),
                                vec![text("VELOX")],
                            ),
                            h(
                                "h1",
                                // App.vx:306-311
                                Props::new().set(
                                    "style",
                                    "margin:0;font-size:27px;line-height:1.15;color:#14181b",
                                ),
                                vec![text("Velox Todo")],
                            ),
                            h(
                                "p",
                                // App.vx:312-317
                                Props::new().set(
                                    "style",
                                    "margin:7px 0 0;font-size:14px;line-height:1.45;\
                                     color:#5a6469",
                                ),
                                vec![text(TAGLINE)],
                            ),
                        ],
                    ),
                    h(
                        "button",
                        // App.vx:319-330, the flex row's `flex: 0 0 auto` sibling.
                        Props::new().set(
                            "style",
                            "flex:0 0 auto;display:flex;align-items:center;\
                             justify-content:center;width:38px;height:38px;padding:0",
                        ),
                        vec![text("☾")],
                    ),
                ],
            )],
        )],
    )
}

/// The line boxes layout gave the tagline text, in line order.
///
/// Found by walking the layout alongside the VNode tree to the text node that
/// holds the tagline, rather than by matching a rect: the cascade has already
/// consumed the `class` attribute by the time layout runs, so a selector cannot
/// be used here, and a rect would be assuming the answer.
fn tagline_line_boxes(laid: &LayoutNode, vnode: &VNode) -> Vec<(i32, i32, i32, i32)> {
    fn leaves(node: &LayoutNode, out: &mut Vec<(i32, i32, i32, i32)>) {
        if node.children.is_empty() {
            let r = node.rect;
            out.push((r.x, r.y, r.w, r.h));
            return;
        }
        for child in &node.children {
            leaves(child, out);
        }
    }
    fn walk(node: &LayoutNode, vnode: &VNode) -> Option<Vec<(i32, i32, i32, i32)>> {
        match vnode {
            VNode::Text(_) => None,
            VNode::Element { children, .. } => {
                // The element that DIRECTLY holds the tagline is the answer, and
                // every line box under it belongs to it. Descending to the text
                // node instead would stop at the first line box, because each of
                // them resolves to that same text.
                if children
                    .iter()
                    .any(|c| matches!(c, VNode::Text(t) if t == TAGLINE))
                {
                    let mut out = Vec::new();
                    leaves(node, &mut out);
                    return Some(out);
                }
                for layout_child in &node.children {
                    let Some(idx) = layout_child.source_index else {
                        continue;
                    };
                    if let Some(found) = walk(layout_child, &children[idx]) {
                        return Some(found);
                    }
                }
                None
            }
        }
    }
    walk(laid, vnode).expect("the tagline text node is in the tree it was laid out from")
}

/// The bands of lit pixels in `rows` and `columns` `0..=x_max`.
///
/// "Lit" is "not white and not transparent": the page is white, so every
/// non-white pixel is a glyph or a glyph's antialiased fringe.
fn ink_bands(
    buf: &[u8],
    surface_w: i32,
    surface_h: i32,
    y0: i32,
    y1: i32,
    x_max: i32,
) -> Vec<Band> {
    let mut bands: Vec<Band> = Vec::new();
    for y in y0..y1.min(surface_h) {
        let mut left = i32::MAX;
        let mut right = i32::MIN;
        let mut ink = 0usize;
        for x in 0..x_max.min(surface_w) {
            let o = (y as usize * surface_w as usize + x as usize) * 4;
            let p = [buf[o], buf[o + 1], buf[o + 2], buf[o + 3]];
            if p[3] == 255 && (p[0] < 250 || p[1] < 250 || p[2] < 250) {
                ink += 1;
                left = left.min(x);
                right = right.max(x);
            }
        }
        if ink == 0 {
            continue;
        }
        match bands.last_mut() {
            Some(last) if y <= last.bottom + ROW_GAP => {
                last.bottom = y;
                last.left = last.left.min(left);
                last.right = last.right.max(right);
                last.ink += ink;
            }
            _ => bands.push(Band {
                top: y,
                bottom: y,
                left,
                right,
                ink,
            }),
        }
    }
    bands
}

/// The one band a string paints in a box wide enough not to wrap, as
/// `(column extent, lit pixels)`.
fn unwrapped_profile(s: &str) -> Option<(i32, usize)> {
    let vnode = h(
        "div",
        // The tagline's own declarations, so the glyphs are the same glyphs at the
        // same size as the real thing.
        Props::new().set(
            "style",
            "background:#ffffff;color:#5a6469;font-size:14px;line-height:1.45;\
             width:880px;height:40px;margin:0",
        ),
        vec![text(s)],
    );
    let rgba = render_vnode_to_rgba(&vnode, &Stylesheet::default(), REF_W, REF_H).expect("render");
    let bands = ink_bands(&rgba, REF_W, REF_H, 0, REF_H, REF_W);
    match bands.as_slice() {
        [one] => Some((one.width(), one.ink)),
        _ => None,
    }
}

fn gate_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("gates")
        .join("t3");
    std::fs::create_dir_all(&dir).expect("create gate dir");
    dir
}

/// The gate's artefact, and the proof that it carries the tail.
#[test]
fn the_tagline_paints_its_tail_and_the_png_is_written() {
    let sheet = Stylesheet::default();
    // Render BEFORE laying out: the render installs the real font measurer
    // globally, so a layout taken first would be a different face's advances
    // than the ones the glyphs are drawn at.
    let app = scaffolded_app();
    let rgba = render_vnode_to_rgba(&app, &sheet, WINDOW_W, WINDOW_H).expect("render");
    // Lay out the CASCADED tree: that is what the paint followed
    // (`prepare_frame`, `skia_render.rs:1433-1436`), and it is not a detail. The
    // UA sheet puts `box-sizing: border-box` on every element, which takes the
    // shell's 620px `max-width` as a BORDER box, so the masthead's content is the
    // 572px the task file names and the brand column is 572 - 20 - 38 = 514px.
    // Laid out uncascaded the shell is 668px wide, the brand 562px, and the wrap
    // point moves with it (509/167 against 556/120) — a different paragraph than
    // the one that was painted.
    let styled = velox_style::apply_with_cascade(&app, &sheet);
    let laid = velox_dom::layout::compute_layout(&styled, WINDOW_W, WINDOW_H);
    let lines = tagline_line_boxes(&laid, &styled);
    assert_eq!(
        lines.len(),
        2,
        "the tagline must wrap onto exactly two line boxes at this width, or this \
         test is not exercising the defect: {lines:?}"
    );

    // Scan only the tagline's own rows. The page above it holds the eyebrow and
    // the title, whose bands would otherwise be grouped into this one.
    let rows_from = lines[0].1;
    let rows_to = lines[1].1 + lines[1].3;
    let bands = ink_bands(&rgba, WINDOW_W, WINDOW_H, rows_from, rows_to, WINDOW_W);
    assert_eq!(
        bands.len(),
        2,
        "one band per tagline line box, {lines:?} line boxes but {bands:?} bands: a line \
         is either painted twice or not painted at all"
    );
    for (i, (b, l)) in bands.iter().zip(lines.iter()).enumerate() {
        assert!(
            b.top >= l.1 - SLACK && b.bottom <= l.1 + l.3 - 1 + SLACK,
            "tagline band {i} is on rows {}..{} but its line box is rows {}..{}: a line is \
             painted at another line's position",
            b.top,
            b.bottom,
            l.1,
            l.1 + l.3 - 1
        );
        assert!(
            b.left >= l.0 - SLACK && b.right <= l.0 + l.2 - 1 + SLACK,
            "tagline band {i} spans columns {}..{} but its line box is columns {}..{}: the \
             text overruns the box layout gave it",
            b.left,
            b.right,
            l.0,
            l.0 + l.2 - 1
        );
    }
    assert_ne!(
        bands[0].width(),
        bands[1].width(),
        "both tagline lines are {}px wide: line 0 was painted twice",
        bands[0].width()
    );

    // The last band IS the tail: it matches exactly one suffix of the string, the
    // longest one, and that suffix has to reach the last character.
    let last = bands[1];
    let mut matched: Option<&str> = None;
    for (i, _) in TAGLINE.char_indices() {
        // Trimmed, because layout drops leading whitespace and a reference render
        // of " drawn …" inks exactly the same pixels as one of "drawn …" — the
        // same reason the longest hit is the right one to report.
        let candidate = TAGLINE[i..].trim();
        if candidate.is_empty() {
            break;
        }
        if unwrapped_profile(candidate) == Some((last.width(), last.ink)) {
            // Suffixes are tried longest first, so the first hit is the longest.
            matched = Some(candidate);
            break;
        }
    }
    let matched = matched.unwrap_or_else(|| {
        panic!(
            "the tagline's last line ({}px wide, {} lit pixels) matches NO suffix of the \
             string rendered on its own, so it is not any suffix of it: line 0's paint \
             repeated, or a line the painter invented. Bands {bands:?}",
            last.width(),
            last.ink
        )
    });
    assert!(
        matched.ends_with(TAIL),
        "the last painted line is {matched:?}, which does not end with {TAIL:?}: the \
         paragraph's tail is not on the image"
    );

    // The artefact itself: a PNG of the app with the tagline on it.
    let png = velox_renderer::render_vnode_to_raster_png(&app, &sheet, WINDOW_W, WINDOW_H)
        .expect("raster png");
    let path = gate_dir().join(format!("tagline-{WINDOW_W}x{WINDOW_H}.png"));
    std::fs::write(&path, &png).expect("write png");
    assert!(path.exists(), "tagline png missing: {}", path.display());
    assert!(png.len() > 2000, "tagline png is {} bytes", png.len());
    println!(
        "wrote {} — tagline lines {lines:?}, bands {bands:?}, last line {matched:?}",
        path.display()
    );
}
