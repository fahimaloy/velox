use velox_dom::{
    Props, VNode,
    diff::{Patch, diff},
    h, text,
};

// =============================================================================
// Keyed children diff tests
// =============================================================================

#[test]
fn diff_keyed_children_reorder() {
    // old: a, b, c
    // new: c, a, b
    let old = h(
        "ul",
        (),
        vec![
            h("li", vec![("key", "a")], vec![text("A")]),
            h("li", vec![("key", "b")], vec![text("B")]),
            h("li", vec![("key", "c")], vec![text("C")]),
        ],
    );
    let new = h(
        "ul",
        (),
        vec![
            h("li", vec![("key", "c")], vec![text("C")]),
            h("li", vec![("key", "a")], vec![text("A")]),
            h("li", vec![("key", "b")], vec![text("B")]),
        ],
    );

    let patches = diff(&old, &new);
    // Reordering [a,b,c] -> [c,a,b] with identical content is expressed as a
    // single move (node keyed "c" relocates from old index 2 to new index 0),
    // preserving each keyed node's identity. No Insert/Remove is emitted.
    assert_eq!(patches, vec![Patch::MoveChild(2, 0)]);
}

#[test]
fn diff_keyed_children_reorder_with_updates() {
    // old: a, b, c   new: c, a, b  (reorder) with changed text on every node
    let old = h(
        "ul",
        (),
        vec![
            h("li", vec![("key", "a")], vec![text("A")]),
            h("li", vec![("key", "b")], vec![text("B")]),
            h("li", vec![("key", "c")], vec![text("C")]),
        ],
    );
    let new = h(
        "ul",
        (),
        vec![
            h("li", vec![("key", "c")], vec![text("C2")]),
            h("li", vec![("key", "a")], vec![text("A2")]),
            h("li", vec![("key", "b")], vec![text("B2")]),
        ],
    );

    let patches = diff(&old, &new);

    // "c" moves to the front...
    assert!(patches.contains(&Patch::MoveChild(2, 0)));
    // ...and every keyed node is updated in place at its new index. The whole
    // diff is a pure reorder + in-place updates — no Insert/Remove — so keyed
    // identity is preserved across the reorder.
    assert!(patches.iter().all(|p| matches!(
        p,
        Patch::MoveChild(_, _) | Patch::UpdateChild(_, _)
    )));
}

#[test]
fn diff_keyed_children_insert_in_middle() {
    let old = h(
        "ul",
        (),
        vec![
            h("li", vec![("key", "a")], vec![text("A")]),
            h("li", vec![("key", "c")], vec![text("C")]),
        ],
    );
    let new = h(
        "ul",
        (),
        vec![
            h("li", vec![("key", "a")], vec![text("A")]),
            h("li", vec![("key", "b")], vec![text("B")]),
            h("li", vec![("key", "c")], vec![text("C")]),
        ],
    );

    let patches = diff(&old, &new);
    // Insert in the middle: only the brand-new "b" node is inserted at index 1;
    // the keyed "a" and "c" nodes are matched by key (no-op content), so no
    // insert/remove is emitted for them.
    let b_node = h("li", vec![("key", "b")], vec![text("B")]);
    assert_eq!(patches, vec![Patch::InsertChild(1, b_node)]);
}

#[test]
fn diff_keyed_children_remove_from_middle() {
    let old = h(
        "ul",
        (),
        vec![
            h("li", vec![("key", "a")], vec![text("A")]),
            h("li", vec![("key", "b")], vec![text("B")]),
            h("li", vec![("key", "c")], vec![text("C")]),
        ],
    );
    let new = h(
        "ul",
        (),
        vec![
            h("li", vec![("key", "a")], vec![text("A")]),
            h("li", vec![("key", "c")], vec![text("C")]),
        ],
    );

    let patches = diff(&old, &new);
    // Removing "b" from the middle: "c" moves up (2 -> 1) and the leftover "b"
    // at live index 2 is removed.
    assert_eq!(patches, vec![Patch::MoveChild(2, 1), Patch::RemoveChild(2)]);
}

