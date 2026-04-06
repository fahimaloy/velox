use crate::{Props, VNode};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub enum Patch {
    Replace(VNode),
    SetAttr(String, String),
    RemoveAttr(String),
    UpdateChild(usize, Vec<Patch>),
    InsertChild(usize, VNode),
    RemoveChild(usize),
}

impl VNode {
    pub fn key(&self) -> Option<String> {
        match self {
            VNode::Element { props, .. } => props.attrs.get("key").cloned(),
            _ => None,
        }
    }
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

fn diff_children_keyed(
    old: &[VNode],
    new: &[VNode],
    get_key: impl Fn(&VNode) -> Option<String> + 'static,
) -> Vec<Patch> {
    let mut patches = Vec::new();

    // Build key -> index map for old children
    let mut old_key_map: HashMap<String, usize> = HashMap::new();
    for (i, node) in old.iter().enumerate() {
        if let Some(key) = get_key(node) {
            old_key_map.insert(key, i);
        }
    }

    // Track which old nodes have been used
    let mut used_old_indices: HashSet<usize> = HashSet::new();

    // Process new children
    for (new_idx, new_node) in new.iter().enumerate() {
        if let Some(key) = get_key(new_node)
            && let Some(&old_idx) = old_key_map.get(&key)
        {
            // Key matches - diff at that position
            used_old_indices.insert(old_idx);
            let child_patches = diff(&old[old_idx], new_node);
            if !child_patches.is_empty() {
                patches.push(Patch::UpdateChild(new_idx, child_patches));
            }
            continue;
        }

        // No key or key doesn't match - treat as insert or replace
        if new_idx < old.len() && !used_old_indices.contains(&new_idx) {
            patches.push(Patch::UpdateChild(
                new_idx,
                vec![Patch::Replace(new_node.clone())],
            ));
            used_old_indices.insert(new_idx);
        } else {
            patches.push(Patch::InsertChild(new_idx, new_node.clone()));
        }
    }

    // Remaining old nodes that weren't matched are removals
    for i in 0..old.len() {
        if !used_old_indices.contains(&i) {
            patches.push(Patch::RemoveChild(i));
        }
    }

    patches
}

fn diff_children(a: &[VNode], b: &[VNode]) -> Vec<Patch> {
    let has_keys = b.iter().any(|n| n.key().is_some());
    if has_keys {
        return diff_children_keyed(a, b, |n| n.key());
    }

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
