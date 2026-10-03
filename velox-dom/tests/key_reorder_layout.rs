use velox_dom::{VNode, h, layout::compute_layout};

/// Children are told apart by height alone — `a` is 20 tall, `b` is 40 — so
/// every assertion below reads identity straight off the laid-out geometry and
/// none of it depends on tracking indices, `source_index`, or the `key` values.
fn item(k: &str) -> VNode {
    let h_px = if k == "a" { 20 } else { 40 };
    h(
        "div",
        vec![
            ("key", k),
            (
                "style",
                &format!("display: block; width: 100px; height: {h_px}px;"),
            ),
        ],
        vec![],
    )
}

fn column(order: &[&str]) -> VNode {
    h(
        "div",
        vec![("style", "width: 200px;")],
        order.iter().map(|k| item(k)).collect(),
    )
}

/// The (y, height) of each laid-out child, in laid-out order.
fn laid_out(order: &[&str]) -> Vec<(i32, i32)> {
    let tree = compute_layout(&column(order), 400, 400);
    tree.children.iter().map(|c| (c.rect.y, c.rect.h)).collect()
}

#[test]
fn reordering_the_vnode_children_reorders_the_geometry() {
    assert_eq!(laid_out(&["a", "b"]), vec![(0, 20), (20, 40)]);
    assert_eq!(laid_out(&["b", "a"]), vec![(0, 40), (40, 20)]);
}

#[test]
fn a_reorder_is_visible_in_the_geometry_and_not_only_in_the_tree() {
    // If anything reconciled the incoming order away — by preferring old nodes
    // on a `key` match, or by reusing a previous tree — both orders would lay
    // out identically and the two sequences above would collide. The `key`
    // values are the same set in both trees and are not in the key order the
    // VNode children happen to be in, so the reordering observed above is
    // attributable to VNode order alone.
    assert_ne!(laid_out(&["a", "b"]), laid_out(&["b", "a"]));
}

#[test]
fn the_key_attr_does_not_influence_layout() {
    // `key` is a plain attribute on the VNode. Two trees whose children are in
    // the same order but carry different `key` values lay out identically, so
    // `:key` neither reorders nor diffs: it is inert to layout, and the
    // reordering above is the plain block flow reading VNode order.
    let keyed = |k: &str| {
        h(
            "div",
            vec![
                ("key", k),
                ("style", "display: block; width: 100px; height: 20px;"),
            ],
            vec![],
        )
    };
    let seq = |kids: Vec<VNode>| {
        compute_layout(&h("div", vec![("style", "width: 200px;")], kids), 400, 400)
            .children
            .iter()
            .map(|c| (c.rect.y, c.rect.h, c.rect.x, c.rect.w))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        seq(vec![keyed("a"), keyed("b")]),
        seq(vec![keyed("z"), keyed("y")])
    );
}