#[test]
fn diff_keyed_children_full_replace() {
    let old = h(
        "ul",
        (),
        vec![
            h("li", vec![("key", "a")], vec![text("A")]),
            h("li", vec![("key", "b")], vec![text("B")]),
        ],
    );
    let new = h(
        "ul",
        (),
        vec![
            h("li", vec![("key", "x")], vec![text("X")]),
            h("li", vec![("key", "y")], vec![text("Y")]),
        ],
    );

    let patches = diff(&old, &new);
    // All keys differ: every old node is removed and every new node inserted.
    // The two RemoveChild(2) refer to the two old nodes, which collapse onto
    // live index 2 as each is removed in sequence.
    let x = h("li", vec![("key", "x")], vec![text("X")]);
    let y = h("li", vec![("key", "y")], vec![text("Y")]);
    assert_eq!(
        patches,
        vec![
            Patch::InsertChild(0, x),
            Patch::InsertChild(1, y),
            Patch::RemoveChild(2),
            Patch::RemoveChild(2),
        ]
    );
}

#[test]
fn diff_keyed_children_with_nested_updates() {
    let old = h(
        "ul",
        (),
        vec![
            h(
                "li",
                vec![("key", "a")],
                vec![h("span", (), vec![text("old-a")])],
            ),
            h(
                "li",
                vec![("key", "b")],
                vec![h("span", (), vec![text("old-b")])],
            ),
        ],
    );
    let new = h(
        "ul",
        (),
        vec![
            h(
                "li",
                vec![("key", "a")],
                vec![h("span", (), vec![text("new-a")])],
            ),
            h(
                "li",
                vec![("key", "b")],
                vec![h("span", (), vec![text("new-b")])],
            ),
        ],
    );

    let patches = diff(&old, &new);
    // Should have UpdateChild patches for both
    let update_count = patches
        .iter()
        .filter(|p| matches!(p, Patch::UpdateChild(_, _)))
        .count();
    assert_eq!(update_count, 2);
}

#[test]
fn diff_keyed_children_mixed_keyed_and_unkeyed() {
    // When new children have keys but old don't, it falls back to unkeyed diff
    let old = h("ul", (), vec![text("a"), text("b"), text("c")]);
    let new = h(
        "ul",
        (),
        vec![
            h("li", vec![("key", "a")], vec![text("A")]),
            h("li", vec![("key", "b")], vec![text("B")]),
        ],
    );

    let patches = diff(&old, &new);
    assert!(!patches.is_empty());
}

// =============================================================================
// Props diff edge cases
// =============================================================================

#[test]
fn diff_no_prop_change() {
    let a = h("div", vec![("class", "x"), ("id", "y")], vec![]);
    let b = h("div", vec![("class", "x"), ("id", "y")], vec![]);
    let patches = diff(&a, &b);
    assert!(patches.is_empty());
}

#[test]
fn diff_add_prop() {
    let a = h("div", (), vec![]);
    let b = h("div", vec![("class", "x")], vec![]);
    let patches = diff(&a, &b);
    assert_eq!(patches, vec![Patch::SetAttr("class".into(), "x".into())]);
}

#[test]
fn diff_remove_all_props() {
    let a = h("div", vec![("class", "x"), ("id", "y")], vec![]);
    let b = h("div", (), vec![]);
    let patches = diff(&a, &b);
    assert!(patches.contains(&Patch::RemoveAttr("class".into())));
    assert!(patches.contains(&Patch::RemoveAttr("id".into())));
}

#[test]
fn diff_prop_value_change() {
    let a = h("div", vec![("class", "old")], vec![]);
    let b = h("div", vec![("class", "new")], vec![]);
    let patches = diff(&a, &b);
    assert_eq!(patches, vec![Patch::SetAttr("class".into(), "new".into())]);
}

