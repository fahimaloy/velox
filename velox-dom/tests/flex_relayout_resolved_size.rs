//! REGRESSION GUARD: a flex item whose RESOLVED main size differs from the size
//! it was MEASURED at must have its subtree re-laid-out at the resolved size.
//!
//! ## The defect
//!
//! Every flex child is pre-laid-out ONCE, at the container's full main size
//! (`velox-dom/src/layout.rs`, the measure pass:
//! `let child_avail_main = if is_column { content_h_available } else { main_size };`).
//! The placement pass then applies the resolved size to the item's OWN box
//!
//! ```text
//! let fb = items[item_idx].target_main_size.round() as i32;
//! if is_column { ln.rect.h = fb } else { ln.rect.w = fb }
//! ```
//!
//! and moves the subtree with `translate_layout_subtree`, which adds `dx`/`dy`
//! to every rect and clip and NEVER re-measures. There was no `at()` call
//! anywhere in the placement region. So when the resolved size differs from the
//! measured size, the item's box is correct and its SUBTREE IS STALE: every
//! child keeps the size it was measured at.
//!
//! ## The arithmetic this pins
//!
//! The composer's case, in isolation (the scaffolded row's own content box):
//!
//! | step | value |
//! |---|---|
//! | row (`.composer`) content width | 556 |
//! | `.field` base (`flex: 1 1 auto` -> content size) | 556 |
//! | `.add` base (`flex: 0 0 auto`, 68 px) | 68 |
//! | `total_basis` | 624 |
//! | `free_space` = 556 - 624 - 8 (gap) | -76 |
//! | `.field` resolved target | 480 |
//! | `.add` resolved target | 68 |
//!
//! Placement is CORRECT and always was: `.field` spans `[0, 480]` and `.add`
//! spans `[488, 556]`. But `.input` was measured at `width: 100%` = 556 and was
//! never told, so it spans `[0, 556]` -- a 556 px box inside a 480 px flex
//! item, OVERHANGING BY 76 px (= 68 + 8, the Add button's width plus the gap).
//! Its right 68 px sit exactly on top of the Add button, and paint order is DOM
//! order, so `.add` paints over them. At EVERY window width: the overhang is
//! structural, not a narrow-window bug.
//!
//! These tests assert OBSERVED engine output against what CSS requires. Nothing
//! here re-implements the flex algorithm -- a re-implementation would agree
//! with a broken engine and prove nothing.
//!
//! ## What each test is FOR
//!
//! (1) and (2) are FAILING-FIRST reproductions of the defect, on each axis.
//!
//! (3) `free_space_is_zero_needs_no_relayout` is a GATE guard, not a
//! reproduction: it passes before and after, and its job is to go RED if the
//! re-layout is applied at the wrong size or the item is then translated by a
//! stale delta. Its rects are pinned EXACTLY rather than relatively.
//!
//! **What (3) cannot do, measured not assumed:** it does NOT catch the gate
//! being REMOVED. Both items declare `width:` (which is what makes the case a
//! genuine no-op — an item with no declared width is measured at the container's
//! 556 px, so the delta is not zero and the case is not a no-op at all), and with
//! a zero delta the ungated re-layout hands `at()` the same size it already
//! measured at, so the numbers agree. The sweep's M1 (gate deleted) is
//! COMPILED-but-GREEN against this whole suite by construction; it is caught by
//! perf only (measured: `layout_us` at 200 todos, 57.4 ms gated). That is a
//! property of what the gate is — a cost guard, not a correctness guard — not a
//! gap in the test.
//!
//! ## A note on the column case's shape
//!
//! (2) makes `.field` a COLUMN container and its `.input` a `flex: 1 1 0` item,
//! rather than giving `.input` `height: 100%`. A percentage main-axis size
//! cannot be used here: `.field` has no declared height, so `height: 100%` on
//! its child resolves against an indefinite containing block and the engine
//! resolves it to the child's own fallback (40) both before AND after the
//! re-layout -- it is a percentage-resolution question, not a stale-subtree
//! one. The nested-container shape exercises the same placement code path
//! (`is_column` writes `rect.h`, the re-layout hands `at()` the resolved HEIGHT)
//! and the pre-fix failure is the same shape of failure: 556 measured, 480
//! resolved, 76 px of overhang.

use velox_dom::{VNode, h, layout::compute_layout};

const VW: i32 = 1200;
const VH: i32 = 900;

type Rect = (i32, i32, i32, i32);

fn rect(n: &velox_dom::layout::LayoutNode) -> Rect {
    (n.rect.x, n.rect.y, n.rect.w, n.rect.h)
}

const ROW: &str = "display:flex; flex-direction:row; gap:8px; width:556px; height:48px;";
const FIELD: &str = "flex: 1 1 auto;";
const ADD: &str = "flex: 0 0 auto; width: 68px; height: 40px;";

