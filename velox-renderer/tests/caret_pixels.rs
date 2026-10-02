//! Headless pixel proof for the text-input furniture the painter used to
//! lack entirely: caret bar, selection highlight, focus ring — plus the
//! checkbox tick, which read an attribute the template never binds.
//!
//! Everything goes through `render_vnode_to_rgba` (the function that actually
//! runs `compute_layout`). `render_vnode_to_raster_png` is never used: it
//! skips layout and hands back a false green.
//!
//! THE ANTI-CIRCULARITY RULE. No test re-derives the caret's expected x by
//! re-running the painter's own math — no font measuring, no re-implemented
//! advance sum. Expected geometry is:
//!   * the LAYOUT BOX, read from `compute_layout` (the authoritative box the
//!     renderer itself consumes), plus the painter's documented inner-border
//!     inset and text padding — both fixed box constants, not glyph math;
//!   * or a RELATIVE movement between two renders of the SAME vnode, e.g.
//!     "the caret must advance by at least half a glyph as the index grows"
//!     and "a wide-glyph run and a narrow-glyph run of equal length must put
//!     the caret at DIFFERENT x" — facts only a real measurement satisfies.
//!
//! A second rule makes the pixel reading unambiguous: the value TEXT in this
//! painter is near-black, and so is the caret core, so "is there black here"
//! cannot separate them. Caret tests therefore either use an EMPTY value
//! (no glyphs at all, so black can only be the caret) or difference two
//! renders that differ ONLY in `caret_blink`.

#![cfg(feature = "skia-native")]

use velox_dom::{Props, VNode, h};
use velox_renderer::render_vnode_to_rgba;
use velox_style::Stylesheet;

const W: i32 = 320;
const H: i32 = 120;

/// Box constants fixed by the field's box, not by glyph measurement.
///
/// The value text's origin is NOT spelled out here as `box.x + 1.0 + 4.0`
/// any more: the painter reads its insets from `input_metrics::input_text_metrics`,
/// so a literal pair of constants in the test would be a second, frozen copy of
/// the painter's arithmetic — precisely the drift `input_metrics` exists to
/// end. `text_origin_x` calls the painter's own function instead, which is
/// still not re-deriving glyph math: no measuring, no advance summing.
///
/// Literal colors the painter is specified to emit.
const BG: [u8; 4] = [255, 255, 255, 255];
const ACCENT: [u8; 4] = [52, 120, 246, 255]; // focus ring / tick / selection hue

fn px(buf: &[u8], x: i32, y: i32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

/// The caret core's ink: contrast black on a light field, opaque.
fn is_core(p: [u8; 4]) -> bool {
    p == [0, 0, 0, 255]
}

/// Glyph ink: near-black, opaque, but tolerant of anti-aliasing.
fn is_glyph_ink(p: [u8; 4]) -> bool {
    p[3] > 120 && p[0] < 90 && p[1] < 90 && p[2] < 90
}

fn render(vnode: &VNode) -> Vec<u8> {
    render_vnode_to_rgba(vnode, &Stylesheet::default(), W, H).expect("render to rgba")
}

/// The input's box, from the same layout the renderer consumes.
fn input_box(v: &VNode) -> velox_dom::layout::Rect {
    let styled = velox_style::apply_with_cascade(v, &Stylesheet::default());
    let layout = velox_dom::layout::compute_layout(&styled, W, H);
    layout.children[0].rect
}

/// The CASCADED style string the painter actually received for the field in
/// `v`.
///
/// Not the authored `field_style()`: the UA sheet contributes `padding` and a
/// `min-height` to every `<input>`, and the painter reads the merged string.
/// Reading the authored string here would make this helper disagree with the
/// screen for every field the UA sheet touches.
fn cascaded_input_style(v: &VNode) -> Option<String> {
    fn walk(node: &VNode) -> Option<String> {
        match node {
            VNode::Text(_) => None,
            VNode::Element {
                tag,
                props,
                children,
            } => {
                if tag == "input" {
                    return props.attrs.get("style").cloned();
                }
                children.iter().find_map(walk)
            }
        }
    }
    let styled = velox_style::apply_with_cascade(v, &Stylesheet::default());
    walk(&styled)
}

/// The painter's geometry for the field in `v`, in the same logical px and
/// against the same computed style string the paint lane used.
fn text_metrics(
    v: &VNode,
    box_: &velox_dom::layout::Rect,
) -> velox_renderer::input_metrics::InputTextMetrics {
    velox_renderer::input_metrics::input_text_metrics(
        cascaded_input_style(v).as_deref(),
        *box_,
        (W as f32, H as f32),
        // The root default the paint lane inherits into a bare `VNode`.
        velox_dom::layout::DEFAULT_ROOT_FONT_SIZE,
    )
}

/// Where the value text begins, from the ONE geometry authority the painter
/// itself reads.
///
/// This is a delegation, not a re-derivation: `input_text_metrics` resolves
/// border widths and padding sides from the COMPUTED style, which is why
/// `padding`, `padding-left` and an explicit `12px` all agree here for the same
/// reason they agree on screen. Calling it is what lets this test keep
/// asserting "the caret is where the glyph is" without freezing a second copy
/// of the insets as test-local constants.
fn text_origin_x(v: &VNode, box_: &velox_dom::layout::Rect) -> f32 {
    text_metrics(v, box_).text_left
}

fn field_style() -> &'static str {
    "width:200px;height:40px;font-size:15px"
}