#[test]
fn diff_many_props() {
    let a = h("div", vec![("a", "1"), ("b", "2"), ("c", "3")], vec![]);
    let b = h("div", vec![("a", "1"), ("b", "20"), ("d", "4")], vec![]);
    let patches = diff(&a, &b);
    assert!(patches.contains(&Patch::SetAttr("b".into(), "20".into())));
    assert!(patches.contains(&Patch::SetAttr("d".into(), "4".into())));
    assert!(patches.contains(&Patch::RemoveAttr("c".into())));
    // "a" should not be in patches (same value)
    assert!(
        !patches
            .iter()
            .any(|p| matches!(p, Patch::SetAttr(k, _) if k == "a"))
    );
}

// =============================================================================
// Children diff edge cases
// =============================================================================

#[test]
fn diff_empty_to_single_child() {
    let a = h("div", (), vec![]);
    let b = h("div", (), vec![text("hello")]);
    let patches = diff(&a, &b);
    assert_eq!(patches, vec![Patch::InsertChild(0, text("hello"))]);
}

#[test]
fn diff_single_to_empty() {
    let a = h("div", (), vec![text("hello")]);
    let b = h("div", (), vec![]);
    let patches = diff(&a, &b);
    assert!(patches.contains(&Patch::RemoveChild(0)));
}

#[test]
fn diff_replace_first_child() {
    let a = h("div", (), vec![text("a"), text("b"), text("c")]);
    let b = h("div", (), vec![text("x"), text("b"), text("c")]);
    let patches = diff(&a, &b);
    assert_eq!(patches.len(), 1);
    assert!(matches!(&patches[0], Patch::UpdateChild(0, _)));
}

#[test]
fn diff_replace_last_child() {
    let a = h("div", (), vec![text("a"), text("b"), text("c")]);
    let b = h("div", (), vec![text("a"), text("b"), text("z")]);
    let patches = diff(&a, &b);
    assert_eq!(patches.len(), 1);
    assert!(matches!(&patches[0], Patch::UpdateChild(2, _)));
}

#[test]
fn diff_add_multiple_children() {
    let a = h("div", (), vec![text("a")]);
    let b = h("div", (), vec![text("a"), text("b"), text("c")]);
    let patches = diff(&a, &b);
    let inserts: Vec<_> = patches
        .iter()
        .filter(|p| matches!(p, Patch::InsertChild(_, _)))
        .collect();
    assert_eq!(inserts.len(), 2);
}

#[test]
fn diff_remove_multiple_children() {
    let a = h("div", (), vec![text("a"), text("b"), text("c"), text("d")]);
    let b = h("div", (), vec![text("a")]);
    let patches = diff(&a, &b);
    let removals: Vec<_> = patches
        .iter()
        .filter(|p| matches!(p, Patch::RemoveChild(_)))
        .collect();
    assert_eq!(removals.len(), 3);
}

#[test]
fn diff_completely_different_children() {
    let a = h("div", (), vec![text("a"), text("b")]);
    let b = h("div", (), vec![h("span", (), vec![]), h("p", (), vec![])]);
    let patches = diff(&a, &b);
    assert_eq!(patches.len(), 2);
    assert!(matches!(&patches[0], Patch::UpdateChild(0, _)));
    assert!(matches!(&patches[1], Patch::UpdateChild(1, _)));
}

// =============================================================================
// VNode key method tests
// =============================================================================

#[test]
fn vnode_key_returns_none_for_text() {
    let node = text("hello");
    assert!(node.key().is_none());
}

#[test]
fn vnode_key_returns_none_for_element_without_key() {
    let node = h("div", vec![("class", "x")], vec![]);
    assert!(node.key().is_none());
}

#[test]
fn vnode_key_returns_some_for_element_with_key() {
    let node = h("div", vec![("key", "my-key")], vec![]);
    assert_eq!(node.key(), Some("my-key".to_string()));
}

