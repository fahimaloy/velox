use crate::style::{Sides, TextOverflow, VerticalAlign, WhiteSpace};
use crate::{Length, Props, VNode};
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::rc::Rc;
use std::sync::RwLock;

/// Default font size for root element (used for rem calculations)
///
/// `pub` because the renderer resolves `rem` in a `font-size` declaration and
/// must use the SAME constant `compute_layout` seeds its recursion with as
/// `root_font_size`. A second literal `16.0` in the renderer would be a value
/// that could drift away from the one the boxes were laid out with, and nothing
/// at the call site would notice.
pub const DEFAULT_ROOT_FONT_SIZE: f32 = 16.0;

/// Font family used for text measurement when a caller has no stylesheet
/// context to name one. Matches the family `text_wrap::wrap_text` assumes.
pub const DEFAULT_TEXT_FAMILY: &str = "system-ui";

// ===== INLINE FORMATTING CONTEXT =========================================
//
// A `display: inline` box has no box of its own: its text participates in the
// line boxes of the BLOCK that contains it, not in a line box of its own. That
// is the whole of requirement 1, and two consequences shape everything below.
//
// 1. An inline run is FLATTENED before it is laid out. `<b>one</b> two` becomes
//    one ordered list of text leaves, so a line may break between "one" and
//    "two" and the two are still one paragraph and one line box. It cannot be
//    had any other way: lay the inline element out on its own first and the
//    break opportunity at its trailing edge is gone before anything can use it.
//
// 2. The emitted `LayoutNode` tree still MIRRORS the `VNode` tree level for
//    level. The renderer resolves `source_index` against the VNode children at
//    each level (`children.get(src_idx)`) and skips any LayoutNode it cannot
//    resolve, so a synthetic wrapper node would take its whole subtree with it
//    -- there is no way to represent a flattened fragment without one. An inline
//    element therefore still gets a LayoutNode: a degenerate one, since it has no
//    box, whose only job is to keep the mirroring. Its text children hang off it
//    carrying the real line geometry. An inline element fragmented across two
//    lines appears on both, which is what a browser does too.

/// A member of an inline run, addressed by its index path from the block
/// container's `children`.
enum InlineRunItem<'a> {
    /// An inline-level element that contributes no box. Its text is flattened
    /// into the run.
    Fragment {
        path: Vec<usize>,
        node: &'a VNode,
        font_size: f32,
        font_family: String,
        align: VerticalAlign,
        /// Whether this element's subtree contains any text. An inline element
        /// with none -- an empty `<span>`, an `<img>` with no `src`, which is not
        /// replaced and so has no box of its own -- still gets a box, because a box
        /// of zero size is still a box, and the `LayoutNode` tree has to mirror the
        /// `VNode` tree for the renderer to be able to reach it at all.
        ///
        /// An `<img>` that DOES carry a `src` never arrives here: it is replaced, so
        /// it is an `Atomic` and is measured by `lay_out_atomic`.
        has_text: bool,
    },
    /// `display: inline-block`, and every replaced element: one unbreakable box
    /// that establishes its own block formatting context, so it is never flattened
    /// and never split. See `is_atomic_inline_box` for the two ways in.
    Atomic {
        path: Vec<usize>,
        node: &'a VNode,
        font_size: f32,
        font_family: String,
        align: VerticalAlign,
    },
}

/// One unbreakable piece of a flattened inline run, already measured.
///
/// Splitting happens on whitespace RUNS and never inside a word. The split is
/// per leaf, so a break can land between two leaves exactly as easily as it can
/// inside one; that is what makes wrapping across an inline boundary work.
struct InlinePiece {
    /// Index into the run's items, so the emitter can recover the path.
    item: usize,
    text: String,
    width: i32,
    /// Ink above and below this piece's own baseline, from the seam. Zero for a
    /// whitespace piece, which has no ink -- it is still placed, and it still
    /// occupies width.
    ascent: f32,
    descent: f32,
    is_space: bool,
    /// The leaf this piece came from, for an `Atomic` item's baseline.
    atomic: bool,
    /// The line's own font's typographic ascent and descent. A text fragment's
    /// BOX is this tall -- a browser's inline content area is the font's
    /// ascent + descent, not the ink -- so `vertical-align` moves the box
    /// without changing its height, which is what makes the alignment legible
    /// in the geometry at all.
    strut_a: f32,
    strut_d: f32,
    /// A forced break, from a newline in `pre` or `pre-wrap`. It carries no text
    /// and no width; it exists so the line filler can see the break the
    /// tokenizer found.
    hard_break: bool,
    /// An atomic's laid-out box, with its own children and its own layout. It is
    /// moved into place once the line is filled, not rebuilt: laying it out again
    /// at the final position could reach a different result, since its width
    /// depends on the space left on the line.
    node: Option<LayoutNode>,
}

/// A token of a flattened run, before measurement.
enum InlineToken {
    Word(String),
    /// A whitespace run. In a collapsing mode the text is the single space a
    /// browser collapses to; in a preserving mode it is the author's own run.
    Space(String),
    /// A forced break, from a newline in `pre` / `pre-wrap`.
    Break,
}

/// Where the inline run ended, and how tall it was.
struct InlineRunResult {
    /// `cur_y` after the run: below its last line.
    cur_y: i32,
    /// The run's own furthest bottom, for the block loop's `max_y_end`.
    max_y_end: i32,
}

/// Resolve an element's `font-size`, in the same px-only convention the rest of
/// the layout path uses. `em`/`%` are not resolved and inherit instead.
fn inline_font_size(style: Option<&str>, inherited: f32) -> f32 {
    style_lookup_str(style, "font-size")
        .and_then(|v| {
            v.trim()
                .strip_suffix("px")
                .and_then(|n| n.trim().parse::<f32>().ok())
        })
        .unwrap_or(inherited)
}

fn inline_font_family(style: Option<&str>, inherited: &str) -> String {
    style_lookup_str(style, "font-family").unwrap_or_else(|| inherited.to_string())
}

/// Read `vertical-align` off a style string, falling back to the inherited
/// value when absent. An unparseable value leaves the inherited value in place
/// rather than being read as `baseline`.
fn inline_vertical_align(style: Option<&str>, inherited: VerticalAlign) -> VerticalAlign {
    style_lookup_str(style, "vertical-align")
        .and_then(|v| VerticalAlign::parse(&v))
        .unwrap_or(inherited)
}

/// Whether a child belongs to the enclosing block's inline run.
///
/// An inline-level box in flow. `display: inline-block` is INCLUDED, as an
/// atomic: it is inline-level, so it sits in a line box, but it is not
/// flattened, because it establishes a block formatting context of its own.
/// `inline-flex` and `inline-grid` are excluded deliberately -- they are block
/// containers and the flex path owns them, and routing them into the inline run
/// is the one thing that must not happen to it.
fn is_inline_run_member(node: &VNode) -> bool {
    match node {
        VNode::Text(t) => !t.is_empty(),
        VNode::Element { .. } => {
            // `inline-flex` and `inline-grid` ARE inline-level boxes (CSS Display 3
            // §2.1), and they are also block containers. They keep routing to the
            // flex path, which is the one real layout engine in this crate, so they
            // are excluded here: flattening one would put its children in the
            // enclosing block's line boxes and lose the flex layout entirely.
            if matches!(
                explicit_display(node).as_deref(),
                Some("inline-flex" | "inline-grid")
            ) {
                return false;
            }
            is_inline_level_box(node) && !is_out_of_flow(node)
        }
    }
}

/// Whether a subtree contains any non-empty text, at any depth.
fn subtree_has_text(node: &VNode) -> bool {
    match node {
        VNode::Text(t) => !t.is_empty(),
        VNode::Element { children, .. } => children.iter().any(subtree_has_text),
    }
}

/// Flatten `node` and its inline descendants into `out`.
///
/// Returns `false` when the node is not an inline run member at all, in which
/// case nothing was pushed and the caller must lay the child out as a block.
fn collect_inline_run<'a>(
    node: &'a VNode,
    path: &mut Vec<usize>,
    inherited_size: f32,
    inherited_family: &str,
    inherited_align: VerticalAlign,
    out: &mut Vec<InlineRunItem<'a>>,
) -> bool {
    match node {
        VNode::Text(t) => {
            if t.is_empty() {
                return false;
            }
            let (size, family, align) = match out.last() {
                Some(InlineRunItem::Fragment {
                    font_size,
                    font_family,
                    align,
                    ..
                }) => (*font_size, font_family.clone(), *align),
                Some(InlineRunItem::Atomic {
                    font_size,
                    font_family,
                    align,
                    ..
                }) => (*font_size, font_family.clone(), *align),
                None => (
                    inherited_size,
                    inherited_family.to_string(),
                    inherited_align,
                ),
            };
            out.push(InlineRunItem::Fragment {
                path: path.clone(),
                node,
                font_size: size,
                font_family: family,
                align,
                has_text: true,
            });
            true
        }
        VNode::Element {
            props, children, ..
        } => {
            if !is_inline_run_member(node) {
                return false;
            }
            let style = props.attrs.get("style").map(|s| s.as_str());
            let size = inline_font_size(style, inherited_size);
            let family = inline_font_family(style, inherited_family);
            let align = inline_vertical_align(style, inherited_align);
            let item = usize::from(is_atomic_inline_box(node));
            let boxed = family.clone();
            out.push(if item == 1 {
                InlineRunItem::Atomic {
                    path: path.clone(),
                    node,
                    font_size: size,
                    font_family: boxed.clone(),
                    align,
                }
            } else {
                InlineRunItem::Fragment {
                    path: path.clone(),
                    node,
                    font_size: size,
                    font_family: boxed,
                    align,
                    has_text: children.iter().any(subtree_has_text),
                }
            });
            // An atomic's children are its own block content, not this line's.
            if item == 0 {
                for (i, c) in children.iter().enumerate() {
                    path.push(i);
                    collect_inline_run(c, path, size, &family, align, out);
                    path.pop();
                }
            }
            true
        }
    }
}

/// Split one leaf's text into tokens.
///
/// `preserving` keeps the author's whitespace verbatim and turns a newline into
/// a forced break. Otherwise every whitespace run collapses to the single space
/// a browser collapses to. There is no `break-word` behaviour: a word too long
/// for a line overflows it rather than being split, which is the CSS default
/// (`overflow-wrap: normal`).
fn tokenize_inline(text: &str, preserving: bool, out: &mut Vec<InlineToken>) {
    // In a preserving mode the author's own run is emitted verbatim; otherwise
    // every run collapses to the single space a browser collapses to. `\n` is
    // only a forced break when whitespace is preserved, because a browser
    // collapses it to a space in the other modes.
    let emit_space = |space: &mut String, out: &mut Vec<InlineToken>| {
        if !space.is_empty() {
            let t = if preserving {
                space.clone()
            } else {
                " ".to_string()
            };
            out.push(InlineToken::Space(t));
            space.clear();
        }
    };
    let mut word = String::new();
    let mut space = String::new();
    for ch in text.chars() {
        if ch == '\n' && preserving {
            emit_space(&mut space, out);
            if !word.is_empty() {
                out.push(InlineToken::Word(std::mem::take(&mut word)));
            }
            out.push(InlineToken::Break);
        } else if ch.is_whitespace() {
            if !word.is_empty() {
                out.push(InlineToken::Word(std::mem::take(&mut word)));
            }
            if preserving {
                space.push(ch);
            } else {
                emit_space(&mut space, out);
                out.push(InlineToken::Space(" ".to_string()));
            }
        } else {
            emit_space(&mut space, out);
            word.push(ch);
        }
    }
    if !word.is_empty() {
        out.push(InlineToken::Word(word));
    }
    emit_space(&mut space, out);
}

/// `compute_layout`'s recursive box worker, as a plain function pointer.
type LayoutFn = fn(
    &VNode,
    i32,
    i32,
    i32,
    i32,
    i32,
    i32,
    ContainingBlock,
    Option<usize>,
    f32,
    f32,
) -> LayoutNode;

/// The context the inline formatting context takes from its block container.
struct InlineContext<'a> {
    /// `compute_layout`'s recursive worker, passed in as a pointer because it is
    /// a nested function and this code is not.
    at: LayoutFn,
    content_x: i32,
    line_limit: i32,
    font_size: f32,
    text_align: &'a str,
    ws: WhiteSpace,
    /// The container's `text-overflow`. Read by the line filler, and it was the
    /// one thing the block text branch did that the inline formatting context did
    /// not: R-5b deleted that branch, and with it the only production caller of
    /// `text_wrap`'s single-string wrapper, so `text-overflow: ellipsis` stopped
    /// reaching the layout tree while the renderer kept truncating at paint time.
    text_overflow: TextOverflow,
    scale: f32,
    viewport_w: i32,
    viewport_h: i32,
    cb: ContainingBlock,
    root_font_size: f32,
}

/// An empty `LayoutNode` carrying only a rect and a source index, which is all
/// a fragment of an inline run is. Spelled out rather than derived from
/// `Default` so a field added to `LayoutNode` cannot silently take a default here.
fn inline_leaf_node(rect: Rect, source_index: usize, children: Vec<LayoutNode>) -> LayoutNode {
    LayoutNode {
        rect,
        z_index: 0,
        display_none: false,
        source_index: Some(source_index),
        scroll_x: 0,
        scroll_y: 0,
        clip: None,
        stacking_context: false,
        scroll_height: 0,
        max_scroll_y: 0,
        scrollable: false,
        children,
    }
}

/// Lay one atomic inline-level box out: an `inline-block` or a replaced element.
///
/// Shrink-to-fit is CSS 2.1 §10.3.5: `min(max(preferred minimum, available),
/// preferred)`. The crate has no intrinsic-size helper, so `preferred` is taken
/// as the max-content width -- what the box wants when nothing constrains it --
/// by laying it out against the full line limit. When that overflows, it is laid
/// out a second time against the space actually left on the line. The
/// preferred-MINIMUM term is not modelled: a box whose content cannot shrink
/// below its widest unbreakable piece gets the available width instead, which for
/// a single text child is the same number.
fn lay_out_atomic(
    node: &VNode,
    source_index: usize,
    ctx: &InlineContext<'_>,
    line_width_used: i32,
    y: i32,
) -> LayoutNode {
    let available = (ctx.line_limit - line_width_used).max(0);

    // A REPLACED element is not shrink-to-fit, and the two-pass probe below would
    // quietly make it so. `max_content_width` of a source with no content of its
    // own is 0, so the second pass would hand `at` a target of 0 and a
    // `width: 25%` would resolve a quarter of ZERO rather than a quarter of the
    // containing block. CSS 2.1 §10.3.5 is for a width of `auto`, and a replaced
    // element's width is not `auto` when a `src` names a source: it is the
    // declaration, or the presentational hint, or the intrinsic size -- the order
    // `at` already resolves. So the available width goes in unchanged and `at`
    // decides, which also keeps the fallback order in one place instead of two.
    //
    // A replaced element wider than the space left on the line overflows it,
    // which is what a browser does and what a shrink-to-fit would have hidden.
    if is_replaced_element(node) {
        return lay_out_atomic_at(node, source_index, available, ctx, y).0;
    }

    let inset = atomic_padding_and_border(node, ctx);

    // `at` lays a block out at whatever width it is given and a block with no
    // declared `width` FILLS it, so a first pass at the available width cannot
    // report how wide the content would like to be -- it just reports the
    // available width. CSS 2.1 §10.3.5 wants the max-content width clamped by
    // the available space, so the content is measured first, at a width wide
    // enough that it cannot wrap, and read back off the result.
    //
    // APPROXIMATION, and it is a real one: a descendant whose width is a
    // PERCENTAGE resolves against the probe, so it asks for the probe's share
    // and the box comes out as wide as the line instead of its max-content
    // width. A child with no percentage width is exact. The preferred-MINIMUM
    // term of §10.3.5 is not modelled either, so content with no break
    // opportunity does not overflow the way a browser's would.
    let probe = available
        .saturating_mul(4)
        .max(4096)
        .saturating_add(inset)
        .min(i32::MAX / 4);
    let (probe_layout, _) = lay_out_atomic_at(node, source_index, probe, ctx, y);
    let max_content = max_content_width(&probe_layout);
    // The second pass is UNCONDITIONAL, and the arithmetic is why it can be.
    // `min(max_content, available) <= available < max(4 * available, 4096)` for
    // every `available >= 0`, so `target < probe` always holds and there is
    // nothing to guard.
    //
    // An earlier version guarded on `max_content > 0`, reading as "do not shrink
    // to nothing". Its effect was the opposite: `max_content_width` returns 0
    // when the tree has no content at all, which is exactly the case that
    // should shrink to nothing, so an inline-block with no children came out
    // `probe` wide -- 4096px. The content box of an empty atomic is 0 plus its
    // padding and border, and `target` says so. Dropping the guard also closed
    // the other degenerate case the guard was standing in for: content that asks
    // for MORE than the line has used to be left at the probe width, so a box
    // whose single unbreakable child is wider than the line came out as wide as
    // the probe instead of as wide as the line.
    let target = max_content.min(available).saturating_add(inset);
    lay_out_atomic_at(node, source_index, target, ctx, y).0
}

/// Lay one atomic at an exact available width. `source_index` is the atomic's
/// own index among the block's children, and it is REQUIRED: `at` treats a
/// `None` as "this is the viewport root" and hands back a box that fills the
/// viewport in both axes, and the renderer and the hit tester both resolve a
/// `None` by skipping the node's whole subtree.
fn lay_out_atomic_at(
    node: &VNode,
    source_index: usize,
    avail_w: i32,
    ctx: &InlineContext<'_>,
    y: i32,
) -> (LayoutNode, i32) {
    let laid = (ctx.at)(
        node,
        ctx.content_x,
        y,
        avail_w,
        ctx.cb.h,
        ctx.viewport_w,
        ctx.viewport_h,
        ctx.cb,
        Some(source_index),
        ctx.root_font_size,
        font_size_of(node, ctx),
    );
    (laid, avail_w)
}

/// The padding and border an atomic adds around its content, which is the
/// difference between the CONTENT width §10.3.5 clamps and the containing width
/// `at` is handed.
fn atomic_padding_and_border(node: &VNode, ctx: &InlineContext<'_>) -> i32 {
    let VNode::Element { props, .. } = node else {
        return 0;
    };
    let style = props.attrs.get("style").map(|s| s.as_str());
    let basis = ctx.cb.w as f32;
    let (vw, vh) = (ctx.viewport_w as f32, ctx.viewport_h as f32);
    let fs = font_size_of(node, ctx);
    let (pl, pr, _, _) =
        style_box_sides_full(style, "padding", basis, fs, ctx.root_font_size, vw, vh);
    let (bl, br, _, _) = style_border_widths(style, basis, fs, ctx.root_font_size, vw, vh);
    pl + pr + bl + br
}

/// The font size an atomic's own style declares, for resolving its padding and
/// border percentages.
fn font_size_of(node: &VNode, ctx: &InlineContext<'_>) -> f32 {
    let VNode::Element { props, .. } = node else {
        return ctx.font_size;
    };
    let style = props.attrs.get("style").map(|s| s.as_str());
    style_lookup_font_size(
        style,
        ctx.font_size,
        ctx.root_font_size,
        (ctx.viewport_w as f32, ctx.viewport_h as f32),
    )
    .unwrap_or(ctx.font_size)
}

/// The ascent above, and descent below, an atomic's own baseline.
fn atomic_baseline(node: &VNode, laid: &LayoutNode) -> (f32, f32) {
    let h = laid.rect.h as f32;
    let asc = atomic_baseline_offset(node, laid).clamp(0.0, h);
    (asc, (h - asc).max(0.0))
}

/// Where an atomic's baseline sits, measured down from its own top edge.
///
/// CSS 2.1 §10.8.1 makes an inline box's baseline the baseline of its last
/// in-flow line box, "unless it has either no in-flow line boxes or if its
/// 'overflow' property has a computed value other than 'visible', in which case
/// the baseline is the bottom margin edge". Both halves of that are honoured
/// here. The last line box is not marked in the laid-out tree, so it is found
/// in the VNode instead: the last text or atomic-inline descendant, which is
/// where a browser looks too, paired with the last leaf box in the tree, which
/// is that line's box.
///
/// APPROXIMATION, and the exactness is stated rather than assumed: the offset
/// added is the descendant's own font ascent, which is EXACT when the tallest
/// thing on that line is its own strut, and short by the overshoot when a run on
/// that line reaches higher than the strut. An atomic with no in-flow text or
/// atomic-inline descendant reports its bottom edge, which is also what a
/// browser does.
fn atomic_baseline_offset(node: &VNode, laid: &LayoutNode) -> f32 {
    if !atomic_overflow_is_visible(node) {
        return laid.rect.h as f32;
    }
    let mut last: Option<&LayoutNode> = None;
    let mut stack: Vec<&LayoutNode> = laid.children.iter().collect();
    while let Some(n) = stack.pop() {
        if n.children.is_empty()
            && n.rect.h > 0
            && last.is_none_or(|l: &LayoutNode| n.rect.y >= l.rect.y)
        {
            last = Some(n);
        }
        stack.extend(n.children.iter());
    }
    let Some(leaf) = last else {
        return laid.rect.h as f32;
    };
    let top = (leaf.rect.y - laid.rect.y) as f32;
    match last_inline_leaf_below(node, inherited_font_size(node)) {
        Some(InlineLeaf::Text(fs)) => top + FontMetrics::from_font_size(fs).ascent,
        Some(InlineLeaf::Atomic(child)) => top + atomic_baseline_offset(child, leaf),
        None => laid.rect.h as f32,
    }
}

/// The two kinds of descendant an inline formatting context is made of, for the
/// purpose of finding a line's baseline.
enum InlineLeaf<'a> {
    Text(f32),
    Atomic(&'a VNode),
}

/// The last in-flow text or atomic-inline descendant BENEATH `node`, in
/// document order, and the font size it inherits. `node` itself is never the
/// answer: this is only ever asked about an atomic, and asking an atomic about
/// itself would recurse forever.
///
/// An atomic-inline child is returned whole and not descended into: it
/// establishes its own block formatting context, so what is inside it is not on
/// this line.
fn last_inline_leaf_below<'a>(node: &'a VNode, inherited: f32) -> Option<InlineLeaf<'a>> {
    let VNode::Element {
        props, children, ..
    } = node
    else {
        return None;
    };
    let style = props.attrs.get("style").map(|s| s.as_str());
    let fs = own_font_size(style, inherited);
    children.iter().rev().find_map(|c| match c {
        VNode::Text(_) => Some(InlineLeaf::Text(fs)),
        VNode::Element { .. } if is_out_of_flow(c) => None,
        VNode::Element { .. } if is_atomic_inline_box(c) => Some(InlineLeaf::Atomic(c)),
        other => last_inline_leaf_below(other, fs),
    })
}

/// The font size `style` declares, or `inherited` when it declares none.
fn own_font_size(style: Option<&str>, inherited: f32) -> f32 {
    style_lookup_font_size(style, inherited, inherited, (0.0, 0.0)).unwrap_or(inherited)
}

/// The font size a node inherits, which is its own when it does not declare
/// one. `last_inline_leaf` only needs it to read the STRUT off, and a strut is
/// defined against the element's own font, so `DEFAULT_ROOT_FONT_SIZE` is the
/// right answer for a node that inherits from nothing.
fn inherited_font_size(node: &VNode) -> f32 {
    let VNode::Element { props, .. } = node else {
        return DEFAULT_ROOT_FONT_SIZE;
    };
    let style = props.attrs.get("style").map(|s| s.as_str());
    style_lookup_font_size(
        style,
        DEFAULT_ROOT_FONT_SIZE,
        DEFAULT_ROOT_FONT_SIZE,
        (0.0, 0.0),
    )
    .unwrap_or(DEFAULT_ROOT_FONT_SIZE)
}

/// Whether an atomic's computed `overflow` is `visible`, which decides whether
/// it has a baseline at all (CSS 2.1 §10.8.1). An absent `overflow` is
/// `visible`.
fn atomic_overflow_is_visible(node: &VNode) -> bool {
    let VNode::Element { props, .. } = node else {
        return true;
    };
    match style_lookup_str(props.attrs.get("style").map(|s| s.as_str()), "overflow") {
        None => true,
        Some(v) => v.trim() == "visible",
    }
}

/// How wide the content of a laid-out tree asked to be.
///
/// Read back rather than computed, because nothing in this crate returns an
/// intrinsic size. It is the distance from the leftmost content edge to the
/// furthest right edge any descendant reaches, so the box's own padding and
/// border cancel out and no box-model arithmetic is needed. The root's own
/// width is excluded: it is the measurement probe, not the content.
///
/// ZERO means the tree has no content at all, which is a real answer and not a
/// failure: CSS 2.1 §10.3.5's `min(max(preferred minimum, available),
/// preferred)` gives an empty box a content width of 0. It used to be read as
/// "unknown" by the caller, which turned every empty atomic into a 4096px box.
fn max_content_width(root: &LayoutNode) -> i32 {
    let mut left = i32::MAX;
    let mut right = i32::MIN;
    let mut stack: Vec<&LayoutNode> = root.children.iter().collect();
    while let Some(n) = stack.pop() {
        left = left.min(n.rect.x);
        right = right.max(n.rect.x + n.rect.w);
        stack.extend(n.children.iter());
    }
    if left == i32::MAX { 0 } else { right - left }
}

/// A piece of one line, merged back into the single `LayoutNode` the renderer
/// expects for that VNode on that line.
struct MergedRun {
    item: usize,
    text: String,
    x: i32,
    w: i32,
    ascent: f32,
    descent: f32,
    is_atomic: bool,
    /// Top of this merged run's box, which is NOT the line's top unless the run
    /// is `vertical-align: top`.
    y: i32,
    /// Height of this merged run's box.
    h: i32,
    /// An atomic's own box, moved into place rather than rebuilt.
    node: Option<LayoutNode>,
}