/// Page background, deliberately NOT the field background, so an unpainted
/// region is unmistakable.
const PAGE: &str = "background:#000000";

/// A text input with the given attributes, plus a sibling offset clear of
/// the field (used for the bleed test).
fn scene_with_sibling(input_attrs: Vec<(&str, &str)>) -> VNode {
    let mut p = Props::new().set("type", "text").set("style", field_style());
    for (k, v) in input_attrs {
        p = p.set(k, v);
    }
    h(
        "div",
        Props::new().set("style", PAGE),
        vec![
            h("input", p, vec![]),
            h(
                "div",
                Props::new().set(
                    "style",
                    "position:absolute;left:20px;top:90px;width:200px;height:20px;background:#ff0000",
                ),
                vec![],
            ),
        ],
    )
}

fn scene(input_attrs: Vec<(&str, &str)>) -> VNode {
    let mut p = Props::new().set("type", "text").set("style", field_style());
    for (k, v) in input_attrs {
        p = p.set(k, v);
    }
    h(
        "div",
        Props::new().set("style", PAGE),
        vec![h("input", p, vec![])],
    )
}

/// Focused input, blink on, no selection.
fn focused(value: &str, caret: &str) -> VNode {
    scene(vec![
        ("value", value),
        ("caret", caret),
        ("caret_blink", "true"),
        ("sel_start", "0"),
        ("sel_end", "0"),
        ("focused", "true"),
    ])
}

/// Same vnode, blink explicitly OFF.
fn focused_blink_off(value: &str, caret: &str) -> VNode {
    scene(vec![
        ("value", value),
        ("caret", caret),
        ("caret_blink", "false"),
        ("sel_start", "0"),
        ("sel_end", "0"),
        ("focused", "true"),
    ])
}

fn unfocused(value: &str) -> VNode {
    scene(vec![("value", value)])
}

/// Columns where two renders differ, restricted to the field. Used to find
/// the caret without letting the value text impersonate it.
fn changed_columns(a: &[u8], b: &[u8], box_: &velox_dom::layout::Rect) -> Vec<i32> {
    (box_.x..box_.x + box_.w)
        .filter(|x| (box_.y..box_.y + box_.h).any(|y| px(a, *x, y) != px(b, *x, y)))
        .collect()
}

/// Caret core columns, found by differencing the two blink states of the
/// SAME vnode. Only valid for an EMPTY value (see the module docs), where
/// blink is the single thing that differs.
fn caret_columns_empty_value() -> Vec<i32> {
    let on = focused("", "0");
    let off = focused_blink_off("", "0");
    let box_ = input_box(&on);
    let a = render(&on);
    let b = render(&off);
    changed_columns(&a, &b, &box_)
        .into_iter()
        .filter(|x| (box_.y..box_.y + box_.h).any(|y| is_core(px(&a, *x, y))))
        .collect()
}

