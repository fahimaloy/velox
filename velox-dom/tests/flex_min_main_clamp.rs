//! The flex automatic minimum size, main axis: the target main size must be
//! clamped by `min-width`/`min-height` UNCONDITIONALLY, not only inside the
//! grow and shrink branches of css-flexbox-1 §9.7.3.
//!
//! Both tests assert OBSERVED engine output against what the spec requires.
//! Neither re-implements the flex algorithm; a re-implementation would agree
//! with a broken engine and prove nothing.
//!
//! ## The defect
//!
//! The resolve loop clamped the growing/shrunk main size inside two nested
//! conditionals, so three degenerate paths reached the line's main-size sum
//! with the RAW, UNCLAMPED base size still sitting in `flex_basis`:
//!
//!   1. `free_space < 0.0` and `total_shrink == 0.0` — every `flex-shrink: 0`,
//!      or every base is `0` so `flex_shrink * flex_basis` sums to `0`;
//!   2. `free_space > 0.0` and `total_grow == 0.0` — every `flex-grow: 0`;
//!   3. `free_space == 0.0` exactly — neither `> 0.0` nor `< 0.0` holds.
//!
//! Test (a) is case 1, and it is the shape real code hits: a `flex: 1 1 0`
//! item with a `min-width` larger than the container. `free_space` is
//! negative, `total_shrink` is `1 * 0 == 0`, the branch is skipped, and the
//! unclamped base of `0` is what gets written to `rect.w`.
//!
//! ## What these two tests are FOR
//!
//! They have different jobs and only one of them is about the bug.
//!
//!   - (a) is a FAILING-FIRST regression test. It goes red on the pre-fix tree.
//!   - (b) is an ORDERING GUARD. It passes both before and after, and its job
//!     is to go red if the refactor ever clamps the BASE at read time instead
//!     of the TARGET after resolution. css-flexbox-1 §9.2 says the base size is
//!     NOT min/max-clamped, and §9.7.1 scales shrink by the base. Clamping the
//!     base silently changes the shrink distribution, and only the SUM reveals
//!     it — every individual item still looks plausible. Do not delete (b)
//!     because it is green: it is the guard against the most plausible way
//!     this change could ship a subtle regression that no other test catches.

use velox_dom::{VNode, h, layout::compute_layout};

/// A row flex container of `width` px holding one child with `child_style`.
fn row(width: i32, child_style: &str) -> VNode {
    h(
        "div",
        vec![(
            "style",
            format!("display:flex; flex-direction:row; width:{width}px; height:50px;").as_str(),
        )],
        vec![h("div", vec![("style", child_style)], vec![])],
    )
}

// ---------------------------------------------------------------------------
// (a) Failing-first — the real bug
// ---------------------------------------------------------------------------

/// A `min-width` larger than the container must raise the item's used main
/// size to the floor. css-flexbox-1 §9.7.3 step 4 requires clamping the target
/// main size by min/max, and §9.2's hypothetical main size is what made the
/// free space negative in the first place — so the free space already accounts
/// for a 300px floor, and the item's final size must honour that same floor.
///
/// The assertion is the EXACT value. `>= 300` would also pass a fix that
/// special-cases `total_shrink == 0` and inflates the item to fill the
/// container, which is not what the spec says: `min-width` is a floor, not a
/// target, and the container is OVERFLOWED, not filled.
#[test]
fn a_min_width_larger_than_the_container_floors_the_flex_item() {
    let laid = compute_layout(&row(100, "flex: 1 1 0; min-width: 300px;"), 400, 100);
    let w = laid.children[0].rect.w;
    println!("GATE 0 measured rect.w = {w} (container 100, min-width 300, flex: 1 1 0)");
    assert_eq!(
        w, 300,
        "a 300px min-width on a `flex: 1 1 0` item in a 100px row must floor the \
         used main size at 300, not drop the base 0 through the degenerate \
         `total_shrink == 0` branch"
    );
}

