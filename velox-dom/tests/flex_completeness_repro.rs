use velox_dom::{Props, h, layout::compute_layout, text};

fn find_child<'a>(
    layout: &'a velox_dom::layout::LayoutNode,
    idx: usize,
) -> &'a velox_dom::layout::LayoutNode {
    layout
        .children
        .iter()
        .find(|c| c.source_index == Some(idx))
        .expect("child not found")
}

#[test]
fn repro_flex_item_descendants_use_resolved_item_position() {
    let root = h(
        "div",
        Props::new().set(
            "style",
            "width:300px;height:100px;display:flex;\
             justify-content:center;align-items:center;gap:20px;",
        ),
        vec![
            h(
                "button",
                Props::new().set("style", "width:80px;height:30px;text-align:center;"),
                vec![text("A")],
            ),
            h(
                "button",
                Props::new().set("style", "width:80px;height:30px;text-align:center;"),
                vec![text("B")],
            ),
        ],
    );

    let layout = compute_layout(&root, 300, 100);
    let first = find_child(&layout, 0);
    let second = find_child(&layout, 1);

    // Item roots: main cursor + cross alignment still correct.
    assert_eq!(first.rect.x, 60);
    assert_eq!(second.rect.x, 160);
    assert_eq!(first.rect.y, 35);
    assert_eq!(second.rect.y, 35);

    // Descendants must sit inside their own resolved item, centered in it.
    let a = &first.children[0];
    let b = &second.children[0];
    assert_eq!(a.rect.x, first.rect.x + (first.rect.w - a.rect.w) / 2);
    assert_eq!(a.rect.y, first.rect.y + (first.rect.h - a.rect.h) / 2);
    assert_eq!(b.rect.x, second.rect.x + (second.rect.w - b.rect.w) / 2);
    assert_eq!(b.rect.y, second.rect.y + (second.rect.h - b.rect.h) / 2);
}

#[test]
fn repro_space_between_variable_sizes() {
    // container 300, gap 10, 3 items 50,80,60 -> positions 0, 105, 240
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; justify-content: space-between; gap: 10px; width: 300px; height: 100px;",
        ),
        vec![
            h("div", Props::new().set("style", "width: 50px; height: 20px;"), vec![]),
            h("div", Props::new().set("style", "width: 80px; height: 20px;"), vec![]),
            h("div", Props::new().set("style", "width: 60px; height: 20px;"), vec![]),
        ],
    );
    let lt = compute_layout(&root, 800, 600);
    assert_eq!(lt.children.len(), 3);
    let x0 = find_child(&lt, 0).rect.x;
    let x1 = find_child(&lt, 1).rect.x;
    let x2 = find_child(&lt, 2).rect.x;
    // expected as per spec: 0, 105, 240 relative to content_x (which is 0)
    assert!(
        (x1 - 105).abs() <= 2,
        "space-between variable: x1 expected ~105 got {} (x0 {} x2 {})",
        x1,
        x0,
        x2
    );
    assert!(
        (x2 - 240).abs() <= 2,
        "space-between variable: x2 expected ~240 got {} (x0 {} x1 {})",
        x2,
        x0,
        x1
    );
    assert!((x0 - 0).abs() <= 2, "x0 expected 0 got {}", x0);
}

#[test]
fn repro_space_around_variable_sizes() {
    // container 300 gap10 3 items 50,80,60 => inter=30, start=15
    // x0=15, x1=15+50+10+30=105, x2=105+80+10+30=225
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; justify-content: space-around; gap: 10px; width: 300px; height: 100px;",
        ),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 50px; height: 20px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 80px; height: 20px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 60px; height: 20px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);
    let x0 = find_child(&lt, 0).rect.x;
    let x1 = find_child(&lt, 1).rect.x;
    let x2 = find_child(&lt, 2).rect.x;
    assert!((x0 - 15).abs() <= 2, "space-around x0 exp 15 got {}", x0);
    assert!((x1 - 105).abs() <= 2, "space-around x1 exp 105 got {}", x1);
    assert!((x2 - 225).abs() <= 2, "space-around x2 exp 225 got {}", x2);
}

#[test]
fn repro_space_evenly_variable_sizes() {
    // container 300 gap10 3 items 50,80,60 => inter=22.5 ~22/23, start 22
    // free=90, n+1=4 => 22.5 each. x0=22, x1=22+50+10+22=104~105, x2=~216
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; justify-content: space-evenly; gap: 10px; width: 300px; height: 100px;",
        ),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 50px; height: 20px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 80px; height: 20px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 60px; height: 20px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);
    let x0 = find_child(&lt, 0).rect.x;
    let x1 = find_child(&lt, 1).rect.x;
    let x2 = find_child(&lt, 2).rect.x;
    assert!((x0 - 22).abs() <= 3, "space-evenly x0 exp 22 got {}", x0);
    assert!((x1 - 104).abs() <= 3, "space-evenly x1 exp ~104 got {}", x1);
    assert!((x2 - 217).abs() <= 3, "space-evenly x2 exp ~217 got {}", x2);
}

#[test]
fn repro_row_x_includes_scroll() {
    // container with scroll-left 20, padding 5, row flex should subtract scroll
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; width: 300px; height: 100px; padding-left: 5px; scroll-left: 20px; border-left: 2px solid black;",
        ),
        vec![h("div", Props::new().set("style", "width: 50px; height: 20px;"), vec![])],
    );
    let lt = compute_layout(&root, 800, 600);
    let child = find_child(&lt, 0);
    // content_x = elem_x + bl(2) + pl(5)=7, scrolled = 7-20 = -13 relative to viewport origin? Actually elem_x 0, so child x should be -13 + offset
    // Without scrolled offset bug, x would be 7
    // We expect scrolled value: 0+2+5-20 = -13
    assert!(
        child.rect.x == -13,
        "row x should include content_x_scrolled (-13), got {}",
        child.rect.x
    );
}

