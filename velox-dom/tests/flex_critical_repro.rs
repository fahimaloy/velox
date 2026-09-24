use velox_dom::{Props, h, layout::compute_layout};

#[test]
fn repro_wrap_reverse_reverses_cross_order() {
    // Container 200px wide forces wrapping: 3 items of 80px each => 2 on line0, 1 on line1
    // With wrap, second line Y > first line. With wrap-reverse, order reversed => first items should be below last.
    let normal = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; flex-wrap: wrap; width: 200px; height: 200px;",
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
    let rev = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; flex-wrap: wrap-reverse; width: 200px; height: 200px;",
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
    let lt_normal = compute_layout(&normal, 800, 600);
    let lt_rev = compute_layout(&rev, 800, 600);
    assert_eq!(lt_normal.children.len(), 3);
    assert_eq!(lt_rev.children.len(), 3);
    // In normal wrap, item 0 and 1 on first line (y ~0), item 2 on second line (y larger)
    let n_y0 = lt_normal.children[0].rect.y;
    let n_y2 = lt_normal.children[2].rect.y;
    assert!(
        n_y2 > n_y0,
        "normal wrap second line should be below first: y0={} y2={}",
        n_y0,
        n_y2
    );
    // In wrap-reverse, cross order reversed => item 0,1 should be BELOW item 2
    let r_y0 = lt_rev.children[0].rect.y;
    let r_y2 = lt_rev.children[2].rect.y;
    assert!(
        r_y0 > r_y2,
        "wrap-reverse first items should be below last: r_y0={} r_y2={}",
        r_y0,
        r_y2
    );
}

#[test]
fn repro_multi_line_cross_via_cross_offset() {
    // Row wrap second line must be offset on cross axis (Y), not main (X)
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; flex-wrap: wrap; width: 200px; height: 200px;",
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
    assert_eq!(lt.children.len(), 3);
    // Second line item should have same X as first line start (not offset in X), and Y > first
    let y0 = lt.children[0].rect.y;
    let x0 = lt.children[0].rect.x;
    let x2 = lt.children[2].rect.x;
    let y2 = lt.children[2].rect.y;
    assert!(
        y2 > y0,
        "second line Y {} should be > first line Y {}",
        y2,
        y0
    );
    // X of second line first item should be at content start (approx 0), not at previous line's main_offset
    assert!(
        x2 <= x0 + 5,
        "second line X {} should be at start, x0={}",
        x2,
        x0
    );
    assert!(
        x2 >= 0 && x2 < 50,
        "second line X should be near 0, got {}",
        x2
    );
}

#[test]
fn repro_n_zero_no_div_by_zero() {
    // Empty flex container with justify space-around: should not produce Inf/NaN
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; justify-content: space-around; width: 300px; height: 100px;",
        ),
        vec![],
    );
    let lt = compute_layout(&root, 800, 600);
    assert_eq!(lt.children.len(), 0);
    assert!((lt.rect.w as f32).is_finite()); // just ensure not panicking
    // Also single item with space-between should not div-by-zero
    let root2 = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; justify-content: space-between; width: 300px; height: 100px;",
        ),
        vec![h(
            "div",
            Props::new().set("style", "width: 50px; height: 20px;"),
            vec![],
        )],
    );
    let lt2 = compute_layout(&root2, 800, 600);
    assert_eq!(lt2.children.len(), 1);
    assert!(lt2.children[0].rect.x >= 0 && lt2.children[0].rect.x < 300);
    let ws = format!("{:?}", lt2.children[0].rect);
    assert!(
        !ws.contains("inf") && !ws.contains("NaN"),
        "rect should not contain inf/nan: {}",
        ws
    );
}

#[test]
fn repro_nan_inf_sanitized() {
    // 1e40 overflows f32 -> Inf, should be sanitized to 0 not Inf rect
    let root = h(
        "div",
        Props::new().set("style", "display: flex; width: 300px; height: 100px;"),
        vec![h(
            "div",
            Props::new().set("style", "width: 1e40px; height: 20px;"),
            vec![],
        )],
    );
    let lt = compute_layout(&root, 800, 600);
    assert_eq!(lt.children.len(), 1);
    let rect = lt.children[0].rect;
    // width should be sanitized (0 or clamped) not Inf
    assert!(
        (rect.w as f32).is_finite(),
        "rect.w should be finite, got {}",
        rect.w
    );
    assert!(
        rect.w >= 0 && rect.w < i32::MAX / 2,
        "rect.w unreasonable: {}",
        rect.w
    );
    assert!(!format!("{:?}", rect).contains("inf"));

    // Also test opacity inf and flex-grow inf not crash
    let root2 = h(
        "div",
        Props::new().set("style", "display: flex; width: 300px; height: 100px;"),
        vec![h(
            "div",
            Props::new().set(
                "style",
                "width: 50px; height: 20px; flex-grow: 1e40; opacity: 1e40;",
            ),
            vec![],
        )],
    );
    let lt2 = compute_layout(&root2, 800, 600);
    assert_eq!(lt2.children.len(), 1);
    assert!((lt2.children[0].rect.w as f32).is_finite());
}

#[test]
fn repro_max_height_zero_clamps() {
    // max-height:0 should collapse to 0, not be treated as infinity
    let root = h(
        "div",
        Props::new().set("style", "width: 200px; height: 100px; max-height: 0px;"),
        vec![h("div", Props::new().set("style", "height: 50px;"), vec![])],
    );
    let lt = compute_layout(&root, 800, 600);
    // outer rect h should be 0 (clamped)
    assert_eq!(
        lt.rect.h, 0,
        "max-height:0 should clamp to 0, got {}",
        lt.rect.h
    );

    // also max-height inside flex
    let root2 = h(
        "div",
        Props::new().set("style", "display: block; width: 200px; height: 100px;"),
        vec![h(
            "div",
            Props::new().set("style", "height: 100px; max-height: 0px;"),
            vec![],
        )],
    );
    let lt2 = compute_layout(&root2, 800, 600);
    // child should be 0 height
    assert_eq!(
        lt2.children[0].rect.h, 0,
        "child max-height:0 should be 0, got {}",
        lt2.children[0].rect.h
    );
}
