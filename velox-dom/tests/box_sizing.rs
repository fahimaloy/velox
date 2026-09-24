use velox_dom::{Props, h, layout::compute_layout, text};

#[test]
fn flex_column_min_height_definite_children_shrink() {
    let lo = compute_layout(
        &h(
            "div",
            Props::from_inline("display:flex; flex-direction:column; min-height:100vh"),
            vec![h(
                "div",
                Props::from_inline("height:auto"),
                vec![text("hi")],
            )],
        ),
        800,
        600,
    );
    assert!(
        lo.children[0].rect.h < 600,
        "child should shrink-to-content, not stretch to 600, got {}",
        lo.children[0].rect.h
    );
}

#[test]
fn border_box_avail_subtracts_correctly() {
    let lo = compute_layout(
        &h(
            "div",
            Props::from_inline("box-sizing:border-box; width:100%; padding:20px; border:4px solid"),
            vec![],
        ),
        800,
        600,
    );
    assert_eq!(
        lo.rect.w, 800,
        "border-box outer width must be 800, got {}",
        lo.rect.w
    );
    assert_eq!(lo.children.is_empty(), true);
}

#[test]
fn margin_auto_centers_block() {
    let lo = compute_layout(
        &h(
            "div",
            Props::from_inline("width:200px; margin:0 auto"),
            vec![],
        ),
        800,
        600,
    );
    assert_eq!(lo.rect.x, 300); // (800-200)/2
}

#[test]
fn margin_auto_left_only_absorbs_free_space() {
    // margin-left:auto with a fixed right margin resolves left = free.
    let lo = compute_layout(
        &h(
            "div",
            Props::from_inline("width:200px; margin-left:auto; margin-right:40px"),
            vec![],
        ),
        800,
        600,
    );
    // free = 800 - 200 - 40 = 560
    assert_eq!(lo.rect.x, 560);
}

#[test]
fn margin_auto_over_wide_box_centers_negative() {
    // CSS 2.1 §10.3.3: auto margins resolve with no clamping, so an over-wide
    // box centers with negative margins exactly like a browser:
    // ml = mr = (800 - 900) / 2 = -50 -> left edge at x = -50.
    let lo = compute_layout(
        &h(
            "div",
            Props::from_inline("width:900px; margin:0 auto"),
            vec![],
        ),
        800,
        600,
    );
    assert_eq!(lo.rect.x, -50); // (800 - 900) / 2
}

#[test]
fn margin_auto_shorthand_keeps_vertical_margins() {
    // `margin: 10px auto` must keep top/bottom at 10px (the shorthand must
    // not be dropped because one side is auto) while left/right center.
    let lo = compute_layout(
        &h(
            "div",
            Props::from_inline("width:200px; margin:10px auto"),
            vec![],
        ),
        800,
        600,
    );
    assert_eq!(lo.rect.x, 300);
    assert_eq!(lo.rect.y, 10);
}

#[test]
fn margin_auto_content_box_subtracts_padding_once() {
    // content-box: outer = 200 + 2*20 = 240; free = 800 - 240 = 560 -> 280 each.
    let lo = compute_layout(
        &h(
            "div",
            Props::from_inline("width:200px; padding:0 20px; margin:0 auto"),
            vec![],
        ),
        800,
        600,
    );
    assert_eq!(lo.rect.x, 280);
}

#[test]
fn margin_auto_border_box_no_double_subtraction() {
    // border-box: declared 200 already contains the padding; free = 600 -> 300.
    let lo = compute_layout(
        &h(
            "div",
            Props::from_inline("box-sizing:border-box; width:200px; padding:0 20px; margin:0 auto"),
            vec![],
        ),
        800,
        600,
    );
    assert_eq!(lo.rect.x, 300);
}

#[test]
fn percent_margin_uses_width_basis() {
    let lo = compute_layout(
        &h(
            "div",
            Props::from_inline("width:400px"),
            vec![h("div", Props::from_inline("margin-top:10%"), vec![])],
        ),
        800,
        600,
    );
    // 10% of containing width 400 = 40, not 10% of height
    let gap_top = lo.children[0].rect.y - lo.rect.y;
    assert_eq!(
        gap_top, 40,
        "margin-top:10% should be 40 (10% of 400), got {}",
        gap_top
    );
}