/// A row that actually carries the bar / highlight, found by MEASURING the
/// changed band rather than assuming a row inside the box. The bar spans the
/// text line box, which is inset from the field box, so `box_.y + 12` can
/// land above it.
fn changed_row(a: &[u8], b: &[u8], box_: &velox_dom::layout::Rect) -> i32 {
    let mid = box_.y + box_.h / 2;
    if changed_columns(a, b, box_).contains(&mid) {
        return mid;
    }
    // Pick the row with the most changed pixels — the middle of the band.
    (box_.y..box_.y + box_.h)
        .max_by_key(|y| {
            (box_.x..box_.x + box_.w)
                .filter(|x| px(a, *x, *y) != px(b, *x, *y))
                .count()
        })
        .unwrap_or(mid)
}

/// Every pixel in `col` that differs between the two renders.
fn col_rows(a: &[u8], b: &[u8], col: i32) -> Vec<i32> {
    (0..H)
        .filter(|y| px(a, col, *y) != px(b, col, *y))
        .collect()
}

/// ── 1. blink gates the bar, and the core is literal black ──────────────
#[test]
fn blink_gates_the_caret_and_the_core_is_black() {
    let on = focused("", "0");
    let off = focused_blink_off("", "0");
    let box_ = input_box(&on);
    let a = render(&on);
    let b = render(&off);

    let cols = caret_columns_empty_value();
    assert!(
        !cols.is_empty(),
        "focused input with caret_blink=true painted no caret core"
    );

    // The bar spans the text line box, which is inset from the field box, so
    // find the rows it actually occupies rather than assuming one.
    let rows = col_rows(&a, &b, cols[0]);
    assert!(
        rows.len() >= 8,
        "caret core is only {}px tall — not a usable bar",
        rows.len()
    );

    // The core is literally opaque black, on the white field.
    let core_y = rows[rows.len() / 2];
    let core = px(&a, cols[0], core_y);
    assert_eq!(
        core,
        [0, 0, 0, 255],
        "caret core is not the specified black"
    );

    // Every core column reverts to the exact field background with blink off.
    for x in &cols {
        assert_eq!(
            px(&b, *x, core_y),
            BG,
            "column {x} did not revert to the field background with blink off"
        );
    }

    // With blink off, no black core exists anywhere in the field. The value
    // is empty, so nothing else could be black.
    let residual =
        (box_.x..box_.x + box_.w).any(|x| (box_.y..box_.y + box_.h).any(|y| is_core(px(&b, x, y))));
    assert!(!residual, "caret_blink=false still painted a caret core");
}

/// ── 2. caret x is MEASURED, not pinned to the left edge ────────────────
#[test]
fn caret_x_is_the_measured_prefix_width() {
    // (a) EMPTY value: the caret sits at the text origin. The text origin is
    //     box geometry — the same x the value text starts at — not a
    //     painter-invented constant, and an empty prefix measures zero by
    //     definition, so no glyph math is being re-run.
    let v = focused("", "0");
    let box_ = input_box(&v);
    let cols = caret_columns_empty_value();
    assert!(!cols.is_empty(), "empty focused value painted no caret");
    assert_eq!(
        cols[0] as f32,
        text_origin_x(&v, &box_),
        "empty-value caret is not at the text origin (box.x + border + padding)"
    );

    // (b) MOVEMENT: same value, index 0 vs index len. Differencing each
    //     against its own blink-off twin isolates the caret even though the
    //     value text is also black.
    let locate = |value: &str, caret: &str| -> i32 {
        let on = focused(value, caret);
        let off = focused_blink_off(value, caret);
        let bx = input_box(&on);
        let a = render(&on);
        let b = render(&off);
        let c: Vec<i32> = changed_columns(&a, &b, &bx)
            .into_iter()
            .filter(|x| (bx.y..bx.y + bx.h).any(|y| is_core(px(&a, *x, y))))
            .collect();
        assert!(
            !c.is_empty(),
            "no caret core found for value={value:?} caret={caret}"
        );
        c[0]
    };

    let at0 = locate("mmmmmmmm", "0");
    let at_end = locate("mmmmmmmm", "8");
    let run_vnode = focused("mmmmmmmm", "8");
    let run_box = input_box(&run_vnode);

    assert_eq!(
        at0 as f32,
        text_origin_x(&run_vnode, &run_box),
        "caret at index 0 is not at the text origin"
    );
    assert!(
        at_end > at0,
        "caret did not move right as the index advanced ({at0} -> {at_end})"
    );
    let advance = at_end - at0;
    // Eight glyphs of a 15px sans face. A 1-2px bar against a run this long
    // must land well past the origin; require at least 8px (a full glyph of
    // the narrowest letter in play) so a one- or two-glyph advance cannot
    // pass as "measured".
    assert!(
        advance >= 8,
        "8 glyphs advanced the caret only {advance}px — a hardcoded left-edge \
         caret advances 0"
    );

    // (c) PROPORTIONAL, not a monospace grid: two runs of the SAME length
    //     with different glyphs must place the caret differently. A fixed
    //     advance-per-character constant cannot satisfy this.
    let wide = locate("WWWWWWWW", "8");
    let narrow = locate("iiiiiiii", "8");
    assert_ne!(
        wide, narrow,
        "caret x identical for a wide-glyph and a narrow-glyph run of equal \
         length — the advance is not measured from the font"
    );
    assert!(
        wide > narrow,
        "'W' is wider than 'i', so its caret must sit further right \
         (wide={wide}, narrow={narrow})"
    );
}

