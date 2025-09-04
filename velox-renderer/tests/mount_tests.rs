use velox_dom::{h, text, Props};
use velox_renderer::Renderer;

#[test]
fn mount_counts_nodes_and_texts() {
    let vnode = h(
        "div",
        Props::new().set("class", "app"),
        vec![text("hi"), h("span", Props::new(), vec![text("there")])],
    );
    let r = velox_renderer::new_selected_renderer();
    let tree = r.mount(&vnode);
    assert_eq!(tree.node_count, 4, "div + text + span + text");
    assert_eq!(tree.text_count, 2);
}