#[test]
fn repro_flex_basis_auto_vs_0percent() {
    // Two containers: one with flex-grow:1 (auto basis) vs flex:1 (0% basis)
    // Both have two children with widths 50 and 80, container 300
    // flex-grow:1 alone => basis auto uses widths, each grows: 135 and 165
    // flex:1 => basis 0% => each 150 (since total 0)
    let root_auto = h(
        "div",
        Props::new().set("style", "display: flex; width: 300px; height: 100px;"),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 50px; height: 20px; flex-grow: 1;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 80px; height: 20px; flex-grow: 1;"),
                vec![],
            ),
        ],
    );
    let lt_auto = compute_layout(&root_auto, 800, 600);
    let w0_auto = find_child(&lt_auto, 0).rect.w;
    let w1_auto = find_child(&lt_auto, 1).rect.w;
    // auto basis: total 130, free 170 => +85 each => 135,165
    assert!(
        (w0_auto - 135).abs() <= 2,
        "auto basis w0 expected 135 got {}",
        w0_auto
    );
    assert!(
        (w1_auto - 165).abs() <= 2,
        "auto basis w1 expected 165 got {}",
        w1_auto
    );

    let root_zero = h(
        "div",
        Props::new().set("style", "display: flex; width: 300px; height: 100px;"),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 50px; height: 20px; flex: 1;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 80px; height: 20px; flex: 1;"),
                vec![],
            ),
        ],
    );
    let lt_zero = compute_layout(&root_zero, 800, 600);
    let w0_zero = find_child(&lt_zero, 0).rect.w;
    let w1_zero = find_child(&lt_zero, 1).rect.w;
    assert!(
        (w0_zero - 150).abs() <= 2,
        "0% basis w0 expected 150 got {}",
        w0_zero
    );
    assert!(
        (w1_zero - 150).abs() <= 2,
        "0% basis w1 expected 150 got {}",
        w1_zero
    );
}

#[test]
fn repro_order_sort() {
    let root = h(
        "div",
        Props::new().set("style", "display: flex; width: 300px; height: 100px;"),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 50px; height: 20px; order: 2;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 50px; height: 20px; order: 0;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 50px; height: 20px; order: 1;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);
    // visual order should be idx 1 (order0) at x0, idx2 at ~50, idx0 at ~100
    let x0 = find_child(&lt, 0).rect.x;
    let x1 = find_child(&lt, 1).rect.x;
    let x2 = find_child(&lt, 2).rect.x;
    assert!(
        x1 < x2 && x2 < x0,
        "order sort failed: x1 {} x2 {} x0 {} should be x1 < x2 < x0",
        x1,
        x2,
        x0
    );
    assert!(x1 == 0, "first visual should be at 0, got {}", x1);
}

#[test]
fn repro_align_content_center() {
    // container 200x200, row wrap, 3 items 80x40 => 2 lines, free cross 120, center start 60
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; flex-wrap: wrap; align-content: center; width: 200px; height: 200px;",
        ),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 80px; height: 40px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 80px; height: 40px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 80px; height: 40px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);
    let y0 = find_child(&lt, 0).rect.y;
    let y2 = find_child(&lt, 2).rect.y;
    // y0 should be ~60, y2 = 60+40=100
    assert!(
        (y0 - 60).abs() <= 2,
        "align-content center y0 exp 60 got {}",
        y0
    );
    assert!(
        (y2 - 100).abs() <= 2,
        "align-content center y2 exp 100 got {}",
        y2
    );
}

#[test]
fn repro_flex_shorthand_and_flex_flow() {
    // flex: 1 0 100px should give basis 100
    let root = h(
        "div",
        Props::new().set("style", "display: flex; width: 400px; height: 100px;"),
        vec![
            h(
                "div",
                Props::new().set("style", "flex: 1 0 100px; height: 20px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 50px; height: 20px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);
    let w0 = find_child(&lt, 0).rect.w;
    // free after basis: 400-100-50=250, grow distributes to first only (grow1) => 100+250=350
    assert!(
        (w0 - 350).abs() <= 2,
        "flex shorthand 1 0 100px w0 exp 350 got {}",
        w0
    );

    // flex-flow: row wrap + width 200 with 3 items 80 => wraps
    let root2 = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; flex-flow: row wrap; width: 200px; height: 200px;",
        ),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 80px; height: 40px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 80px; height: 40px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 80px; height: 40px;"),
                vec![],
            ),
        ],
    );
    let lt2 = compute_layout(&root2, 800, 600);
    let y2 = find_child(&lt2, 2).rect.y;
    let y0 = find_child(&lt2, 0).rect.y;
    assert!(y2 > y0, "flex-flow wrap y2 {} should be > y0 {}", y2, y0);
}

#[test]
fn repro_baseline_fallback() {
    // baseline should not crash and behaves like flex-start
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; align-items: baseline; width: 300px; height: 100px;",
        ),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 50px; height: 20px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 50px; height: 60px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);
    // both at y 0 (fallback to start)
    let y0 = find_child(&lt, 0).rect.y;
    let y1 = find_child(&lt, 1).rect.y;
    assert!(
        y0 == y1 && y0 == lt.children[0].rect.y,
        "baseline fallback y0 {} y1 {} should be equal and at top",
        y0,
        y1
    );
}