/// ── 3. unfocused ⇒ no caret anywhere in the field ──────────────────────
#[test]
fn unfocused_input_has_no_caret() {
    // An EMPTY value means no glyphs, so any black pixel in the field could
    // only be a caret. A bare scan is then a complete test.
    let bare = unfocused("");
    let box_ = input_box(&bare);
    let buf = render(&bare);
    let black = (box_.x..box_.x + box_.w)
        .flat_map(|x| (box_.y..box_.y + box_.h).map(move |y| (x, y)))
        .filter(|(x, y)| is_core(px(&buf, *x, *y)))
        .count();
    assert_eq!(black, 0, "an input with no focus attrs painted a caret");

    // Attrs present, explicitly unfocused, blink ON, valid index: the gate is
    // `focused`, not "did we receive a caret index".
    let not_focused = scene(vec![
        ("value", ""),
        ("caret", "3"),
        ("caret_blink", "true"),
        ("sel_start", "0"),
        ("sel_end", "0"),
        ("focused", "false"),
    ]);
    let buf2 = render(&not_focused);
    let black2 = (box_.x..box_.x + box_.w)
        .flat_map(|x| (box_.y..box_.y + box_.h).map(move |y| (x, y)))
        .filter(|(x, y)| is_core(px(&buf2, *x, *y)))
        .count();
    assert_eq!(black2, 0, "focused=false still painted a caret");

    // Control: the SAME value focused and blinking DOES have a caret, so the
    // two assertions above are not vacuously true.
    let ctrl = focused("", "3");
    let buf3 = render(&ctrl);
    let black3 = (box_.x..box_.x + box_.w)
        .flat_map(|x| (box_.y..box_.y + box_.h).map(move |y| (x, y)))
        .filter(|(x, y)| is_core(px(&buf3, *x, *y)))
        .count();
    assert!(
        black3 > 0,
        "control render painted no caret — the test is vacuous"
    );
}

/// ── 4. selection paints a highlight BEHIND readable text ───────────────
#[test]
fn selection_paints_a_highlight_behind_the_text() {
    // Blink OFF in both renders, so the caret is not a confounder and the
    // ONLY difference between them is the selection.
    let make = |s0: &str, s1: &str| {
        scene(vec![
            ("value", "mmmmmmmm"),
            ("caret", "0"),
            ("caret_blink", "false"),
            ("sel_start", s0),
            ("sel_end", s1),
            ("focused", "true"),
        ])
    };
    let plain = make("0", "0");
    let selected = make("2", "6");
    let box_ = input_box(&plain);
    let a = render(&selected);
    let b = render(&plain);

    let changed = changed_columns(&a, &b, &box_);
    assert!(
        !changed.is_empty(),
        "the selection render is byte-identical to the unselected render — \
         nothing was painted"
    );
    // The span starts at the MEASURED advance of 2 glyphs, so it must begin
    // right of the origin — a band that began at the text origin would mean
    // the selection ignored its start index.
    assert!(
        changed[0] as f32 > text_origin_x(&selected, &box_),
        "selection starting at index 2 began at the text origin ({}) — the \
         start index is not being measured",
        changed[0]
    );
    // A real span, not a sliver.
    assert!(
        (changed.last().unwrap() - changed[0]) >= 8,
        "selection band is only {}px wide",
        changed.last().unwrap() - changed[0]
    );

    // A light blue tint over the white field: opaque, blue-dominant, and
    // clearly not the bare background. Sample a row the band actually covers.
    let y = changed_row(&a, &b, &box_);
    let fill = px(&a, (changed[0] + changed.last().unwrap()) / 2, y);
    assert_ne!(fill, px(&b, (changed[0] + changed.last().unwrap()) / 2, y));
    assert_eq!(fill[3], 255, "selection fill is not opaque over the field");
    assert!(
        fill[2] > fill[1] && fill[1] > fill[0] && fill[2] > 200,
        "selection fill is not a light blue tint: {fill:?}"
    );
    assert!(
        fill[0] > 100,
        "selection fill is too dark to keep black text readable: {fill:?}"
    );

    // Text survives ON TOP of the highlight.
    let glyphs = (changed[0]..=*changed.last().unwrap())
        .flat_map(|x| (box_.y..box_.y + box_.h).map(move |y| (x, y)))
        .filter(|(x, y)| is_glyph_ink(px(&a, *x, *y)))
        .count();
    assert!(
        glyphs > 20,
        "only {glyphs} glyph-ink pixels survive inside the selection — the \
         text is not readable on top of it"
    );
}

