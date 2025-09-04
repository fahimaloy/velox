use velox_dom::{diff::{diff, Patch}, h, text, Props, VNode};

#[test]
fn props_set_and_remove() {
    let a = h("div", vec![("class", "a"), ("id", "x")], vec![]);
    let b = h("div", vec![("class", "b")], vec![]);

    let patches = diff(&a, &b);

    assert!(patches.contains(&Patch::SetAttr("class".into(), "b".into())));
    assert!(patches.contains(&Patch::RemoveAttr("id".into())));
}

#[test]
fn insert_child() {
    let a = h("ul", Props::new(), vec![]);
    let b = h("ul", Props::new(), vec![text("item")]);

    let patches = diff(&a, &b);

    assert_eq!(patches, vec![Patch::InsertChild(0, text("item"))]);
}

#[test]
fn remove_child() {
    let a = h("ul", Props::new(), vec![text("a"), text("b")]);
    let b = h("ul", Props::new(), vec![text("a")]);

    let patches = diff(&a, &b);

    // expect removal of index 1
    assert!(patches.contains(&Patch::RemoveChild(1)));
}

#[test]
fn replace_on_tag_change() {
    let a = h("div", Props::new(), vec![]);
    let b = h("span", Props::new(), vec![]);

    let patches = diff(&a, &b);
    assert_eq!(patches, vec![Patch::Replace(b.clone())]);
}

#[test]
fn text_change_replaces() {
    let a = text("hello");
    let b = text("world");
    let patches = diff(&a, &b);
    assert_eq!(patches, vec![Patch::Replace(b.clone())]);
}

