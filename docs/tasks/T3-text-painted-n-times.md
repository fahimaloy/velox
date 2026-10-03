# T3 — one text `VNode` must not be painted N times (and N−1 lines must not be dropped)

**Risk: HIGHEST in this plan** · **Scope:** `velox-dom/**`, `velox-renderer/**`
**This one silently deletes text. It is not cosmetic. Treat it as a data-loss bug.**

## Root cause

### Stage 1 — layout emits one `LayoutNode` per wrapped line, all sharing one `source_index`

`velox-dom/src/layout.rs:1104` opens a loop over line boxes and declares `merged` **inside** it:

```rust
// layout.rs:1197  (inside `for (li, line) in lines.iter().enumerate()`)
let mut merged: Vec<MergedRun> = Vec::new();
```

The merge that collapses pieces of one text `VNode` into one node therefore fires **only within a
single line** (`layout.rs:1240-1250`) — and the comment there already names the exact hazard:

```rust
// Consecutive pieces of one text VNode are ONE LayoutNode. Two
// nodes pointing at the same text VNode would make the renderer
// draw the same string twice.
```

Each line then appends its nodes unconditionally:

```rust
// layout.rs:1292-1295
merged.sort_by_key(|m| m.item);
let slots = build_inline_slots(&merged, &run);
let mut nodes = inline_slots_to_nodes(&slots, &merged);
laid_children.append(&mut nodes);
```

and `inline_leaf_node` (`layout.rs:370-385`) stamps every fragment with the same `source_index`.

**An N-line text node therefore yields N `LayoutNode`s all carrying `source_index: Some(k)`.** The
merge is scoped one line too narrowly to prevent precisely what its own comment warns about.

### Stage 2 — the renderer resolves by `source_index` and has no dedup

`velox-renderer/src/skia_render.rs:2695-2714` makes one `render_with_layout` call per layout child,
resolving the `VNode` by index:

```rust
for (_, layout_idx) in ordered {
    if let Some(child_layout) = layout.children.get(layout_idx)
        && let Some(src_idx) = child_layout.source_index
        && let Some(child) = children.get(src_idx)
    {
        render_with_layout(canvas, child, child_layout, ...);
```

Two layout children with the same `source_index` ⇒ the **same `&VNode::Text`** is rendered twice, at
two different rects.

### Stage 3 — the paint re-wraps the whole string and breaks after line 0

`skia_render.rs:2774-2777` ignores the per-line slice layout assigned to this node and re-wraps the
**entire** original string; `:2788-2794` breaks after the first line because
`text_bottom = rect.y + max(rect.h, line_height)` and a per-line rect is only one line tall.

### Net effect

**Line 0 is painted N times. Lines 1 through N−1 are never painted at all.**

For `.tagline` (`App.vx:9`, 110 chars in a 572 px box at 14 px) that is the visible duplication *and*
the silent loss of the tail `"natively with Skia."`.

## Blast radius

Any text node inside a block container that wraps to ≥2 line boxes, at any tag:

| element | est. lines | effect |
|---|---|---|
| `App.vx:9` `p.tagline` | 2 | line 0 ×2, `"natively with Skia."` lost |
| `Modal.vx:12` `p.body-text` | ~4 | line 0 ×4, 3 lines lost |
| `Confirm.vx:9` `p.message` | 2 | line 0 ×2, 1 line lost |
| `TodoItem.vx:7` `div.todo-text` | varies | any long todo loses its tail |

## Why no existing test caught it

- `velox-renderer/tests/text_decoration_render.rs:279-316` (`the_rule_is_drawn_once_per_wrapped_line`)
  uses the same 90 px-wrapped shape and asserts one decorated band per line. **It passes under the
  bug**: every invocation paints `lines[0]` and measures `lines[0]`'s own width, so the band geometry
  looks correct while the glyphs are wrong. Fix it as part of this task.
- `velox-renderer/tests/skia_text_wrap_render.rs:8` (`render_wrapped_text_checksum`) is a checksum
  that was **recorded from the buggy output**. It pins the bug as correct. It must be regenerated
  and re-justified, and a checksum alone is not an acceptable guard here.

## DESIGN GATE — read before writing code

This is the one task in the plan where the shape of the fix is not obvious, because layout and paint
share a contract that is currently wrong in both directions. **A design pass was commissioned; its
answer is the input to implementation.** Evaluate at least these three, and pick one with a stated
reason:

**(a) Widen the merge in layout across line boundaries.** One `LayoutNode` per text `VNode` with the
full multi-line rect. The paint loop then walks all lines correctly from `rect.y`. *Risk:* the inline
slot algorithm positions same-line siblings by the per-line run geometry; collapsing across lines
loses that. Must prove no inline-layout regression.

**(b) Record the per-line slice on the `LayoutNode` and have the painter draw that slice.**
Structurally the most honest: the node knows what it is. *Cost:* a new `LayoutNode` field, which is
the destination a prior design pass (`ora-7`) already identified for the `em` font-size work and
explicitly judged **"the right destination and the wrong first move"** — for a *different* reason
(the layout→vnode mapping is partial by design at `skia_render.rs:2059-2066`, so a new field would
not reliably reach the painter). **That objection does not obviously apply to a text field on a node
that is already being painted**, but you must address it rather than assume it away.

**(c) Dedupe by `source_index` in the painter and paint once with a full-height rect.** Cheapest, but
it papers over a layout contract violation and will mis-paint any inline run where a sibling shares
the line.

Whichever you pick, the acceptance test is identical and does not care: **the wrapped string must be
drawn exactly once, in full, with every line at its correct position.**

## Tests

1. **No duplication.** A 90 px-wrapped string rendered to pixels: the ink must appear once per line,
   N times in total, and never N² times.
2. **No loss.** The **last** line's glyphs must be present. Assert on the actual ink bounds, not on
   a checksum — a checksum recorded from buggy output is how this got pinned.
3. **Multi-line inline run.** Two spans on the same line inside a wrapping container, to prove (a)
   does not break sibling positioning.
4. **Single-line regression.** Existing single-line behaviour must be bit-identical.
5. **Repair** `text_decoration_render.rs:279-316` and `skia_text_wrap_render.rs:8` so they would have
   caught this. Demonstrate that: mutate the fix away and confirm both go RED.

## Falsification (required)

- M1: revert the layout change → tests 1 and 2 must go RED.
- M2: dedupe only (option c) without fixing the loss → test 2 must go RED. **This is the mutation
  most likely to slip through; do not accept a pass here without checking it individually.**
- M3: make the painter draw the whole string without the `text_bottom` break → every line is drawn at
  every node → test 1 must go RED.

## Gate

```
cargo test -p velox-dom
cargo test -p velox-renderer
cargo test -p velox-renderer --features skia-native --test todo_item_pixels
cargo test -p velox-renderer --test presenter_pixel_bytes
cargo test -p velox-renderer --test caret_pixels
cargo test --workspace --no-fail-fast
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features velox-renderer/skia-native -- -D warnings
```

**A screenshot of `.tagline` showing the tail `"natively with Skia."` is part of the gate.** A green
suite is not sufficient evidence for a text-loss bug.

## Sequencing

Runs **after** T4 and T2, both of which are in `velox-dom/src/layout.rs`. Never two lanes on that
file. If you pick option (b) it also touches `velox-renderer/src/skia_render.rs`, so it must follow
T5.

## Out of scope

Do not "fix" this by making `line-height` behave differently, by changing the 0.6em width heuristic,
or by editing the templates to keep text short enough not to wrap. The bug reproduces at two lines in
a 572 px box; templates must not be shaped around engine defects.