//! Keyed diffing for `VNode` trees.
//!
//! # `:key` is a stated non-goal, not an oversight
//!
//! `:key` compiles, resolves, and lands as a **plain runtime `key` attribute on
//! the `VNode`** — a stable per-render string and nothing more. In the shipped
//! pipeline it does **not** reorder, does **not** diff, and does **not**
//! preserve identity across a reorder.
//!
//! Reordering nonetheless appears to work, and it is worth being precise about
//! why: `compute_layout` lays children out in `VNode` order, so a reordered
//! list already lays out reordered. That is plain block flow over the current
//! tree, not reconciliation. `tests/key_reorder_layout.rs` pins this: two trees
//! with the same keyed children in opposite orders produce different laid-out
//! geometry, and the `key` values are inert to layout.
//!
//! Identity and state preservation across a reorder is **not implemented and
//! not implementable as a codegen or renderer change**: `VNode` is pure data
//! (`Element { tag, props, children } | Text(String)`) with no state cell, no
//! instance handle, and no lifecycle hook, so there is no per-node state for a
//! reorder to preserve or destroy — and consequently no observable that a
//! correct keyed reconciler would produce and a broken one would not.
//! Implementing it means giving `VNode` a state slot or standing up a
//! component-instance registry, which is a public shape change to `VNode`.
//!
//! This module IS that reconciler, and it is complete and duplicate-key safe
//! (`diff_children_keyed` tracks a consumed set). It has no production caller.
//! The renderer's own `reconcile_keyed_children` helper, which was incorrect and
//! reachable from nothing but tests, has been deleted. The project decision that
//! `:key` stays a plain attribute — and what would reopen it — is written down
//! in `docs/RECONCILER.md`.

use crate::{Props, VNode};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub enum Patch {
    Replace(VNode),
    SetAttr(String, String),
    RemoveAttr(String),
    UpdateChild(usize, Vec<Patch>),
    InsertChild(usize, VNode),
    RemoveChild(usize),
    /// Relocate the node currently at `from` (in the live DOM order) to `to`.
    ///
    /// All patch indices — for `UpdateChild`, `InsertChild`, `RemoveChild` and
    /// `MoveChild` — are interpreted against the DOM as it exists *after* the
    /// patches already emitted in the same sequence have been applied. This
    /// lets a single keyed diff express add/remove/update *and* reorder as one
    /// coherent, replayable list while preserving each keyed node's identity
    /// (its DOM element / event listeners / internal state).
    MoveChild(usize, usize),
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
    get_key: impl Fn(&VNode) -> Option<String>,
) -> Vec<Patch> {
    // We simulate the live DOM child list as patches are emitted so that every
    // index (Update / Insert / Remove / Move) refers to the DOM order as it is
    // mutated by the patches already produced before it. This is what allows a
    // single keyed diff to correctly express insert, remove, update AND reorder
    // as one coherent, replayable sequence while preserving each keyed node's
    // identity (its DOM element and any attached state / listeners).
    //
    //   Slot::Old(i)  -> a live node still backed by old child `i`; its DOM
    //                    element can be reused, moved and updated in place.
    //   Slot::New      -> a brand-new node that must be inserted. The node
    //                     itself travels in the InsertChild patch; no payload
    //                     is needed here, and caching a clone would be unused.
    enum Slot {
        Old(usize),
        New,
    }
    let mut sim: Vec<Slot> = (0..old.len()).map(Slot::Old).collect();
    // Tracks old children already consumed by a keyed match so a duplicate key
    // does not cause a single old node to be reused twice.
    let mut used_old: HashSet<usize> = HashSet::new();
    let mut patches: Vec<Patch> = Vec::new();

    for (new_idx, new_node) in new.iter().enumerate() {
        let key = get_key(new_node);

        // Find a still-unused old child carrying the same key, in live order.
        let mut reuse: Option<usize> = None; // index into `sim`
        if let Some(key) = &key {
            for (j, slot) in sim.iter().enumerate() {
                if let Slot::Old(i) = slot
                    && !used_old.contains(i)
                    && get_key(&old[*i]).as_ref() == Some(key)
                {
                    reuse = Some(j);
                    break;
                }
            }
        }

        match reuse {
            Some(cur) => {
                // Reorder: relocate the reused node to its target index.
                let old_idx = match &sim[cur] {
                    Slot::Old(i) => *i,
                    Slot::New => unreachable!("reuse index always points at an Old slot"),
                };
                if cur != new_idx {
                    let slot = sim.remove(cur);
                    sim.insert(new_idx, slot);
                    patches.push(Patch::MoveChild(cur, new_idx));
                }
                used_old.insert(old_idx);
                // Update content in place at its (possibly new) position.
                let child_patches = diff(&old[old_idx], new_node);
                if !child_patches.is_empty() {
                    patches.push(Patch::UpdateChild(new_idx, child_patches));
                }
            }
            None => {
                // No reusable old node (new key, or unkeyed node): insert anew.
                sim.insert(new_idx, Slot::New);
                patches.push(Patch::InsertChild(new_idx, new_node.clone()));
            }
        }
    }

    // Old children that were never matched are genuine removals. Removing each
    // from the live `sim` keeps the indices emitted for later removals correct.
    let mut j = 0;
    while j < sim.len() {
        if let Slot::Old(i) = sim[j]
            && !used_old.contains(&i)
        {
            patches.push(Patch::RemoveChild(j));
            sim.remove(j);
        } else {
            j += 1;
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