/// (1) The composer's case, row axis. `.field` is `flex: 1 1 auto` so its base
/// is its CONTENT size, and because its only child is `width: 100%` that is the
/// whole 556. `.add` is `flex: 0 0 auto` at 68 px, so free space is
/// 556 - 624 - 8 = -76 and `.field` shrinks to 480.
#[test]
fn row_resolved_size_relayouts_the_field_subtree() {
    // `compute_layout` returns the ROOT itself, so the row's items are
    // `laid.children` directly; `.input` is the field's only child.
    let laid = compute_layout(
        &h(
            "div",
            vec![("style", ROW)],
            vec![
                h(
                    "div",
                    vec![("class", "field"), ("style", FIELD)],
                    vec![h(
                        "input",
                        vec![("class", "input"), ("style", "width: 100%; height: 40px;")],
                        vec![],
                    )],
                ),
                h(
                    "button",
                    vec![("class", "add"), ("style", ADD)],
                    vec![VNode::Text("Add".into())],
                ),
            ],
        ),
        VW,
        VH,
    );
    let field = rect(&laid.children[0]);
    let input = rect(&laid.children[0].children[0]);
    let add = rect(&laid.children[1]);

    // The flex algorithm's own arithmetic, which was always right, and the
    // item's own box, which the placement pass has always written correctly.
    //
    // The 48 is `align-items: stretch` on the 48 px row. It is asserted because
    // a re-layout derives the cross axis from the item's CONTENT, so a
    // stretched item comes back un-stretched unless the cross size is carried
    // across -- which it must be, since the cross axis was final long before the
    // resolved main size was known.
    assert_eq!(field, (0, 0, 480, 48), "`.field` resolves to 556 - 68 - 8");
    assert_eq!(add.2, 68, "the Add button is 68 px wide");
    assert_eq!(
        (field.0, add.0),
        (0, 488),
        "main-axis placement was already correct and must stay correct"
    );

    // The defect, pinned on the WHOLE rect rather than the width alone.
    //
    // The width is not enough on its own: a mutation that re-lays the subtree
    // out correctly and then also translates it by the size delta puts the
    // input 76 px to the LEFT at the right width, and only the position
    // assertion catches that. Both halves are asserted, exactly.
    assert_eq!(
        input,
        (0, 0, 480, 40),
        "`.input` fills `.field`: it was measured at `width: 100%` of the 556 px \
         line and never told, so it came out at {} and painted under the Add \
         button by {} px (= the button's {} px plus the 8 px gap)",
        input.2,
        add.2,
        add.2
    );
    // Stated as an overlap rather than a size, because that is the visible bug.
    assert!(
        input.0 + input.2 <= add.0,
        "`.input` spans [{}, {}) and `.add` starts at {} -- the input's right \
         {} px are painted over",
        input.0,
        input.0 + input.2,
        add.0,
        (input.0 + input.2) - add.0
    );
}

/// (2) The same defect on the column axis. `is_column` writes `rect.h`, so the
/// re-layout must hand `at()` the resolved HEIGHT.
#[test]
fn column_resolved_size_relayouts_the_field_subtree() {
    let laid = compute_layout(
        &h(
            "div",
            vec![(
                "style",
                "display:flex; flex-direction:column; gap:8px; width:200px; height:556px;",
            )],
            vec![
                h(
                    "div",
                    vec![
                        ("class", "field"),
                        (
                            "style",
                            "flex: 1 1 auto; display:flex; flex-direction:column;",
                        ),
                    ],
                    vec![h(
                        "input",
                        vec![("class", "input"), ("style", "flex: 1 1 0; width: 40px;")],
                        vec![],
                    )],
                ),
                h(
                    "button",
                    vec![
                        ("class", "add"),
                        ("style", "flex: 0 0 auto; height: 68px; width: 40px;"),
                    ],
                    vec![VNode::Text("Add".into())],
                ),
            ],
        ),
        VW,
        VH,
    );
    let field = rect(&laid.children[0]);
    let input = rect(&laid.children[0].children[0]);
    let add = rect(&laid.children[1]);

    assert_eq!(add, (0, 488, 40, 68), "the Add button is 68 px tall");
    assert_eq!(field, (0, 0, 200, 480), "`.field` resolves to 556 - 68 - 8");
    assert_eq!((field.1, add.1), (0, 488), "main-axis placement is correct");

    assert_eq!(
        input,
        (0, 0, 40, 480),
        "`.input` fills `.field` on the column axis: it was measured at {} and \
         never told, so it left {} px of `.field` empty at the bottom",
        input.3,
        field.3 - input.3
    );
    assert!(
        input.1 + input.3 <= add.1,
        "`.input` spans [{}, {}) and `.add` starts at {}",
        input.1,
        input.1 + input.3,
        add.1
    );
}

/// (3) THE GATE GUARD. Free space is exactly zero AND both items declare their
/// own main size, so every item's resolved main size equals the size its own
/// box was measured at, and the re-layout must not run at all.
///
/// This passes before and after the change -- that is the point. It goes RED if
/// the re-layout is ever applied when the resolved size already matched, applied
/// at the wrong size, or if the item is then translated by a stale delta.
///
/// Note that a declared `width` is what makes this a no-op. The measure pass
/// hands a WIDTH-LESS item the container's full main size, so an item that only
/// declares `flex: 0 0 480px` is still measured at 556 and is NOT a no-op --
/// which is the same defect (1) is about, and the reason this test declares
/// `width` on both items rather than only a flex basis.
#[test]
fn free_space_is_zero_needs_no_relayout() {
    // 480 + 68 + 8 = 556, so free_space == 0 exactly.
    let laid = compute_layout(
        &h(
            "div",
            vec![("style", ROW)],
            vec![
                h(
                    "div",
                    vec![(
                        "style",
                        "width: 480px; flex: 0 0 auto; display:flex; flex-direction:row;",
                    )],
                    vec![h(
                        "input",
                        vec![("style", "width: 100%; height: 40px;")],
                        vec![],
                    )],
                ),
                h(
                    "button",
                    vec![("style", "width: 68px; flex: 0 0 auto; height: 40px;")],
                    vec![VNode::Text("Add".into())],
                ),
            ],
        ),
        VW,
        VH,
    );
    let field = rect(&laid.children[0]);
    let input = rect(&laid.children[0].children[0]);
    let add = rect(&laid.children[1]);

    assert_eq!(field, (0, 0, 480, 48));
    assert_eq!(add, (488, 0, 68, 40));
    assert_eq!(
        input,
        (0, 0, 480, 40),
        "no-op path: the subtree rects must be bit-identical"
    );
}
