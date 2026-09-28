//! REGRESSION GUARD for the content-basis flex PROBE PASSES.
//!
//! `velox-dom/src/layout.rs` used to take THREE `at()` descents per nesting
//! level for a content-basis flex item: an unconditional pre-layout, then a
//! wide probe, then a final layout at the clamped content width. The
//! pre-layout's result was read ONLY on the `!basis_is_content` arm, so on the
//! content-basis arm it was a full subtree layout computed and thrown away --
//! a cost of 3^depth subtree layouts across nested flex containers, of which
//! one descent in three was pure waste.
//!
//! The fix reorders so the pre-layout runs ONLY when `!basis_is_content`,
//! leaving a content-basis item with TWO descents per level (probe + target),
//! i.e. 2^depth.
//!
//! Because the removed descent's result was never read, the reordering is
//! behaviour-preserving BY CONSTRUCTION -- which is exactly why the rects
//! pinned below are bit-identical before and after the change. They are a
//! record of that invariance, not evidence of the speed-up.
//!
//! WHAT THIS TEST DOES AND DOES NOT GUARD
//! It is a BIT-EXACTNESS guard on the reorder. It is NOT a performance
//! regression guard, and it CANNOT detect a reintroduced descent: pinning
//! bit-exact rects is invariant to how many times a subtree was laid out. If
//! the pre-layout is moved back onto the content-basis arm, every test in this
//! file still passes. This was verified, not assumed: with the discarded
//! descent reinstated, all 5 tests here passed unchanged, while the descent
//! count for `app_tree(3)` rose from 34 to 150. The 3^depth -> 2^depth claim is
//! therefore established by the task-2.1b measurement against a real
//! benchmark, NOT by this unit test. The file's name promises more than it
//! delivers; the header is the honest version of the contract.
//!
//! It also pins the `!basis_is_content` branch (explicit `width` /
//! `flex-basis`), so that arm's 1-pass behaviour stays pinned too.
//!
//! No features, no Skia: `compute_layout` takes inline styles, so the tree can
//! be built directly. Run by default with `cargo test -p velox-dom`.

use velox_dom::{VNode, h, layout::compute_layout, text};

const VW: i32 = 800;
const VH: i32 = 600;

/// The shipped app's shape, with inline styles: `.app` (column) > `.todos`
/// (column) > `.todo-item` (row). `.todo-text` is `flex: 1` and `.btn-danger`
/// declares no width, so BOTH are content-basis; the checkbox declares a width
/// so it is not.
fn app_tree(n: usize) -> VNode {
    let items: Vec<VNode> = (0..n)
        .map(|i| {
            h(
                "div",
                vec![(
                    "style",
                    "display:flex; flex-direction:row; align-items:center; gap:8px;",
                )],
                vec![
                    h("input", vec![("style", "width:18px;")], vec![]),
                    h(
                        "span",
                        vec![("style", "flex: 1; font-size: 15px;")],
                        vec![text(format!("Todo number {i}"))],
                    ),
                    h(
                        "button",
                        vec![("style", "padding: 4px 8px; font-size: 14px;")],
                        vec![text("×")],
                    ),
                ],
            )
        })
        .collect();
    h(
        "div",
        vec![(
            "style",
            "display:flex; flex-direction:column; width:100%; min-height:100vh; padding:16px;",
        )],
        vec![
            h(
                "h1",
                vec![("style", "font-size: 24px; margin: 0 0 12px 0;")],
                vec![text("Todos")],
            ),
            h(
                "div",
                vec![(
                    "style",
                    "display:flex; flex-direction:column; flex:1; gap:8px;",
                )],
                items,
            ),
        ],
    )
}

/// One row with a declared width/height/flex-basis on SOME children, so the
/// `!basis_is_content` arm (which keeps its single pass) is exercised too.
fn declared_row() -> VNode {
    h(
        "div",
        vec![("style", "display:flex; width:400px; gap:10px; padding:5px;")],
        vec![
            h(
                "div",
                vec![("style", "width:60px; height:20px;")],
                vec![text("W")],
            ),
            h(
                "div",
                vec![("style", "flex: 0 0 40px; height:24px;")],
                vec![text("B")],
            ),
            h("div", vec![("style", "flex: 1;")], vec![text("grow")]),
        ],
    )
}

/// Flatten the layout tree to `(depth, source_index, x, y, w, h)` rows so the
/// pinned expectation covers EVERY node, not just the ones a test author
/// happened to think to assert on. Order is the child order of the layout
/// tree, so the whole list is a fingerprint of the layout.
fn flatten(node: &velox_dom::layout::LayoutNode, depth: usize, out: &mut Vec<String>) {
    out.push(format!(
        "d{depth} si{:?} {}x{}+{}+{}",
        node.source_index, node.rect.w, node.rect.h, node.rect.x, node.rect.y
    ));
    for c in &node.children {
        flatten(c, depth + 1, out);
    }
}

fn rects(tree: &VNode) -> Vec<String> {
    let laid = compute_layout(tree, VW, VH);
    let mut rows = Vec::new();
    flatten(&laid, 0, &mut rows);
    rows
}

