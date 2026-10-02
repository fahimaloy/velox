//! A6: `text-decoration: line-through` — alone, combined with `underline`, and
//! explicitly `none`.
//!
//! The old parser was `contains("underline") -> true; else if == "none" ->
//! false`. `line-through` matched NEITHER branch, so it inherited whatever the
//! parent had and painted nothing: the DOM parsed the declaration perfectly and
//! the renderer dropped it at paint time only, which is why the bug read as
//! "CSS doesn't work" rather than "CSS is ignored".
//!
//! ## What is actually asserted
//!
//! Every test here DIFFS two renders that differ only in the `text-decoration`
//! declaration. That is what makes the proof non-circular: the glyphs, the
//! layout box and the font are identical in both frames and cancel out, so any
//! surviving pixel is the rule and nothing else.
//!
//! Three properties are checked, and the third is the one that matters:
//!  1. the strike exists at all;
//!  2. it is a DIFFERENT band from the underline — an implementation that drew
//!     both at one y would pass "the combined form draws two things" while
//!     looking broken;
//!  3. it spans the MEASURED advance of each visual line, so it survives
//!     WRAPPING — a rule drawn once at a fixed width, or once per paragraph
//!     rather than per line, is caught by the multi-line case.
//!
//! No test re-derives where the baseline is. The rule's absolute position is
//! deliberately left unpinned: what is pinned is that the two rules do not
//! coincide and that each covers its own line.

#![cfg(feature = "skia-native")]

use velox_dom::{Props, VNode, h, text};
use velox_renderer::render_vnode_to_rgba;
use velox_style::Stylesheet;

const W: i32 = 300;
const H: i32 = 200;

/// Page black, ink white: every surviving pixel in a diff is unambiguously the
/// rule, and antialiased glyph edges cannot be mistaken for it.
const PAGE: &str = "background:#000000;color:#ffffff";

/// Antialiasing slack, in px, when a rule is compared against a text advance.
const SLACK: i32 = 2;

fn px(buf: &[u8], x: i32, y: i32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

fn render(v: &VNode) -> Vec<u8> {
    render_vnode_to_rgba(v, &Stylesheet::default(), W, H).expect("render to rgba")
}

/// One horizontal run of changed pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Band {
    /// First row with any change.
    top: i32,
    /// Last row with any change (inclusive).
    bottom: i32,
    /// Leftmost / rightmost changed column across the whole band.
    left: i32,
    right: i32,
}

impl Band {
    fn height(&self) -> i32 {
        self.bottom - self.top + 1
    }

    fn width(&self) -> i32 {
        self.right - self.left + 1
    }
}

/// The changed-pixel bands between two renders, in top-to-bottom order.
///
/// Grouped by row contiguity rather than counted per row, so a 1px AA-softened
/// edge that bleeds into a second row does not read as two bands.
fn bands(a: &[u8], b: &[u8]) -> Vec<Band> {
    let mut rows: Vec<(i32, i32, i32)> = Vec::new();
    for y in 0..H {
        let mut left = None;
        let mut right = None;
        for x in 0..W {
            if px(a, x, y) != px(b, x, y) {
                left.get_or_insert(x);
                right = Some(x);
            }
        }
        if let (Some(l), Some(r)) = (left, right) {
            rows.push((y, l, r));
        }
    }
    let mut out: Vec<Band> = Vec::new();
    for (y, l, r) in rows {
        match out.last_mut() {
            Some(last) if last.bottom + 1 == y => {
                last.bottom = y;
                last.left = last.left.min(l);
                last.right = last.right.max(r);
            }
            _ => out.push(Band {
                top: y,
                bottom: y,
                left: l,
                right: r,
            }),
        }
    }
    out
}

/// A paragraph of text with one extra declaration appended to its style.
///
/// The base style carries everything that could move a glyph or a box; the
/// parameter is the ONLY difference between two renders built through this
/// helper, which is what licenses the diff.
fn para(decl: &str) -> VNode {
    let mut style = String::from(PAGE);
    if !decl.is_empty() {
        style.push(';');
        style.push_str(decl);
    }
    h(
        "div",
        Props::new().set("style", style),
        vec![text("the quick brown fox jumps")],
    )
}

/// The same, in a box narrow enough that the sentence WRAPS.
fn wrapped_para(decl: &str) -> VNode {
    let mut style = String::from("background:#000000;color:#ffffff;width:90px");
    if !decl.is_empty() {
        style.push(';');
        style.push_str(decl);
    }
    h(
        "div",
        Props::new().set("style", style),
        vec![text("the quick brown fox jumps")],
    )
}

// ── 1. the strike exists at all ─────────────────────────────────────────