/// The degenerate GROW branch is the same defect from the other side, and it
/// needs a shape that is easy to get wrong: making the free space positive
/// while still binding the `min-width`.
///
/// A single `flex-grow: 0` item with a huge `min-width` does NOT reach the
/// grow branch — the hypothetical is larger than the container, so the free
/// space is NEGATIVE and it lands in the shrink branch that (a) already
/// covers. (That was the first version of this test, and it silently proved
/// nothing new.)
///
/// To skip the grow branch AND have a binding floor, the sum of the
/// HYPOTHETICALS must still fit inside the container while one item's
/// hypothetical exceeds its own base. So: a 300px row, A with
/// `flex: 0 0 0; min-width: 50` (base 0, hypothetical 50) and a sibling
/// `flex: 0 0 0` (base 0, hypothetical 0). Σ hypothetical = 50, so
/// `free_space` is `+250` and positive, and both items have `flex-grow: 0`
/// so `total_grow == 0` — the grow branch is skipped and A's raw base of `0`
/// is what lands in `rect.w`. Spec: 50.
#[test]
fn a_min_width_is_honoured_when_positive_free_space_has_no_grow_factor() {
    let tree = h(
        "div",
        vec![(
            "style",
            "display:flex; flex-direction:row; width:300px; height:50px;",
        )],
        vec![
            h(
                "div",
                vec![("style", "flex: 0 0 0; min-width: 50;")],
                vec![],
            ),
            h("div", vec![("style", "flex: 0 0 0;")], vec![]),
        ],
    );
    let laid = compute_layout(&tree, 400, 100);
    let w = laid.children[0].rect.w;
    println!("degenerate-grow rect.w = {w} (positive free space, total_grow == 0, min-width 50)");
    assert_eq!(
        w, 50,
        "with positive free space but `total_grow == 0` the grow branch is skipped \
         and the unclamped base 0 must not survive to the used main size; the \
         50px min-width governs"
    );
}

// ---------------------------------------------------------------------------
// (b) Ordering guard — distinguishes unclamped-base from clamped-base
// ---------------------------------------------------------------------------

/// The base size must stay UNCLAMPED, and the clamp must happen to the target
/// after grow/shrink resolves. This asserts on the SUM, because the SUM is the
/// only thing that distinguishes the two orderings.
///
/// With the base unclamped (correct, §9.2):
///   Σ hypothetical = 200 + 200 = 400, free = 300 − 400 = −100
///   total_shrink = 1×0 + 1×200 = 200
///   A factor = 0/200 = 0   → target 0 − 0   = 0   → clamp → 200
///   B factor = 200/200 = 1 → target 200 − 100 = 100 → clamp → 100
///   Σ = 300 — the row is exactly filled, no overflow.
///
/// With the base clamped at read (the inversion this guards against):
///   A's base becomes 200, so total_shrink = 1×200 + 1×200 = 400
///   both factors = 0.5, both targets = 150
///   A clamps up to 200, B stays 150
///   Σ = 350 — the row OVERFLOWS its 300px container by 50px.
///
/// The correct outcome is visible in each item's width AND in the sum, so this
/// asserts both: the exact widths, and that the row does not overflow. A fix
/// that clamps at the wrong seam moves the sum, not one item in isolation.
#[test]
fn the_flex_base_size_stays_unclamped_and_only_the_target_is_clamped() {
    let tree = h(
        "div",
        vec![(
            "style",
            "display:flex; flex-direction:row; width:300px; height:50px;",
        )],
        vec![
            h(
                "div",
                vec![("style", "flex: 1 1 0; min-width: 200px;")],
                vec![],
            ),
            h("div", vec![("style", "flex: 1 1 200px;")], vec![]),
        ],
    );
    let laid = compute_layout(&tree, 400, 100);
    let (a, b) = (laid.children[0].rect.w, laid.children[1].rect.w);
    let sum = a + b;
    println!("ordering guard: A={a} B={b} Σ={sum} (correct Σ is 300, inverted Σ is 350)");
    assert_eq!(
        (a, b),
        (200, 100),
        "with an unclamped base A's shrink factor is 0 (target 0, floored to 200) \
         and B's is 1 (target 100); got A={a} B={b}, which is the clamped-base \
         distribution"
    );
    assert_eq!(
        sum, 300,
        "the two items plus the 0 gap must exactly fill the 300px row; Σ={sum} \
         means the base was clamped before the shrink distribution was computed"
    );
}