fn expect(name: &str, got: Vec<String>, want: &[&str]) {
    if got != want {
        eprintln!("--- {name}: MISMATCH ---");
        let n = got.len().max(want.len());
        for i in 0..n {
            let g = got.get(i).map(String::as_str).unwrap_or("<missing>");
            let w = want.get(i).copied().unwrap_or("<missing>");
            if g != w {
                eprintln!("  [{i:>2}] want {w:<28} got {g}");
            }
        }
        panic!("{name}: layout changed");
    }
}

#[test]
fn app_tree_0() {
    expect(
        "app_tree(0)",
        rects(&app_tree(0)),
        &[
            // root: width:100% = 800, + padding 16 either side
            "d0 siNone 832x600+0+0",
            // h1.title: font 24 -> 35 tall, + margin-bottom 12
            "d1 siSome(0) 800x35+16+16",
            "d2 siSome(0) 60x33+16+16",
            "d1 siSome(1) 800x533+16+51",
        ],
    );
}

#[test]
fn app_tree_1() {
    expect(
        "app_tree(1)",
        rects(&app_tree(1)),
        &[
            "d0 siNone 832x600+0+0",
            "d1 siSome(0) 800x35+16+16",
            "d2 siSome(0) 60x33+16+16",
            "d1 siSome(1) 800x533+16+51",
            // .todo-item row: 29 tall, full width
            "d2 siSome(0) 800x29+16+51",
            // input: declared width 18, no content => 0 tall, centred (26+3)
            "d3 siSome(0) 18x0+16+66",
            // span.todo-text: flex:1, content-basis. Probe width is clamped to
            // the line, and the item takes 743 of the 768 left after the
            // fixed-width siblings. The text does NOT wrap.
            "d3 siSome(1) 743x22+42+55",
            "d4 siSome(0) 99x20+42+55",
            // button: no declared width -> content-basis -> 23 wide
            "d3 siSome(2) 23x29+793+51",
            "d4 siSome(0) 7x19+801+56",
        ],
    );
}

/// Three items: the shape that used to cost 3^depth subtree layouts. Every
/// item, at every level, has to land on the same pixel as one item did.
#[test]
fn app_tree_3_is_exactly_repeatable() {
    expect(
        "app_tree(3)",
        rects(&app_tree(3)),
        &[
            "d0 siNone 832x600+0+0",
            "d1 siSome(0) 800x35+16+16",
            "d2 siSome(0) 60x33+16+16",
            "d1 siSome(1) 800x533+16+51",
            "d2 siSome(0) 800x29+16+51",
            "d3 siSome(0) 18x0+16+66",
            "d3 siSome(1) 743x22+42+55",
            "d4 siSome(0) 99x20+42+55",
            "d3 siSome(2) 23x29+793+51",
            "d4 siSome(0) 7x19+801+56",
            // second row is the first row shifted down by exactly one row pitch
            // (29 + 8 gap = 37)
            "d2 siSome(1) 800x29+16+88",
            "d3 siSome(0) 18x0+16+103",
            "d3 siSome(1) 743x22+42+92",
            "d4 siSome(0) 99x20+42+92",
            "d3 siSome(2) 23x29+793+88",
            "d4 siSome(0) 7x19+801+93",
            "d2 siSome(2) 800x29+16+125",
            "d3 siSome(0) 18x0+16+140",
            "d3 siSome(1) 743x22+42+129",
            "d4 siSome(0) 99x20+42+129",
            "d3 siSome(2) 23x29+793+125",
            "d4 siSome(0) 7x19+801+130",
        ],
    );
}

/// Adding an item must not perturb the items above it: rows 0 and 1 of a
/// 3-item tree are byte-for-byte rows 0 and 1 of a 1-item tree. This is the
/// property a wrongly-clamped probe or a leak of the discarded pre-layout into
/// the line sizing would break.
#[test]
fn row_nesting_does_not_perturb_siblings() {
    let one = rects(&app_tree(1));
    let three = rects(&app_tree(3));
    // the 1-item tree is a prefix of the 3-item tree through the first row
    assert_eq!(
        &one[..one.len()],
        &three[..one.len()],
        "a 1-item tree must be a prefix of a 3-item tree"
    );
}

/// The `!basis_is_content` arm: a declared `width`/`height`, a declared
/// `flex-basis`, and a `flex: 1` grower. These took the single pre-layout
/// before the change and must still take exactly one.
#[test]
fn declared_row_keeps_its_single_pass() {
    expect(
        "declared_row",
        rects(&declared_row()),
        &[
            // root 400 + 2*5 padding
            "d0 siNone 410x600+0+0",
            // width:60 height:20 -- declared, not content-basis
            "d1 siSome(0) 60x20+5+5",
            "d2 siSome(0) 8x22+5+5",
            // flex: 0 0 40px height:24
            "d1 siSome(1) 40x24+75+5",
            "d2 siSome(0) 8x22+75+5",
            // flex:1 -> 400 - 2*5 - 60 - 40 - 2*10 gap = 280
            "d1 siSome(2) 280x24+125+5",
            "d2 siSome(0) 32x22+125+5",
        ],
    );
}