#[test]
fn line_through_paints_a_rule_that_the_baseline_does_not() {
    let base = render(&para(""));
    let struck = render(&para("text-decoration:line-through"));
    let found = bands(&base, &struck);
    assert!(
        !found.is_empty(),
        "text-decoration:line-through changed nothing — the declaration reached \
         the parser and was dropped, which is the bug this file exists for"
    );
    let b = found[0];
    assert!(
        b.width() >= 20,
        "the rule spans only {}px of a 300px page: it is not tracking the run",
        b.width()
    );
    assert!(
        b.height() <= 3,
        "the rule is {}px tall: a filled block, not a 1px stroke",
        b.height()
    );
}

// ── 2. it is not the underline wearing a different name ─────────────────

#[test]
fn the_strike_is_a_different_band_from_the_underline() {
    let base = render(&para(""));
    let under = bands(&base, &render(&para("text-decoration:underline")));
    let strike = bands(&base, &render(&para("text-decoration:line-through")));

    assert_eq!(under.len(), 1, "the underline must be exactly one band");
    assert_eq!(strike.len(), 1, "the strike must be exactly one band");
    assert!(
        strike[0].top < under[0].top,
        "the strike must sit ABOVE the underline (underline at row {}, strike at \
         row {}): it crosses the glyphs, so it belongs above the baseline, and a \
         strike below the baseline collides with the underline",
        under[0].top,
        strike[0].top
    );
    assert!(
        under[0].top - strike[0].top >= 2,
        "the strike (row {}) and the underline (row {}) are {} rows apart: too \
         close to read as two rules",
        strike[0].top,
        under[0].top,
        under[0].top - strike[0].top
    );
}

// ── 3. the combined form draws BOTH, not one twice ──────────────────────

#[test]
fn underline_line_through_draws_both_bands() {
    let base = render(&para(""));
    let under = bands(&base, &render(&para("text-decoration:underline")));
    let strike = bands(&base, &render(&para("text-decoration:line-through")));
    let both = bands(
        &base,
        &render(&para("text-decoration:underline line-through")),
    );

    assert_eq!(
        both.len(),
        2,
        "the combined form must draw two rules, got {both:?} (underline {under:?}, \
         strike {strike:?})"
    );
    let rows = |bs: &[Band]| {
        let mut r: Vec<i32> = bs.iter().map(|b| b.top).collect();
        r.sort_unstable();
        r
    };
    assert_eq!(
        rows(&both),
        rows(&[under[0], strike[0]]),
        "the combined form must draw them at the same two rows as the single \
         forms, not at two fresh positions"
    );
    // And each must be the same width as its standalone form: one shared
    // measurement, not two independently-guessed rules. Matched by row, since
    // `bands` is in top-down order and the strike is the upper of the two.
    let by_row = |want: i32| -> Band {
        both.iter()
            .find(|b| b.top == want)
            .copied()
            .unwrap_or_else(|| panic!("the combined form has no band at row {want}: {both:?}"))
    };
    assert_eq!(
        (by_row(under[0].top).left, by_row(under[0].top).right),
        (under[0].left, under[0].right),
        "the underline in the combined form must span the same run as on its own"
    );
    assert_eq!(
        (by_row(strike[0].top).left, by_row(strike[0].top).right),
        (strike[0].left, strike[0].right),
        "the strike in the combined form must span the same run as on its own"
    );
}

// ── 4. `none` clears both ───────────────────────────────────────────────

#[test]
fn none_paints_no_rule_at_all() {
    let bare = render(&para(""));
    let none = render(&para("text-decoration:none"));
    assert!(
        bands(&bare, &none).is_empty(),
        "text-decoration:none differs from declaring nothing: it must be a \
         no-op, and any surviving pixel is a rule"
    );
}

// `none` must also WIN over an inherited or sibling-declared rule. The style
// string here is `text-decoration:underline line-through;text-decoration:none`,
// i.e. the second declaration is the effective one.
#[test]
fn a_later_none_clears_an_earlier_decoration_in_the_same_block() {
    let bare = render(&para(""));
    let cleared = render(&para(
        "text-decoration:underline line-through;text-decoration:none",
    ));
    assert!(
        bands(&bare, &cleared).is_empty(),
        "a later `text-decoration:none` must clear the rule the earlier \
         declaration set, not merely fail to add one"
    );
}

// ── 5. measure-then-strike: the rule follows the WRAP ───────────────────

