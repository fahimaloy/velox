use velox_dom::{h, text};

#[test]
fn a11y_tree_includes_roles_and_names() {
    let vnode = h(
        "div",
        (),
        vec![
            h("button", vec![("aria-label", "Submit")], vec![text("Submit")]),
            h("img", vec![("alt", "Logo"), ("style", "width:10px;height:10px")], vec![]),
        ],
    );

    let tree = velox_renderer::build_a11y_tree(&vnode, 200, 100);
    assert_eq!(tree.root.role, "group");
    assert_eq!(tree.root.children.len(), 2);
    assert_eq!(tree.root.children[0].role, "button");
    assert_eq!(tree.root.children[0].name, "Submit");
    assert_eq!(tree.root.children[1].role, "image");
    assert_eq!(tree.root.children[1].name, "Logo");
}
