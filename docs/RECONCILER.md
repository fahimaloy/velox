# `:key` and the velox reconciler

Status: **decided**. This supersedes the note that lived in the doc comment of
`velox_renderer::reconcile_keyed_children`, which no longer exists.

All citations below were re-grepped against the tree this document was written
in. Line numbers move; re-grep before trusting one.

## The decision table

| | |
|---|---|
| **What `:key` does today** | It compiles, its expression is normalized and analyzed, and it lands as a **plain runtime `key` string attribute** on `VNode::Element.props.attrs["key"]`. It does **not** reorder, does **not** diff, and does **not** preserve any per-node identity or state. `velox-sfc/src/template_codegen.rs:93` (`resolve_key_expr`) normalizes the value; the `v-for` emit sites re-insert it as a runtime attribute at `velox-sfc/src/template_codegen.rs:2241`, `:2251`, `:2854`, `:2864`; a non-`v-for` `:key` reaches `velox-renderer`'s view of it through `emit_bind_attr` (`velox-sfc/src/template_codegen.rs:3873`, the `attr_name == "key"` branch). `VNode::key()` at `velox-dom/src/diff.rs:56` reads it back out. |
| **Reconciler A — `reconcile_keyed_children`** | **Deleted.** Was `velox-renderer/src/lib.rs:891` at commit `5bda43d`. It was `pub` and had **zero production callers** — only six test call sites, which is exactly why it never tripped the dead-code linter. On a key match it pushed `old[idx].clone()` and discarded the incoming node, so a keyed child whose text or attributes changed kept the **stale** previous content. Its `used` set was written and never read. |
| **Reconciler B — `diff::diff`** | **Live code, zero production callers.** `pub fn diff` at `velox-dom/src/diff.rs:64`, keyed path in `diff_children_keyed` at `:115`, `Patch::MoveChild` at `:52`. It is complete, duplicate-key safe, and emits reorder as a *move* rather than an insert/remove pair. Only tests call it. |
| **Decision** | **`:key` is formally declared a non-goal for identity and reconciliation.** It is a plain, well-formed, stable-per-render string attribute, and that is its entire contract. Reorder correctness comes from `VNode` child order, not from `:key` — `compute_layout` (`velox-dom/src/layout.rs:3337`) lays children out in `VNode` order, so a reordered list already lays out reordered. |
| **Tradeoff — what is given up** | Template authors writing `:key` get **no state preservation across a reorder**, and this is a real, currently-shipping cost. All cross-frame state is keyed by the structural `path`, never by `key`: `InputTarget.path: Vec<usize>` (`velox-renderer/src/events.rs:68`) restored by `preserve_input_state` (`velox-renderer/src/events.rs:526`, which matches on `p.path == t.path` alone), and scroll offsets as `HashMap<Vec<usize>, f32>` (`velox-renderer/src/lib.rs:1297`). So when a keyed list reorders, focus, caret, selection and scroll position travel with the **position**, not with the **item**. Concretely: an `<input>` inside a `v-for` that gets reordered loses its focus and caret to whichever item now occupies its index. That is the price of the decision, and it is paid today. |
| **What would revisit it** | Any one of: (a) Task 2.2 (layout invalidation) lands and introduces **content-addressed** node identity, which is the only identity mechanism the ruling permits and the only one that could key cross-frame state without a `VNode` field; (b) a filed user-visible bug for focus/caret/scroll loss across a `v-for` reorder; (c) an introduction of a component-instance registry, which would make per-node state exist at all. Revisiting means re-opening this table, not silently changing `diff`. |

## Why a non-goal rather than real identity semantics

The alternative position — that `:key` gains real identity semantics — was
weighed and rejected, because under the standing `VNode` identity ruling it is not
reachable without re-opening that ruling.

Real `:key` semantics would require one of three things, and the ruling blocks
all three:

1. **A state slot on `VNode`.** Explicitly refused. Not on cost grounds: a
   `node_id` field would duplicate the `"key"` attribute that already exists on
   `VNode::Element.props.attrs`, and `VNode` is pure data — no per-node state, no
   instance handle, no lifecycle hook.
2. **A component-instance registry.** A second instance state, a second lifetime
   to get wrong, and a `VNode`-shaped change at the boundary.
3. **Re-keying cross-frame state by `key` instead of by `path`.** This is the
   cheapest route, and it is still wrong here. The ruling settles that cross-frame
   state uses the existing structural `path`, and `preserve_input_state` is built
   on exactly that. Switching the key would silently move every focus, caret and
   scroll offset in the application the first time a template author used `:key`
   inconsistently — a correctness hazard traded for a feature nobody has asked
   for.