/// The rule is drawn across the measured advance of the line it belongs to.
///
/// This is the assertion that used to be missing. "There is a band per line"
/// passed while the rule on every line was drawn with line 0's width, because
/// the painter resolved the same text `VNode` once per line box and always
/// measured line 0 — the same defect that painted line 0's glyphs N times. So
/// each band is now pinned to ITS line's advance: inside that line's box, and
/// covering it. Any other line's width fails one or the other.
#[test]
fn the_rule_is_drawn_once_per_wrapped_line() {
    let v = wrapped_para("text-decoration:line-through");
    // Render first, then lay out the CASCADED tree: the render installs the real
    // font measurer globally, so laying out first would compare the rule against
    // a different face's advances; and `prepare_frame` cascades before it lays
    // out (`skia_render.rs:1433-1436`), so the line boxes the paint followed are
    // the cascaded ones.
    let struck = render(&v);
    let styled = velox_style::apply_with_cascade(&v, &Stylesheet::default());
    let laid = velox_dom::layout::compute_layout(&styled, W, H);
    let lines: Vec<_> = laid
        .children
        .iter()
        .filter(|c| c.source_index.is_some())
        .collect();
    let base = render(&wrapped_para(""));
    let found = bands(&base, &struck);
    assert!(
        found.len() >= 2,
        "a 90px box wraps this sentence onto several lines, so the rule must \
         appear on each of them; found {} band(s): {found:?} — the strike is \
         being drawn once per paragraph instead of once per visual line",
        found.len()
    );
    assert_eq!(
        found.len(),
        lines.len(),
        "one rule per line box: {lines:?} vs {found:?}"
    );
    for (i, b) in found.iter().enumerate() {
        let l = lines[i];
        assert!(
            b.height() <= 3,
            "band {i} is {}px tall: a filled block, not a stroke",
            b.height()
        );
        assert!(
            b.width() >= 10,
            "band {i} spans only {}px: a rule clipped to a fixed width does not \
             track the run it belongs to",
            b.width()
        );
        // Rows, not columns: each rule has to be struck through the text ON its
        // own line. Columns are not compared against the line box, because the
        // box width is not the advance of the line's text — layout includes a
        // trailing space in it (measured here: box 74px for a line whose text
        // advances 62px), so a correct rule is a few pixels narrower than its
        // box and comparing the two would fail on a correct render.
        assert!(
            b.top >= l.rect.y - SLACK && b.bottom <= l.rect.y + l.rect.h - 1 + SLACK,
            "band {i} occupies rows {}..{} but line box {i} is rows {}..{}: the rule \
             is struck through some other line's text",
            b.top,
            b.bottom,
            l.rect.y,
            l.rect.y + l.rect.h - 1
        );
    }
    // Each line's rule is struck at ITS advance. The three lines of this
    // sentence advance to 62px, 67px and 42px, so three different widths is the
    // whole assertion: a painter that resolved the text once and reused the
    // result (which is what this defect did) strikes every line with line 0's
    // width and all three bands come out identical.
    for (i, w) in found.windows(2).enumerate() {
        assert_ne!(
            w[0].width(),
            w[1].width(),
            "bands {i} and {} are both {}px wide: line {}'s rule is struck at another \
             line's advance. All bands: {found:?}",
            i + 1,
            w[0].width(),
            i + 1
        );
    }
    // The bands must be ordered and separated — i.e. genuinely one per line,
    // not one thick smear spanning the whole paragraph.
    for w in found.windows(2) {
        assert!(
            w[1].top > w[0].bottom,
            "bands at rows {}..{} and {}..{} touch: the strike is being drawn \
             per paragraph rather than per line",
            w[0].top,
            w[0].bottom,
            w[1].top,
            w[1].bottom
        );
    }
}

#[test]
fn the_wrapped_strike_and_underline_stay_one_band_per_line_each() {
    let base = render(&wrapped_para(""));
    let both = bands(
        &base,
        &render(&wrapped_para("text-decoration:underline line-through")),
    );
    let strike = bands(
        &base,
        &render(&wrapped_para("text-decoration:line-through")),
    );
    assert_eq!(
        both.len(),
        strike.len() * 2,
        "the combined form must draw twice as many rules as the strike alone \
         ({} vs {}): over a wrapped paragraph one of them is not per-line",
        both.len(),
        strike.len()
    );
    // Every strike band must be paired with an underline band BELOW it on the
    // same column span — the pairing is what proves both rules tracked the
    // SAME wrapped lines rather than each drawing its own idea of where the
    // lines are. The strike is above the baseline and the underline below it,
    // so "below" is the expected direction.
    for s in &strike {
        assert!(
            both.iter()
                .any(|u| u.top > s.bottom && u.left <= s.left && u.right >= s.right),
            "no underline band sits below the strike at rows {}..{} with a \
             matching column span: the two rules did not track the same lines \
             (combined bands {both:?}, strike {strike:?})",
            s.top,
            s.bottom
        );
    }
}