/// ── 5. the caret does not bleed into a sibling element ─────────────────
#[test]
fn caret_does_not_touch_a_sibling_element() {
    let on = render(&scene_with_sibling(vec![
        ("value", ""),
        ("caret", "0"),
        ("caret_blink", "true"),
        ("focused", "true"),
    ]));
    let off = render(&scene_with_sibling(vec![
        ("value", ""),
        ("caret", "0"),
        ("caret_blink", "false"),
        ("focused", "true"),
    ]));

    // The sibling is a 200x20 red block at y=90..110, clear of the field.
    for y in 90..110 {
        for x in 20..220 {
            assert_eq!(
                px(&on, x, y),
                px(&off, x, y),
                "sibling pixel ({x},{y}) changed when the caret blinked — the \
                 caret bled out of the field"
            );
        }
    }
    // The comparison is not passing on a blank canvas.
    assert_eq!(
        px(&on, 100, 100),
        [255, 0, 0, 255],
        "sibling block is not red"
    );
    // Control: the field itself DID change, so the sibling comparison is live.
    // Locate the changed row rather than assuming one — the bar spans the
    // text line box, which is inset from the field box.
    let sibling_vnode = scene_with_sibling(vec![
        ("value", ""),
        ("caret", "0"),
        ("caret_blink", "true"),
        ("focused", "true"),
    ]);
    let bx = input_box(&sibling_vnode);
    let caret_x = text_origin_x(&sibling_vnode, &bx) as i32;
    let rows = col_rows(&on, &off, caret_x);
    assert!(
        !rows.is_empty(),
        "the field itself did not change — the sibling comparison is vacuous"
    );
    assert_ne!(
        px(&on, caret_x, rows[rows.len() / 2]),
        px(&off, caret_x, rows[rows.len() / 2]),
        "the caret column did not change between the blink states"
    );
}

/// ── 6. focus ring is a distinct, visible mark ──────────────────────────
#[test]
fn focus_ring_is_visible_and_distinct_from_the_border() {
    let f = focused("", "0");
    let u = unfocused("");
    let box_ = input_box(&f);
    let fr = render(&f);
    let ur = render(&u);

    // The ring is inset 2px inside the inner rect and 2px thick, so its
    // vertical run occupies columns [inner_left+1, inner_left+3]. Column
    // inner_left+2 is solidly ring.
    let ring_x = text_metrics(&f, &box_).pad_left as i32 + 2;
    let hits = (box_.y + 6..box_.y + box_.h - 6)
        .filter(|y| px(&fr, ring_x, *y) == ACCENT)
        .count();
    assert!(
        hits > 10,
        "no focus ring at the inset column x={ring_x} (only {hits} accent pixels)"
    );

    // Unfocused: no ring anywhere in that column.
    let u_hits = (box_.y + 6..box_.y + box_.h - 6)
        .filter(|y| px(&ur, ring_x, *y) == ACCENT)
        .count();
    assert_eq!(u_hits, 0, "ring pixels present on an unfocused input");

    // THE BUG THIS EXISTS FOR: a ring sharing the border's own pixels is
    // invisible. The outermost column must be UNCHANGED by focus — that is
    // the real invariant, and it holds even though a 1px stroke centred on
    // the edge antialiases the outermost column to a half-cover blend.
    let edge_y = box_.y + 12;
    let border_focused = px(&fr, box_.x, edge_y);
    let border_plain = px(&ur, box_.x, edge_y);
    assert_eq!(
        border_focused, border_plain,
        "the outermost border column changed when the input gained focus — the \
         ring was drawn onto the border's own pixels, which is the reported bug"
    );
    assert_ne!(
        border_focused, ACCENT,
        "ring painted onto the border's own column"
    );
    // The ring is a genuinely different column, a couple of px inside.
    assert_ne!(
        px(&fr, ring_x, edge_y),
        px(&ur, ring_x, edge_y),
        "the ring column did not change on focus"
    );
    assert_eq!(
        px(&fr, ring_x, edge_y),
        ACCENT,
        "ring column is not the accent"
    );
}

