use crate::{Props, VNode};

#[derive(Debug, Clone, PartialEq)]
pub enum Patch {
    Replace(VNode),
    SetAttr(String, String),
    RemoveAttr(String),
    UpdateChild(usize, Vec<Patch>),
    InsertChild(usize, VNode),
    RemoveChild(usize),
}

pub fn diff(old: &VNode, new: &VNode) -> Vec<Patch> {
    match (old, new) {
        (VNode::Text(a), VNode::Text(b)) => {
            if a != b {
                vec![Patch::Replace(new.clone())]
            } else {
                vec![]
            }
        }
        (
            VNode::Element {
                tag: tag_a,
                props: props_a,
                children: children_a,
            },
            VNode::Element {
                tag: tag_b,
                props: props_b,
                children: children_b,
            },
        ) => {
            if tag_a != tag_b {
                return vec![Patch::Replace(new.clone())];
            }
            let mut patches = Vec::new();
            patches.extend(diff_props(props_a, props_b));
            patches.extend(diff_children(children_a, children_b));
            patches
        }
        _ => vec![Patch::Replace(new.clone())],
    }
}

fn diff_props(a: &Props, b: &Props) -> Vec<Patch> {
    let mut patches = Vec::new();
    // Set new and changed
    for (k, v_new) in &b.attrs {
        match a.attrs.get(k) {
            Some(v_old) if v_old == v_new => {}
            _ => patches.push(Patch::SetAttr(k.clone(), v_new.clone())),
        }
    }
    // Remove missing
    for k in a.attrs.keys() {
        if !b.attrs.contains_key(k) {
            patches.push(Patch::RemoveAttr(k.clone()));
        }
    }
    patches
}

fn diff_children(a: &[VNode], b: &[VNode]) -> Vec<Patch> {
    let mut patches = Vec::new();
    let common = a.len().min(b.len());
    for i in 0..common {
        let child_patches = diff(&a[i], &b[i]);
        if !child_patches.is_empty() {
            patches.push(Patch::UpdateChild(i, child_patches));
        }
    }
    // Inserts
    if b.len() > a.len() {
        for (i, node) in b.iter().enumerate().skip(a.len()) {
            patches.push(Patch::InsertChild(i, node.clone()));
        }
    }
    // Removes
    if a.len() > b.len() {
        for i in (b.len()..a.len()).rev() {
            patches.push(Patch::RemoveChild(i));
        }
    }
    patches
}

