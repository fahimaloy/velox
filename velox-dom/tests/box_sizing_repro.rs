use velox_dom::{Props, h, layout::compute_layout};

#[test]
fn box_sizing_border_box_width_stays_100() {
    // width:100px padding:10px border:2px border-box => rect.w must stay 100, content_w 76
    let node = h(
        "div",
        Props::new().set("style", "width: 100px; padding: 10px; border: 2px solid black; box-sizing: border-box;"),
        vec![],
    );
    let lt = compute_layout(&node, 800, 600);
    assert_eq!(lt.rect.w, 100, "border-box outer width must stay 100, got {}", lt.rect.w);
    // content width = 100 -20 -4 =76 ; check via child avail? We can infer by checking inner layout via a child
    let root = h(
        "div",
        Props::new().set("style", "width: 300px; height: 200px;"),
        vec![h(
            "div",
            Props::new().set("style", "width: 100px; padding: 10px; border: 2px solid black; box-sizing: border-box;"),
            vec![h("div", Props::new().set("style", "width: 10px; height: 10px;"), vec![])],
        )],
    );
    let lt2 = compute_layout(&root, 800, 600);
    let child = &lt2.children[0];
    assert_eq!(child.rect.w, 100, "border-box child outer stays 100");
    // child's content_w = 76, so its inner child x should be at child.x + border+padding = offset 12
    let inner = &child.children[0];
    // inner rect.x should be child.rect.x + pl+bl = 12 beyond child origin
    assert_eq!(inner.rect.x - child.rect.x, 12, "content offset should be pl(10)+bl(2)=12, got {}", inner.rect.x - child.rect.x);
}

#[test]
fn box_sizing_content_box_width_becomes_124() {
    let node = h(
        "div",
        Props::new().set("style", "width: 100px; padding: 10px; border: 2px solid black; box-sizing: content-box;"),
        vec![],
    );
    let lt = compute_layout(&node, 800, 600);
    assert_eq!(lt.rect.w, 124, "content-box outer width must be 100+20+4=124, got {}", lt.rect.w);
}

#[test]
fn box_sizing_content_box_default_width_becomes_124() {
    // default is content-box, omit box-sizing
    let node = h(
        "div",
        Props::new().set("style", "width: 100px; padding: 10px; border: 2px solid black;"),
        vec![],
    );
    let lt = compute_layout(&node, 800, 600);
    assert_eq!(lt.rect.w, 124, "default content-box outer must be 124, got {}", lt.rect.w);
}

#[test]
fn box_sizing_border_box_height_stays_100() {
    let node = h(
        "div",
        Props::new().set("style", "height: 100px; padding: 10px; border: 2px solid black; box-sizing: border-box;"),
        vec![],
    );
    let lt = compute_layout(&node, 800, 600);
    assert_eq!(lt.rect.h, 100, "border-box outer height must stay 100, got {}", lt.rect.h);
}

#[test]
fn box_sizing_content_box_height_becomes_124() {
    let node = h(
        "div",
        Props::new().set("style", "height: 100px; padding: 10px; border: 2px solid black;"),
        vec![],
    );
    let lt = compute_layout(&node, 800, 600);
    assert_eq!(lt.rect.h, 124, "content-box outer height must be 124, got {}", lt.rect.h);
}

#[test]
fn border_widths_subtracted_from_avail() {
    // parent border+padding should reduce child's available width
    // parent: width 200 border 5 padding 10 => content_w = 200-20-10=170 (border-box? explicit width includes border/padding)
    // Use border-box parent to have deterministic outer
    let parent = h(
        "div",
        Props::new().set("style", "width: 200px; height: 100px; padding: 10px; border: 5px solid black; box-sizing: border-box; display: block;"),
        vec![h("div", Props::new().set("style", "height: 10px;"), vec![])],
    );
    let lt = compute_layout(&parent, 800, 600);
    assert_eq!(lt.rect.w, 200);
    // child with no declared width should fill content_w =170
    let child_w = lt.children[0].rect.w;
    assert_eq!(child_w, 170, "child should fill parent content_w 170, got {}", child_w);
}