/// A slot in the per-line `LayoutNode` tree.
///
/// `Leaf` is a real box. `Node` is an inline element: it has no box of its own
/// and exists only so the tree keeps mirroring the `VNode` tree, which is what
/// the renderer requires. `Empty` is a child index the line does not reach --
/// a sibling of different display type, or a dropped whitespace node -- and is
/// skipped without disturbing the indices around it.
#[derive(Clone)]
enum InlineSlot {
    Empty,
    Leaf(usize),
    Node(Vec<InlineSlot>),
}

/// Arrange the merged runs of one line back into the `VNode` tree's shape.
fn build_inline_slots(merged: &[MergedRun], run: &[InlineRunItem<'_>]) -> Vec<InlineSlot> {
    let mut root: Vec<InlineSlot> = Vec::new();
    for (mi, m) in merged.iter().enumerate() {
        let path = match &run[m.item] {
            InlineRunItem::Fragment { path, .. } | InlineRunItem::Atomic { path, .. } => path,
        };
        let mut cur = &mut root;
        for &idx in &path[..path.len() - 1] {
            if cur.len() <= idx {
                cur.resize(idx + 1, InlineSlot::Empty);
            }
            if !matches!(cur[idx], InlineSlot::Node(_)) {
                cur[idx] = InlineSlot::Node(Vec::new());
            }
            cur = match &mut cur[idx] {
                InlineSlot::Node(v) => v,
                _ => unreachable!("just replaced with a Node"),
            };
        }
        let last = path[path.len() - 1];
        if cur.len() <= last {
            cur.resize(last + 1, InlineSlot::Empty);
        }
        cur[last] = InlineSlot::Leaf(mi);
    }
    root
}

/// Move a box and everything under it by `(dx, dy)`. Clips travel with their
/// owner, so a scrolled descendant does not leave its clip behind.
fn translate_inline_box(node: &mut LayoutNode, dx: i32, dy: i32) {
    node.rect.x += dx;
    node.rect.y += dy;
    if let Some(clip) = node.clip.as_mut() {
        clip.x += dx;
        clip.y += dy;
    }
    for child in node.children.iter_mut() {
        translate_inline_box(child, dx, dy);
    }
}

fn inline_slots_to_nodes(slots: &[InlineSlot], merged: &[MergedRun]) -> Vec<LayoutNode> {
    let mut out: Vec<LayoutNode> = Vec::new();
    for (idx, slot) in slots.iter().enumerate() {
        match slot {
            InlineSlot::Empty => {}
            InlineSlot::Leaf(mi) => {
                let m = &merged[*mi];
                if let Some(mut node) = m.node.clone() {
                    // An atomic is laid out before its position on the line is
                    // known, so it is MOVED here rather than rebuilt: its own
                    // layout already decided where its children go, relative to
                    // it, and laying it out again at the final x could reach a
                    // different result, because its width depends on the space
                    // left on the line.
                    let (from_x, from_y) = (node.rect.x, node.rect.y);
                    translate_inline_box(&mut node, m.x - from_x, m.y - from_y);
                    out.push(node);
                    continue;
                }
                out.push(inline_leaf_node(
                    Rect {
                        x: m.x,
                        y: m.y,
                        w: m.w,
                        h: m.h,
                    },
                    idx,
                    Vec::new(),
                ));
            }
            InlineSlot::Node(kids) => {
                let kids = inline_slots_to_nodes(kids, merged);
                if kids.is_empty() {
                    continue;
                }
                // An inline box is a SPAN, not a block box: CSS 2.1 §9.4.2 gives it
                // no box, so this rect is the union of its fragments on this line
                // and nothing more. It is what lets the renderer reach a text node
                // inside an inline element, which it does by index.
                // The rect is the union of the fragments, in BOTH axes. Using
                // the line box's height here instead would report a top-aligned
                // inline element as as tall as the line, which is the one thing
                // its own box is not.
                let x = kids.iter().map(|k| k.rect.x).min().unwrap();
                let right = kids.iter().map(|k| k.rect.x + k.rect.w).max().unwrap();
                let top = kids.iter().map(|k| k.rect.y).min().unwrap();
                let bottom = kids.iter().map(|k| k.rect.y + k.rect.h).max().unwrap();
                out.push(inline_leaf_node(
                    Rect {
                        x,
                        y: top,
                        w: right - x,
                        h: bottom - top,
                    },
                    idx,
                    kids,
                ));
            }
        }
    }
    out
}

/// Truncate one filled line's TEXT pieces to the line limit, appending an
/// ellipsis to the piece the text ran out in, and returning the line's new
/// width. `None` when the line does not truncate.
///
/// Only text pieces take part. An atomic is a box, and clipping one to fit a
/// line needs a clip region the layout tree does not carry, so a line made only
/// of atomics does not truncate.
///
/// The ellipsis lands in the piece where the text ran out, so a run spanning
/// `<b>aa</b> bbbbbb` keeps `aa ` inside the `<b>` and the ellipsis in the text
/// node after it, which is where a browser puts it. Pieces emptied by the
/// truncation keep their identity and stay in the line at width 0, because the
/// tree has to keep mirroring the VNode tree.
fn truncate_line_with_ellipsis(
    pieces: &mut [InlinePiece],
    line: &[usize],
    run: &[InlineRunItem<'_>],
    ctx: &InlineContext<'_>,
) -> Option<i32> {
    let text_pieces: Vec<usize> = line
        .iter()
        .copied()
        .filter(|&pi| {
            let p = &pieces[pi];
            !p.atomic && !p.hard_break && !p.text.is_empty()
        })
        .collect();
    if text_pieces.is_empty() {
        return None;
    }
    let font_of = |item: usize| match &run[item] {
        InlineRunItem::Fragment {
            font_size,
            font_family,
            ..
        }
        | InlineRunItem::Atomic {
            font_size,
            font_family,
            ..
        } => (*font_size, font_family.as_str()),
    };
    let fragments: Vec<crate::text_wrap::MeasurableFragment> = text_pieces
        .iter()
        .map(|&pi| {
            let (font_size, font_family) = font_of(pieces[pi].item);
            crate::text_wrap::MeasurableFragment {
                text: pieces[pi].text.clone(),
                width: pieces[pi].width as f32,
                font_size,
                font_family: font_family.to_string(),
            }
        })
        .collect();
    let kept = crate::text_wrap::truncate_fragments_with_ellipsis(
        &fragments,
        ctx.line_limit as f32,
        ctx.scale,
    )?;

    // Apply: every fragment keeps its prefix, everything from the truncation
    // point on is emptied, and the ellipsis joins the fragment it stopped in.
    for (fi, &pi) in text_pieces.iter().enumerate() {
        let (font_size, font_family) = font_of(pieces[pi].item);
        let full = pieces[pi].text.chars().count();
        let take = kept[fi].min(full);
        let mut text: String = pieces[pi].text.chars().take(take).collect();
        if take < full {
            text.push_str(crate::text_wrap::ELLIPSIS);
        }
        let width = crate::text_wrap::measure_text_metrics(&text, font_size, font_family, ctx.scale)
            .width
            .round() as i32;
        pieces[pi].text = text;
        pieces[pi].width = width;
        if take < full {
            // Everything after the truncation point is dropped, and nothing after
            // it may end up holding the ellipsis.
            for &after in &text_pieces[fi + 1..] {
                pieces[after].text = String::new();
                pieces[after].width = 0;
            }
            break;
        }
    }
    Some(line.iter().map(|&pi| pieces[pi].width).sum())
}

/// Lay out one inline run and append its line boxes to `laid_children`.
///
/// `cur_y` is where the run starts. The returned `cur_x` is the right edge of
/// the last line's ink, which the block loop needs so a following block child
/// knows a line was used.
fn flush_inline_run(
    run: &mut Vec<InlineRunItem<'_>>,
    ctx: &InlineContext<'_>,
    laid_children: &mut Vec<LayoutNode>,
    cur_y: i32,
) -> InlineRunResult {
    if run.is_empty() {
        return InlineRunResult {
            cur_y,
            max_y_end: cur_y,
        };
    }
    // The run is CONSUMED: it is taken, not copied, because leaving it populated
    // would make the next flush lay out everything again on top of the new
    // content, and because the emission below still needs to read the items'
    // index paths.
    let run = std::mem::take(run);
    let preserving = matches!(ctx.ws, WhiteSpace::Pre | WhiteSpace::PreWrap);
    let wrapping = matches!(
        ctx.ws,
        WhiteSpace::Normal | WhiteSpace::PreWrap | WhiteSpace::PreLine
    );

    // --- measure every piece of the run -------------------------------------
    let mut pieces: Vec<InlinePiece> = Vec::new();
    for (ii, item) in run.iter().enumerate() {
        match item {
            InlineRunItem::Fragment {
                node: VNode::Text(t),
                font_size,
                font_family,
                ..
            } => {
                let leaf_strut = FontMetrics::from_font_size(*font_size);
                let mut tokens = Vec::new();
                tokenize_inline(t, preserving, &mut tokens);
                for token in tokens {
                    if let InlineToken::Break = token {
                        pieces.push(InlinePiece {
                            item: ii,
                            text: String::new(),
                            width: 0,
                            ascent: 0.0,
                            descent: 0.0,
                            is_space: false,
                            atomic: false,
                            strut_a: 0.0,
                            strut_d: 0.0,
                            hard_break: true,
                            node: None,
                        });
                        continue;
                    }
                    let (text, is_space) = match token {
                        InlineToken::Word(w) => (w, false),
                        InlineToken::Space(s) => (s, true),
                        InlineToken::Break => unreachable!("handled above"),
                    };
                    let m = crate::text_wrap::measure_text_metrics(
                        &text,
                        *font_size,
                        font_family,
                        ctx.scale,
                    );
                    pieces.push(InlinePiece {
                        item: ii,
                        text,
                        width: m.width.round() as i32,
                        ascent: m.ascent,
                        descent: m.descent,
                        is_space,
                        atomic: false,
                        strut_a: leaf_strut.ascent,
                        strut_d: leaf_strut.descent,
                        hard_break: false,
                        node: None,
                    });
                }
            }
            InlineRunItem::Fragment { .. } => {
                // An inline element with no text of its own still needs to appear
                // in the run so the tree keeps mirroring, but it contributes
                // nothing to the line.
                pieces.push(InlinePiece {
                    item: ii,
                    text: String::new(),
                    width: 0,
                    ascent: 0.0,
                    descent: 0.0,
                    is_space: false,
                    atomic: false,
                    strut_a: 0.0,
                    strut_d: 0.0,
                    hard_break: false,
                    node: None,
                });
            }
            InlineRunItem::Atomic {
                node,
                path,
                font_family: _,
                font_size,
                align: _,
            } => {
                let strut = FontMetrics::from_font_size(*font_size);
                let laid = lay_out_atomic(
                    node,
                    *path
                        .last()
                        .expect("an inline run item's path is never empty"),
                    ctx,
                    0,
                    cur_y,
                );
                let (ascent, descent) = atomic_baseline(node, &laid);
                pieces.push(InlinePiece {
                    item: ii,
                    text: String::new(),
                    width: laid.rect.w,
                    ascent,
                    descent,
                    is_space: false,
                    atomic: true,
                    strut_a: strut.ascent,
                    strut_d: strut.descent,
                    hard_break: false,
                    node: Some(laid),
                });
            }
        }
    }

    // --- fill lines ---------------------------------------------------------
    let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
    let mut line_w: Vec<i32> = vec![0];
    for (pi, p) in pieces.iter().enumerate() {
        if p.hard_break {
            // A break at the start of a line would make an empty line, which has
            // no piece to hang a box on and so cannot be represented; a leading
            // break is dropped, exactly as a leading collapsible space is.
            if !lines.last().is_some_and(Vec::is_empty) {
                lines.push(Vec::new());
                line_w.push(0);
            }
            continue;
        }
        match p.is_space && !p.atomic {
            true => {
                // A line never starts with a COLLAPSIBLE space: the break that
                // produced the line consumed it, which is what CSS 2.1 §16.6 means
                // by it. A PRESERVED space is not collapsible and so is not
                // removed, which is the whole difference `pre` makes.
                if !preserving && lines.last().is_some_and(Vec::is_empty) {
                    continue;
                }
                if wrapping && line_w.last().is_some_and(|w| w + p.width > ctx.line_limit) {
                    lines.push(Vec::new());
                    line_w.push(0);
                    continue;
                }
                lines.last_mut().expect("one line").push(pi);
                *line_w.last_mut().expect("one width") += p.width;
            }
            false => {
                let cur_empty = lines.last().is_some_and(Vec::is_empty);
                if wrapping
                    && !cur_empty
                    && line_w.last().is_some_and(|w| w + p.width > ctx.line_limit)
                {
                    lines.push(Vec::new());
                    line_w.push(0);
                }
                lines.last_mut().expect("one line").push(pi);
                *line_w.last_mut().expect("one width") += p.width;
            }
        }
    }
    if lines.last().is_some_and(Vec::is_empty) && lines.len() > 1 {
        lines.pop();
        line_w.pop();
    }

    // --- `text-overflow: ellipsis` ------------------------------------------
    //
    // Truncation happens AFTER the fill and BEFORE placement, so a truncated
    // line is placed at its truncated width and the ellipsis hangs in the box
    // the break happened in.
    //
    // WHICH lines truncate follows the rule the single-string wrapper used, which
    // is the behaviour the block path had before R-5b: a white-space mode that
    // does not wrap truncates each of its own overflowing lines, and one that
    // does wrap truncates only when the whole run came out as a single
    // overflowing line. A browser puts the ellipsis on the last line a block
    // actually clips, which for a wrapping block is its last line whether or not
    // the run came out as one line. That is a recorded divergence, and matching
    // it needs to know which line is the clipped one, which needs the clip
    // region the layout tree does not carry.
    if matches!(ctx.text_overflow, TextOverflow::Ellipsis) {
        let targets: Vec<usize> = if wrapping {
            if lines.len() == 1 && line_w.first().is_some_and(|w| *w > ctx.line_limit) {
                vec![0]
            } else {
                Vec::new()
            }
        } else {
            (0..lines.len())
                .filter(|li| line_w[*li] > ctx.line_limit)
                .collect()
        };
        for li in targets {
            if let Some(new_w) = truncate_line_with_ellipsis(&mut pieces, &lines[li], &run, ctx) {
                line_w[li] = new_w;
            }
        }
    }

    // --- place each line ----------------------------------------------------
    let strut = FontMetrics::from_font_size(ctx.font_size);
    let align_of = |item: usize| match &run[item] {
        InlineRunItem::Fragment { align, .. } | InlineRunItem::Atomic { align, .. } => *align,
    };
    let mut y = cur_y;
    let mut max_y_end = cur_y;
    for (li, line) in lines.iter().enumerate() {
        // The line box's extents. The strut is a floor, so a run of "xxx" -- whose
        // ink is well under it -- still gets a full line.
        let mut max_a = strut.ascent;
        let mut max_d = strut.descent;
        for &pi in line {
            let p = &pieces[pi];
            // A piece's contribution to its line is its INK, floored by its own
            // font's content area. The floor is not optional: an inline element
            // in a larger font has an ink of its own x-height but a content area
            // of its own em, and without the floor its box would hang off the
            // line instead of sitting in it.
            //
            // A space has no ink at all, so it contributes exactly its content
            // area. That matters because the seam refuses a measurer that reports
            // no vertical extent and substitutes the labelled fallback, so a
            // space's "ink" is really the fallback's guess, and taking it as a
            // run's extent let one space make a line taller than the font that
            // owns it.
            let (ink_a, ink_d) = if p.is_space && !p.atomic {
                (p.strut_a, p.strut_d)
            } else {
                (p.ascent.max(p.strut_a), p.descent.max(p.strut_d))
            };
            match align_of(p.item) {
                VerticalAlign::Baseline => {
                    max_a = max_a.max(ink_a);
                    max_d = max_d.max(ink_d);
                }
                // CSS 2.1 §10.8.1: the box's vertical midpoint goes to the parent's
                // baseline plus half the parent's x-height.
                VerticalAlign::Middle => {
                    let half = strut.x_height / 2.0;
                    let h = ink_a + ink_d;
                    max_a = max_a.max(h / 2.0 - half);
                    max_d = max_d.max(h / 2.0 + half);
                }
                _ => {}
            }
        }
        // `top` and `bottom` are defined against the line box's own edges, which
        // the baseline items have just fixed -- genuinely circular. Resolved in
        // one pass: they are placed against the extents the strut and the
        // baseline items produced, and the line grows by exactly the amount
        // needed to hold them. Two items of these alignments that would each
        // need the other to move first are the one case where this differs from
        // CSS's cycle, and it is recorded as a known limit.
        let (base_a, base_d) = (max_a, max_d);
        for &pi in line {
            let p = &pieces[pi];
            // The height that is being aligned is the BOX's height, and a box's
            // height is not its ink: an atomic inline is its own box, while a
            // text fragment or a plain inline element is the line's font's
            // content area. Using the ink here would let a top-aligned run of
            // "Hg" grow the line by its descender even though its box does not
            // reach that far.
            let h = if p.atomic {
                p.ascent + p.descent
            } else {
                p.strut_a + p.strut_d
            };
            match align_of(p.item) {
                VerticalAlign::Top => max_d = max_d.max((h - base_a).max(0.0)),
                VerticalAlign::Bottom => max_a = max_a.max((h - base_d).max(0.0)),
                _ => {}
            }
        }
        let extents = crate::text_wrap::MeasuredText {
            width: 0.0,
            ascent: max_a,
            descent: max_d,
        };
        let height = crate::text_wrap::line_box_height(&extents, &strut);

        // A space at the very end of a line hangs past it and does not count
        // towards the line's width for `text-align`.
        let mut ink = line_w[li];
        if line
            .last()
            .is_some_and(|pi| pieces[*pi].is_space && !pieces[*pi].atomic)
        {
            ink -= pieces[line[line.len() - 1]].width;
        }
        let left = match ctx.text_align {
            "center" => ctx.content_x + ((ctx.line_limit - ink).max(0) / 2),
            "right" => ctx.content_x + (ctx.line_limit - ink).max(0),
            // `justify` is not implemented; it falls back to left, as it did
            // before the inline formatting context existed.
            _ => ctx.content_x,
        };
        let baseline_y = y as f32 + max_a;
        let x_height_half = strut.x_height / 2.0;
        let mut x = left;
        let mut merged: Vec<MergedRun> = Vec::new();
        for &pi in line {
            let p = &pieces[pi];
            let box_h = p.strut_a + p.strut_d;
            // An atomic's box is itself. A text fragment's box is the line's own
            // font's content box, and `vertical-align` is what moves it off the
            // baseline -- the ink's own extent plays no part in where the box
            // sits, which is why a run of "xxx" and a run of "Hg" get boxes of
            // the same height on the same line.
            let (top, box_h) = if p.atomic {
                let ink_h = p.ascent + p.descent;
                let t = match align_of(p.item) {
                    VerticalAlign::Baseline => baseline_y - p.ascent,
                    VerticalAlign::Middle => baseline_y - (ink_h / 2.0 - x_height_half),
                    VerticalAlign::Top => y as f32,
                    VerticalAlign::Bottom => y as f32 + height as f32 - ink_h,
                };
                (
                    t.round() as i32,
                    p.ascent.round() as i32 + p.descent.round() as i32,
                )
            } else {
                let t = match align_of(p.item) {
                    VerticalAlign::Baseline => baseline_y - p.strut_a,
                    VerticalAlign::Middle => baseline_y - (box_h / 2.0 - x_height_half),
                    VerticalAlign::Top => y as f32,
                    VerticalAlign::Bottom => y as f32 + height as f32 - box_h,
                };
                (t.round() as i32, box_h.round() as i32)
            };
            if p.atomic {
                merged.push(MergedRun {
                    item: p.item,
                    text: String::new(),
                    x,
                    w: p.width,
                    ascent: p.ascent,
                    descent: p.descent,
                    is_atomic: true,
                    y: top,
                    h: box_h,
                    node: p.node.clone(),
                });
            } else if let Some(last) = merged
                .last_mut()
                .filter(|m| m.item == p.item && !m.is_atomic)
            {
                // Consecutive pieces of one text VNode are ONE LayoutNode. Two
                // nodes pointing at the same text VNode would make the renderer
                // draw the same string twice.
                //
                // "Consecutive" is scoped to THIS line on purpose. The invariant
                // this merge maintains is one node per line box, in line order,
                // each carrying the advance of the words on that line — NOT one
                // node per text VNode. Merging across lines would produce a
                // single rect covering every line, and then nothing would say
                // where the line breaks fall: the line advance the painter would
                // have to step by is `line-height` (1.2em by default,
                // `skia_render.rs`), while the advance these line boxes are
                // placed with is the strut box from `line_box_height`
                // (`text_wrap.rs`), which for 14px type is ~22px rather than
                // 16.8px. A merged node's lines would land several pixels above
                // the boxes reserved for them. So the painter reads each line's
                // own `rect` instead — see the `VNode::Text` arm of
                // `render_with_layout`, and the contract pinned by
                // `velox-dom/tests/text_wrap_one_node_per_line.rs`.
                last.text.push_str(&p.text);
                last.w = x + p.width - last.x;
                last.ascent = last.ascent.max(p.ascent);
                last.descent = last.descent.max(p.descent);
            } else if !p.text.is_empty() {
                merged.push(MergedRun {
                    item: p.item,
                    text: p.text.clone(),
                    x,
                    w: p.width,
                    ascent: p.ascent,
                    descent: p.descent,
                    is_atomic: false,
                    y: top,
                    h: box_h,
                    node: None,
                });
            }
            x += p.width;
        }
        // An inline element whose subtree has no text at all -- an empty
        // `<span>`, an `<img>` with no `src` -- contributes no measurable piece, so
        // it never reached `merged` and would be missing from the tree. It still
        // needs a box, or the renderer could not resolve its index and the whole
        // subtree beneath it would be skipped. An `<img>` WITH a `src` is a replaced
        // Atomic and never reaches this loop: the box it gets is a real one.
        for (ii, item) in run.iter().enumerate() {
            let InlineRunItem::Fragment { has_text, .. } = item else {
                continue;
            };
            if *has_text || merged.iter().any(|m| m.item == ii) {
                continue;
            }
            merged.push(MergedRun {
                item: ii,
                text: String::new(),
                x: left,
                w: 0,
                ascent: 0.0,
                descent: 0.0,
                is_atomic: false,
                y,
                h: height,
                node: None,
            });
        }
        merged.sort_by_key(|m| m.item);
        let slots = build_inline_slots(&merged, &run);
        let mut nodes = inline_slots_to_nodes(&slots, &merged);
        laid_children.append(&mut nodes);
        y += height;
        max_y_end = max_y_end.max(y);
    }
    InlineRunResult {
        cur_y: y,
        max_y_end,
    }
}

/// Sentinel value for unconstrained cross-size in flex layout (fit-content)
/// CSS Flexbox spec: when cross-size is indefinite, children lay out at natural size
const UNCONSTRAINED_CROSS_SIZE: f32 = i32::MAX as f32;

/// Font metrics for text measurement
pub struct FontMetrics {
    pub char_width: f32,
    pub line_height: f32,
    /// Distance above the baseline of the font's typographic ascent, and below
    /// the baseline of its typographic descent.
    ///
    /// Unlike the per-run `ascent`/`descent` on `text_wrap::MeasuredText`, these
    /// do not depend on the characters being drawn: they are the *strut*, the
    /// height every line box must be at least as tall as. They are an
    /// approximation, because the layout path has no font backend of its own —
    /// the measurer registered through `text_wrap::set_skia_measurer` is the only
    /// source of real font metrics, and what it reports is a run's ink extent.
    ///
    /// These are the STRUT, and they are the font's typographic metrics, not a
    /// run's ink: a line box is at least as tall as the container's own font
    /// would make it, whatever the characters on the line happen to be. That is
    /// why a line of "xxx" is as tall as a line of "Hg" in a browser, and it is
    /// the floor `text_wrap::line_box_height` applies.
    ///
    /// The values are the `typo` metrics of the default face,
    /// `NotoSans-Regular.ttf` (unitsPerEm 1000): typoAscender 1069,
    /// typoDescender -293, typoLineGap 0, and `fsSelection = 0x00C0` with bit 7
    /// (`USE_TYPO_METRICS`) SET, so these are the numbers a browser's strut uses
    /// for this face — not one of two candidates. `hhea` agrees (1069/293); the
    /// `usWin` pair (1.124em + 0.395em) is what Windows-style GDI would pick and
    /// is deliberately not used, for that reason.
    ///
    /// This is a DIFFERENT quantity from `heuristic_vertical`, which approximates
    /// the ink of a run with no font backend. Confining the ink guess to the
    /// function that names it as a guess is the point: the strut is measured, the
    /// fallback is labelled.
    pub ascent: f32,
    pub descent: f32,
    /// Distance from the baseline to the top of a lowercase `x` (OS/2 `sxHeight`).
    /// Approximated from the default face's 0.536em, for the same reason the
    /// typographic ascent and descent are. `vertical-align: middle` is defined as
    /// half of it.
    pub x_height: f32,
}

impl FontMetrics {
    /// The documented approximation for a run's vertical extent when no font
    /// backend is registered (CSS `normal`-ish ratios: 0.8em up, 0.4em down).
    ///
    /// The SPLIT is a guess and is labelled as one. Measured off the hhea and OS/2
    /// tables of `NotoSans-Regular.ttf` (unitsPerEm 1000, the face these tests
    /// load): ascent 1.069em, descent 0.293em, lineGap 0, capHeight 0.714em,
    /// xHeight 0.536em; usWinAscent 1.124em and usWinDescent 0.395em. So 0.8em up
    /// is well short of this face's ascent, and 0.4em down is deeper than its
    /// descent. Neither number is a claim about a typical Latin face, because a
    /// range would be a guess wearing the clothes of a measurement; these are the
    /// numbers for the one face the tests actually use.
    ///
    /// The TOTAL is not a guess, but it is not a parity claim either: 0.8 + 0.4 is
    /// 1.2, which is exactly the `line_height` `from_font_size` has always
    /// produced, so a headless line box keeps the height it had before vertical
    /// metrics existed. That preserves VELOX's own prior behaviour and nothing
    /// more. A browser's strut comes from the FONT's ascent + descent, which for
    /// this face is 1.362em by hhea or 1.519em by usWin metrics — so 1.2em is
    /// closer to the old engine than to a browser. A strut floor is what closes
    /// that gap, and it is not this function's job.
    pub fn heuristic_vertical(font_size_px: f32) -> (f32, f32) {
        (font_size_px * 0.8, font_size_px * 0.4)
    }

    pub fn from_font_size(font_size_px: f32) -> Self {
        // Approximate character width ratio for typical fonts
        // '0' (zero) is roughly 0.6 * font_size for most fonts
        let char_width = font_size_px * 0.6;
        // Line height is typically 1.2 * font_size
        let line_height = font_size_px * 1.2;
        // The STRUT, from the face's own typographic metrics: 1.069em + 0.293em
        // = 1.362em. NOT `heuristic_vertical`, which is the guess at a run's ink
        // and is used only where there is no font backend to ask.
        let ascent = font_size_px * 1.069;
        let descent = font_size_px * 0.293;
        Self {
            char_width,
            line_height,
            ascent,
            descent,
            x_height: font_size_px * 0.536,
        }
    }
}

/// Returns true when the given source index corresponds to the root VNode.
/// Per spec, the first VNode (index 0 or None for the outermost) always fills the viewport.
/// Only the outermost `compute_layout` call (None) is considered the viewport root;
/// children with `Some(0)` are *not* viewport roots — otherwise every first child
/// would incorrectly fill the viewport and break fit-content sizing.
pub fn root_is_viewport_filling(source_index: Option<usize>) -> bool {
    source_index.is_none()
}

/// Compatibility helper for flat lists where caller tracks numeric index.
/// Index 0 => viewport root per spec phrasing.
pub fn root_is_viewport_filling_index(index: usize) -> bool {
    index == 0
}

/// Expanded viewport-filling predicate for layout's inline-style path.
/// Handles 100% | 100vw | 100dvw | 100vh | 100dvh and min-height variants.
/// `is_root_index` should be `root_is_viewport_filling(source_index)` for the element being laid out.
/// Per spec the first VNode always fills the viewport; explicit fixed sizes are
/// still respected via rect priority (declared wins) so this may return true
/// even when width is fixed — the layout rect will prefer the declared value.
pub fn is_viewport_filling(style: Option<&str>, is_root_index: bool) -> bool {
    if is_root_index {
        return true;
    }
    let parse = |key: &str| -> Option<Length> {
        let s = style?;
        for decl in s.split(';') {
            let d = decl.trim();
            if d.is_empty() {
                continue;
            }
            if let Some((k, v)) = d.split_once(':')
                && k.trim() == key
            {
                return Length::parse(v.trim());
            }
        }
        None
    };
    let width = parse("width");
    let height = parse("height");
    let min_h = parse("min-height");
    // shallow helper for ~100 checks
    let is_100 = |l: Length| match l {
        Length::Percent(v) => (v - 100.0).abs() < 0.01,
        Length::Vw(v) => (v - 100.0).abs() < 0.01,
        Length::Dvw(v) => (v - 100.0).abs() < 0.01,
        Length::Vh(v) => (v - 100.0).abs() < 0.01,
        Length::Dvh(v) => (v - 100.0).abs() < 0.01,
        _ => false,
    };
    let has_100pct_w =
        width.map(is_100).unwrap_or(false) && matches!(width, Some(Length::Percent(_)));
    let has_vw = matches!(
        width,
        Some(Length::Vw(v)) | Some(Length::Dvw(v)) if (v - 100.0).abs() < 0.01
    );
    let has_viewport_h = match height {
        Some(Length::Vh(v)) | Some(Length::Dvh(v)) if (v - 100.0).abs() < 0.01 => true,
        Some(Length::Percent(v)) if (v - 100.0).abs() < 0.01 => true,
        _ => false,
    } || match min_h {
        Some(Length::Vh(v)) | Some(Length::Dvh(v)) if (v - 100.0).abs() < 0.01 => true,
        Some(Length::Percent(v)) if (v - 100.0).abs() < 0.01 => true,
        _ => false,
    };
    // keep old strict combo as sufficient (width 100%|vw) && viewport_h, but root already true covers implicit fill.
    (has_100pct_w || has_vw) && has_viewport_h
}

/// Spec-compliant margin collapse per CSS 2.1 §8.3.1 (positive/negative partition).
/// Consumes two adjacent vertical margins (f32 px) and produces collapsed value:
/// - both >=0 => max (positives)
/// - both <=0 => min (most negative)
/// - opposite signs => sum
pub fn collapse_margins(a: f32, b: f32) -> f32 {
    if a >= 0.0 && b >= 0.0 {
        a.max(b)
    } else if a <= 0.0 && b <= 0.0 {
        a.min(b)
    } else {
        a + b
    }
}

#[allow(dead_code)]
fn collapse(a: f32, b: f32) -> f32 {
    collapse_margins(a, b)
}

/// Definite cross-size helper per CX-03/05.
/// For column flex (cross=width) definite if parent cross definite OR resolved width exists.
/// For row flex (cross=height) definite only if resolved height exists.
fn has_definite_cross(is_column: bool, parent_definite: bool, resolved_cross: Option<i32>) -> bool {
    if is_column {
        parent_definite || resolved_cross.is_some()
    } else {
        resolved_cross.is_some()
    }
}

/// Content vs border-box outer size resolver (F-04).
/// When border-box and no declared size, fallback to avail minus margins (outer fills).
fn content_size_for(
    declared: Option<i32>,
    avail: i32,
    is_border_box: bool,
    padding: &Sides<i32>,
    margin: &Sides<i32>,
    is_viewport_filling: bool,
    legacy_pair: bool,
) -> i32 {
    if is_border_box {
        if let Some(dw) = declared {
            dw
        } else {
            (avail - margin.left - margin.right).max(1)
        }
    } else if let Some(dw) = declared {
        dw + padding.left + padding.right + padding.top + padding.bottom
    } else if is_viewport_filling || legacy_pair {
        (avail - margin.left - margin.right).max(1)
    } else {
        avail
    }
}

/// The used cap from `max-width`, resolved against the containing block's width
/// so `max-width: 50%` tracks the parent (CSS 2.1 §10.4). `None` means the
/// property imposes no constraint: it is absent, `auto`, or negative.
#[allow(clippy::too_many_arguments)]
fn used_max_width(
    style: Option<&str>,
    containing_w: f32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> Option<i32> {
    style_lookup_len_full(
        style,
        "max-width",
        containing_w,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    )
    // A negative `max-width` is an invalid declaration, not a cap of zero: CSS 2.1
    // §10.4 says "Negative values for 'min-width' and 'max-width' are illegal", and
    // an invalid declaration is dropped, so it constrains nothing. Reading it as
    // `max(0)` would collapse the box instead of leaving it alone.
    .filter(|v| *v >= 0)
}

/// The used floor from `min-width`, resolved against the containing block's width
/// so `min-width: 50%` tracks the parent, on the same basis `max-width` uses.
///
/// `None` means the property imposes no constraint: it is absent, `auto`, or
/// negative. `auto` in particular must never be read as `0` — a clamp that did
/// that would collapse every box relying on the default floor.
fn used_min_width(
    style: Option<&str>,
    containing_w: f32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> Option<i32> {
    style_lookup_len_full(
        style,
        "min-width",
        containing_w,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    )
    // A negative `min-width` is an invalid declaration for the same reason a
    // negative `max-width` is (CSS 2.1 §10.4), so it is dropped and constrains
    // nothing. Unlike the cap, the valid value `0` IS meaningful here and is
    // kept: it is an explicit opt-out, not an absent constraint.
    .filter(|v| *v >= 0)
}

/// Clamp a computed border-box width up to an already-resolved `min-width`.
///
/// The mirror of [`cap_to_max_width`]: the floor is expressed in the same box
/// the element's `width` is expressed in, so under `box-sizing: content-box` the
/// padding and border sit outside it and the border box ends up
/// `min-width + padding + border` wide.
///
/// This is applied *outside* the max-width cap by the caller, not inside it, and
/// the ordering is the precedence: CSS 2.1 §10.4 resolves the used width by
/// reapplying the rules with `width` set to `min-width`, and css-sizing-3 §3.1
/// states that "the minimum size constraint is always the strongest constraint".
/// Flooring last is what makes `min-width` win over `max-width`.
fn floor_to_min_width(
    rect_w: i32,
    min_w: Option<i32>,
    is_border_box: bool,
    pl: i32,
    pr: i32,
    bl: i32,
    br: i32,
) -> i32 {
    let Some(min_w) = min_w else {
        return rect_w;
    };
    if is_border_box {
        return rect_w.max(min_w);
    }
    let edges = pl + pr + bl + br;
    // `min_w` is non-negative by the filter in `used_min_width`, so the content
    // width cannot go negative here; no `.max(0)` guard is needed, unlike the cap.
    (rect_w - edges).max(min_w) + edges
}

/// Clamp a computed border-box width to an already-resolved `max-width`.
///
/// The cap is expressed in the same box the element's `width` is expressed in,
/// so under `box-sizing: content-box` the padding and border sit *outside* it
/// and the border box ends up `max-width + padding + border` wide — which is
/// what a browser renders. This is a cap only: the `min-width` floor is a
/// separate, outer step ([`floor_to_min_width`]) so that precedence is decided
/// by the call site's ordering rather than inside either helper.
fn cap_to_max_width(
    rect_w: i32,
    max_w: Option<i32>,
    is_border_box: bool,
    pl: i32,
    pr: i32,
    bl: i32,
    br: i32,
) -> i32 {
    let Some(max_w) = max_w else {
        return rect_w;
    };
    if is_border_box {
        return rect_w.min(max_w);
    }
    let edges = pl + pr + bl + br;
    (rect_w - edges).min(max_w).max(0) + edges
}

/// Clamp an already-computed border-box width to the element's own `max-width`,
/// re-reading the box model out of the style string.
///
/// This is the flex placement pass's entry point and its only one: an item's
/// `is_border_box` and sides are not in scope there, because the flex algorithm
/// overwrites `rect.w` after `at()` returned. `at()` itself calls `cap_to_max_width`
/// directly with the `is_border_box` and sides it already has, so this runs once per
/// flex item rather than once per box of every layout.
#[allow(clippy::too_many_arguments)]
fn clamp_width_to_max_width(
    rect_w: i32,
    style: Option<&str>,
    containing_w: f32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> i32 {
    let Some(max_w) = used_max_width(
        style,
        containing_w,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    ) else {
        return rect_w;
    };
    let is_border_box = style_lookup_str(style, "box-sizing")
        .map(|s| s.trim().eq_ignore_ascii_case("border-box"))
        .unwrap_or(false);
    let (pl, pr, _, _) = style_box_sides_full(
        style,
        "padding",
        containing_w,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );
    let (bl, br, _, _) = style_border_widths(
        style,
        containing_w,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );
    cap_to_max_width(rect_w, Some(max_w), is_border_box, pl, pr, bl, br)
}

/// Clamp a used main size by the item's definite min/max main size properties
/// (css-flexbox-1 §9.2.3 for the hypothetical main size, §9.7.3 for the target
/// main size). The flex resolve calls this at both seams; keeping one
/// implementation is what stops the two from drifting apart.
///
/// `None` means the property imposes no constraint: absent, `auto`, or negative.
/// The main axis reads `min` and `max` as `Option` precisely so the two
/// absences are distinguishable — folding an absent `min` to `0` silently
/// removes the floor, and folding an absent `max` to a finite number silently
/// adds a cap.
///
/// The order is floor-then-cap, which is what the previous inline
/// `.max(min).min(max)` at the grow and shrink seams did, so this is a pure
/// refactor of that expression and changes no result on its own.
///
/// NOTE a known divergence, deliberately NOT changed here: when `min > max`,
/// the floor is applied first and the cap then wins, but css-sizing-3 §3.2 makes
/// the minimum win, and the block flow already implements that
/// ([`floor_to_min_width`] applied after [`cap_to_max_width`], guarded by
/// `block_min_width_overrides_max_width`). Flex and block flow therefore
/// disagree on `min-width: 200px; max-width: 100px` in a flex container: block
/// flow gives 200, flex gives 100. Reordering the two lines here would fix it
/// but would also change flex sizing on every existing `min > max` item, which
/// is a separate change from the unconditional clamp this helper exists for.
/// Filed, not folded in.
fn clamp_to_min_max(v: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    let floored = min.map_or(v, |lo| v.max(lo));
    max.map_or(floored, |hi| floored.min(hi))
}

/// The CSS containing block for out-of-flow descendants (CSS 2.1 §10.1).
///
/// `x`/`y` are the PADDING-edge origin and `w`/`h` the padding-box dimensions of
/// the nearest positioned ancestor, because a padding box is what CSS makes a
/// positioned ancestor establish. For the initial containing block these are the
/// viewport rectangle, which is what the top-level `compute_layout` call passes.
///
/// ## ONE box serves both the offsets and the percentages, because the spec says so
///
/// §10.1 makes the padding box the containing block, which is what these four
/// fields are. §10.3.7 then makes the percentages resolve against that same padding
/// box: *"For absolutely positioned elements whose containing block is based on a
/// block container element, the percentage is calculated with respect to the width
/// of the padding box of that element. This is a change from CSS1, where the
/// percentage width was always calculated with respect to the content box of the
/// parent element."* Confirmed in a browser: `position: absolute; width: 100%` inside
/// `position: relative; padding: 50px; width: 250px` is 350px wide — 100% of the
/// 350px padding box, not of the 250px content box. Yoga matches web here.
///
/// An earlier revision of this engine carried a second field for the containing
/// block's content width and resolved percentages against it. That made one
/// containing block be two different boxes depending on which question was asked of
/// it, and the half that was wrong was the half with no citation on it, so it read
/// as settled. There is no `content_w` field any more.
///
/// For an IN-FLOW box these same four fields are the parent's CONTENT box, which is
/// what a static block container's in-flow child resolves percentages against
/// (§10.1), and both call sites below pass exactly that. So one number per box is
/// correct on both paths; they differ between paths only because CSS says they do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ContainingBlock {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

/// Whether a box's `position` makes it establish a containing block for its
/// absolutely positioned descendants (CSS 2.1 §10.1: any `position` other than
/// `static`).
///
/// `position: sticky` is deliberately excluded. It does establish a containing
/// block in CSS, so this is a known divergence, but sticky is out of scope for
/// this change and nothing here should quietly change sticky behaviour.
fn establishes_containing_block(position: &str) -> bool {
    matches!(position, "relative" | "absolute" | "fixed")
}

/// The containing block an out-of-flow child resolves against.
///
/// `position: fixed` is pinned to the viewport no matter how deeply it is nested
/// (CSS 2.1 §10.3.4, with the transform/filter caveat that is out of scope here);
/// `position: absolute` uses its parent's `descendant_cb`.
fn out_of_flow_containing_block(
    is_fixed: bool,
    descendant_cb: ContainingBlock,
    viewport_w: i32,
    viewport_h: i32,
) -> ContainingBlock {
    if is_fixed {
        ContainingBlock {
            x: 0,
            y: 0,
            w: viewport_w,
            h: viewport_h,
        }
    } else {
        descendant_cb
    }
}

/// An out-of-flow child captured during the in-flow pass, held until the
/// parent's own box is final.
///
/// `apply_absolute_position` needs the containing block's size, and for a
/// positioned ancestor with an auto height that size is not known until the
/// child tree has been built (the parent's `rect_h` is only final after the
/// min/max-height clamp). Deferring also means the containing block reflects any
/// `max-width` clamp, which lands before the child pass.
struct PendingAbsolute {
    style: Option<String>,
    /// `position: fixed` resolves against the viewport regardless of any
    /// ancestor; `absolute` resolves against `descendant_cb`.
    is_fixed: bool,
    /// Where the box lands if it were in flow — its static position
    /// (CSS 2.1 §10.3.7). Used when neither `left`/`right` nor `top`/`bottom`
    /// is specified, where the spec's static-position rule applies rather than a
    /// containing-block corner.
    static_x: i32,
    static_y: i32,
    node: LayoutNode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutNode {
    pub rect: Rect,
    pub z_index: i32,
    pub display_none: bool,
    pub source_index: Option<usize>,
    pub scroll_x: i32,
    pub scroll_y: i32,
    pub clip: Option<Rect>,
    pub stacking_context: bool,
    /// Total scrollable content height (content_h + padding + border).
    pub scroll_height: i32,
    /// Maximum vertical scroll offset (scroll_height - rect.h) clamped to 0.
    pub max_scroll_y: i32,
    /// True when overflow is auto/scroll and content exceeds rect.
    pub scrollable: bool,
    pub children: Vec<LayoutNode>,
}

/// ScrollState tracks the current vertical scroll offset and its clamp.
/// Mutated by `on_wheel(delta)` / `scroll_by(delta)`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollState {
    pub offset_y: f32,
    pub max_y: f32,
}

impl ScrollState {
    pub fn new(max_y: f32) -> Self {
        Self {
            offset_y: 0.0,
            max_y,
        }
    }
    /// Advance by `delta` and clamp to `[0, max_y]`.
    pub fn scroll_by(&mut self, delta: f32) {
        self.offset_y = (self.offset_y + delta).clamp(0.0, self.max_y.max(0.0));
    }
    /// Wheel entry point: advance by a wheel `delta` (positive increases the
    /// offset, i.e. content moves up) and clamp to `[0, max_y]`.
    pub fn on_wheel(&mut self, delta: f32) {
        self.scroll_by(delta);
    }
}

/// Returns true when `overflow` is `auto` or `scroll` and content exceeds viewport.
pub fn is_scrollable(overflow: &str, content_h: f32, rect_h: f32) -> bool {
    matches!(overflow, "auto" | "scroll") && content_h > rect_h
}

fn translate_layout_subtree(node: &mut LayoutNode, dx: i32, dy: i32) {
    node.rect.x += dx;
    node.rect.y += dy;
    if let Some(clip) = &mut node.clip {
        clip.x += dx;
        clip.y += dy;
    }
    for child in &mut node.children {
        translate_layout_subtree(child, dx, dy);
    }
}

fn translate_layout_descendants(node: &mut LayoutNode, dx: i32, dy: i32) {
    for child in &mut node.children {
        translate_layout_subtree(child, dx, dy);
    }
}

#[allow(dead_code)]
fn parse_px(s: &str) -> Option<i32> {
    let t = s.trim();
    if let Some(px) = t.strip_suffix("px") {
        px.trim().parse().ok()
    } else {
        t.parse().ok()
    }
}

// ===== Style parse memo ===================================================
//
// Every `style_*` helper below used to re-split a style string on `;` to find
// a single declaration. Instrumenting the family showed that split happening
// 54.2 times per node descent (flat from 0 to 200 todos) and accounting for
// 84.4-88.3% of `layout_us`.
//
// What is memoized here is the PARSE, not the layout. Two layers, with a
// deliberately different key discipline each:
//
//  1. The declaration table, keyed on the style string's CONTENT. Splitting a
//     style string reads the style string and nothing else -- no containing
//     block, no font size, no viewport -- so content is a *complete* key by
//     construction rather than by argument. `StyleTable::build` is the only
//     place a style string is parsed, and its entire input is that string.
//
//  2. Resolved values for the four helpers whose resolution depends on layout
//     context. Their keys are per-helper structs, each naming exactly the
//     inputs that helper's resolution reads -- see the doc comment on each
//     key type, which is the key-coverage argument made executable. Every `f32`
//     is carried as `to_bits()`, so the key is bit-exact and two contexts that
//     compare equal but differ in representation cannot alias.
//
// Nothing in either layer is derived from tree position, node identity, a
// frame counter, or a style hash. A style string that changes is a different
// key by construction, so it can never be served a stale entry.

/// Distinct style strings held before the table cache starts dropping entries.
const MAX_STYLE_TABLES: usize = 4096;

/// Resolutions held per style table before that resolution memo is dropped.
///
/// Deliberately smaller than `MAX_STYLE_TABLES`: a memo's key carries the
/// layout context as well, so it is bounded by the variety of contexts seen
/// against one style string, not by the variety of style strings.
const MAX_TABLE_MEMO: usize = 4096;

/// FxHash: rustc's multiply-and-rotate hash.
///
/// Used instead of the default SipHash because every helper call hashes the
/// style string, and SipHash's per-call setup dominates at this input size
/// (style strings here are 23-109 bytes, median 72). Dependency-free so this
/// module keeps its `std`-only build.
#[derive(Default, Clone, Copy)]
struct FxHasher {
    hash: u64,
}

/// FxHash's multiplier (fractional bits of the golden ratio).
const FX_SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

impl FxHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(FX_SEED);
    }
}

impl std::hash::Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while let Some((chunk, tail)) = rest.split_first_chunk::<8>() {
            self.add(u64::from_le_bytes(*chunk));
            rest = tail;
        }
        if let Some((chunk, tail)) = rest.split_first_chunk::<4>() {
            self.add(u32::from_le_bytes(*chunk) as u64);
            rest = tail;
        }
        for &b in rest {
            self.add(b as u64);
        }
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

/// `BuildHasher` for [`FxHasher`], so the maps below skip SipHash.
#[derive(Default, Clone, Copy)]
struct FxBuild;

impl std::hash::BuildHasher for FxBuild {
    type Hasher = FxHasher;
    #[inline]
    fn build_hasher(&self) -> FxHasher {
        FxHasher::default()
    }
}

/// The layout context that length resolution depends on, packed for the keys
/// below. A plain carrier so each helper's key can be written once and cannot
/// drift from the argument list it mirrors.
#[derive(Clone, Copy)]
struct LenCtx {
    parent_size: f32,
    parent_font_size: f32,
    root_font_size: f32,
    vw: f32,
    vh: f32,
}

/// Key for [`style_lookup_len_full`]'s memo.
///
/// `style_lookup_len_full` reads exactly: the style string (carried by *which
/// table you are in*, since a table is content-addressed and immutable), the
/// property name (carried by `decl`, the index of that property's first
/// declaration in the table, which identifies the pair uniquely), and the five
/// context values its `parse_length_value` call is passed. All five are here.
/// There is no sixth input.
#[derive(PartialEq, Eq, Hash)]
struct LenFullKey {
    decl: u32,
    parent_size: u32,
    parent_font_size: u32,
    root_font_size: u32,
    vw: u32,
    vh: u32,
}

impl LenFullKey {
    #[inline]
    fn new(decl: u32, ctx: &LenCtx) -> Self {
        Self {
            decl,
            parent_size: ctx.parent_size.to_bits(),
            parent_font_size: ctx.parent_font_size.to_bits(),
            root_font_size: ctx.root_font_size.to_bits(),
            vw: ctx.vw.to_bits(),
            vh: ctx.vh.to_bits(),
        }
    }
}

/// Key for [`style_lookup_font_size`]'s memo.
///
/// That helper reads exactly: the style string (carried by the table), the
/// `font-size` declaration (carried by `decl`), `parent_font_size`,
/// `root_font_size`, and the viewport. Note what is absent: it has no
/// `parent_size` parameter, and it passes `parent_font_size` as *both* the
/// percentage basis and the `em` basis, so four context values is the complete
/// set, not five.
#[derive(PartialEq, Eq, Hash)]
struct FontSizeKey {
    decl: u32,
    parent_font_size: u32,
    root_font_size: u32,
    vw: u32,
    vh: u32,
}

/// Key for [`style_box_sides_full`]'s memo.
///
/// That helper reads exactly: the style string (carried by the table), `base`
/// (carried as content -- it selects which property family is being expanded,
/// and it differs between a `margin` call and a `padding` call), and the five
/// context values it forwards to `parse_length_value`. All six are here.
#[derive(PartialEq, Eq, Hash)]
struct BoxSidesKey {
    base: Rc<str>,
    parent_size: u32,
    parent_font_size: u32,
    root_font_size: u32,
    vw: u32,
    vh: u32,
}

impl BoxSidesKey {
    #[inline]
    fn new(base: &str, ctx: &LenCtx) -> Self {
        Self {
            base: Rc::from(base),
            parent_size: ctx.parent_size.to_bits(),
            parent_font_size: ctx.parent_font_size.to_bits(),
            root_font_size: ctx.root_font_size.to_bits(),
            vw: ctx.vw.to_bits(),
            vh: ctx.vh.to_bits(),
        }
    }
}

/// Key for [`style_border_widths`]'s memo.
///
/// That helper reads exactly: the style string (carried by the table) and the
/// five context values it forwards. It takes no `base`: its property names
/// (`border`, `border-width`, `border-*-width`, `border-left`, ...) are
/// literals inside the function, so there is no caller-supplied selector to key
/// on. The cache is per-table precisely so that "no `base`" is a fact about
/// the function rather than an omission from the key.
#[derive(PartialEq, Eq, Hash)]
struct BorderKey {
    parent_size: u32,
    parent_font_size: u32,
    root_font_size: u32,
    vw: u32,
    vh: u32,
}

impl BorderKey {
    #[inline]
    fn new(ctx: &LenCtx) -> Self {
        Self {
            parent_size: ctx.parent_size.to_bits(),
            parent_font_size: ctx.parent_font_size.to_bits(),
            root_font_size: ctx.root_font_size.to_bits(),
            vw: ctx.vw.to_bits(),
            vh: ctx.vh.to_bits(),
        }
    }
}

/// One style string, split into declarations exactly once.
///
/// Immutable apart from the four resolution memos, which are pure-function
/// caches over a fixed table: an entry can only ever be re-derived to the same
/// value, so a stale entry is not representable.
struct StyleTable {
    /// The style string, owned so the value ranges below can point into it.
    src: String,
    /// Byte range of each declaration's trimmed value, in document order.
    /// Declaration `i`'s value is `src[values[i].0 .. values[i].1]`. Ranges
    /// exist because the original code returned a `&str` into a `String`
    /// the caller's `&str` pointed at; owning `src` keeps those borrows valid
    /// without copying each value.
    values: Vec<(u32, u32)>,
    /// Property name -> index of its FIRST declaration. Every helper except
    /// `style_margin_auto_sides` returns on the first match, so it reads this.
    first: HashMap<Rc<str>, u32, FxBuild>,
    /// Property name -> index of its LAST declaration.
    /// `style_margin_auto_sides` assigns in its loop instead of returning, so
    /// a later declaration overwrites an earlier one and it must read this.
    /// The first/last split is the reason the table keeps both maps rather
    /// than collapsing them: the two access patterns disagree, and picking one
    /// for both would silently change the other's result.
    last: HashMap<Rc<str>, u32, FxBuild>,
    /// Monotonic insertion counter, used only to order entries for eviction.
    /// Never a cache key.
    inserted: u64,
    font_size: RefCell<HashMap<FontSizeKey, Option<f32>, FxBuild>>,
    len_full: RefCell<HashMap<LenFullKey, Option<i32>, FxBuild>>,
    box_sides: RefCell<HashMap<BoxSidesKey, (i32, i32, i32, i32), FxBuild>>,
    border: RefCell<HashMap<BorderKey, (i32, i32, i32, i32), FxBuild>>,
}

impl StyleTable {
    /// Split a style string into its declaration table.
    ///
    /// Reproduces the tokenization every helper performed inline, including its
    /// quirks, because the quirks are observable:
    ///   - `split(';')` then `trim()`, so leading/trailing `;` and whitespace
    ///     are inert, and an empty declaration is skipped;
    ///   - `split_once(':')`, so only the FIRST `:` separates, and a
    ///     declaration with no `:` is skipped (`background: url(http://x)`
    ///     yields key `background`, value `url(http://x)`);
    ///   - `k.trim() == key` is a case-SENSITIVE exact compare, so keys are
    ///     stored as written and never case-folded;
    ///   - a declaration whose key is empty is kept, because the original
    ///     kept it too -- it simply never matched any real property name.
    fn build(s: &str) -> StyleTable {
        let mut values: Vec<(u32, u32)> = Vec::new();
        let mut first: HashMap<Rc<str>, u32, FxBuild> = HashMap::default();
        let mut last: HashMap<Rc<str>, u32, FxBuild> = HashMap::default();
        for decl in s.split(';') {
            let d = decl.trim();
            if d.is_empty() {
                continue;
            }
            let Some((k, v)) = d.split_once(':') else {
                continue;
            };
            let (k, v) = (k.trim(), v.trim());
            // `v` is a subslice of `d`, which is a subslice of `s`: `decl`
            // comes from `s.split(';')` and `trim` only shortens. So this
            // offset is always in range.
            let start = (v.as_ptr() as usize - s.as_ptr() as usize) as u32;
            values.push((start, start + v.len() as u32));
            let idx = (values.len() - 1) as u32;
            // Reuse the first map's `Rc` so a repeated property name does not
            // allocate a second key string.
            match first.entry(Rc::from(k)) {
                Entry::Occupied(e) => {
                    last.insert(e.key().clone(), idx);
                }
                Entry::Vacant(e) => {
                    let key: Rc<str> = e.key().clone();
                    e.insert(idx);
                    last.insert(key, idx);
                }
            }
        }
        StyleTable {
            src: s.to_string(),
            values,
            first,
            last,
            inserted: 0,
            font_size: RefCell::new(HashMap::default()),
            len_full: RefCell::new(HashMap::default()),
            box_sides: RefCell::new(HashMap::default()),
            border: RefCell::new(HashMap::default()),
        }
    }

    /// The trimmed value of declaration `i`.
    #[inline]
    fn value(&self, i: u32) -> &str {
        let (start, end) = self.values[i as usize];
        &self.src[start as usize..end as usize]
    }

    /// Index of the first declaration of `key`, or `None` if undeclared.
    #[inline]
    fn first(&self, key: &str) -> Option<u32> {
        self.first.get(key).copied()
    }

    /// Index of the last declaration of `key`, or `None` if undeclared.
    #[inline]
    fn last(&self, key: &str) -> Option<u32> {
        self.last.get(key).copied()
    }
}

/// Process-wide, thread-local, style-string -> table cache.
struct StyleCache {
    /// Keyed by the style string's CONTENT.
    tables: HashMap<Rc<str>, Rc<StyleTable>, FxBuild>,
    /// Monotonic counter; only ever advances, and is never a cache key.
    seq: u64,
    /// Most recently returned table, plus the exact content it was built from.
    ///
    /// `table_for` is called ~54 times per node descent with the SAME style
    /// string each time, so the overwhelmingly common case is a repeat of the
    /// immediately preceding lookup. Comparing CONTENT here (a length check
    /// plus `memcmp`) avoids re-hashing the whole string into a `HashMap`
    /// probe 54 times per node. This is keyed on content exactly like
    /// `tables` is -- not on a position, index or counter -- and two equal
    /// strings are the same declaration table by construction, because
    /// `StyleTable::build` is a pure function of the style string.
    last: Option<(Rc<str>, Rc<StyleTable>)>,
}

thread_local! {
    static STYLE_TABLES: RefCell<StyleCache> = RefCell::new(StyleCache {
        tables: HashMap::default(),
        seq: 0,
        last: None,
    });
}

/// The parsed table for `style`, built on first sight of this exact string.
///
/// The key is the string's content, which is the complete input to parsing.
/// Callers that want a different result for the same content must pass
/// different context, and that context is part of the *second* layer's key,
/// not of this lookup.
fn table_for(style: &str) -> Rc<StyleTable> {
    STYLE_TABLES.with(|c| {
        let mut cache = c.borrow_mut();
        // Fast path: same content as the last lookup. Byte-compare only.
        if let Some((k, t)) = &cache.last
            && &**k == style
        {
            return t.clone();
        }
        // Clone the value out before assigning to `cache.last`: `cache` is a
        // `RefMut`, so a borrow from `tables.get` would conflict with it.
        let hit = cache.tables.get(style).cloned();
        if let Some(t) = hit {
            cache.last = Some((Rc::from(style), t.clone()));
            return t;
        }
        if cache.tables.len() >= MAX_STYLE_TABLES {
            // Drop the oldest half, then cache this entry anyway, matching
            // `FontCache::remember_advance`. Refusing to insert past the cap
            // would pin the working set to whatever was seen first and
            // permanently exclude every style string seen after it.
            let mut victims: Vec<(u64, Rc<str>)> = cache
                .tables
                .iter()
                .map(|(k, t)| (t.inserted, k.clone()))
                .collect();
            victims.sort_unstable_by_key(|(seq, _)| *seq);
            // `inserted` is unique per insertion and never reused, so this
            // removes exactly the oldest half.
            for (_, key) in victims.into_iter().take(MAX_STYLE_TABLES / 2) {
                cache.tables.remove(&key);
            }
        }
        let seq = cache.seq.wrapping_add(1);
        cache.seq = seq;
        let mut built = StyleTable::build(style);
        built.inserted = seq;
        let table = Rc::new(built);
        cache.tables.insert(Rc::from(style), table.clone());
        // The eviction above can drop a table `last` still points at. That is a
        // memory question, not a correctness one: the `Rc` keeps it alive and
        // it is still the table for exactly this content. Re-seeding `last`
        // here keeps the fast path valid either way.
        cache.last = Some((Rc::from(style), table.clone()));
        table
    })
}

/// Store a resolution in a per-table memo, dropping the memo if it is full.
///
/// Clearing rather than refusing, and this is deliberately *not* the
/// `FontCache` drop-oldest policy: a memoized resolution here is a cheap
/// re-derivation from an already-parsed table, so dropping entries costs a
/// recomputation and nothing else. The advance memo in the renderer is the
/// opposite case, which is why it keeps its oldest entries and this one does
/// not.
fn memo_put<K, V>(cell: &RefCell<HashMap<K, V, FxBuild>>, key: K, value: V)
where
    K: std::hash::Hash + Eq,
{
    let mut map = cell.borrow_mut();
    if map.len() >= MAX_TABLE_MEMO {
        map.clear();
    }
    map.insert(key, value);
}

/// Build `"<base>-<side>"` on the stack, so the four longhand lookups in
/// `style_box_sides_full` stop allocating a `String` each.
///
/// Only called with `base` and `side` that are short compile-time constants at
/// every call site, so 48 bytes of stack is ample; the buffer is sized to fail
/// loudly in a debug build rather than truncate silently.
#[inline]
fn side_key<'a>(buf: &'a mut [u8; 48], base: &str, side: &str) -> &'a str {
    let n = base.len() + 1 + side.len();
    assert!(
        n <= buf.len(),
        "side_key buffer too small for {base}-{side}"
    );
    buf[..base.len()].copy_from_slice(base.as_bytes());
    buf[base.len()] = b'-';
    buf[base.len() + 1..n].copy_from_slice(side.as_bytes());
    std::str::from_utf8(&buf[..n]).expect("side key is built from strs")
}

#[allow(dead_code)]
fn style_lookup(style: Option<&str>, key: &str) -> Option<i32> {
    let t = table_for(style?);
    parse_px(t.value(t.first(key)?))
}

#[allow(dead_code)]
fn style_lookup_len(style: Option<&str>, key: &str, base: i32) -> Option<i32> {
    let t = table_for(style?);
    let val = t.value(t.first(key)?);
    if let Some(p) = val.strip_suffix('%')
        && let Ok(pct) = p.trim().parse::<f32>()
    {
        return Some(((pct / 100.0) * base as f32).round() as i32);
    }
    parse_px(val)
}

/// Extract font-size from style string, resolving all units to pixels.
/// Returns None if font-size is not declared, allowing callers to use inherited/default.
fn style_lookup_font_size(
    style: Option<&str>,
    parent_font_size: f32,
    root_font_size: f32,
    viewport: (f32, f32),
) -> Option<f32> {
    let t = table_for(style?);
    // `parent_font_size` is passed as BOTH the percentage basis and the `em`
    // basis below, which is the original call; a `LenCtx` would imply a
    // separate `parent_size` input that this helper does not have, so the key
    // is spelled out directly instead of being derived from one.
    let key = FontSizeKey {
        decl: t.first("font-size")?,
        parent_font_size: parent_font_size.to_bits(),
        root_font_size: root_font_size.to_bits(),
        vw: viewport.0.to_bits(),
        vh: viewport.1.to_bits(),
    };
    if let Some(hit) = t.font_size.borrow().get(&key).copied() {
        return hit;
    }
    let out = parse_length_value(
        t.value(key.decl),
        parent_font_size,
        parent_font_size,
        root_font_size,
        viewport,
    );
    memo_put(&t.font_size, key, out);
    out
}

/// Resolve a CSS length value with ALL units to pixels.
///
/// Supported units:
/// - `px`: direct pixel value
/// - `%`: percentage of `parent_size`
/// - `rem`: relative to `root_font_size`
/// - `em`: relative to `parent_font_size`
/// - `vw`: percentage of viewport width
/// - `vh`: percentage of viewport height
/// - `auto`: returns None (caller decides)
/// - `0`: returns Some(0)
/// - plain number: treated as pixels
fn style_lookup_len_full(
    style: Option<&str>,
    key: &str,
    parent_size: f32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> Option<i32> {
    let t = table_for(style?);
    len_full_in(
        &t,
        key,
        &LenCtx {
            parent_size,
            parent_font_size,
            root_font_size,
            vw: viewport_w,
            vh: viewport_h,
        },
    )
}

/// The memoized body of [`style_lookup_len_full`], for callers that already
/// hold the table. `style_box_sides_full` and `style_border_widths` both
/// resolve four properties against one style string, and re-hashing that
/// string once per property was a large part of their cost.
#[inline]
fn len_full_in(t: &StyleTable, key: &str, ctx: &LenCtx) -> Option<i32> {
    // An undeclared property is the common case for the longhand probes, and
    // it is answered entirely by the table: there is nothing to memoize.
    let mkey = LenFullKey::new(t.first(key)?, ctx);
    if let Some(hit) = t.len_full.borrow().get(&mkey).copied() {
        return hit;
    }
    let out = parse_length_value(
        t.value(mkey.decl),
        ctx.parent_size,
        ctx.parent_font_size,
        ctx.root_font_size,
        (ctx.vw, ctx.vh),
    )
    .map(|f| f.round() as i32);
    memo_put(&t.len_full, mkey, out);
    out
}

/// Parse a single CSS length value string and convert to pixels.
fn parse_length_value(
    val: &str,
    parent_size: f32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport: (f32, f32),
) -> Option<f32> {
    let val = val.trim();

    if val == "auto" {
        return None;
    }

    if val == "0" {
        return Some(0.0);
    }

    // Percentage
    if let Some(p) = val.strip_suffix('%')
        && let Ok(pct) = p.trim().parse::<f32>()
    {
        let pct = if pct.is_finite() { pct } else { 0.0 };
        return Some((pct / 100.0) * parent_size);
    }

    // Pixels
    if let Some(px) = val.strip_suffix("px")
        && let Ok(v) = px.trim().parse::<f32>()
    {
        return Some(if v.is_finite() { v } else { 0.0 });
    }

    // rem (root em)
    if let Some(rem) = val.strip_suffix("rem")
        && let Ok(v) = rem.trim().parse::<f32>()
    {
        let v = if v.is_finite() { v } else { 0.0 };
        return Some(v * root_font_size);
    }

    // em (parent-relative)
    if let Some(em) = val.strip_suffix("em")
        && let Ok(v) = em.trim().parse::<f32>()
    {
        let v = if v.is_finite() { v } else { 0.0 };
        return Some(v * parent_font_size);
    }

    // dynamic viewport width
    if let Some(dvw) = val.strip_suffix("dvw")
        && let Ok(v) = dvw.trim().parse::<f32>()
    {
        let v = if v.is_finite() { v } else { 0.0 };
        return Some((v / 100.0) * viewport.0);
    }

    // dynamic viewport height
    if let Some(dvh) = val.strip_suffix("dvh")
        && let Ok(v) = dvh.trim().parse::<f32>()
    {
        let v = if v.is_finite() { v } else { 0.0 };
        return Some((v / 100.0) * viewport.1);
    }

    // viewport width
    if let Some(vw) = val.strip_suffix("vw")
        && let Ok(v) = vw.trim().parse::<f32>()
    {
        let v = if v.is_finite() { v } else { 0.0 };
        return Some((v / 100.0) * viewport.0);
    }

    // viewport height
    if let Some(vh) = val.strip_suffix("vh")
        && let Ok(v) = vh.trim().parse::<f32>()
    {
        let v = if v.is_finite() { v } else { 0.0 };
        return Some((v / 100.0) * viewport.1);
    }

    // Plain number -> pixels
    if let Ok(v) = val.parse::<f32>() {
        return Some(if v.is_finite() { v } else { 0.0 });
    }

    None
}

fn style_lookup_str(style: Option<&str>, key: &str) -> Option<String> {
    let t = table_for(style?);
    Some(t.value(t.first(key)?).to_string())
}

/// Elements whose Velox default `display` is `inline`, i.e. the elements that
/// get a `display: inline` rule in the `velox-style/src/ua.css` UA sheet.
///
/// Public so `velox-style`'s cascade test can assert that the UA sheet and
/// this table carry the same set: `ua.css` is the cascade-side source of these
/// defaults, but velox-dom cannot depend on velox-style, so the same list is
/// kept here for the layout engine's own notion of an element's default
/// display. `ua_inline_tag_list_matches_the_layout_fallback_table` in
/// `velox-style/tests/cascade.rs` fails if the two drift apart.
///
/// In browsers almost all of these take CSS's initial value, `inline`,
/// because the HTML rendering section declares no `display` for them. The
/// documented deviations from a literal browser reading:
///
/// - `progress` and `meter` are `inline-block` in browsers, not `inline`.
///   Velox has no `inline-block` layout, and `display: inline` is a far closer
///   approximation than `block`, so they are claimed as `inline`. Do not
///   "correct" them back to block; an `inline-block` UA rule is not available
///   until inline-block layout exists.
/// - The obsolete phrasing elements `big`, `tt`, `font`, `nobr` and `strike`
///   are also `inline` in browsers, but are deliberately out of scope: nothing
///   in the framework or the examples needs them, and every tag in this list
///   is meant to be individually checkable against the spec.
///
/// `wbr` belongs here because its initial `display` is `inline`;
/// `display-outside: break-opportunity` is a separate property Velox does not
/// implement (same for `br`'s `display-outside: newline`).
pub const INLINE_BY_DEFAULT_TAGS: &[&str] = &[
    "a", "abbr", "b", "bdi", "bdo", "cite", "code", "data", "del", "dfn", "em", "i", "img", "ins",
    "kbd", "label", "mark", "meter", "output", "progress", "q", "s", "samp", "small", "span",
    "strong", "sub", "sup", "time", "u", "var", "wbr",
];

/// The `display` an element gets when neither the cascade nor an author rule
/// specifies one.
///
/// `button`, `input`, `select` and `textarea` are `inline-block` in a browser's
/// UA sheet. Velox does not reproduce that: they get no `display` rule, and this
/// function answers `block` for them. The deviation is deliberate and was
/// recorded for a long time in an `INLINE_BLOCK_BY_DEFAULT_TAGS` table that
/// answered a second, subtly different question ("would this neighbour share a
/// line in a browser?") from this one. Two answers to the same question is the
/// defect, so the table is gone and this is the single place the deviation is
/// decided. An author who wants the browser's behaviour writes
/// `display: inline-block` on the element, and now gets a real atomic inline
/// box.
fn default_display_for_tag(tag: &str) -> &'static str {
    if INLINE_BY_DEFAULT_TAGS.contains(&tag.to_ascii_lowercase().as_str()) {
        "inline"
    } else {
        "block"
    }
}

/// A node's EXPLICIT `display`, trimmed and lowercased. `None` when it has none,
/// which is not the same as `"block"`: the initial value of `display` depends on
/// the element, and `default_display_for_tag` is the one place that knows it.
///
/// Every `display` test in the layout path reads the declaration through this,
/// so the trimming, the lowercasing and the absent case are decided once.
pub fn explicit_display(node: &VNode) -> Option<String> {
    match node {
        VNode::Text(_) => None,
        VNode::Element { props, .. } => {
            let style = props.attrs.get("style").map(|s| s.as_str());
            style_lookup_str(style, "display").map(|value| value.trim().to_ascii_lowercase())
        }
    }
}

/// Whether `position` takes the box out of flow.
pub fn is_out_of_flow(node: &VNode) -> bool {
    match node {
        VNode::Text(_) => false,
        VNode::Element { props, .. } => {
            let style = props.attrs.get("style").map(|s| s.as_str());
            style_lookup_str(style, "position")
                .map(|value| value.trim().to_ascii_lowercase())
                .is_some_and(|p| p == "absolute" || p == "fixed")
        }
    }
}

/// Whether a box is INLINE-LEVEL: it sits in a line box beside its siblings
/// rather than establishing a block formatting context next to them.
///
/// This is the display classification alone. It says nothing about flow, which
/// is what `is_inline_formatting_participant` adds, and it distinguishes the
/// ATOMIC inline-level boxes from the ones that contribute only text, which is
/// what `is_atomic_inline_box` adds.
///
/// `inline-flex` and `inline-grid` ARE inline-level, and are answered `true`
/// here, because that is what CSS says. They are routed to the flex and grid
/// paths unchanged by the caller, which is a decision about which engine owns
/// them, not about what level they sit at.
pub fn is_inline_level_box(node: &VNode) -> bool {
    match node {
        VNode::Text(_) => false,
        VNode::Element { tag, .. } => match explicit_display(node).as_deref() {
            // Block-level: each establishes a block formatting context and is a
            // SIBLING of the inline run, never a member of it. `flex` and `grid`
            // are here because a block container is block-level whatever it does
            // with its own children.
            Some("block") | Some("flex") | Some("grid") | Some("flow-root") => false,
            Some("inline") | Some("inline-block") | Some("inline-flex") | Some("inline-grid") => {
                true
            }
            // Any other explicit display (`none`, `table`, `list-item`, an
            // unrecognised keyword) is not inline-level. `none` in particular
            // means the box is not generated at all.
            Some(_) => false,
            // No `display` declared: the element's own initial value decides,
            // and `default_display_for_tag` is where that lives.
            None => default_display_for_tag(tag) == "inline",
        },
    }
}

/// Whether a box is an ATOMIC inline-level box: inline-level, but establishing
/// its own block formatting context, so a line box places it whole and its
/// contents do not join the surrounding line. `display: inline-block` is the
/// only value with that shape here, plus every REPLACED element.
///
/// A replaced element is atomic by definition rather than by declaration — CSS 2.1
/// §10.8.1 puts it in the same category — and it matters that the test does not
/// stop at `display: inline-block`. An `<img src=…>` with no `display` at all is
/// `inline` by tag, so without this it entered the inline run as a text-less
/// Fragment, the back-fill at the end of `flush_inline_run` gave it width 0, the
/// renderer drew into a zero-width rect, and no image was ever painted. With this
/// it enters as an Atomic and is sized by `replaced_used_size`.
///
/// `button`, `input`, `select` and `textarea` are `inline-block` in a browser's
/// UA sheet and Velox makes them blocks instead; see `default_display_for_tag`
/// for why that deviation lives there and not here.
pub fn is_atomic_inline_box(node: &VNode) -> bool {
    explicit_display(node).as_deref() == Some("inline-block") || is_replaced_element(node)
}

// ===== REPLACED ELEMENTS ==================================================
//
// A replaced element has no content of its own to lay out: its box comes from
// outside the tree. CSS 2.1 §10.3.2 resolves that box in this order:
//
//   1. the used value of `width`/`height` (CSS 2.1 §10.2),
//   2. otherwise the intrinsic width/height of the source, with the one axis
//      that IS specified setting the ratio for the other,
//   3. otherwise 300x150, the default object size.
//
// Step 3 is not modelled: with no probe registered and no attribute, an `<img>`
// is a zero-size box, which is also what an `inline-block` with no content
// measures today (`max_content_width` returns 0), so the two routes agree
// rather than one of them inventing a size. A broken or not-yet-decoded source
// is therefore invisible, which is what a browser does with it too.
//
// The HTML `width`/`height` ATTRIBUTES enter at step 1, BELOW the CSS
// declarations: they are presentational hints, which the HTML spec maps to
// `width`/`height` declarations that any author rule overrides. Reading them
// here is that mapping, not a second source of truth — and CSS wins per AXIS,
// so `width: 120px` beside `height="30"` takes its width from CSS and its
// height from the attribute.

/// A probe that reports the PIXEL size of a replaced element's source.
///
/// Registered by the renderer against its image backend through
/// [`set_intrinsic_size_probe`]; `None` means no backend, and
/// `velox-dom` decodes nothing itself — this crate has no image decoder and
/// must not grow one.
///
/// The seam is a function pointer rather than a table of sizes so the answer
/// comes from the renderer's OWN cache at layout time, which is where a decode
/// belongs: one registration at init, no second copy of the sizes, and nothing
/// to invalidate when the cache evicts.
pub type IntrinsicSizeProbe = fn(src: &str) -> Option<(i32, i32)>;

static INTRINSIC_SIZE_PROBE: RwLock<Option<IntrinsicSizeProbe>> = RwLock::new(None);

/// Register a probe backed by a real image backend (called by velox-renderer at
/// init).
///
/// Shape and locking are [`crate::text_wrap::set_skia_measurer`]'s, deliberately:
/// this is the second such seam in the crate and there is one way to write it.
/// A probe answers for one `src` at a time and is asked only for `src`
/// attributes, so an unregistered or unhelpful probe costs a replaced element its
/// intrinsic size and nothing else.
pub fn set_intrinsic_size_probe(f: IntrinsicSizeProbe) {
    if let Ok(mut g) = INTRINSIC_SIZE_PROBE.write() {
        *g = Some(f);
    }
}

/// What the registered probe makes of this element's `src`, in pixels.
///
/// Non-positive extents are dropped: a decoder that reports a zero or negative
/// size has not measured the source, and a box of that size paints nothing. An
/// empty `src` is not special-cased either — it is a present attribute, and the
/// probe is the only thing that can know what it resolves to.
fn intrinsic_size_of(props: &Props) -> Option<(i32, i32)> {
    let src = props.attrs.get("src")?;
    let (w, h) = INTRINSIC_SIZE_PROBE.read().ok().and_then(|g| *g)?(src)?;
    (w > 0 && h > 0).then_some((w, h))
}

/// Whether the box's size comes from OUTSIDE the tree, so it has none of its
/// own content to be sized by.
///
/// Presence of a `src` ATTRIBUTE is the test, and it is deliberately not a tag
/// test. Every element that can name an external source is replaced in CSS —
/// `img`, `video`, `iframe`, `embed`, `object`, `input type=image` — and a
/// per-tag list would be a list to extend by hand each time one of those is
/// used, with a silent zero-size box as the failure mode when it was missed.
///
/// What the rule costs is `script`, `link`, `source` and `track`, which carry a
/// `src` and generate no box in a browser because the UA sheet gives them
/// `display: none`. Velox has no such sheet, so those elements lay out as a
/// zero-size box where they used to lay out as a full-container-width block.
/// Nothing is painted either way and neither is hit-testable, so the whole
/// divergence is the width of a box that was never going to be drawn in.
///
/// An element with NO `src` is never replaced. That is why an `<img>` with no
/// source keeps the box it had: it names nothing, so it has no intrinsic size
/// to be sized by, and an unsized one is still a zero-width line-box member
/// rather than a zero-width atomic.
pub fn is_replaced_element(node: &VNode) -> bool {
    match node {
        VNode::Text(_) => false,
        VNode::Element { props, .. } => props.attrs.contains_key("src"),
    }
}

/// The size a replaced element uses for one axis when its CSS declaration says
/// nothing, following the order at the top of this section: the presentational
/// attribute first, then the intrinsic size.
///
/// ZERO when neither exists, which is the one value a replaced element must
/// never inherit from a block: `content_size_for` answers the available width
/// for a block with no declared width, and a replaced element that filled its
/// container because nothing told it how wide it is would be the opposite of
/// what a replaced element is.
///
/// `basis` is what a percentage resolves against — the same basis the CSS
/// declaration at the call site resolves against, so `width="50%"` and
/// `width: 50%` are one number and not two. The intrinsic half is in PIXELS
/// already, because that is what a decoder reports.
fn replaced_used_size(
    props: &Props,
    attr: &str,
    intrinsic: Option<i32>,
    basis: f32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport: (f32, f32),
) -> i32 {
    let hinted = props.attrs.get(attr).and_then(|raw| {
        is_presentational_dimension(raw)
            .then(|| parse_length_value(raw, basis, parent_font_size, root_font_size, viewport))?
    });
    hinted
        .filter(|v| *v >= 0.0)
        .map(|v| v.round() as i32)
        .or(intrinsic)
        .unwrap_or(0)
}

/// Whether an attribute value is in the grammar HTML gives a replaced element's
/// `width`/`height`: a `<dimension>` (`200`, `200px`) or a `<percentage>`
/// (`50%`).
///
/// `parse_length_value` resolves every unit this engine knows, which is more
/// than the HTML grammar allows — `width="2em"` would resolve against the font
/// size here and is dropped by a browser. The hint is applied only when it is a
/// value HTML actually defines, so the presentational mapping cannot be widened
/// into a longer unit list by accident. Note what this rejects for free: a
/// negative or signed value, which HTML's grammar excludes and CSS 2.1 §10.4
/// drops as an invalid `width` anyway.
fn is_presentational_dimension(raw: &str) -> bool {
    let raw = raw.trim();
    let number = raw
        .strip_suffix("px")
        .or_else(|| raw.strip_suffix('%'))
        .unwrap_or(raw)
        .trim();
    !number.is_empty() && number.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn is_inline_formatting_participant(node: &VNode) -> bool {
    match node {
        VNode::Text(text) => !text.chars().all(|c| c.is_whitespace()),
        VNode::Element { .. } => is_inline_level_box(node) && !is_out_of_flow(node),
    }
}

fn is_formatting_participant(node: &VNode) -> bool {
    match node {
        VNode::Text(text) => !text.chars().all(|c| c.is_whitespace()),
        VNode::Element { .. } => {
            // Any box that is generated and in flow, whatever its display: a
            // block box is a formatting participant too, and
            // `should_drop_collapsible_whitespace` has to see past one to find
            // the inline box on the other side.
            explicit_display(node).as_deref() != Some("none") && !is_out_of_flow(node)
        }
    }
}

fn should_drop_collapsible_whitespace(
    children: &[VNode],
    idx: usize,
    parent_style: Option<&str>,
) -> bool {
    let Some(VNode::Text(text)) = children.get(idx) else {
        return false;
    };
    if !text.chars().all(|c| c.is_whitespace()) {
        return false;
    }
    let white_space = style_lookup_str(parent_style, "white-space")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "normal".to_string());
    if !matches!(white_space.as_str(), "normal" | "nowrap") {
        return false;
    }
    let has_inline_before = children[..idx]
        .iter()
        .rev()
        .find(|candidate| is_formatting_participant(candidate))
        .is_some_and(is_inline_formatting_participant);
    let has_inline_after = children[idx + 1..]
        .iter()
        .find(|candidate| is_formatting_participant(candidate))
        .is_some_and(is_inline_formatting_participant);
    !has_inline_before || !has_inline_after
}

fn style_lookup_i32(style: Option<&str>, key: &str) -> Option<i32> {
    let t = table_for(style?);
    t.value(t.first(key)?).parse::<i32>().ok()
}

#[allow(dead_code)]
fn style_box_sides(style: Option<&str>, base: &str) -> (i32, i32, i32, i32) {
    // returns (left, right, top, bottom)
    let t = table_for(style.unwrap_or(""));
    let get = |k: &str| -> Option<i32> { parse_px(t.value(t.first(k)?)) };
    let all = get(base).unwrap_or(0);
    let mut buf = [0u8; 48];
    let l = get(side_key(&mut buf, base, "left")).unwrap_or(all);
    let r = get(side_key(&mut buf, base, "right")).unwrap_or(all);
    let t_ = get(side_key(&mut buf, base, "top")).unwrap_or(all);
    let b = get(side_key(&mut buf, base, "bottom")).unwrap_or(all);
    (l, r, t_, b)
}

/// The displacement `position: relative` / `position: sticky` applies to a box,
/// derived from the style alone.
///
/// `base_w`/`base_h` are the percentages' basis, and they are the PARENT's content
/// box, not this element's own. `apply_relative_position` is handed the parent's
/// `content_w` and `content_h_available` and passes them straight here, and
/// `at()` hands the same two numbers as `containing.w`/`containing.h`; the two
/// therefore resolve a percentage offset identically, which they did not when
/// `at()` used its own content width.
///
/// A positioned element's PADDING box after this move is the containing block for
/// its absolutely positioned descendants, so the same offset has to be added to
/// that padding box too. Both reads come from here, which is the point of the
/// extraction — but extraction only stops drift if the BASE is threaded the same
/// way too, and that was the half that drifted.
fn relative_offset_delta(
    style: Option<&str>,
    base_w: i32,
    base_h: i32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> (i32, i32) {
    let pos = style_lookup_str(style, "position").unwrap_or_else(|| "static".to_string());
    if pos != "relative" && pos != "sticky" {
        return (0, 0);
    }
    let axis_delta = |near: &str, far: &str, base: i32| -> i32 {
        if let Some(v) = style_lookup_len_full(
            style,
            near,
            base as f32,
            parent_font_size,
            root_font_size,
            viewport_w,
            viewport_h,
        ) {
            v
        } else if let Some(v) = style_lookup_len_full(
            style,
            far,
            base as f32,
            parent_font_size,
            root_font_size,
            viewport_w,
            viewport_h,
        ) {
            -v
        } else {
            0
        }
    };
    (
        axis_delta("left", "right", base_w),
        axis_delta("top", "bottom", base_h),
    )
}

fn apply_relative_position(
    style: Option<&str>,
    node: &mut LayoutNode,
    base_w: i32,
    base_h: i32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) {
    let pos = style_lookup_str(style, "position").unwrap_or_else(|| "static".to_string());
    if pos != "relative" && pos != "sticky" {
        return;
    }
    let (dx, dy) = relative_offset_delta(
        style,
        base_w,
        base_h,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );
    node.rect.x += dx;
    node.rect.y += dy;
}

#[allow(clippy::too_many_arguments)]
fn apply_sticky_position(
    style: Option<&str>,
    node: &mut LayoutNode,
    container_x: i32,
    container_y: i32,
    container_w: i32,
    container_h: i32,
    scroll_offset_x: i32,
    scroll_offset_y: i32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) {
    let pos = style_lookup_str(style, "position").unwrap_or_else(|| "static".to_string());
    if pos != "sticky" {
        return;
    }

    // The sticky element's natural position (where it would be without sticky)
    let natural_x = node.rect.x;
    let natural_y = node.rect.y;

    // The viewport/scroll container edges
    let viewport_left = scroll_offset_x;
    let viewport_top = scroll_offset_y;
    let viewport_right = scroll_offset_x + container_w;
    let viewport_bottom = scroll_offset_y + container_h;

    // Sticky: element is constrained by both its natural position AND sticky offsets
    // For top/left: take the MAX of natural and sticky constraint
    // For bottom/right: take the MIN of natural and sticky constraint
    // Then clamp within parent container

    // Horizontal sticky
    let sticky_left = style_lookup_len_full(
        style,
        "left",
        container_w as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );
    let sticky_right = style_lookup_len_full(
        style,
        "right",
        container_w as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );

    let mut final_x = natural_x;
    if let Some(left) = sticky_left {
        // Element should not go above viewport_left + left
        let sticky_x = viewport_left + left;
        final_x = final_x.max(sticky_x);
    }
    if let Some(right) = sticky_right {
        // Element should not go beyond viewport_right - right
        let sticky_x = viewport_right - right - node.rect.w;
        final_x = final_x.min(sticky_x);
    }

    // Vertical sticky
    let sticky_top = style_lookup_len_full(
        style,
        "top",
        container_h as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );
    let sticky_bottom = style_lookup_len_full(
        style,
        "bottom",
        container_h as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );

    let mut final_y = natural_y;
    if let Some(top) = sticky_top {
        // Element should not go above viewport_top + top
        let sticky_y = viewport_top + top;
        final_y = final_y.max(sticky_y);
    }
    if let Some(bottom) = sticky_bottom {
        // Element should not go beyond viewport_bottom - bottom
        let sticky_y = viewport_bottom - bottom - node.rect.h;
        final_y = final_y.min(sticky_y);
    }

    // Clamp within parent container bounds
    final_x = final_x.max(container_x);
    final_x = final_x.min(container_x + container_w - node.rect.w);
    final_y = final_y.max(container_y);
    final_y = final_y.min(container_y + container_h - node.rect.h);

    node.rect.x = final_x;
    node.rect.y = final_y;
}

/// Resolve an out-of-flow box against its containing block (CSS 2.1 §10.3.7).
///
/// `cb` is the PADDING box of the nearest positioned ancestor, or the viewport
/// for the initial containing block — CSS 2.1 §10.1 forms a containing block
/// from an ancestor's padding edges, not its content box.
///
/// `static_pos` is where the box would have been placed by the in-flow pass.
/// It is used for whichever axis has neither offset, because in that case the
/// spec's static-position rule applies instead of a containing-block corner.
#[allow(clippy::too_many_arguments)]
fn apply_absolute_position(
    style: Option<&str>,
    node: &mut LayoutNode,
    cb: ContainingBlock,
    static_pos: (i32, i32),
    parent_font_size: f32,
    root_font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) {
    let left = style_lookup_len_full(
        style,
        "left",
        cb.w as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );
    let right = style_lookup_len_full(
        style,
        "right",
        cb.w as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );
    let top = style_lookup_len_full(
        style,
        "top",
        cb.h as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );
    let bottom = style_lookup_len_full(
        style,
        "bottom",
        cb.h as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );
    let declared_w = style_lookup_len_full(
        style,
        "width",
        cb.w as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );
    let declared_h = style_lookup_len_full(
        style,
        "height",
        cb.h as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );

    // Both offsets on an axis: the box spans the gap between them, which is how
    // an element with `left:0; right:0` fills its containing block.
    if declared_w.is_none()
        && let (Some(l), Some(r)) = (left, right)
    {
        node.rect.w = (cb.w - l - r).max(0);
    }
    if declared_h.is_none()
        && let (Some(t), Some(b)) = (top, bottom)
    {
        node.rect.h = (cb.h - t - b).max(0);
    }

    if let Some(l) = left {
        node.rect.x = cb.x + l;
    } else if let Some(r) = right {
        node.rect.x = cb.x + (cb.w - r - node.rect.w);
    } else {
        node.rect.x = static_pos.0;
    }

    if let Some(t) = top {
        node.rect.y = cb.y + t;
    } else if let Some(b) = bottom {
        node.rect.y = cb.y + (cb.h - b - node.rect.h);
    } else {
        node.rect.y = static_pos.1;
    }
}

/// Calculate text dimensions using font-based metrics
fn text_dimensions(t: &str, font_size_px: f32) -> (i32, i32) {
    let metrics = FontMetrics::from_font_size(font_size_px);
    let len = t.chars().count() as f32;
    let w = if len > 0.0 {
        (len * metrics.char_width).round() as i32
    } else {
        0
    };
    // The height follows the run, through the same seam the wrapped-lines path
    // uses. The WIDTH above is deliberately still the 0.6 heuristic: this run
    // was never measured, and switching it to the seam would change every text
    // width in a Skia run. Height was a fixed multiplier, so changing it is the
    // point of the task.
    //
    // `line_extent` is the seam's own sum of the two numbers, not a re-derivation
    // of it. `text_wrap::line_box_height` is the same expression as this is, but it
    // is private, and a second copy of a line-box formula in a different module is
    // exactly how the next task's strut would reach the wrapping path and not this
    // one. The height is built from the run's measured extent here for the same
    // reason and by the same call.
    // One function builds every line box, and it is the one the strut lives in.
    // This used to re-derive the formula inline, which meant a strut added to
    // `line_box_height` would have reached the wrapped-text path and silently
    // skipped this one. It is the same call the wrapping path makes.
    let h = {
        let run = crate::text_wrap::measure_text_metrics(
            t,
            font_size_px,
            DEFAULT_TEXT_FAMILY,
            crate::text_wrap::current_scale(),
        );
        crate::text_wrap::line_box_height(&run, &metrics)
    };
    (w, h)
}

/// Convert a Length value to pixels given context
#[allow(dead_code)]
fn length_to_px(len: Length, parent_size: f32, root_size: f32, viewport: (f32, f32)) -> f32 {
    match len {
        Length::Px(v) => v,
        Length::Percent(v) => parent_size * v / 100.0,
        Length::Rem(v) => v * root_size,
        Length::Em(v) => v * parent_size,
        Length::Vw(v) => v * viewport.0 / 100.0,
        Length::Vh(v) => v * viewport.1 / 100.0,
        Length::Dvw(v) => v * viewport.0 / 100.0,
        Length::Dvh(v) => v * viewport.1 / 100.0,
        Length::Auto => 0.0,
        Length::Zero => 0.0,
    }
}

/// Box sides (margin/padding) with full CSS unit support.
///
/// Resolves CSS shorthand values (e.g. `padding: 10px 20px`) by splitting on
/// whitespace and expanding into individual sides following the CSS shorthand
/// rules:
///   1 value  → all sides
///   2 values → top/bottom, left/right
///   3 values → top, left/right, bottom
///   4 values → top, right, bottom, left
///
/// Individual longhand properties (e.g. `padding-top`) always take precedence
/// over the shorthand.
fn style_box_sides_full(
    style: Option<&str>,
    base: &str,
    parent_size: f32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> (i32, i32, i32, i32) {
    let resolve = |val: &str| -> Option<i32> {
        parse_length_value(
            val,
            parent_size,
            parent_font_size,
            root_font_size,
            (viewport_w, viewport_h),
        )
        .map(|f| f.round() as i32)
    };

    // `margin` accepts `auto`, which has no numeric value until the block's
    // free space is resolved (CSS 2.1 §10.3.3). Keep the whole shorthand
    // intact by treating those sides as 0 here; the auto sides are tracked
    // separately via `style_margin_auto_sides` and centered downstream.
    let resolve_side = |val: &str| -> Option<i32> {
        if base == "margin" && val.trim().eq_ignore_ascii_case("auto") {
            Some(0)
        } else {
            resolve(val)
        }
    };

    let Some(t) = style.map(table_for) else {
        // No style: no shorthand and no longhands, so every side is 0. This
        // is what the original produced, where the shorthand `and_then` saw
        // `None` and each `style_lookup_len_full(None, ..)` returned `None`.
        return (0, 0, 0, 0);
    };
    let ctx = LenCtx {
        parent_size,
        parent_font_size,
        root_font_size,
        vw: viewport_w,
        vh: viewport_h,
    };
    let memo = BoxSidesKey::new(base, &ctx);
    if let Some(hit) = t.box_sides.borrow().get(&memo).copied() {
        return hit;
    }

    // Try expanding the shorthand value into individual sides.
    // CSS shorthand rules: 1 val = all, 2 = v h, 3 = t h b, 4 = t r b l
    let shorthand_sides: Option<(i32, i32, i32, i32)> = (|| {
        // The first `base` declaration, e.g. "padding: 10px 20px". Written as a
        // closure so `?` still means "this declaration is not a usable
        // shorthand", exactly as the original `style.and_then(..)` did.
        let raw = t.value(t.first(base)?);
        let parts: Vec<&str> = raw.split_whitespace().collect();
        match parts.len() {
            0 => None,
            1 => resolve_side(parts[0]).map(|v| (v, v, v, v)),
            2 => {
                let v = resolve_side(parts[0])?;
                let h = resolve_side(parts[1])?;
                Some((h, h, v, v)) // left, right, top, bottom
            }
            3 => {
                let top = resolve_side(parts[0])?;
                let h = resolve_side(parts[1])?;
                let b = resolve_side(parts[2])?;
                Some((h, h, top, b))
            }
            4 => {
                let top = resolve_side(parts[0])?;
                let r = resolve_side(parts[1])?;
                let b = resolve_side(parts[2])?;
                let l = resolve_side(parts[3])?;
                Some((l, r, top, b))
            }
            _ => None,
        }
    })();

    // Destructure shorthand: (left, right, top, bottom)
    let (sh_l, sh_r, sh_t, sh_b) = shorthand_sides.unwrap_or((0, 0, 0, 0));

    // Individual longhand properties override the shorthand. The four keys are
    // built on the stack rather than with `format!`, and resolved against the
    // table we already hold rather than re-hashing the style string per side.
    let mut buf = [0u8; 48];
    let out = (
        len_full_in(&t, side_key(&mut buf, base, "left"), &ctx).unwrap_or(sh_l),
        len_full_in(&t, side_key(&mut buf, base, "right"), &ctx).unwrap_or(sh_r),
        len_full_in(&t, side_key(&mut buf, base, "top"), &ctx).unwrap_or(sh_t),
        len_full_in(&t, side_key(&mut buf, base, "bottom"), &ctx).unwrap_or(sh_b),
    );
    memo_put(&t.box_sides, memo, out);
    out
}

/// Whether `margin-left` / `margin-right` resolve to `auto` for the given
/// inline style. Honors the CSS shorthand expansion (1-4 values) and the
/// longhand-over-shorthand precedence used by `style_box_sides_full`.
fn style_margin_auto_sides(style: Option<&str>) -> (bool, bool) {
    let Some(s) = style else {
        return (false, false);
    };
    let t = table_for(s);
    let is_auto = |tok: &str| tok.trim().eq_ignore_ascii_case("auto");

    // This helper is the one place in the family that reads the LAST
    // occurrence rather than the first: its original loop ASSIGNED on every
    // match instead of returning, so a later declaration overwrote an earlier
    // one. Hence `last` here and `first` everywhere else. Getting this wrong
    // would silently change which of two conflicting declarations wins.
    let shorthand: Option<(bool, bool)> = match t.last("margin") {
        Some(i) => {
            let parts: Vec<&str> = t.value(i).split_whitespace().collect();
            match parts.as_slice() {
                [all] => Some((is_auto(all), is_auto(all))),
                [_, h] => Some((is_auto(h), is_auto(h))),
                [_, h, _] => Some((is_auto(h), is_auto(h))),
                [_, right, _, left] => Some((is_auto(left), is_auto(right))),
                _ => None,
            }
        }
        None => None,
    };
    let long_l = t.last("margin-left").map(|i| is_auto(t.value(i)));
    let long_r = t.last("margin-right").map(|i| is_auto(t.value(i)));
    let (sh_l, sh_r) = shorthand.unwrap_or((false, false));
    (long_l.unwrap_or(sh_l), long_r.unwrap_or(sh_r))
}

fn style_border_widths(
    style: Option<&str>,
    parent_size: f32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> (i32, i32, i32, i32) {
    let Some(t) = style.map(table_for) else {
        // No style at all. The original resolved every source to `None` and
        // therefore returned all zeros; short-circuiting keeps that exact
        // result instead of building an empty table to derive it.
        return (0, 0, 0, 0);
    };
    let ctx = LenCtx {
        parent_size,
        parent_font_size,
        root_font_size,
        vw: viewport_w,
        vh: viewport_h,
    };
    let resolve = |val: &str| -> Option<i32> {
        parse_length_value(
            val,
            parent_size,
            parent_font_size,
            root_font_size,
            (viewport_w, viewport_h),
        )
        .map(|f| f.round() as i32)
    };

    let memo = BorderKey::new(&ctx);
    if let Some(hit) = t.border.borrow().get(&memo).copied() {
        return hit;
    }

    // default from `border` shorthand first token if it parses as length
    let mut default_border: Option<i32> = None;
    if let Some(i) = t.first("border") {
        let first = t.value(i).split_whitespace().next().unwrap_or("");
        if let Some(v) = resolve(first) {
            default_border = Some(v);
        } else if first == "0" {
            default_border = Some(0);
        }
    }
    let (mut bl, mut br, mut bt, mut bb) =
        default_border.map(|v| (v, v, v, v)).unwrap_or((0, 0, 0, 0));

    // `border-width` shorthand (1-4 values) overrides `border` default if present
    if t.first("border-width").is_some() {
        let (l, r, top, b) = style_box_sides_full(
            style,
            "border-width",
            parent_size,
            parent_font_size,
            root_font_size,
            viewport_w,
            viewport_h,
        );
        bl = l;
        br = r;
        bt = top;
        bb = b;
    }

    // individual `border-*-width` overrides
    for (key, target) in [
        ("border-left-width", &mut bl),
        ("border-right-width", &mut br),
        ("border-top-width", &mut bt),
        ("border-bottom-width", &mut bb),
    ] {
        if let Some(v) = len_full_in(&t, key, &ctx) {
            *target = v;
        }
    }
    // also support legacy `border-left` etc shorthand width extraction
    for (key, target) in [
        ("border-left", &mut bl),
        ("border-right", &mut br),
        ("border-top", &mut bt),
        ("border-bottom", &mut bb),
    ] {
        if let Some(i) = t.first(key) {
            let first = t.value(i).split_whitespace().next().unwrap_or("");
            if let Some(v) = resolve(first) {
                *target = v;
            }
        }
    }

    let out = (bl, br, bt, bb);
    memo_put(&t.border, memo, out);
    out
}

pub fn compute_layout(node: &VNode, viewport_w: i32, viewport_h: i32) -> LayoutNode {
    #[allow(clippy::too_many_arguments)]
    fn at(
        node: &VNode,
        x: i32,
        y: i32,
        avail_w: i32,
        avail_h: i32,
        viewport_w: i32,
        viewport_h: i32,
        containing: ContainingBlock,
        source_index: Option<usize>,
        root_font_size: f32,
        parent_font_size: f32,
    ) -> LayoutNode {
        match node {
            VNode::Text(t) => {
                // Text nodes use inherited font size
                let (w, h) = text_dimensions(t, parent_font_size);
                LayoutNode {
                    rect: Rect { x, y, w, h },
                    z_index: 0,
                    display_none: false,
                    source_index,
                    scroll_x: 0,
                    scroll_y: 0,
                    clip: None,
                    stacking_context: false,
                    scroll_height: h,
                    max_scroll_y: 0,
                    scrollable: false,
                    children: vec![],
                }
            }
            VNode::Element {
                tag,
                props,
                children,
            } => {
                let style = props.attrs.get("style").map(|s| s.as_str());

                // Resolve this element's font-size for children to inherit
                let my_font_size = style_lookup_font_size(
                    style,
                    parent_font_size,
                    root_font_size,
                    (viewport_w as f32, viewport_h as f32),
                )
                .unwrap_or(parent_font_size);

                let vw_f = viewport_w as f32;
                let vh_f = viewport_h as f32;

                let (ml, mr, mt, mb) = style_box_sides_full(
                    style,
                    "margin",
                    containing.w as f32,
                    parent_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );
                let (pl, pr, pt, pb) = style_box_sides_full(
                    style,
                    "padding",
                    containing.w as f32,
                    parent_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );
                let (bl, br, bt, bb) = style_border_widths(
                    style,
                    containing.w as f32,
                    parent_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );
                let box_sizing = style_lookup_str(style, "box-sizing")
                    .map(|s| s.trim().to_ascii_lowercase())
                    .unwrap_or_else(|| "content-box".to_string());
                let is_border_box = box_sizing == "border-box";
                let is_root_tag = matches!(tag.as_str(), "body" | "html");
                let is_root_index = root_is_viewport_filling(source_index);
                // Viewport root normalization: first VNode (and html/body) always fills.
                // Expanded predicate handles 100% | 100vw | 100dvw | 100vh | 100dvh | min-height:100%/vh/dvh
                let is_viewport_filling = is_viewport_filling(style, is_root_tag || is_root_index);

                // Check if element has height: 100vh / 100dvh (viewport-relative) — pixel equality covers both vh/dvh
                let has_viewport_height = style_lookup_len_full(
                    style,
                    "height",
                    vh_f,
                    parent_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                )
                .map(|h| (h as f32 - vh_f).abs() < 0.5)
                .unwrap_or(false);
                let min_height_vh = style_lookup_len_full(
                    style,
                    "min-height",
                    vh_f,
                    parent_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                )
                .map(|h| (h as f32 - vh_f).abs() < 0.5)
                .unwrap_or(false);

                // Also treat min-height:100% as viewport-filling when it resolves to available/viewport height
                let min_height_is_100pct = {
                    let raw = style.and_then(|s| {
                        for decl in s.split(';') {
                            let d = decl.trim();
                            if d.is_empty() {
                                continue;
                            }
                            if let Some((k, v)) = d.split_once(':')
                                && k.trim() == "min-height"
                            {
                                return Some(v.trim().to_string());
                            }
                        }
                        None
                    });
                    raw.as_deref()
                        .and_then(crate::Length::parse)
                        .map(|l| matches!(l, crate::Length::Percent(p) if (p - 100.0).abs() < 0.01))
                        .unwrap_or(false)
                };

                // Element outer position with margins; `margin: auto` on a
                // block-level box with a declared width centers it by
                // splitting the free space equally (CSS 2.1 §10.3.3).
                let elem_y = y + mt;

                // Determine width: if set, use as content+padding width; else take available width
                //
                // A REPLACED element takes its width from the order in the replaced
                // section above when the declaration says nothing, and never from
                // `avail_w` — which is the branch every other undeclared block
                // takes, and the whole reason an `<img>` used to be sized by its
                // container instead of by its source.
                let replaced = is_replaced_element(node);
                let intrinsic = replaced.then(|| intrinsic_size_of(props)).flatten();
                let declared_w = style_lookup_len_full(
                    style,
                    "width",
                    avail_w as f32,
                    parent_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                )
                .or_else(|| {
                    replaced.then(|| {
                        replaced_used_size(
                            props,
                            "width",
                            intrinsic.map(|(w, _)| w),
                            avail_w as f32,
                            parent_font_size,
                            root_font_size,
                            (vw_f, vh_f),
                        )
                    })
                });

                let (ml_auto, mr_auto) = style_margin_auto_sides(style);
                let (ml, mr) = if let Some(dw) = declared_w.filter(|_| ml_auto || mr_auto) {
                    // Border-box: the declared width already includes
                    // padding+border (content_size_for convention, F-04) —
                    // no double subtraction.
                    let outer_w = if is_border_box {
                        dw as f32
                    } else {
                        dw as f32 + pl as f32 + pr as f32 + bl as f32 + br as f32
                    };
                    let (l, r) = crate::style::resolve_auto_margins_core(
                        avail_w as f32,
                        outer_w,
                        ml as f32,
                        mr as f32,
                        ml_auto,
                        mr_auto,
                    );
                    (l.round() as i32, r.round() as i32)
                } else {
                    (ml, mr)
                };
                let elem_x = x + ml;

                // Determine height: handle viewport-relative heights (100vh, min-height: 100vh)
                //
                // The replaced fallback is here as well as on the width, and for
                // the same reason: this is the `avail_h` the children and the
                // out-of-flow descendants are measured against, so a replaced
                // element whose height comes from its source must say so here
                // too, not only at the height the rect is finally given.
                let declared_h = style_lookup_len_full(
                    style,
                    "height",
                    avail_h as f32,
                    parent_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                )
                .or_else(|| {
                    replaced.then(|| {
                        replaced_used_size(
                            props,
                            "height",
                            intrinsic.map(|(_, h)| h),
                            avail_h as f32,
                            parent_font_size,
                            root_font_size,
                            (vw_f, vh_f),
                        )
                    })
                });

                // Legacy 100% viewport-filling pair still respected, but root already true covers implicit fill
                let has_100p_width = declared_w
                    .map(|w| (w as f32 - avail_w as f32).abs() < 0.5)
                    .unwrap_or(false);
                let has_100p_height = declared_h
                    .map(|h| (h as f32 - avail_h as f32).abs() < 0.5)
                    .unwrap_or(false);
                let legacy_pair = has_100p_width && has_100p_height;

                // Update is_viewport_height to include viewport-filling elements (must be before rect calcs)
                // Covers root, expanded predicate, legacy 100% pair, explicit vh/dvh, and min-height variants
                let is_viewport_height = is_viewport_filling
                    || legacy_pair
                    || has_viewport_height
                    || min_height_vh
                    || min_height_is_100pct;

                // `max-width` caps the used width (CSS 2.1 §10.4), so the smaller
                // of the declared/available width and the cap wins.
                //
                // This has to sit between content_size_for and `content_w` below:
                // `content_w` is what every child is measured against, what text is
                // wrapped to, and — via `rect_w` — what `overflow`, `clip` and the
                // scroll metrics are derived from once the child tree exists. A
                // clamp placed after the child pass would move the box without
                // moving anything inside it.
                // `is_border_box` and the four sides are already in scope — they are
                // the arguments to `content_size_for` immediately above — so the cap
                // is expressed with them directly rather than by re-reading the style
                // string on every box of every layout. `clamp_width_to_max_width` does
                // that re-read and exists only for the flex placement pass, where the
                // item's box model is genuinely not in scope.
                let padding = Sides {
                    top: bl,
                    right: pr,
                    bottom: br,
                    left: pl,
                };
                let margin = Sides {
                    top: 0,
                    right: mr,
                    bottom: 0,
                    left: ml,
                };
                // The `min-width` floor is applied *outside* the cap, so the
                // ordering of these two calls IS the precedence rule: css-sizing-3
                // §3.1 makes the minimum size constraint the strongest one, and
                // CSS 2.1 §10.4's algorithm re-runs the width rules with `width`
                // set to `min-width` precisely so a too-small cap loses.
                // Flooring first and capping second would leave `max-width`
                // winning, which is the divergence the tripwire in
                // `tests/maxwidth_absolute.rs` is named for.
                let rect_w = floor_to_min_width(
                    cap_to_max_width(
                        content_size_for(
                            declared_w,
                            avail_w,
                            is_border_box,
                            &padding,
                            &margin,
                            is_viewport_filling,
                            legacy_pair,
                        ),
                        used_max_width(
                            style,
                            containing.w as f32,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        ),
                        is_border_box,
                        pl,
                        pr,
                        bl,
                        br,
                    ),
                    used_min_width(
                        style,
                        containing.w as f32,
                        my_font_size,
                        root_font_size,
                        vw_f,
                        vh_f,
                    ),
                    is_border_box,
                    pl,
                    pr,
                    bl,
                    br,
                );

                // For viewport-height elements, use viewport height as the base, otherwise box-sizing adjusted
                // Declared height takes precedence over viewport filling so explicit fixed heights are respected
                let mut _rect_h = if let Some(dh) = declared_h {
                    if is_border_box {
                        dh
                    } else {
                        dh + pt + pb + bt + bb
                    }
                } else if is_viewport_height {
                    (avail_h - mt - mb).max(1)
                } else {
                    declared_h.unwrap_or(avail_h)
                };

                // Content box (border inside padding offset)
                let content_x = elem_x + bl + pl;
                let content_y_start = elem_y + bt + pt;
                let content_w = (rect_w - pl - pr - bl - br).max(0);
                let content_h_available = (_rect_h - pt - pb - bt - bb).max(0);

                let overflow =
                    style_lookup_str(style, "overflow").unwrap_or_else(|| "visible".to_string());
                // Deprecated synthetic scroll-left/top — keep parsing for compat with warning, map to ScrollState
                let raw_scroll_x = style_lookup_len_full(
                    style,
                    "scroll-left",
                    content_w as f32,
                    my_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );
                let raw_scroll_y = style_lookup_len_full(
                    style,
                    "scroll-top",
                    content_h_available as f32,
                    my_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );
                if raw_scroll_x.is_some() || raw_scroll_y.is_some() {
                    // Deprecated synthetic scroll-left/scroll-top: still parsed and
                    // applied for compat, but warn once per process.
                    static SCROLL_DEPRECATION_WARNED: std::sync::atomic::AtomicBool =
                        std::sync::atomic::AtomicBool::new(false);
                    if !SCROLL_DEPRECATION_WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                        eprintln!(
                            "[velox] warning: scroll-left/scroll-top styles are deprecated; \
                             use overflow:auto/scroll with wheel scrolling instead"
                        );
                    }
                }
                let scroll_x = raw_scroll_x.unwrap_or(0);
                let scroll_y = raw_scroll_y.unwrap_or(0);
                let content_x_scrolled = content_x - scroll_x;
                let content_y_scrolled = content_y_start - scroll_y;

                // Layout strategy: block (default) or flex. An element's
                // default `display` comes from `default_display_for_tag`, which
                // agrees with the UA sheet in velox-style; both routes below
                // only test for "none" and "flex", so an `inline` default keeps
                // using the block flow (there is no inline layout yet).
                let display = style_lookup_str(style, "display")
                    .unwrap_or_else(|| default_display_for_tag(tag).to_string());
                let position =
                    style_lookup_str(style, "position").unwrap_or_else(|| "static".to_string());
                let z_index = if position != "static" {
                    style_lookup_i32(style, "z-index").unwrap_or(0)
                } else {
                    0
                };
                let opacity = style_lookup_str(style, "opacity")
                    .and_then(|v| {
                        v.parse::<f32>()
                            .ok()
                            .map(|f| if f.is_finite() { f } else { 1.0 })
                    })
                    .unwrap_or(1.0);
                let transform =
                    style_lookup_str(style, "transform").unwrap_or_else(|| "none".to_string());
                let stacking_context = opacity < 1.0
                    || (transform != "none" && !transform.is_empty())
                    || position != "static";
                if display == "none" {
                    return LayoutNode {
                        rect: Rect {
                            x: elem_x,
                            y: elem_y,
                            w: 0,
                            h: 0,
                        },
                        z_index,
                        display_none: true,
                        source_index,
                        scroll_x: 0,
                        scroll_y: 0,
                        clip: None,
                        stacking_context: false,
                        scroll_height: 0,
                        max_scroll_y: 0,
                        scrollable: false,
                        children: vec![],
                    };
                }

                let mut laid_children: Vec<LayoutNode> = Vec::new();
                let mut abs_children: Vec<PendingAbsolute> = Vec::new();
                let mut max_y_end = content_y_start;

                // The containing block for out-of-flow children (CSS 2.1 §10.1): a
                // positioned element's PADDING box, or else the containing block
                // this element was itself handed. A `position: relative` ancestor
                // displaces its padding box, and that displaced box is what its
                // absolutely positioned descendants resolve against, so the
                // relative offset is folded in — from the same helper
                // `apply_relative_position` moves the box with, so the two cannot
                // disagree.
                //
                // Read twice. The provisional read sizes an out-of-flow child's own
                // subtree during the child pass; the read in the shared tail, after
                // `rect_h` exists, is what offsets are resolved against. They differ
                // only in height, because a positioned ancestor with an auto height
                // has no height until its children have been laid out.
                //
                // The residual the double read leaves, stated here because it is
                // invisible from the tail alone: an absolutely positioned child's own
                // IN-FLOW content is measured against the provisional available
                // height and is not re-laid-out against the final one. It is reachable
                // only through a percentage or `100%` height inside an absolutely
                // positioned box under an auto-height positioned ancestor.
                //
                // The base is the PARENT's content box, which is what `containing.w`
                // and `containing.h` are — the same two numbers the parent's child
                // pass hands `apply_relative_position`. This element's own `content_w`
                // is not the base: using it made `left: 10%` mean one thing when the
                // parent's child pass moved the box and another when this read moved
                // the box it establishes, and the box only ever moved one of the two
                // ways. `relative_offset_delta` early-returns for anything that is
                // not relative or sticky, so `establishes_containing_block` below
                // stays the only place that decides which positions are special.
                let (rel_dx, rel_dy) = relative_offset_delta(
                    style,
                    containing.w,
                    containing.h,
                    my_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );
                let out_of_flow_cb = |border_box_h: i32| -> ContainingBlock {
                    if establishes_containing_block(&position) {
                        ContainingBlock {
                            x: elem_x + bl + rel_dx,
                            y: elem_y + bt + rel_dy,
                            w: (rect_w - bl - br).max(0),
                            h: (border_box_h - bt - bb).max(0),
                        }
                    } else {
                        containing
                    }
                };
                let descendant_cb = out_of_flow_cb(_rect_h);

                if display == "flex" {
                    // Full CSS Flexbox implementation
                    let flex_dir = style_lookup_str(style, "flex-direction")
                        .unwrap_or_else(|| "row".to_string());
                    let flex_wrap = style_lookup_str(style, "flex-wrap")
                        .unwrap_or_else(|| "nowrap".to_string());
                    let justify_content = style_lookup_str(style, "justify-content")
                        .unwrap_or_else(|| "flex-start".to_string());
                    let align_items = style_lookup_str(style, "align-items")
                        .unwrap_or_else(|| "stretch".to_string());
                    let row_gap = style_lookup_len_full(
                        style,
                        "row-gap",
                        0.0,
                        my_font_size,
                        root_font_size,
                        vw_f,
                        vh_f,
                    )
                    .or_else(|| {
                        style_lookup_len_full(
                            style,
                            "gap",
                            0.0,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        )
                    })
                    .unwrap_or(0);
                    let column_gap = style_lookup_len_full(
                        style,
                        "column-gap",
                        0.0,
                        my_font_size,
                        root_font_size,
                        vw_f,
                        vh_f,
                    )
                    .or_else(|| {
                        style_lookup_len_full(
                            style,
                            "gap",
                            0.0,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        )
                    })
                    .unwrap_or(0);

                    // Collect flex children (exclude absolute/fixed, display:none)
                    struct FlexChild<'a> {
                        index: usize,
                        node: &'a VNode,
                        style: Option<&'a str>,
                    }
                    let mut flex_children: Vec<FlexChild> = Vec::new();
                    for (idx, c) in children.iter().enumerate() {
                        // Skip whitespace-only text nodes (generated by template indentation)
                        if let VNode::Text(t) = c
                            && t.chars().all(|c| c.is_whitespace())
                        {
                            continue;
                        }
                        let child_style = match c {
                            VNode::Element { props, .. } => {
                                props.attrs.get("style").map(|s| s.as_str())
                            }
                            _ => None,
                        };
                        let child_display = style_lookup_str(child_style, "display")
                            .unwrap_or_else(|| "block".to_string());
                        if child_display == "none" {
                            continue;
                        }
                        let position = style_lookup_str(child_style, "position")
                            .unwrap_or_else(|| "static".to_string());
                        if position == "absolute" || position == "fixed" {
                            // Out of flow: laid out but never a flex item. `fixed` is
                            // pinned to the viewport; `absolute` resolves against the
                            // container's own containing block. The actual offset
                            // resolution is deferred to the shared tail, once this
                            // element's own box is final.
                            let is_fixed = position == "fixed";
                            let out_cb = out_of_flow_containing_block(
                                is_fixed,
                                descendant_cb,
                                viewport_w,
                                viewport_h,
                            );
                            // Laid out at the flex container's content-box origin: for a
                            // flex container that is also the static position (CSS Flexbox
                            // §4.1 puts an out-of-flow child at the content-box start).
                            let child_ln = at(
                                c,
                                content_x_scrolled,
                                content_y_scrolled,
                                out_cb.w,
                                out_cb.h,
                                viewport_w,
                                viewport_h,
                                out_cb,
                                Some(idx),
                                root_font_size,
                                my_font_size,
                            );
                            abs_children.push(PendingAbsolute {
                                style: child_style.map(str::to_string),
                                is_fixed,
                                static_x: child_ln.rect.x,
                                static_y: child_ln.rect.y,
                                node: child_ln,
                            });
                            continue;
                        }
                        flex_children.push(FlexChild {
                            index: idx,
                            node: c,
                            style: child_style,
                        });
                    }

                    // Determine main/cross axis dimensions — check flex-flow shorthand first
                    let mut flex_dir_val = flex_dir.clone();
                    let mut flex_wrap_val = flex_wrap.clone();
                    if let Some(flow_raw) = style_lookup_str(style, "flex-flow") {
                        let tokens: Vec<String> = flow_raw
                            .split_whitespace()
                            .map(|s| s.to_ascii_lowercase())
                            .collect();
                        for tok in &tokens {
                            match tok.as_str() {
                                "row" | "row-reverse" | "column" | "column-reverse" => {
                                    flex_dir_val = tok.clone();
                                }
                                "nowrap" | "wrap" | "wrap-reverse" => {
                                    flex_wrap_val = tok.clone();
                                }
                                _ => {}
                            }
                        }
                    }
                    // also handle flex-direction / flex-wrap longhands overriding flow if they appear later —
                    // style_lookup_str already returns last occurrence, so respect that if different from defaults
                    // (we already parsed those above, but flex-flow may have set them; if individual exists, use it)
                    let has_dir = style.is_some_and(|s| {
                        s.split(';').any(|decl| {
                            if let Some((k, _)) = decl.split_once(':') {
                                k.trim() == "flex-direction"
                            } else {
                                false
                            }
                        })
                    });
                    let has_wrap = style.is_some_and(|s| {
                        s.split(';').any(|decl| {
                            if let Some((k, _)) = decl.split_once(':') {
                                k.trim() == "flex-wrap"
                            } else {
                                false
                            }
                        })
                    });
                    if has_dir {
                        flex_dir_val = flex_dir.clone();
                    }
                    if has_wrap {
                        flex_wrap_val = flex_wrap.clone();
                    }
                    let is_column = flex_dir_val == "column" || flex_dir_val == "column-reverse";
                    let is_reverse =
                        flex_dir_val == "row-reverse" || flex_dir_val == "column-reverse";
                    let is_wrap = flex_wrap_val == "wrap" || flex_wrap_val == "wrap-reverse";
                    let wrap_reverse = flex_wrap_val == "wrap-reverse";
                    let align_content = style_lookup_str(style, "align-content")
                        .unwrap_or_else(|| "stretch".to_string());

                    let main_size = if is_column {
                        content_h_available
                    } else {
                        content_w
                    };
                    // Cross size: use explicit container dimension if set, otherwise use available
                    let explicit_h_raw = style_lookup_len_full(
                        style,
                        "height",
                        avail_h as f32,
                        my_font_size,
                        root_font_size,
                        vw_f,
                        vh_f,
                    );
                    let explicit_h_content = explicit_h_raw.map(|v| {
                        if is_border_box {
                            (v - pt - pb - bt - bb).max(0)
                        } else {
                            v
                        }
                    });
                    let _explicit_w = style_lookup_len_full(
                        style,
                        "width",
                        avail_w as f32,
                        my_font_size,
                        root_font_size,
                        vw_f,
                        vh_f,
                    );

                    // Determine if flex container has a definite cross-size per CX-03/05
                    // For row flex: cross = height, definite only if resolved height present
                    // For column flex: cross = width, definite if parent cross definite OR resolved width present
                    let explicit_w_content = _explicit_w.map(|v| {
                        if is_border_box {
                            (v - pl - pr - bl - br).max(0)
                        } else {
                            v
                        }
                    });
                    let parent_definite = containing.w > 0;
                    let has_definite_cross_size = has_definite_cross(
                        is_column,
                        parent_definite,
                        if is_column {
                            explicit_w_content
                        } else {
                            explicit_h_content
                        },
                    );

                    // Initial cross_size: if definite, use it; if indefinite, start with 0 (will compute from children)
                    let cross_size = if is_column {
                        content_w
                    } else if has_definite_cross_size {
                        explicit_h_content.unwrap_or(content_h_available).max(0)
                    } else {
                        0 // indefinite - will be computed from children's natural sizes
                    };

                    // Step 1: Compute flex-basis for each child
                    //
                    // The three main sizes css-flexbox-1 §9.2 defines are three
                    // SEPARATE quantities, and this used to alias all of them
                    // into one `flex_basis` field that grow/shrink overwrote in
                    // place. That aliasing is what made correctness depend on
                    // every read site remembering to re-clamp: the clamping ones
                    // were right, the three that read the value after the
                    // if/else chain (the line's main size, the cursor advance,
                    // and the final `rect.w`) silently consumed whatever
                    // survived, including an unclamped base. Splitting the
                    // field makes each quantity a name you cannot read by
                    // accident.
                    struct FlexItem {
                        child_index: usize,
                        layout_node: Option<LayoutNode>,
                        /// §9.2 — the flex BASE size, NOT min/max clamped. §9.7.1
                        /// scales the shrink distribution by the base, so
                        /// clamping here would redistribute space between items.
                        flex_base_size: f32,
                        /// §9.2 — the base size clamped by min/max. This, not the
                        /// base, is what the line-breaking loop and the free
                        /// space computation consume.
                        hypothetical_main_size: f32,
                        /// §9.3 / §9.7.3 — the RESOLVED used main size: base →
                        /// grow/shrink → clamp. Written unconditionally at the
                        /// end of the resolve, so every later read gets the
                        /// clamped value without each site having to remember.
                        target_main_size: f32,
                        flex_grow: f32,
                        flex_shrink: f32,
                        align_self: String,
                        /// `None` = the property imposes no constraint. When
                        /// a declaration is present and explicit (a real
                        /// length, including `0`) that is the floor. When no
                        /// declaration is present, or the authored value is
                        /// the keyword `auto` (the property's initial value),
                        /// this holds the §4.5 automatic minimum — the
                        /// item's content-based minimum size — NOT zero and
                        /// NOT no constraint.
                        min_main_size: Option<f32>,
                        max_main_size: Option<f32>,
                        /// The size this item's OWN box came out of the measure
                        /// pass, on the main axis: `ln.rect.h` in a column,
                        /// `ln.rect.w` in a row.
                        ///
                        /// NOT the available size that was handed to `at()`.
                        /// Those differ whenever the item declared its own main
                        /// size, and gating on the available size would re-lay out
                        /// every fixed-width item in every row for nothing. This
                        /// is the size the item's DESCENDANTS were measured
                        /// against, which is what makes it the right thing to
                        /// compare the resolved size to.
                        pre_measure_main: f32,
                    }

                    // L-H4: order support — stable sort flex children by `order`
                    flex_children
                        .sort_by_key(|fc| style_lookup_i32(fc.style, "order").unwrap_or(0));

                    // Helper to parse `flex` shorthand per CSS spec
                    let parse_flex_shorthand =
                        |style: Option<&str>| -> Option<(f32, f32, Option<String>)> {
                            let raw = style_lookup_str(style, "flex")?;
                            let raw = raw.trim();
                            if raw.is_empty() {
                                return None;
                            }
                            if raw.eq_ignore_ascii_case("auto") {
                                Some((1.0, 1.0, None))
                            } else if raw.eq_ignore_ascii_case("none") {
                                Some((0.0, 0.0, None))
                            } else if raw.eq_ignore_ascii_case("initial") {
                                Some((0.0, 1.0, None))
                            } else {
                                let toks: Vec<&str> = raw.split_whitespace().collect();
                                match toks.len() {
                                    1 => {
                                        if let Ok(g) = toks[0].parse::<f32>() {
                                            if g.is_finite() {
                                                Some((g, 1.0, Some("0".to_string())))
                                            } else {
                                                Some((1.0, 1.0, Some(toks[0].to_string())))
                                            }
                                        } else {
                                            // single length basis e.g. "100px"
                                            Some((1.0, 1.0, Some(toks[0].to_string())))
                                        }
                                    }
                                    2 => {
                                        // either grow shrink or grow basis
                                        if let Ok(g) = toks[0].parse::<f32>() {
                                            if g.is_finite() {
                                                // second token: check if it's a number (shrink) or length
                                                if let Ok(s) = toks[1].parse::<f32>() {
                                                    if s.is_finite()
                                                        && !toks[1].contains('%')
                                                        && !toks[1].contains("px")
                                                        && !toks[1].contains("rem")
                                                        && !toks[1].contains("em")
                                                        && !toks[1].contains("vw")
                                                        && !toks[1].contains("vh")
                                                        && toks[1] != "auto"
                                                    {
                                                        Some((g, s, Some("0".to_string())))
                                                    } else {
                                                        Some((g, 1.0, Some(toks[1].to_string())))
                                                    }
                                                } else {
                                                    // otherwise basis
                                                    Some((g, 1.0, Some(toks[1].to_string())))
                                                }
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        }
                                    }
                                    3 => {
                                        let g = toks[0]
                                            .parse::<f32>()
                                            .ok()
                                            .filter(|v| v.is_finite())?;
                                        let s = toks[1]
                                            .parse::<f32>()
                                            .ok()
                                            .filter(|v| v.is_finite())?;
                                        Some((g, s, Some(toks[2].to_string())))
                                    }
                                    _ => None,
                                }
                            }
                        };

                    let mut items: Vec<FlexItem> = Vec::new();
                    for fc in &flex_children {
                        // Pre-layout child to get its natural size
                        let child_avail_main = if is_column {
                            content_h_available
                        } else {
                            main_size
                        };
                        // For indefinite cross-size, give children unconstrained cross-size so they lay out at natural size
                        let child_avail_cross = if is_column || has_definite_cross_size {
                            // column flex cross-size (width) is always definite; a
                            // row's is definite here when the container declared one
                            // (align-items: stretch will then apply to it).
                            cross_size as f32
                        } else {
                            UNCONSTRAINED_CROSS_SIZE // indefinite: unconstrained, children use natural size
                        };
                        // `at` takes (avail_w, avail_h) in the child's own axes, so
                        // main and cross have to be routed by direction. A ROW
                        // container's main axis is horizontal, so the MAIN size is
                        // the width and the CROSS size is the height; a COLUMN
                        // container is the other way round. Passing main and cross
                        // straight through regardless of direction is what handed a
                        // row item its container's cross (vertical) size as a width.
                        let (child_avail_w, child_avail_h) = if is_column {
                            (child_avail_cross, child_avail_main as f32)
                        } else {
                            (child_avail_main as f32, child_avail_cross)
                        };
                        // The basis ladder below reads only `fc.style`, `main_size`
                        // and the font sizes -- never the child's laid-out size --
                        // so it is safe, and much cheaper, to decide the basis
                        // BEFORE paying for a subtree layout. A content-basis item
                        // re-lays its subtree out at a wide probe width and then
                        // again at the clamped content width; the old unconditional
                        // pre-layout ran FIRST, so such an item cost three subtree
                        // layouts (3^depth across nested containers) for a value
                        // the content branch threw away. It now runs only on the
                        // `else` arm below, which is the sole place that reads it.
                        let flex_basis_val = style_lookup_len_full(
                            fc.style,
                            "flex-basis",
                            main_size as f32,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        );
                        // Check if child has explicit main size
                        let explicit_main = style_lookup_len_full(
                            fc.style,
                            if is_column { "height" } else { "width" },
                            main_size as f32,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        );
                        // Parse `flex` shorthand if present; it overrides flex-grow/shrink/basis specifics
                        let flex_shorthand = parse_flex_shorthand(fc.style);
                        let (flex_grow, flex_shrink, shorthand_basis): (f32, f32, Option<String>) =
                            if let Some((g, s, b)) = flex_shorthand.as_ref() {
                                (*g, *s, b.clone())
                            } else {
                                (
                                    style_lookup_str(fc.style, "flex-grow")
                                        .and_then(|v| {
                                            v.parse::<f32>()
                                                .ok()
                                                .map(|f| if f.is_finite() { f } else { 0.0 })
                                        })
                                        .unwrap_or(0.0),
                                    style_lookup_str(fc.style, "flex-shrink")
                                        .and_then(|v| {
                                            v.parse::<f32>()
                                                .ok()
                                                .map(|f| if f.is_finite() { f } else { 1.0 })
                                        })
                                        .unwrap_or(1.0),
                                    None,
                                )
                            };

                        // If flex shorthand present, also use it for basis unless longhand overrides
                        // For `flex: 1` => basis 0% (not auto); for `flex: auto` => auto; for shorthand_basis "auto" => auto.
                        let shorthand_basis_px: Option<f32> =
                            shorthand_basis.as_deref().and_then(|raw| {
                                if raw.eq_ignore_ascii_case("auto") {
                                    return None; // handled as auto below
                                }
                                if raw == "0" || raw == "0%" || raw == "0px" {
                                    // 0 can be unitless -> 0px, already 0
                                    return Some(0.0);
                                }
                                parse_length_value(
                                    raw,
                                    main_size as f32,
                                    my_font_size,
                                    root_font_size,
                                    (vw_f, vh_f),
                                )
                            });
                        let is_flex_one_zero = flex_shorthand.is_some()
                            && flex_grow == 1.0
                            && flex_shrink == 1.0
                            && shorthand_basis.is_none(); // flex: auto case handled else

                        // An auto-sized flex item's base size is its CONTENT
                        // size (CSS Flexbox §9.2.3), not the width it happens to
                        // fill. `at` lays a block out at whatever width it is
                        // given and a block with no declared `width` FILLS it,
                        // so measuring the item at the line's main size just
                        // reported the line's main size straight back. Every such
                        // item then claimed the whole row, the line summed to more
                        // than it had, and shrink took the space back out of the
                        // one item not allowed to shrink -- which is why a
                        // `flex: 1` sibling collapsed to zero while a plain
                        // sibling ate the row.
                        //
                        // The probe therefore has to produce `ln` BEFORE the basis
                        // ladder below, because that ladder's content arms read
                        // `ln.rect` and would otherwise read the fill.
                        //
                        // Re-measure at a probe wide enough that the content
                        // cannot wrap and read the content width back off, the
                        // same probe-then-clamp `lay_out_atomic` documents. The
                        // probe is capped at `i32::MAX / 4` and the result is
                        // clamped to the available main size, so this can neither
                        // overflow nor exceed the line.
                        let basis_is_content = shorthand_basis_px.is_none()
                            && flex_basis_val.is_none()
                            && explicit_main.is_none();
                        let ln = if basis_is_content {
                            let (pad_l, pad_r, pad_t, pad_b) = style_box_sides_full(
                                fc.style,
                                "padding",
                                main_size as f32,
                                my_font_size,
                                root_font_size,
                                vw_f,
                                vh_f,
                            );
                            let (bor_l, bor_r, bor_t, bor_b) = style_border_widths(
                                fc.style,
                                main_size as f32,
                                my_font_size,
                                root_font_size,
                                vw_f,
                                vh_f,
                            );
                            let inset = if is_column {
                                pad_t + pad_b + bor_t + bor_b
                            } else {
                                pad_l + pad_r + bor_l + bor_r
                            };
                            let keep_w = child_avail_w as i32;
                            let keep_h = child_avail_h as i32;
                            let probe = (main_size as i32)
                                .saturating_mul(4)
                                .max(4096)
                                .saturating_add(inset)
                                .min(i32::MAX / 4);
                            let probe_layout = at(
                                fc.node,
                                0,
                                0,
                                if is_column { keep_w } else { probe },
                                if is_column { probe } else { keep_h },
                                viewport_w,
                                viewport_h,
                                ContainingBlock {
                                    x: content_x,
                                    y: content_y_start,
                                    w: content_w,
                                    h: content_h_available,
                                },
                                Some(fc.index),
                                root_font_size,
                                my_font_size,
                            );
                            let target = max_content_width(&probe_layout)
                                .min(main_size as i32)
                                .saturating_add(inset)
                                .max(0);
                            at(
                                fc.node,
                                0,
                                0,
                                if is_column { keep_w } else { target },
                                if is_column { target } else { keep_h },
                                viewport_w,
                                viewport_h,
                                ContainingBlock {
                                    x: content_x,
                                    y: content_y_start,
                                    w: content_w,
                                    h: content_h_available,
                                },
                                Some(fc.index),
                                root_font_size,
                                my_font_size,
                            )
                        } else {
                            // Not content-basis: the basis ladder wants the child's
                            // laid-out size, so lay it out ONCE, here. Laid out at
                            // the origin: the position is assigned in the placement
                            // pass below, and the measure pass needs a definite
                            // offset base rather than a guess.
                            at(
                                fc.node,
                                0,
                                0,
                                child_avail_w as i32,
                                child_avail_h as i32,
                                viewport_w,
                                viewport_h,
                                ContainingBlock {
                                    x: content_x,
                                    y: content_y_start,
                                    w: content_w,
                                    h: content_h_available,
                                },
                                Some(fc.index),
                                root_font_size,
                                my_font_size,
                            )
                        };
                        // Determine effective basis:
                        // - shorthand_basis_px if explicit
                        // - else flex_basis_val longhand
                        // - else explicit_main
                        // - else auto vs 0% depending on shorthand semantics
                        let flex_basis = if let Some(v) = shorthand_basis_px {
                            v
                        } else if shorthand_basis
                            .as_deref()
                            .map(|s| s.eq_ignore_ascii_case("auto"))
                            .unwrap_or(false)
                            && is_flex_one_zero
                        {
                            // auto => content size
                            if is_column {
                                ln.rect.h as f32
                            } else {
                                ln.rect.w as f32
                            }
                        } else if flex_shorthand.is_some()
                            && shorthand_basis.as_deref() == Some("0")
                        {
                            0.0
                        } else if let Some(fb) = flex_basis_val {
                            fb as f32
                        } else if let Some(exp) = explicit_main {
                            // If flex shorthand existed and gave implicit 0 basis, ignore explicit width for basis 0 case
                            if flex_shorthand.is_some() && shorthand_basis == Some("0".to_string())
                            {
                                0.0
                            } else {
                                exp as f32
                            }
                        } else if let Some(raw) = flex_shorthand.as_ref().map(|_| {
                            style_lookup_str(fc.style, "flex")
                                .unwrap_or_default()
                                .trim()
                                .to_ascii_lowercase()
                        }) {
                            // flex shorthand present without basis token => 0%
                            // unless it's `auto` / `none` / `initial` handled above
                            //
                            // The content keywords can also arrive as the THIRD
                            // token (`flex: 0 0 auto`), so compare against the
                            // last whitespace-separated token rather than the
                            // whole declaration. `flex: 0 0 auto` and
                            // `flex: 0 0 content` used to fall into the `else`
                            // below and get a 0px base size.
                            let basis_token =
                                raw.split_whitespace().next_back().unwrap_or_default();
                            if matches!(
                                basis_token,
                                "auto"
                                    | "none"
                                    | "initial"
                                    | "content"
                                    | "max-content"
                                    | "min-content"
                                    | "fit-content"
                            ) {
                                if is_column {
                                    ln.rect.h as f32
                                } else {
                                    ln.rect.w as f32
                                }
                            } else {
                                0.0
                            }
                        } else if flex_grow > 0.0 {
                            0.0
                        } else if is_column {
                            ln.rect.h as f32
                        } else {
                            ln.rect.w as f32
                        };

                        // §4.5 — the automatic minimum size. The authored
                        // `min-width`/`min-height` is the floor only when it
                        // resolves to a real length. When it is absent, or the
                        // keyword `auto` (the initial value), the floor is the
                        // item's content-based minimum instead. An explicit
                        // length — including `min-width: 0` — REPLACES that
                        // floor; that is what makes `min-width: 0` the standard
                        // escape hatch. `min-width: 0` and no `min-width` are
                        // therefore distinguishable in this code path even
                        // when both happen to floor at 0.
                        //
                        // Parsing note: `parse_length_value` returns `None` for
                        // the keyword `auto`, and this branch keys the
                        // automatic floor off exactly that `None` result for
                        // an authored `auto` — if the parser ever started
                        // returning `Some(0.0)` for `auto`, this branch would
                        // stop firing and the floor would silently collapse to
                        // the escape-hatch value. The distinguishing test
                        // pins that.
                        let min_key = if is_column { "min-height" } else { "min-width" };
                        let raw_min = style_lookup_str(fc.style, min_key);
                        let parsed_min = style_lookup_len_full(
                            fc.style,
                            min_key,
                            main_size as f32,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        )
                        .filter(|&v| v >= 0)
                        .map(|v| v as f32);
                        // The content-based minimum, §4.5. Velox has no
                        // min-content helper, so this is the narrow probe
                        // analogue of the content-basis descent above: force
                        // the content to wrap at every opportunity, then
                        // measure the widest piece with `max_content_width`.
                        // For a row item that is the longest unbreakable run
                        // (e.g. the longest word); for a column item there is
                        // nothing to wrap on the main axis, so the content
                        // height of the item's own pre-measure is the floor.
                        // APPROXIMATION, documented: children whose own
                        // sizing clamps to the probe (e.g. an inline-block
                        // that shrink-wraps at the probe width) under-report.
                        let flex_auto_min_main = || {
                            if is_column {
                                ln.rect.h as f32
                            } else {
                                let (pad_l, pad_r, _, _) = style_box_sides_full(
                                    fc.style,
                                    "padding",
                                    main_size as f32,
                                    my_font_size,
                                    root_font_size,
                                    vw_f,
                                    vh_f,
                                );
                                let (bor_l, bor_r, _, _) = style_border_widths(
                                    fc.style,
                                    main_size as f32,
                                    my_font_size,
                                    root_font_size,
                                    vw_f,
                                    vh_f,
                                );
                                let inset = pad_l + pad_r + bor_l + bor_r;
                                let narrow = at(
                                    fc.node,
                                    0,
                                    0,
                                    1,
                                    child_avail_h as i32,
                                    viewport_w,
                                    viewport_h,
                                    ContainingBlock {
                                        x: content_x,
                                        y: content_y_start,
                                        w: content_w,
                                        h: content_h_available,
                                    },
                                    Some(fc.index),
                                    root_font_size,
                                    my_font_size,
                                );
                                max_content_width(&narrow) as f32 + inset as f32
                            }
                        };
                        let min_main: Option<f32> = match (raw_min.as_deref(), parsed_min) {
                            // No declaration: the property's initial value is
                            // `auto`, so the automatic floor applies.
                            (None, _) => Some(flex_auto_min_main()),
                            // An authored `auto`: same floor, keyed off the
                            // parser's `None` for that keyword.
                            (Some(raw), None) if raw.trim().eq_ignore_ascii_case("auto") => {
                                Some(flex_auto_min_main())
                            }
                            // Explicit length (including 0) or invalid: the
                            // authored value is the floor, or no constraint.
                            (Some(_), parsed) => parsed,
                        };
                        let max_main = style_lookup_len_full(
                            fc.style,
                            if is_column { "max-height" } else { "max-width" },
                            main_size as f32,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        )
                        .filter(|&v| v >= 0)
                        .map(|v| v as f32);

                        // §9.2: the hypothetical main size is the base size
                        // CLAMPED by min/max. It is what the line-breaking loop
                        // and the free-space sum consume — NOT what the item
                        // finally gets, which is the target.
                        let hypothetical_main_size =
                            clamp_to_min_max(flex_basis, min_main, max_main);
                        // The size this item's own box came out of the measure
                        // pass, on the MAIN axis. Read here, before `ln` moves
                        // into the item, because it is what the placement pass
                        // compares the resolved size against.
                        let pre_measure_main = if is_column { ln.rect.h } else { ln.rect.w } as f32;
                        items.push(FlexItem {
                            child_index: fc.index,
                            layout_node: Some(ln),
                            flex_base_size: flex_basis,
                            hypothetical_main_size,
                            // §9.7.3 starts the target at the BASE, not at the
                            // hypothetical, and grow/shrink distribute from there.
                            // The clamp that makes the target spec-legal is the
                            // UNCONDITIONAL pass after the grow/shrink chain
                            // below, not here — starting the target at the
                            // already-clamped hypothetical would fix the
                            // degenerate branches here instead, but that pass
                            // would then be provably dead code, and it would
                            // silently change what every growing item starts
                            // from. Keeping the base here makes the pass the
                            // load-bearing fix and leaves the grow/shrink
                            // arithmetic identical to what it was.
                            target_main_size: flex_basis,
                            flex_grow,
                            flex_shrink,
                            align_self: style_lookup_str(fc.style, "align-self")
                                .unwrap_or_else(|| "auto".to_string()),
                            min_main_size: min_main,
                            max_main_size: max_main,
                            pre_measure_main,
                        });
                    }

                    // Step 2: Line breaking (flex-wrap)
                    struct FlexLine {
                        items: Vec<usize>, // indices into items
                        main_size: f32,
                        cross_size: f32,
                        main_positions: Vec<(usize, f32)>, // (item_idx, main_pos)
                    }
                    let mut lines: Vec<FlexLine> = Vec::new();

                    if !is_wrap {
                        // Single line - all items
                        lines.push(FlexLine {
                            items: (0..items.len()).collect(),
                            main_size: 0.0,
                            cross_size: 0.0,
                            main_positions: Vec::new(),
                        });
                    } else {
                        let mut current_line = FlexLine {
                            items: Vec::new(),
                            main_size: 0.0,
                            cross_size: 0.0,
                            main_positions: Vec::new(),
                        };
                        #[allow(clippy::needless_range_loop)]
                        for i in 0..items.len() {
                            let item_size = items[i].hypothetical_main_size;
                            let gap_to_add = if current_line.items.is_empty() {
                                0.0
                            } else {
                                (if is_column { row_gap } else { column_gap }) as f32
                            };
                            if !current_line.items.is_empty()
                                && current_line.main_size + gap_to_add + item_size
                                    > main_size as f32
                            {
                                // Start new line
                                lines.push(current_line);
                                current_line = FlexLine {
                                    items: vec![i],
                                    main_size: item_size,
                                    cross_size: 0.0,
                                    main_positions: Vec::new(),
                                };
                            } else {
                                current_line.items.push(i);
                                current_line.main_size += item_size + gap_to_add;
                            }
                        }
                        if !current_line.items.is_empty() {
                            lines.push(current_line);
                        }
                    }
                    if lines.is_empty() && !items.is_empty() {
                        lines.push(FlexLine {
                            items: (0..items.len()).collect(),
                            main_size: 0.0,
                            cross_size: 0.0,
                            main_positions: Vec::new(),
                        });
                    }

                    // Step 3: For each line, distribute flex & compute line cross sizes (pre-positioning)
                    // L-C1: wrap-reverse support; L-C2: track cross_offset not main_offset; L-M5: stretch & align-content
                    let cross_gap = (if is_column { column_gap } else { row_gap }) as f32;
                    let main_start = if is_column { pt as f32 } else { pl as f32 };
                    // Pre-compute reversal by reversing lines order; later we restore DOM order via sort
                    if wrap_reverse {
                        lines.reverse();
                    }
                    // Two-pass flex layout: pass 1 distributes flex + justify + computes per-line cross sizes
                    let pre_lines_len = lines.len();
                    for line in &mut lines {
                        let total_gap = if line.items.len() > 1 {
                            (line.items.len() as i32 - 1) as f32
                                * if is_column {
                                    row_gap as f32
                                } else {
                                    column_gap as f32
                                }
                        } else {
                            0.0
                        };
                        // §9.7.3 resolve loop, with the freeze-and-redistribute
                        // step the spec attaches to a target that gets clamped
                        // by min/max. Velox previously had the clamp but not
                        // the re-run, which is why adding §4.5 floors would
                        // otherwise leave the line under- or over-filled by
                        // exactly the floored amount.
                        //
                        // The free space is measured against the BASE sizes
                        // (the distribute target starts at the base, §9.7.3):
                        // with a floor in play the hypothetical exceeds the
                        // base by the floor amount, so a hypothetical-based
                        // free space would feed that floor to the grow
                        // distribution and the line would under-fill by it.
                        // With no floors the two sums are identical, so this
                        // changes nothing on previously-floored-less trees.
                        let mut frozen = vec![false; line.items.len()];
                        loop {
                            let frozen_sum: f32 = line
                                .items
                                .iter()
                                .enumerate()
                                .filter(|(pos, _)| frozen[*pos])
                                .map(|(_, &i)| items[i].target_main_size)
                                .sum();
                            let unfrozen_base: f32 = line
                                .items
                                .iter()
                                .enumerate()
                                .filter(|(pos, _)| !frozen[*pos])
                                .map(|(_, &i)| items[i].flex_base_size)
                                .sum();
                            let capacity = main_size as f32 - total_gap - frozen_sum;
                            let free_space = capacity - unfrozen_base;
                            // Every unfrozen item restarts from its base, per
                            // the spec's re-run with frozen items pinned.
                            for (pos, &i) in line.items.iter().enumerate() {
                                if !frozen[pos] {
                                    items[i].target_main_size = items[i].flex_base_size;
                                }
                            }
                            if free_space > 0.0 {
                                let total_grow: f32 = line
                                    .items
                                    .iter()
                                    .enumerate()
                                    .filter(|(pos, _)| !frozen[*pos])
                                    .map(|(_, &i)| items[i].flex_grow)
                                    .sum();
                                if total_grow > 0.0 {
                                    for (pos, &i) in line.items.iter().enumerate() {
                                        if !frozen[pos] {
                                            let grow_amount =
                                                (items[i].flex_grow / total_grow) * free_space;
                                            items[i].target_main_size += grow_amount;
                                        }
                                    }
                                }
                            } else if free_space < 0.0 {
                                // §9.7.1: shrink distributes proportional to
                                // the BASE, never the partially-updated
                                // target, so the loop stays order-independent.
                                let total_shrink: f32 = line
                                    .items
                                    .iter()
                                    .enumerate()
                                    .filter(|(pos, _)| !frozen[*pos])
                                    .map(|(_, &i)| items[i].flex_shrink * items[i].flex_base_size)
                                    .sum();
                                if total_shrink > 0.0 {
                                    for (pos, &i) in line.items.iter().enumerate() {
                                        if !frozen[pos] {
                                            let shrink_factor = items[i].flex_shrink
                                                * items[i].flex_base_size
                                                / total_shrink;
                                            let shrink_amount = shrink_factor * free_space.abs();
                                            items[i].target_main_size -= shrink_amount;
                                        }
                                    }
                                }
                            }
                            // THE FIX, kept from before: the target is
                            // clamped UNCONDITIONALLY, even when no grow or
                            // shrink branch ran (free_space == 0, or the
                            // relevant factor sums to 0). Otherwise an
                            // authored `min-width` would silently not apply on
                            // those degenerate paths.
                            let mut newly_frozen = false;
                            for (pos, &i) in line.items.iter().enumerate() {
                                if frozen[pos] {
                                    continue;
                                }
                                let before = items[i].target_main_size;
                                let after = clamp_to_min_max(
                                    before,
                                    items[i].min_main_size,
                                    items[i].max_main_size,
                                );
                                items[i].target_main_size = after;
                                if after != before {
                                    frozen[pos] = true;
                                    newly_frozen = true;
                                }
                            }
                            if !newly_frozen {
                                break;
                            }
                        }
                        line.main_size = line
                            .items
                            .iter()
                            .map(|&i| items[i].target_main_size)
                            .sum::<f32>()
                            + total_gap;
                        if line.items.is_empty() {
                            line.main_positions = Vec::new();
                            line.cross_size = 0.0;
                            continue;
                        }
                        let effective_main = line.main_size;
                        let extra_space = (main_size as f32 - effective_main).max(0.0);
                        let gap_val = if is_column { row_gap } else { column_gap };
                        let n = line.items.len();
                        let per_gap_extra: f32 = match justify_content.as_str() {
                            "space-between" => {
                                if n > 1 {
                                    extra_space / (n as f32 - 1.0)
                                } else {
                                    0.0
                                }
                            }
                            "space-around" => {
                                if n > 0 {
                                    extra_space / n as f32
                                } else {
                                    0.0
                                }
                            }
                            "space-evenly" => {
                                if n > 0 {
                                    extra_space / (n as f32 + 1.0)
                                } else {
                                    0.0
                                }
                            }
                            _ => 0.0,
                        };
                        #[allow(unused_assignments)]
                        let mut cursor = main_start;
                        cursor = match justify_content.as_str() {
                            "flex-start" | "start" => main_start,
                            "flex-end" | "end" => main_start + extra_space,
                            "center" => main_start + extra_space / 2.0,
                            "space-between" => main_start,
                            "space-around" => main_start + per_gap_extra / 2.0,
                            "space-evenly" => main_start + per_gap_extra,
                            _ => main_start,
                        };
                        let mut mpos: Vec<(usize, f32)> = Vec::new();
                        let extra_for_gap = match justify_content.as_str() {
                            "space-between" | "space-around" | "space-evenly" => per_gap_extra,
                            _ => 0.0,
                        };
                        for &item_idx in &line.items {
                            let pos = cursor;
                            mpos.push((item_idx, pos));
                            cursor = pos
                                + items[item_idx].target_main_size
                                + gap_val as f32
                                + extra_for_gap;
                        }
                        line.main_positions = mpos;
                        // Compute line cross size
                        let mut max_cross_size: f32 = 0.0;
                        for &(item_idx, _) in &line.main_positions {
                            if let Some(ref ln) = items[item_idx].layout_node {
                                let cross_sz = if is_column {
                                    ln.rect.w as f32
                                } else {
                                    ln.rect.h as f32
                                };
                                if cross_sz > max_cross_size {
                                    max_cross_size = cross_sz;
                                }
                            }
                        }
                        // L-M5: stretch — only applies when cross-size is DEFINITE
                        // When cross-size is indefinite (fit-content), stretch behaves as flex-start
                        let effective_align_items = align_items.clone();
                        let is_single_line = pre_lines_len == 1;
                        let can_stretch =
                            has_definite_cross_size && effective_align_items == "stretch";
                        if can_stretch {
                            let stretch_target = if is_single_line {
                                cross_size as f32
                            } else {
                                max_cross_size
                            };
                            for &(item_idx, _) in &line.main_positions {
                                let child_style = flex_children
                                    .iter()
                                    .find(|fc| fc.index == items[item_idx].child_index)
                                    .map(|fc| fc.style)
                                    .unwrap_or(None);
                                let explicit_cross = style_lookup_len_full(
                                    child_style,
                                    if is_column { "width" } else { "height" },
                                    cross_size as f32,
                                    my_font_size,
                                    root_font_size,
                                    vw_f,
                                    vh_f,
                                );
                                // baseline fallback: treat as flex-start, so don't stretch baseline items
                                let is_baseline = items[item_idx].align_self == "baseline"
                                    || (items[item_idx].align_self == "auto"
                                        && effective_align_items == "baseline");
                                if explicit_cross.is_none()
                                    && items[item_idx].align_self == "auto"
                                    && !is_baseline
                                    && let Some(ref mut ln) = items[item_idx].layout_node
                                {
                                    if is_column {
                                        ln.rect.w = stretch_target as i32;
                                    } else {
                                        ln.rect.h = stretch_target as i32;
                                    }
                                }
                            }
                            if is_single_line {
                                max_cross_size = max_cross_size.max(cross_size as f32);
                            }
                        }
                        // For indefinite cross-size, line.cross_size = max_cross_size (natural size)
                        // For definite cross-size with stretch, line.cross_size may be stretched to cross_size
                        line.cross_size = max_cross_size;
                    }
                    // Compute align-content distribution for multi-line
                    let total_cross: f32 = if lines.is_empty() {
                        0.0
                    } else {
                        lines.iter().map(|l| l.cross_size).sum::<f32>()
                            + cross_gap * (lines.len() as f32 - 1.0)
                    };
                    // For indefinite cross-size (fit-content), there's no free space to distribute
                    let free_cross = if has_definite_cross_size {
                        (cross_size as f32 - total_cross).max(0.0)
                    } else {
                        0.0
                    };
                    let n_lines = lines.len();
                    let mut cross_start: f32 = 0.0;
                    let mut cross_extra_per_gap: f32 = 0.0;
                    #[allow(unused_assignments)]
                    let mut cross_line_extra: f32 = 0.0;
                    match align_content.as_str() {
                        "flex-start" | "start" => cross_start = 0.0,
                        "flex-end" | "end" => cross_start = free_cross,
                        "center" => cross_start = free_cross / 2.0,
                        "space-between" => {
                            if n_lines > 1 {
                                cross_extra_per_gap = free_cross / (n_lines as f32 - 1.0);
                            }
                        }
                        "space-around" => {
                            if n_lines > 0 {
                                cross_extra_per_gap = free_cross / n_lines as f32;
                                cross_start = cross_extra_per_gap / 2.0;
                            }
                        }
                        "space-evenly" => {
                            if n_lines > 0 {
                                cross_extra_per_gap = free_cross / (n_lines as f32 + 1.0);
                                cross_start = cross_extra_per_gap;
                            }
                        }
                        "stretch" => {
                            if has_definite_cross_size && n_lines > 1 && free_cross > 0.0 {
                                cross_line_extra = free_cross / n_lines as f32;
                                for line in &mut lines {
                                    line.cross_size += cross_line_extra;
                                }
                                // Re-stretch items that were stretch to new line size
                                if align_items == "stretch" {
                                    for line in &lines {
                                        for &(item_idx, _) in &line.main_positions {
                                            if items[item_idx].align_self == "auto"
                                                && let Some(ref mut ln) =
                                                    items[item_idx].layout_node
                                            {
                                                if !is_column {
                                                    ln.rect.h = line.cross_size as i32;
                                                } else {
                                                    ln.rect.w = line.cross_size as i32;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    // For indefinite cross-size, update cross_size to the computed total_cross
                    // so that the flex container's layout node gets the correct height
                    let final_cross_size = if has_definite_cross_size {
                        cross_size as f32
                    } else {
                        total_cross
                    };
                    // Second pass: position items with computed cross offsets
                    let mut cross_offset: f32 = cross_start;
                    let single_line_mode = n_lines == 1;
                    for line in &lines {
                        for &(item_idx, main_pos) in &line.main_positions {
                            if let Some(mut ln) = items[item_idx].layout_node.take() {
                                // The resolved target main size IS the item's used
                                // main size. Gating this write on
                                // `grow > 0 || shrink > 0` meant `flex: 0 0 20px`
                                // and `flex: none` — both grow 0, shrink 0 — kept
                                // whatever `at()` measured instead, which for a
                                // width-less block child is the full available
                                // width. The basis was computed correctly and then
                                // thrown away. Grow and shrink write
                                // `target_main_size` in the pass above, so writing
                                // it unconditionally is what lands those too.
                                let fb = items[item_idx].target_main_size.round() as i32;
                                // Look the item's own child record up ONCE: the
                                // re-layout below needs its `node`, and
                                // `max-width` below needs its `style`.
                                let item_fc = flex_children
                                    .iter()
                                    .find(|fc| fc.index == items[item_idx].child_index);
                                let item_style = item_fc.and_then(|fc| fc.style);
                                // RE-LAY OUT A RESIZED ITEM'S SUBTREE.
                                //
                                // The measure pass laid every item out ONCE, at the
                                // container's full main size, and the placement pass
                                // used to resize the item's OWN box and then call
                                // `translate_layout_subtree`, which only adds `dx`/`dy`
                                // to every rect and clip. It never re-measures. So
                                // when the resolved main size differs from the size
                                // the item was measured at, the item's box became
                                // correct while its DESCENDANTS kept the old size --
                                // a subtree that is merely moved, never resized. The
                                // composer is that bug: `.field` is
                                // `flex: 1 1 auto` and its only child is
                                // `width: 100%`, so the input was measured at the
                                // row's full 556 px, `.field` shrank to 480, and the
                                // input kept spanning 556 -- overhanging `.field` by
                                // 76 px, the Add button's 68 px plus the 8 px gap,
                                // at EVERY window width. The Add button then paints
                                // over the input's right 68 px, because paint order
                                // is DOM order.
                                //
                                // GATED ON THE DELTA, and this is load-bearing, not
                                // an optimisation. The re-layout is a whole extra
                                // `at()` descent, and a second descent per flex item
                                // is exactly how layout became `2^depth` (and `3^depth`
                                // before tasks 2.1a/2.1c removed one). The no-change
                                // case is the common case and pays nothing: the gate
                                // is one float compare, and it is compared against
                                // `pre_measure_main`, the size the item's own box
                                // actually came out of `at()`, which is the size its
                                // descendants were laid out against.
                                //
                                // The gate compares ROUNDED integer sizes because
                                // that is the resolution the boxes are written at: a
                                // sub-pixel delta cannot move a rect.
                                let pre_measure_main =
                                    items[item_idx].pre_measure_main.round() as i32;
                                if fb != pre_measure_main
                                    && let Some(fc) = item_fc
                                {
                                    // The CROSS axis is already final -- main size
                                    // from the flex algorithm, cross size from the
                                    // stretch pass above -- so it is handed through
                                    // as measured and never re-derived.
                                    //
                                    // The containing block's MAIN extent is the
                                    // resolved size rather than the container's
                                    // content box. `at()` resolves the subtree root's
                                    // own declared main size against `containing`,
                                    // and for a flex item that declaration is
                                    // overridden by the flex algorithm, so handing
                                    // back the container's content box would let the
                                    // stale size win the second time round -- which
                                    // is how a column item declaring `height: 100%`
                                    // kept a 556 px child inside a 480 px item.
                                    let (rl_w, rl_h, rl_cb) = if is_column {
                                        (
                                            ln.rect.w,
                                            fb,
                                            ContainingBlock {
                                                x: content_x,
                                                y: content_y_start,
                                                w: content_w,
                                                h: fb,
                                            },
                                        )
                                    } else {
                                        (
                                            fb,
                                            ln.rect.h,
                                            ContainingBlock {
                                                x: content_x,
                                                y: content_y_start,
                                                w: fb,
                                                h: content_h_available,
                                            },
                                        )
                                    };
                                    ln = at(
                                        fc.node,
                                        0,
                                        0,
                                        rl_w,
                                        rl_h,
                                        viewport_w,
                                        viewport_h,
                                        rl_cb,
                                        Some(fc.index),
                                        root_font_size,
                                        my_font_size,
                                    );
                                    // The cross-axis size is FINAL before this
                                    // point -- `align-items: stretch` wrote it onto
                                    // the item's own box in the pass above, and
                                    // nothing after here changes it -- but `at()`
                                    // derives a block's cross size from its
                                    // content and cannot know a stretch is
                                    // coming. So the fresh node comes back with
                                    // the CONTENT cross size and the stretch is
                                    // silently dropped: a stretched `.field` came
                                    // back 40 px tall inside a 48 px row.
                                    // Writing it back restores it, and the
                                    // subtree is unaffected either way because it
                                    // never saw the stretch: the pre-layout that
                                    // produced the node being replaced had not been
                                    // stretched when it laid its own children out
                                    // either. Main-axis re-measure only; the cross
                                    // axis is carried across untouched.
                                    if is_column {
                                        ln.rect.w = rl_w;
                                    } else {
                                        ln.rect.h = rl_h;
                                    }
                                }
                                // Read the translate source AFTER the re-layout:
                                // the fresh node was laid out at the origin, so its
                                // rect is the new `pre_x`/`pre_y` and reusing the
                                // discarded node's would double-count the offset.
                                let pre_x = ln.rect.x;
                                let pre_y = ln.rect.y;
                                if is_column {
                                    ln.rect.h = fb;
                                } else {
                                    ln.rect.w = fb;
                                }
                                ln.rect.w = clamp_width_to_max_width(
                                    ln.rect.w,
                                    item_style,
                                    content_w as f32,
                                    my_font_size,
                                    root_font_size,
                                    vw_f,
                                    vh_f,
                                );
                                let item_align = if items[item_idx].align_self == "auto" {
                                    align_items.clone()
                                } else {
                                    items[item_idx].align_self.clone()
                                };
                                let item_cross_size = if is_column {
                                    ln.rect.w as f32
                                } else {
                                    ln.rect.h as f32
                                };
                                // baseline fallback to flex-start
                                let resolved_align = if item_align == "baseline" {
                                    "flex-start"
                                } else {
                                    item_align.as_str()
                                };
                                let effective_cross = if single_line_mode {
                                    final_cross_size
                                } else {
                                    line.cross_size
                                };
                                let cross_pos = match resolved_align {
                                    "flex-end" | "end" => {
                                        (effective_cross - item_cross_size).max(0.0)
                                    }
                                    "center" => (effective_cross - item_cross_size) / 2.0,
                                    _ => 0.0,
                                };
                                let line_cross = cross_offset + cross_pos;
                                let (mut resolved_x, mut resolved_y) = if is_column {
                                    (
                                        (content_x_scrolled as f32 + line_cross).round() as i32,
                                        (content_y_scrolled as f32 + (main_pos - main_start))
                                            .round() as i32,
                                    )
                                } else {
                                    (
                                        (content_x_scrolled as f32 + (main_pos - main_start))
                                            .round() as i32,
                                        (content_y_scrolled as f32 + line_cross).round() as i32,
                                    )
                                };
                                if is_reverse {
                                    if is_column {
                                        let container_bottom =
                                            content_y_start + content_h_available;
                                        resolved_y = container_bottom - resolved_y - ln.rect.h;
                                    } else {
                                        let container_right = content_x_scrolled + content_w;
                                        resolved_x = container_right - resolved_x - ln.rect.w;
                                    }
                                }
                                translate_layout_subtree(
                                    &mut ln,
                                    resolved_x - pre_x,
                                    resolved_y - pre_y,
                                );
                                ln.rect.x = resolved_x;
                                ln.rect.y = resolved_y;
                                let child_style = item_style;
                                // KNOWN DEFECT, tracked separately.
                                //
                                // SYMPTOM: an absolutely positioned descendant of a
                                // `position: relative` flex item lands at the wrong offset
                                // in the flex path.
                                //
                                // The MECHANISM IS NOT ESTABLISHED. What IS established is that
                                // the item's descendants are ALREADY re-displaced by the item's
                                // own displacement, so an earlier account of this comment — that
                                // `apply_relative_position` leaves the descendants behind and they
                                // land at twice the offset — cannot be the cause, and an earlier
                                // LEAD proposing that descendants be re-displaced with the
                                // relative offset described code that is already here.
                                // `apply_relative_position` mutates only `node.rect`, and the
                                // `translate_layout_descendants` call a few lines below this
                                // comment re-displaces the whole subtree by exactly the distance
                                // this call moved the item by, so descendants and item end up
                                // moved together. That call site is the only one in the file: the
                                // block path's `apply_relative_position` call, further down, has no
                                // matching descendant pass, which is why the two paths can differ
                                // at all.
                                //
                                // The remaining suspect is the out-of-flow pass, which resolves
                                // `PendingAbsolute` entries after every child has been laid out
                                // and therefore runs after the displacement above. Establish the
                                // mechanism before changing anything here.
                                apply_relative_position(
                                    child_style,
                                    &mut ln,
                                    content_w,
                                    content_h_available,
                                    my_font_size,
                                    root_font_size,
                                    vw_f,
                                    vh_f,
                                );
                                apply_sticky_position(
                                    child_style,
                                    &mut ln,
                                    content_x,
                                    content_y_start,
                                    content_w,
                                    content_h_available,
                                    scroll_x,
                                    scroll_y,
                                    my_font_size,
                                    root_font_size,
                                    vw_f,
                                    vh_f,
                                );
                                let descendant_dx = ln.rect.x - resolved_x;
                                let descendant_dy = ln.rect.y - resolved_y;
                                translate_layout_descendants(&mut ln, descendant_dx, descendant_dy);
                                max_y_end = max_y_end.max(ln.rect.y + ln.rect.h);
                                laid_children.push(ln);
                            }
                        }
                        cross_offset += line.cross_size + cross_gap + cross_extra_per_gap;
                    }
                    if wrap_reverse {
                        laid_children.sort_by_key(|n| n.source_index.unwrap_or(usize::MAX));
                    }

                    // Handle reverse main axis ordering for multi-line
                    if is_reverse && lines.len() > 1 {
                        // Already handled above per-item
                    }
                } else {
                    // block with inline text flow
                    let mut cur_x = content_x_scrolled;
                    let mut cur_y = content_y_scrolled;
                    let mut last_bottom_margin = 0;
                    // The inline run's own settings, read once: every text child
                    // and every inline descendant inherits them unless it sets its
                    // own. `WhiteSpace::parse` is the single definition of the
                    // keyword mapping, not a second hand-rolled match.
                    let container_font_size =
                        style_lookup_font_size(style, my_font_size, root_font_size, (vw_f, vh_f))
                            .unwrap_or(my_font_size);
                    let container_font_family = style_lookup_str(style, "font-family")
                        .unwrap_or_else(|| DEFAULT_TEXT_FAMILY.to_string());
                    let container_align = inline_vertical_align(style, VerticalAlign::Baseline);
                    let container_text_align = style_lookup_str(style, "text-align")
                        .map(|v| v.trim().to_ascii_lowercase())
                        .unwrap_or_else(|| "left".to_string());
                    let container_ws = style_lookup_str(style, "white-space")
                        .and_then(|v| WhiteSpace::parse(&v))
                        .unwrap_or_default();
                    let container_text_overflow = style_lookup_str(style, "text-overflow")
                        .and_then(|v| TextOverflow::parse(&v))
                        .unwrap_or_default();
                    let mut inline_run: Vec<InlineRunItem<'_>> = Vec::new();
                    for (idx, c) in children.iter().enumerate() {
                        let is_text = matches!(c, VNode::Text(_));
                        if should_drop_collapsible_whitespace(children, idx, style) {
                            continue;
                        }
                        let child_style = match c {
                            VNode::Element { props, .. } => {
                                props.attrs.get("style").map(|s| s.as_str())
                            }
                            _ => None,
                        };
                        let child_display = style_lookup_str(child_style, "display")
                            .unwrap_or_else(|| "block".to_string());
                        if child_display == "none" {
                            continue;
                        }
                        // Inline-level content is COLLECTED into a run and laid out
                        // by the inline formatting context, which is what decides
                        // where lines break -- including across the edge of an
                        // inline element. The run is flushed before anything that
                        // is not inline-level, so `cur_x` and an
                        // out-of-flow child's static position all see the real end
                        // of the text that precedes them.
                        //
                        // The run is flushed only when the child is NOT part of
                        // it. Flushing unconditionally would give every inline
                        // child a line box of its own, which is the flattening
                        // requirement 2 exists to prevent.
                        if !is_inline_run_member(c) && !inline_run.is_empty() {
                            let flushed = flush_inline_run(
                                &mut inline_run,
                                &InlineContext {
                                    at,
                                    content_x: content_x_scrolled,
                                    line_limit: content_w,
                                    font_size: container_font_size,
                                    text_align: &container_text_align,
                                    ws: container_ws,
                                    text_overflow: container_text_overflow,
                                    scale: crate::text_wrap::current_scale(),
                                    viewport_w,
                                    viewport_h,
                                    cb: descendant_cb,
                                    root_font_size,
                                },
                                &mut laid_children,
                                cur_y,
                            );
                            cur_y = flushed.cur_y;
                            // The run CONSUMES its last line: `cur_y` already
                            // sits below it, so the block child's own
                            // line-advance must not be charged a second time.
                            // Resetting `cur_x` to the content origin also makes
                            // the block loop's `cur_x != content_x` test agree
                            // that the next child starts a fresh line, rather
                            // than reaching the same conclusion from a stale
                            // cursor and advancing again.
                            cur_x = content_x_scrolled;
                            max_y_end = max_y_end.max(flushed.max_y_end);
                        }
                        let mut child_path = vec![idx];
                        if collect_inline_run(
                            c,
                            &mut child_path,
                            container_font_size,
                            &container_font_family,
                            container_align,
                            &mut inline_run,
                        ) {
                            continue;
                        }
                        let position = style_lookup_str(child_style, "position")
                            .unwrap_or_else(|| "static".to_string());
                        if position == "absolute" || position == "fixed" {
                            // Out of flow: laid out, but it never advances `cur_y` and
                            // never reaches `max_y_end`, so the parent's height is
                            // unaffected. `fixed` is pinned to the viewport; `absolute`
                            // resolves against this element's own containing block.
                            // Offset resolution is deferred to the shared tail, once
                            // this element's box is final.
                            let is_fixed = position == "fixed";
                            let out_cb = out_of_flow_containing_block(
                                is_fixed,
                                descendant_cb,
                                viewport_w,
                                viewport_h,
                            );

                            // The static position (CSS 2.1 §10.3.7) is where the box
                            // would have landed in flow. A block-level box following an
                            // open inline line starts a new line, which is the same
                            // cursor advance the in-flow path does below — computed here
                            // without moving the cursor, so the following sibling is
                            // unaffected.
                            let (static_x, static_y) = if cur_x != content_x_scrolled {
                                (content_x_scrolled, cur_y + last_bottom_margin.max(0))
                            } else {
                                (cur_x, cur_y)
                            };

                            let child_ln = at(
                                c,
                                static_x,
                                static_y,
                                out_cb.w,
                                out_cb.h,
                                viewport_w,
                                viewport_h,
                                out_cb,
                                Some(idx),
                                root_font_size,
                                my_font_size,
                            );
                            abs_children.push(PendingAbsolute {
                                style: child_style.map(str::to_string),
                                is_fixed,
                                static_x: child_ln.rect.x,
                                static_y: child_ln.rect.y,
                                node: child_ln,
                            });
                            continue;
                        }

                        if !is_text && cur_x != content_x_scrolled {
                            cur_y += last_bottom_margin.max(0); // Consider bottom margin of last child
                            cur_x = content_x_scrolled;
                        }

                        // Get child's margins for collapsing
                        let (_, _, cmt, cmb) = style_box_sides_full(
                            child_style,
                            "margin",
                            content_w as f32,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        );

                        // Margin collapsing: spec-compliant positive/negative partition
                        // First child may collapse through parent if parent has no top border/padding
                        let collapsed_margin_top = if idx == 0 {
                            if pt == 0 && bt == 0 {
                                collapse_margins(mt as f32, cmt as f32).round() as i32
                            } else {
                                cmt
                            }
                        } else {
                            collapse_margins(last_bottom_margin as f32, cmt as f32).round() as i32
                        };

                        // Adjust cur_y for collapsed margin.
                        // at() will add cmt, so we pass cur_y + collapsed_margin_top - cmt
                        // so that at() computes elem_y = (cur_y + collapsed_margin_top - cmt) + cmt = cur_y + collapsed_margin_top
                        // For parent-through, collapsed includes parent mt, but cur_y already includes parent mt via content_y_start;
                        // per brief we keep parent rect y not offset and adjust child via collapsed difference, so the above
                        // formula places child at cur_y + collapsed (where collapsed may be > cmt).
                        // If parent had pt/bt, collapsed == cmt so child at cur_y + cmt as before.
                        let adjusted_cur_y = cur_y + collapsed_margin_top - cmt;

                        // In flow: a static box's containing block is its parent's
                        // content box, which is what percentages resolve against.
                        let mut child_ln = at(
                            c,
                            cur_x,
                            adjusted_cur_y,
                            (content_w - (cur_x - content_x_scrolled)).max(0),
                            content_h_available,
                            viewport_w,
                            viewport_h,
                            ContainingBlock {
                                x: content_x,
                                y: content_y_start,
                                w: content_w,
                                h: content_h_available,
                            },
                            Some(idx),
                            root_font_size,
                            my_font_size,
                        );

                        apply_relative_position(
                            child_style,
                            &mut child_ln,
                            content_w,
                            content_h_available,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        );
                        apply_sticky_position(
                            child_style,
                            &mut child_ln,
                            content_x,
                            content_y_start,
                            content_w,
                            content_h_available,
                            scroll_x,
                            scroll_y,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        );

                        // Determine if this block child is empty (no content height, no children)
                        // Empty blocks collapse their own margins together and do not advance cur_y
                        let is_empty_block = child_ln.rect.h == 0 && child_ln.children.is_empty();
                        if is_empty_block {
                            // Effective collapsed for empty = collapse(collapsed_top, cmb)
                            // collapsed_top already is collapse(prev, cmt) or collapse(parent,cmt)
                            // This merges previous bottom, empty top, and empty bottom into one partition value
                            last_bottom_margin =
                                collapse_margins(collapsed_margin_top as f32, cmb as f32).round()
                                    as i32;
                            // Empty does not advance cur_y beyond previous position;
                            // its collapsed margins are represented via last_bottom_margin for next sibling.
                            cur_x = content_x;
                            max_y_end = max_y_end.max(cur_y);
                            laid_children.push(child_ln);
                        } else {
                            cur_y = child_ln.rect.y + child_ln.rect.h;
                            last_bottom_margin = cmb;
                            cur_x = content_x;

                            max_y_end = max_y_end.max(child_ln.rect.y + child_ln.rect.h);
                            laid_children.push(child_ln);
                        }
                    }
                    // A run left open at the end of the children is the last
                    // thing in the block, so it is flushed after the loop. Only
                    // `cur_y` and `max_y_end` are read from the result: a block
                    // loop that ends here has no following child to place, so
                    // there is no cursor to leave it at.
                    if !inline_run.is_empty() {
                        let flushed = flush_inline_run(
                            &mut inline_run,
                            &InlineContext {
                                at,
                                content_x: content_x_scrolled,
                                line_limit: content_w,
                                font_size: container_font_size,
                                text_align: &container_text_align,
                                ws: container_ws,
                                text_overflow: container_text_overflow,
                                scale: crate::text_wrap::current_scale(),
                                viewport_w,
                                viewport_h,
                                cb: descendant_cb,
                                root_font_size,
                            },
                            &mut laid_children,
                            cur_y,
                        );
                        // `cur_y` is deliberately not taken from the result:
                        // this flush is the last thing the loop does and nothing
                        // after it reads the cursor. `max_y_end` is what decides
                        // the block's height, and it is what the flush reports.
                        max_y_end = max_y_end.max(flushed.max_y_end);
                    }
                }

                // Height: declared or content height + paddings/borders, clamped by min/max-height
                //
                // This re-reads `height` against `my_font_size` where the first read
                // above used `parent_font_size` — an `em` height means different
                // numbers on the two, which is pre-existing and left alone. The
                // replaced fallback is added to BOTH rather than hoisted into one
                // value, so that difference keeps being the only difference.
                let declared_h2 = style_lookup_len_full(
                    style,
                    "height",
                    avail_h as f32,
                    my_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                )
                .or_else(|| {
                    replaced.then(|| {
                        replaced_used_size(
                            props,
                            "height",
                            intrinsic.map(|(_, h)| h),
                            avail_h as f32,
                            my_font_size,
                            root_font_size,
                            (vw_f, vh_f),
                        )
                    })
                });
                // Use max_y_end which correctly tracks the spatial extent of all children,
                // including those positioned above content_y_start via negative margins/offsets.
                let content_h = (max_y_end - content_y_start).max(0);
                let mut rect_h = if let Some(dh) = declared_h2 {
                    if is_border_box {
                        dh
                    } else {
                        dh + pt + pb + bt + bb
                    }
                } else if is_viewport_height {
                    // viewport/root heights are viewport-relative - already computed as _rect_h outer
                    _rect_h
                } else {
                    content_h + pt + pb + bt + bb
                };
                let min_h = style_lookup_len_full(
                    style,
                    "min-height",
                    avail_h as f32,
                    my_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                )
                .unwrap_or(0);
                let max_h_val = style_lookup_len_full(
                    style,
                    "max-height",
                    avail_h as f32,
                    my_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );
                let max_h = match max_h_val {
                    Some(v) if v >= 0 => v,
                    _ => i32::MAX,
                };
                rect_h = rect_h.max(min_h).min(max_h);

                if tag == "button"
                    && children.len() == 1
                    && let Some(child) = laid_children.get_mut(0)
                {
                    let content_h = (rect_h - pt - pb - bt - bb).max(0);
                    let child_h = child.rect.h;
                    let offset_y = ((content_h - child_h).max(0)) / 2;
                    child.rect.y = elem_y + bt + pt + offset_y;

                    // Horizontal: only when the author actually declared
                    // `text-align`. Without it, `child.rect.x` is left exactly as
                    // the flow above computed it -- the flex pass's
                    // `justify-content` main-axis placement, or the block/inline
                    // flow's own. The previous unconditional `child.rect.x =
                    // content_x + offset_x` with a `left` default threw that away,
                    // so `justify-content: center` was silently ignored on every
                    // single-child button that did not ALSO carry `text-align:
                    // center` (which is why `.remove` rendered ~8px left of
                    // centre while `.toggle`/`.check`, being multi-child, were
                    // fine).
                    //
                    // The discriminator is PRESENCE, not `== "center"`: `left` is
                    // the initial value, so testing the value would make an
                    // explicit `text-align: right` indistinguishable from
                    // silence and lose it. Presence includes an INHERITED value:
                    // `text-align` is in velox-style's `INHERITABLE` set, so the
                    // cascade has already copied an ancestor's declaration into
                    // this element's computed style string -- and a browser does
                    // align a button's inline content by an inherited
                    // `text-align`, so treating it as a real request matches
                    // browsers.
                    if let Some(align) = style_lookup_str(style, "text-align") {
                        let child_w = child.rect.w;
                        let offset_x = match align.as_str() {
                            "center" => ((content_w - child_w).max(0)) / 2,
                            "right" => (content_w - child_w).max(0),
                            _ => 0,
                        };
                        child.rect.x = content_x + offset_x;
                    }
                }

                // Scrollable overflow model: scrollHeight separation, is_scrollable, clip logic
                let scroll_height = content_h + pt + pb + bt + bb;
                let overflow_lower = overflow.to_ascii_lowercase();
                let scrollable =
                    is_scrollable(&overflow_lower, scroll_height as f32, rect_h as f32);
                let clip = if scrollable || overflow_lower == "hidden" {
                    Some(Rect {
                        x: elem_x,
                        y: elem_y,
                        w: rect_w,
                        h: rect_h,
                    })
                } else {
                    None
                };
                let max_scroll_y = (scroll_height - rect_h).max(0);
                // Synthetic scroll-left/scroll-top stay raw here for compat: children
                // are positioned from the raw values above, so the stored fields must
                // match or render/hit-test would disagree. Wheel-driven scrolling
                // clamps via ScrollState / apply_scroll_offsets instead.
                // Out-of-flow children are resolved last, now that `rect_w`/`rect_h`
                // are final, and appended last so they paint above their in-flow
                // siblings (velox-renderer sorts by `z_index` within list order).
                let out_cb = out_of_flow_cb(rect_h);
                for mut pending in abs_children {
                    let cb = out_of_flow_containing_block(
                        pending.is_fixed,
                        out_cb,
                        viewport_w,
                        viewport_h,
                    );
                    apply_absolute_position(
                        pending.style.as_deref(),
                        &mut pending.node,
                        cb,
                        (pending.static_x, pending.static_y),
                        my_font_size,
                        root_font_size,
                        vw_f,
                        vh_f,
                    );
                    laid_children.push(pending.node);
                }
                LayoutNode {
                    rect: Rect {
                        x: elem_x,
                        y: elem_y,
                        w: rect_w,
                        h: rect_h,
                    },
                    z_index,
                    display_none: false,
                    source_index,
                    scroll_x,
                    scroll_y,
                    clip,
                    stacking_context,
                    scroll_height,
                    max_scroll_y,
                    scrollable,
                    children: laid_children,
                }
            }
        }
    }
    // The initial containing block is the viewport, so that is what an out-of-flow
    // box with no positioned ancestor resolves against (CSS 2.1 §10.1).
    at(
        node,
        0,
        0,
        viewport_w,
        viewport_h,
        viewport_w,
        viewport_h,
        ContainingBlock {
            x: 0,
            y: 0,
            w: viewport_w,
            h: viewport_h,
        },
        None,
        DEFAULT_ROOT_FONT_SIZE,
        DEFAULT_ROOT_FONT_SIZE,
    )
}
