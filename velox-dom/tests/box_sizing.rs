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