// =============================================================================
// Large tree tests
// =============================================================================

#[test]
fn diff_large_flat_list() {
    let old_children: Vec<VNode> = (0..100)
        .map(|i| {
            let s = i.to_string();
            h(
                "li",
                vec![("key", s.as_str())],
                vec![text(format!("old-{}", i))],
            )
        })
        .collect();
    let new_children: Vec<VNode> = (0..100)
        .map(|i| {
            let s = i.to_string();
            h(
                "li",
                vec![("key", s.as_str())],
                vec![text(format!("new-{}", i))],
            )
        })
        .collect();

    let old = h("ul", (), old_children);
    let new = h("ul", (), new_children);

    let patches = diff(&old, &new);
    // Should have 100 UpdateChild patches (one per item with changed text)
    assert_eq!(patches.len(), 100);
}

#[test]
fn diff_large_tree_deep() {
    // Create a deeply nested tree (50 levels deep)
    fn make_deep_tree(depth: usize) -> VNode {
        if depth == 0 {
            text("leaf")
        } else {
            h("div", (), vec![make_deep_tree(depth - 1)])
        }
    }
    let a = make_deep_tree(50);
    let b = make_deep_tree(50);

    // Same tree, should have no patches
    let patches = diff(&a, &b);
    assert!(patches.is_empty());

    // Change the leaf
    let c = h("div", (), vec![make_deep_tree(49)]);
    let c = h("div", (), vec![c]);
    let patches2 = diff(&a, &c);
    assert!(!patches2.is_empty());
}

#[test]
fn diff_large_tree_wide() {
    fn make_wide_tree(width: usize) -> VNode {
        let children: Vec<VNode> = (0..width)
            .map(|i| h("span", (), vec![text(i.to_string())]))
            .collect();
        h("div", (), children)
    }

    let a = make_wide_tree(200);
    let b = make_wide_tree(200);

    let patches = diff(&a, &b);
    assert!(patches.is_empty());
}

#[test]
fn diff_large_tree_text_change() {
    fn make_wide_tree_with_prefix(width: usize, prefix: &str) -> VNode {
        let children: Vec<VNode> = (0..width)
            .map(|i| h("span", (), vec![text(format!("{}{}", prefix, i))]))
            .collect();
        h("div", (), children)
    }

    let a = make_wide_tree_with_prefix(100, "a-");
    let b = make_wide_tree_with_prefix(100, "b-");

    let patches = diff(&a, &b);
    // Every text node should be replaced
    assert_eq!(patches.len(), 100);
}

#[test]
fn diff_deeply_nested_element_change() {
    let old = h(
        "div",
        (),
        vec![h(
            "section",
            (),
            vec![h("article", (), vec![h("p", (), vec![text("old")])])],
        )],
    );
    let new = h(
        "div",
        (),
        vec![h(
            "section",
            (),
            vec![h("article", (), vec![h("p", (), vec![text("new")])])],
        )],
    );

    let patches = diff(&old, &new);
    // Should drill down to the text change
    assert_eq!(patches.len(), 1);
    assert!(
        matches!(&patches[0], Patch::UpdateChild(0, inner_patches) if inner_patches.len() == 1)
    );
}

// =============================================================================
// Props builder tests
// =============================================================================

#[test]
fn props_builder_chaining() {
    let props = Props::new()
        .set("class", "btn")
        .set("id", "submit")
        .set("data-x", "1");
    assert_eq!(props.attrs.get("class"), Some(&"btn".to_string()));
    assert_eq!(props.attrs.get("id"), Some(&"submit".to_string()));
    assert_eq!(props.attrs.get("data-x"), Some(&"1".to_string()));
}

#[test]
fn props_default_is_empty() {
    let props = Props::default();
    assert!(props.attrs.is_empty());
}

#[test]
fn props_new_is_empty() {
    let props = Props::new();
    assert!(props.attrs.is_empty());
}