A non-goal is also the honest reading of the architecture rather than a retreat.
Reconciler B is complete and correct, and it is unused — not because it is
unfinished, but because **there is no live consumer to feed**. The render loop is
immediate-mode: it paints a `&VNode` handed to it each frame
(`run_window_vnode_skia` at `velox-renderer/src/lib.rs:1566`,
`run_window_vnode_skia_with_hmr` at `:2133`). There is no retained tree, no patch
applier, and no previous tree to diff *against* in the first place. Reconciliation
is a technique for mutating a live tree in place, and velox does not keep one.

## What "non-goal" is not

The failure mode this decision exists to prevent is not "`:key` is inert". It is
"`:key` is inert **and nobody wrote that down**", which is the state this
repository was in: the only record of the non-goal was a module doc inside
`velox-dom/src/diff.rs` and a doc comment on a function no production code
called. A template author had no way to learn it and no way to plan around it.

So the non-goal is declared with teeth:

- It is written down here, and linked from the module that implements the
  reconciler (`velox-dom/src/diff.rs`) and from the codegen that emits the
  attribute.
- It is **pinned by a test**: `velox-dom/tests/key_reorder_layout.rs` asserts
  that a reordered tree produces reordered geometry
  (`reordering_the_vnode_children_reorders_the_geometry`, `:36`; and
  `a_reorder_is_visible_in_the_geometry_and_not_only_in_the_tree`, `:42`) and
  that the key value itself is inert to layout
  (`the_key_attr_does_not_influence_layout`, `:53`). Those three tests are the
  executable form of this decision. They are the tests that fail if someone
  quietly gives `key` layout meaning.
- It has a stated revisit trigger, above.

The honest caveat: `:key` still does not warn when used. A template author who
writes `:key` today gets no diagnostic, only this document. Adding a
compile-time warning is a deliberate, separate change with a migration cost for
every existing template in the wild; it is recorded in the task report as
recommended follow-up rather than smuggled into this change.

## Why reconciler A was deleted rather than fixed

Fixing A would have meant keeping two keyed reconcilers in one codebase — a
correct one in `velox-dom` and an incorrect one in `velox-renderer` — and the
incorrect one had the more discoverable name. It was `pub`, so `rustc` never
reported it, and its only callers were tests, so no exercise of the live path
would ever have caught it. Deleting it also removed the only test in the
repository that asserted discarding-new-content as intended behaviour.

Reconciler B is retained. It is correct, it is tested, and it is the thing a
future step 3 would wire in. Deleting A does not delete the capability.

## Preconditions for wiring `diff::diff` into the render loop (not done here)

Task 4.6 step 3 — wiring `diff::diff` into the render loop so `:key` reorders and
preserves state — is **not implemented**, and is blocked. It is deferred pending
Task 2.2 (layout invalidation), not cancelled. Before it may be attempted, all of
the following must be true:

1. **An identity that survives a reorder must exist and be decided.** Today a
   reorder changes every descendant's `path`, so `preserve_input_state`
   (`velox-renderer/src/events.rs:526`) matches nothing and all cross-frame
   state is lost. The decision must name the identity source, and under the
   standing ruling it must be content-addressed (a hash of the node's own content
   plus its layout inputs) and must never be a `source_index` or child ordinal,
   because those shift when a sibling is inserted. This is the precondition the
   current work explicitly does **not** satisfy — which is why the decision above
   is a non-goal.
2. **The consumer for `Patch::MoveChild` must be defined.** The renderer keeps no
   retained tree, so there is nothing for a move to move. Either a patch applier
   is written, or the diff is used *only* to compute an old→new `path` remap for
   the state maps, with `InsertChild` / `RemoveChild` / `Replace` ignored.
3. **The interaction with layout caching must be settled by Task 2.2.** If layout
   is recomputed wholesale each frame, a reorder is already handled by
   `compute_layout` and the diff is needed only for state remapping. If layout is
   to be cached and invalidated incrementally, how `MoveChild` interacts with
   that cache must be specified first.
4. **Duplicate-key semantics must be specified at the call site.**
   `diff_children_keyed` (`velox-dom/src/diff.rs:115`) is duplicate-key safe — it
   tracks a consumed set and falls through to insert — but what a *duplicate*
   `:key` means to a template author is currently unspecified, and nothing
   reports one.
5. **The vnode-identity ruling must still hold.** No new `VNode` field. If
   implementing this seems to require one, that is the signal to stop and reopen
   the ruling explicitly, not to add the field.