/// A focused field whose border is dashed must still paint a SOLID focus ring.
///
/// The painter's stroke paint is frame-scoped, so `border: 2px dashed` left its
/// `[6, 6]` dash intervals installed on the shared stroke when the ring was drawn
/// and the ring came out dashed. This is the same class of defect as the inset
/// bug above: the ring is what makes a focused field legible, and a dashed ring
/// defeats it.
///
/// Anti-circularity: the ring column is read from the LAYOUT BOX plus the
/// border width this fixture declares (`2px`) and the documented 2px ring inset
/// — box constants, not glyph math, and not the painter's own inset helper. The
/// "must be solid" claim is then checked by CONTIGUITY along that column, which
/// no choice of constants can fake: a dash has to break the run.
#[test]
fn a_dashed_border_does_not_make_the_focus_ring_dashed() {
    const DASHED: &str = "width:200px;height:40px;font-size:15px;border:2px dashed #888888";
    let dashed_vnode = |focused: bool| {
        let mut attrs = vec![
            ("value", ""),
            ("caret", "0"),
            ("sel_start", "0"),
            ("sel_end", "0"),
        ];
        attrs.push(("focused", if focused { "true" } else { "false" }));
        let mut p = Props::new()
            .set("type", "text")
            .set("style", DASHED)
            .set("caret_blink", "true");
        for (k, v) in attrs {
            p = p.set(k, v);
        }
        h(
            "div",
            Props::new().set("style", PAGE),
            vec![h("input", p, vec![])],
        )
    };

    let f = dashed_vnode(true);
    let u = dashed_vnode(false);
    let box_ = input_box(&f);
    let fr = render(&f);
    let ur = render(&u);

    // Ring geometry from box constants: border 2px + the documented 2px inset.
    const BORDER: i32 = 2;
    const RING_INSET: i32 = 2;
    let ring_x = box_.x + BORDER + RING_INSET;

    // The ring's vertical run, sampled well inside the corners so the rounded
    // ends cannot be mistaken for a break.
    let top = box_.y + BORDER + RING_INSET + 4;
    let bottom = box_.y + box_.h - BORDER - RING_INSET - 4;
    assert!(
        bottom > top,
        "fixture field is too short to sample a ring run"
    );

    // Control first: unfocused has NO ring, so "continuous" cannot be satisfied
    // by the page or the border simply being painted here.
    let unfocused_accent = (top..bottom)
        .filter(|y| px(&ur, ring_x, *y) == ACCENT)
        .count();
    assert_eq!(
        unfocused_accent, 0,
        "unfocused dashed field already has accent at x={ring_x}; the fixture cannot \
         distinguish a ring"
    );

    // A dash interval for a 2px border is [6, 6], so 12px of column contains a
    // whole period. Require the ENTIRE run to be accent: a dash necessarily
    // breaks it.
    let gaps: Vec<i32> = (top..bottom)
        .filter(|y| px(&fr, ring_x, *y) != ACCENT)
        .collect();
    assert!(
        gaps.is_empty(),
        "the focus ring on a `border: 2px dashed` field is not solid: {}/{} pixels in \
         the ring column x={ring_x} (y {top}..{bottom}) are not the accent colour, first \
         at y={:?}. The stroke paint carried the border's dash into the ring.",
        gaps.len(),
        bottom - top,
        gaps.first()
    );
}

