# Reconciler

How Velox handles list reordering, what `:key` does today, and the deliberate decision behind it.

## What `:key` does today

`:key` compiles, its expression is normalized and analyzed, and it lands as a **plain runtime `key` string attribute** on `VNode::Element.props.attrs["key"]`. It does **not** reorder, does **not** diff, and does **not** preserve per-node identity or state.

Concretely: `velox-sfc/src/template_codegen.rs:93` (`resolve_key_expr`) normalizes the value; the `v-for` emit sites re-insert it as a runtime attribute (`velox-sfc/src/template_codegen.rs:2241`, `:2251`, `:2854`, `:2864`); a non-`v-for` `:key` reaches the renderer through `emit_bind_attr` (`:3873`, the `attr_name == "key"` branch). `VNode::key()` at `velox-dom/src/diff.rs:56` reads it back out.

## Why there is no production reconciler

The render loop is **immediate-mode**: it paints the `&VNode` handed to it each frame (`run_window_vnode_skia` at `velox-renderer/src/lib.rs:1566`, `run_window_vnode_skia_with_hmr` at `:2133`). There is no retained tree, no patch applier, and no previous tree to diff *against*. Reconciliation is a technique for mutating a live tree in place — and Velox does not keep one.

Two keyed reconcilers have existed:

- **A retired one** (`reconcile_keyed_children`) was **deleted**: it was `pub` with zero production callers (only test call sites), and on a key match it kept the *stale* previous content — a keyed child whose text or attributes changed kept the old version.
- **A live, correct one** — `pub fn diff` at `velox-dom/src/diff.rs:64` (keyed path in `diff_children_keyed` at `:115`, `Patch::MoveChild` at `:52`). It is complete, duplicate-key safe, and emits reorder as a *move* rather than an insert/remove pair. It is retained, tested, and is what a future retained-rendering step would wire in. Today only tests call it.

## Where reorder correctness comes from

Reorder correctness comes from `VNode` child order, not from `:key`: `compute_layout` (`velox-dom/src/layout.rs:3337`) lays children out in `VNode` order, so a reordered list already lays out reordered. It is pinned by tests — `velox-dom/tests/key_reorder_layout.rs` asserts that a reordered tree produces reordered geometry and that the key value itself is inert to layout.

## The tradeoff

Template authors writing `:key` get **no state preservation across a reorder**, and this is a real, currently-shipping cost. All cross-frame state is keyed by the structural `path`, never by `key`:

- Input state is `InputTarget.path: Vec<usize>` (`velox-renderer/src/events.rs:68`), restored by `preserve_input_state` (`velox-renderer/src/events.rs:526`), which matches on `p.path == t.path` alone.
- Scroll offsets live in a `HashMap<Vec<usize>, f32>` (`velox-renderer/src/lib.rs:1297`).

So when a keyed list reorders, focus, caret, selection and scroll position travel with the **position**, not with the **item**: an `<input>` inside a reordered `v-for` loses its focus and caret to whichever item now occupies its index.

## The decision

**`:key` is formally declared a non-goal for identity and reconciliation.** It is a plain, well-formed, stable-per-render string attribute, and that is its entire contract.

Real `:key` semantics would require one of three things, each of which was weighed and rejected:

1. **A state slot on `VNode`.** `VNode` is pure data — no per-node state, no instance handle, no lifecycle hook. A `node_id` field would duplicate the `"key"` attribute that already exists.
2. **A component-instance registry.** A second instance state and a second lifetime to get wrong, plus a `VNode`-shaped change at the boundary.
3. **Re-keying cross-frame state by `key` instead of by `path`.** The cheapest route, and still wrong here: switching would silently move every focus, caret and scroll offset in the application the first time a template author used `:key` inconsistently.

## What would revisit it

Any one of:

- **Layout invalidation lands** and introduces *content-addressed* node identity — the only identity mechanism the ruling permits, and the only one that could key cross-frame state without a `VNode` field.
- **A user-visible bug** for focus/caret/scroll loss across a `v-for` reorder is filed.
- **A component-instance registry** is introduced, making per-node state exist at all.

Revisiting means re-opening this decision explicitly — not silently changing `diff`. Wiring `diff::diff` into the render loop is additionally blocked until an order-surviving identity exists, a consumer for `Patch::MoveChild` is defined, layout caching is settled, and duplicate-key semantics are specified at the call site.
