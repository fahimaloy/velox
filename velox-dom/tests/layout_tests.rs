use velox_dom::{h, text, Props, layout::{compute_layout, Rect}};

#[test]
fn block_stacks_children_and_uses_style_size() {
    let root = h(
        "div",
        Props::new().set("style", "width: 300px; height: 100px;"),
        vec![text("hello"), text("world")],
    );
    let lt = compute_layout(&root, 800);
    assert_eq!(lt.rect, Rect { x: 0, y: 0, w: 300, h: 100 });
    assert_eq!(lt.children.len(), 2);
    assert_eq!(lt.children[0].rect.y, 0);
    assert!(lt.children[1].rect.y >= lt.children[0].rect.h);
}

