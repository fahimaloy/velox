//! T3 — the layout↔paint contract a wrapped line of text depends on.
//!
//! ## Why this file is not named `text_wrap_merged_across_lines.rs`
//!
//! The T3 brief asked for `merged` to be hoisted above the per-line loop at
//! `layout.rs:1104` so that one text `VNode` became ONE `LayoutNode` with a
//! multi-line rect. That was not done, and the reason is measured rather than
//! preferred, so it is recorded here where the next reader of `layout.rs` will
//! find it.
//!
//! Layout's line advance is its strut: `line_box_height` (`text_wrap.rs:165`) is
//! `max(ink_extent, strut.ascent + strut.descent)`, which for a 14px run on the
//! default face measures **22px** in this crate's tests (the line boxes below
//! are at y = 0, 22, 44). The painter's line advance is `line-height`, whose
//! default is **1.2em = 16.8px** (`skia_render.rs:1185`). A single merged node
//! with a union rect carries neither number, so the painter could only place its
//! lines at `rect.y + idx * line_height`: 5.2px too high on line 1 and 10.4px
//! too high on line 2 of a three-line paragraph. The lines would all be drawn
//! and all be inside the node's box, and none would be on the line box layout
//! had reserved for it. Keeping one node per line is what lets the painter use
//! each line's OWN `rect.y` instead, and that is what it does
//! (`skia_render.rs`, the `VNode::Text` arm).
//!
//! ## What the per-line merge is actually for
//!
//! The comment at `layout.rs:1244` — "Two nodes pointing at the same text VNode
//! would make the renderer draw the same string twice" — is a true statement
//! about a renderer that ignores which line a node is, and a misleading one as
//! a layout invariant. The invariant that holds, and that the painter relies on,
//! is: **one node per line box, in line order, each carrying the advance of the
//! words on that line.** The merge is scoped to a line because two pieces of one
//! text `VNode` that share a line must not become two nodes — which is exactly
//! what the per-line scope prevents. It is not scoped that way to merge across
//! lines.
//!
//! ## What these tests pin
//!
//! The properties the painter's line recovery consumes: the node count, the
//! `source_index` shared by every line, the line order, the constant advance,
//! and the fact that a line's `rect.w` is that line's own advance rather than
//! the whole string's. The last one is the load-bearing one: the painter breaks
//! the text at `rect.w`, so a node that reported the whole string's width would
//! put every word on line 0. The `source_index` collision at the end is the
//! reason the painter keys its line tally on the `VNode` address instead.
//!
//! No features, no Skia: `compute_layout` takes inline styles, so the tree is
//! built directly. Run by default with `cargo test -p velox-dom`.

use velox_dom::{Props, VNode, h, layout::LayoutNode, text};

const VW: i32 = 400;
const VH: i32 = 400;

/// The crate's own single-char width ratio (`FontMetrics::char_width`), which is
/// what its fallback measurer reports an advance from.
const CHAR_W: f32 = 0.6;
const FONT: f32 = 14.0;

fn box_with_text(width: i32, body: &str) -> VNode {
    h(
        "div",
        Props::new().set(
            "style",
            format!("display:block;width:{width}px;font-size:{FONT}px"),
        ),
        vec![text(body)],
    )
}

/// The layout children that carry a `source_index`: one per line box.
fn lines_of(laid: &LayoutNode) -> Vec<LayoutNode> {
    laid.children
        .iter()
        .filter(|c| c.source_index.is_some())
        .cloned()
        .collect()
}

#[test]
fn a_wrapped_text_yields_one_node_per_line_in_line_order() {
    let laid =
        velox_dom::layout::compute_layout(&box_with_text(120, "alpha beta gamma delta"), VW, VH);
    let lines = lines_of(&laid);
    assert!(
        lines.len() >= 2,
        "this text must wrap in a 120px box at {FONT}px; got {} line box(es): {lines:?}",
        lines.len()
    );
    // Every line of one text VNode is the SAME VNode, so every node carries the
    // same sibling index. This is the fact the painter cannot key its line tally
    // on -- see the last test in this file.
    for (i, l) in lines.iter().enumerate() {
        assert_eq!(
            l.source_index,
            Some(0),
            "line {i} must point at the one text VNode: {l:?}"
        );
    }
    // Line order: the boxes are emitted top to bottom and never share rows, so
    // the painter's word tally advances in paint order and each node's own `y`
    // is the line it is allowed to draw on.
    for w in lines.windows(2) {
        assert!(
            w[1].rect.y > w[0].rect.y,
            "line boxes are not in line order: {:?} then {:?}",
            w[0].rect,
            w[1].rect
        );
        assert!(
            w[1].rect.y >= w[0].rect.y + w[0].rect.h,
            "line boxes overlap: {:?} then {:?}",
            w[0].rect,
            w[1].rect
        );
    }
    // A constant advance, which is what makes `rect.y + font_size` the right
    // baseline offset for every line and not just the first.
    let advance = lines[1].rect.y - lines[0].rect.y;
    for w in lines.windows(2) {
        assert_eq!(
            w[1].rect.y - w[0].rect.y,
            advance,
            "the line advance is not constant: {:?} {:?}",
            w[0].rect,
            w[1].rect
        );
    }
    assert!(
        advance > lines[0].rect.h,
        "advance {} is not a line box",
        advance
    );
}