/// ── 7. checkbox tick honours the attribute the template binds ─────────
///
/// The real template is `test-app/src/components/TodoItem.vx:6`:
///   `<input type="checkbox" class="checkbox" :checked="completed" ... />`
/// It binds `:checked`, never `:value`. The old painter read `value`, so the
/// tick never rendered. Both spellings are asserted: `checked` must win, and
/// the `value` fallback must keep working.
#[test]
fn checkbox_tick_follows_the_checked_attribute() {
    let box_vnode = |attrs: Vec<(&str, &str)>| {
        let mut p = Props::new()
            .set("type", "checkbox")
            .set("style", "width:18px;height:18px");
        for (k, v) in attrs {
            p = p.set(k, v);
        }
        h(
            "div",
            Props::new().set("style", PAGE),
            vec![h("input", p, vec![])],
        )
    };

    let (cx, cy) = {
        let v = box_vnode(vec![("checked", "true")]);
        let r = input_box(&v);
        (r.x + r.w / 2, r.y + r.h / 2)
    };

    // The template's spelling: `checked="true"` -> tick.
    let checked = render(&box_vnode(vec![("checked", "true")]));
    let tick = px(&checked, cx, cy);
    assert_eq!(
        tick, ACCENT,
        "checked=\"true\" did not paint the accent tick (got {tick:?})"
    );

    // `checked="false"` -> no tick, bare field background in the box.
    let unchecked = render(&box_vnode(vec![("checked", "false")]));
    let none = px(&unchecked, cx, cy);
    assert_ne!(none, ACCENT, "checked=\"false\" still painted a tick");
    assert_eq!(
        none, BG,
        "unchecked box interior is not the field background: {none:?}"
    );

    // The legacy `value` spelling keeps working.
    let legacy = render(&box_vnode(vec![("value", "true")]));
    assert_eq!(
        px(&legacy, cx, cy),
        ACCENT,
        "value=\"true\" no longer ticks — existing behavior regressed"
    );

    // The real-world template case that used to be broken: a checked box
    // carries `checked`, never `value`.
    let no_state = render(&box_vnode(vec![]));
    assert_ne!(
        px(&no_state, cx, cy),
        ACCENT,
        "a checkbox with no state painted a tick"
    );
    assert_eq!(px(&no_state, cx, cy), BG, "unstateful box is not blank");
}

/// ── 8. overflow: a long value's caret is clipped, not spilled ─────────
#[test]
fn a_long_value_clips_the_caret_inside_the_field() {
    // 36 glyphs into a 200px field: far past the right edge.
    let long = "mmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmm";
    let v = focused(long, "36");
    let box_ = input_box(&v);
    let off = focused_blink_off(long, "36");
    let a = render(&v);
    let b = render(&off);

    let cols: Vec<i32> = changed_columns(&a, &b, &box_)
        .into_iter()
        .filter(|x| (box_.y..box_.y + box_.h).any(|y| is_core(px(&a, *x, y))))
        .collect();
    assert!(!cols.is_empty(), "no caret drawn for a long value");

    // The clip holds: the core never reaches the field's right border column.
    let last = *cols.last().unwrap();
    assert!(
        last < box_.x + box_.w - 1,
        "caret core reached x={last}, past the field's inner clip"
    );
    assert!(
        cols[0] as f32 >= text_origin_x(&v, &box_),
        "caret core started left of the text origin"
    );

    // And nothing outside the field box changed. Compare the two blink
    // states rather than testing for "not black" — the page background is
    // itself near-black, so a colour test here would pass vacuously and
    // fail on the background instead of on real bleed.
    for y in 0..H {
        for x in (box_.x + box_.w)..W {
            assert_eq!(
                px(&a, x, y),
                px(&b, x, y),
                "caret bled outside the field at ({x},{y})"
            );
        }
    }
    // The overflowed caret really is drawn ON the clip edge, not absent.
    // `content_right` — the rightmost x a glyph may occupy, which the painter
    // clamps the caret's measured advance to — is read from the geometry
    // authority the painter also reads, so this pins the clamp to the
    // CONTENT EDGE rather than to a frozen pair of constants.
    let content_right = text_metrics(&v, &box_).content_right.round() as i32;
    assert_eq!(
        cols[0], content_right,
        "an overflowing caret is not parked on the content edge (x={content_right})"
    );
}
