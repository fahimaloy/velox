use velox_dom::{h, text, VNode};

// Simple test to ensure `reconcile_keyed_children` reorders and prefers
// reusing old nodes when the `key` prop matches. We encode a `uid`
// attribute on old nodes so reused nodes keep that uid after reconciliation.
#[test]
fn reconcile_reorders_and_reuses_by_key() {
    // old children: a(uid=old-a), b(uid=old-b), c(uid=old-c)
    let mut old: Vec<VNode> = vec![
        h("li", vec![("key", "a"), ("uid", "old-a")], vec![text("A")]),
        h("li", vec![("key", "b"), ("uid", "old-b")], vec![text("B")]),
        h("li", vec![("key", "c"), ("uid", "old-c")], vec![text("C")]),
    ];

    // new children: b, a, d (b and a should be reused from `old`, d is new)
    let new: Vec<VNode> = vec![
        h("li", vec![("key", "b")], vec![text("B2")]),
        h("li", vec![("key", "a")], vec![text("A2")]),
        h("li", vec![("key", "d")], vec![text("D")]),
    ];

    // Call reconciliation helper from the renderer crate root
    velox_renderer::reconcile_keyed_children(&mut old, &new);

    // Expect old to be reordered to match new's key order
    assert_eq!(old.len(), 3);

    // old[0] should be the previously-existing node with key "b" (uid old-b)
    match &old[0] {
        VNode::Element { props, children, .. } => {
            assert_eq!(props.attrs.get("key").map(|s| s.as_str()), Some("b"));
            assert_eq!(props.attrs.get("uid").map(|s| s.as_str()), Some("old-b"));
            // since we reused the old node, its child text remains the original "B"
            assert!(matches!(children.get(0), Some(VNode::Text(t)) if t == "B"));
        }
        _ => panic!("expected element at old[0]"),
    }

    // old[1] should be the previously-existing node with key "a"
    match &old[1] {
        VNode::Element { props, .. } => {
            assert_eq!(props.attrs.get("key").map(|s| s.as_str()), Some("a"));
            assert_eq!(props.attrs.get("uid").map(|s| s.as_str()), Some("old-a"));
        }
        _ => panic!("expected element at old[1]"),
    }

    // old[2] is the new node with key "d" and has no uid
    match &old[2] {
        VNode::Element { props, .. } => {
            assert_eq!(props.attrs.get("key").map(|s| s.as_str()), Some("d"));
            assert!(props.attrs.get("uid").is_none());
        }
        _ => panic!("expected element at old[2]"),
    }
}
