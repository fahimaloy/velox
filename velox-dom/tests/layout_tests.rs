use velox_dom::{h, Props, layout::{compute_layout, Rect}};

#[test]
fn block_stacks_children_and_uses_style_size() {
    // Use block-level divs instead of text nodes to test vertical stacking
    let root = h(
        "div",
        Props::new().set("style", "width: 300px; height: 100px;"),
        vec![
            h("div", Props::new().set("style", "height: 20px;"), vec![]),
            h("div", Props::new().set("style", "height: 30px;"), vec![]),
        ],
    );
    let lt = compute_layout(&root, 800, 600);
    
    assert_eq!(lt.rect, Rect { x: 0, y: 0, w: 300, h: 100 });
    assert_eq!(lt.children.len(), 2);
    // First child at y=0 (with padding)
    assert_eq!(lt.children[0].rect.y, 0);
    // Second child should be below first (y >= first child's height)
    assert!(lt.children[1].rect.y >= lt.children[0].rect.h, 
        "Expected children[1].rect.y ({}) >= children[0].rect.h ({})", 
        lt.children[1].rect.y, lt.children[0].rect.h);
}
