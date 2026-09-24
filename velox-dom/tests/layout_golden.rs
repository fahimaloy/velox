use velox_dom::{Props, h, layout::compute_layout, text};

#[test]
fn root_fills_without_explicit_styles() {
    let vnode = h("div", Props::new(), vec![text("hi")]);
    let lo = compute_layout(&vnode, 800, 600);
    assert_eq!(
        lo.rect.w, 800,
        "root should fill avail even without width:100%"
    );
}

#[test]
fn percent_chain_fills_when_parent_definite() {
    let child = h("div", Props::from_inline("height:100%"), vec![]);
    let parent = h("div", Props::from_inline("height:600px"), vec![child]);
    let lo = compute_layout(&parent, 800, 600);
    assert_eq!(
        lo.children[0].rect.h, 600,
        "child 100% should fill definite parent"
    );
}

#[test]
fn root_fills_viewport_with_vw_vh_units() {
    // 100vw / 100vh should be recognized as viewport-filling and fill viewport when root
    let vnode_vw = h("div", Props::from_inline("width:100vw"), vec![]);
    let lo_vw = compute_layout(&vnode_vw, 1024, 768);
    assert_eq!(lo_vw.rect.w, 1024);

    let vnode_vh = h("div", Props::from_inline("height:100vh"), vec![]);
    let lo_vh = compute_layout(&vnode_vh, 1024, 768);
    // root already fills, but explicit viewport height also ensures full height
    assert!(lo_vh.rect.h >= 768);
}

#[test]
fn dvh_and_min_height_viewport_filling() {
    let vnode_dvh = h("div", Props::from_inline("height:100dvh"), vec![]);
    let lo = compute_layout(&vnode_dvh, 800, 600);
    assert!(lo.rect.h >= 600, "100dvh should fill viewport");

    let vnode_min = h("div", Props::from_inline("min-height:100vh"), vec![]);
    let lo_min = compute_layout(&vnode_min, 800, 600);
    assert!(
        lo_min.rect.h >= 600,
        "min-height:100vh should be viewport-filling"
    );

    let vnode_min_pct = h("div", Props::from_inline("min-height:100%"), vec![]);
    let lo_min_pct = compute_layout(&vnode_min_pct, 800, 600);
    // root fills anyway, but check not panics and fills
    assert_eq!(lo_min_pct.rect.w, 800);
}

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