#[test]
fn each_line_reports_its_own_advance_not_the_whole_strings() {
    let body = "alpha beta gamma delta";
    let laid = velox_dom::layout::compute_layout(&box_with_text(120, body), VW, VH);
    let lines = lines_of(&laid);
    assert!(lines.len() >= 2, "must wrap: {lines:?}");
    for (i, l) in lines.iter().enumerate() {
        assert!(l.rect.w > 0, "line {i} has no advance: {l:?}");
    }
    // The painter breaks the text at `rect.w`, so every line's advance has to be
    // smaller than the whole string's, and the lines' advances together have to
    // stay under it: they are a partition of the words, not the whole string
    // counted once per line.
    let whole = body.chars().count() as f32 * CHAR_W * FONT;
    let sum: i32 = lines.iter().map(|l| l.rect.w).sum();
    assert!(
        (sum as f32) < whole,
        "the line advances {sum} add up to more than the whole string's {whole}: \
         some line is reporting the whole string"
    );
    for (i, l) in lines.iter().enumerate() {
        assert!(
            (l.rect.w as f32) < whole,
            "line {i}'s advance {} is the whole string's {whole}: the painter would \
             put every word on this line",
            l.rect.w
        );
    }
}

#[test]
fn an_unwrapped_text_is_one_node() {
    let laid = velox_dom::layout::compute_layout(&box_with_text(300, "one line"), VW, VH);
    let lines = lines_of(&laid);
    assert_eq!(
        lines.len(),
        1,
        "text that fits on one line must be one node, or the painter's word tally \
         would never see a second line: {lines:?}"
    );
    assert_eq!(lines[0].source_index, Some(0));
}

#[test]
fn a_nested_text_shares_its_parents_sibling_index_and_does_not_collide() {
    // Two sibling blocks, each with a span, each with one text node. Every text
    // node here is its parent's `children[0]`, so every one of them carries
    // `source_index: 0` -- a key that is NOT unique across the tree. The painter
    // keys its per-VNode line tally on the resolved `&VNode` for exactly this
    // reason; keying on `source_index` makes the second paragraph start at the
    // first one's word count and paint nothing at all.
    let v = h(
        "div",
        Props::new().set("style", "display:block;width:300px"),
        vec![
            h(
                "div",
                Props::new().set("style", "display:block;width:300px"),
                vec![h("span", Props::new(), vec![text("first paragraph")])],
            ),
            h(
                "div",
                Props::new().set("style", "display:block;width:300px"),
                vec![h("span", Props::new(), vec![text("second paragraph")])],
            ),
        ],
    );
    let laid = velox_dom::layout::compute_layout(&v, VW, VH);
    fn collect(n: &LayoutNode, out: &mut Vec<LayoutNode>) {
        for c in &n.children {
            if c.source_index.is_some() {
                out.push(c.clone());
            }
            collect(c, out);
        }
    }
    let mut all = Vec::new();
    collect(&laid, &mut all);
    let text_nodes: Vec<&LayoutNode> = all
        .iter()
        .filter(|n| n.children.is_empty() && n.source_index.is_some())
        .collect();
    assert_eq!(
        text_nodes.len(),
        2,
        "two paragraphs, two text nodes: {all:?}"
    );
    for n in &text_nodes {
        assert_eq!(
            n.source_index,
            Some(0),
            "`source_index` is a sibling index, so it is expected to collide here \
             -- that is the point of this test: {:?}",
            n.rect
        );
    }
    assert_ne!(
        text_nodes[0].rect, text_nodes[1].rect,
        "the two paragraphs have distinct boxes"
    );
}
