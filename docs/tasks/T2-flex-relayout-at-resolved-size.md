# T2 — flex placement must re-lay-out a resized item's subtree

**Risk: HIGH** · **Scope:** `velox-dom/**` · **Sequence:** commit after T4 in the same lane
**This is the flex hot path. The Phase 2 performance gate is already failing; do not regress it.**

## Root cause

Every flex child is pre-laid-out **once**, at the container's full main size:

```rust
// velox-dom/src/layout.rs:4153-4174
let child_avail_main = if is_column { content_h_available } else { main_size };
```

The basis ladder (`:4380-4447`) computes `flex-basis`, then grow/shrink write `target_main_size`.
The placement pass applies it:

```rust
// layout.rs:4898-4902
let fb = items[item_idx].target_main_size.round() as i32;
if is_column { ln.rect.h = fb } else { ln.rect.w = fb }

// layout.rs:4976-4980
translate_layout_subtree(&mut ln, resolved_x - pre_x, resolved_y - pre_y);
ln.rect.x = resolved_x;
ln.rect.y = resolved_y;
```

`translate_layout_subtree` (`layout.rs:1874-1888`) adds `dx`/`dy` to every rect and clip in the
subtree. **It never re-measures.** There is no `at()` call anywhere in the placement region
(`layout.rs:4880-5120`).

So when a flex item's *final* main size differs from the size it was *measured* at, its children keep
the old size and merely move. The item's own box is correct; its subtree is stale.

## Reproduction (the composer's arithmetic)

`.shell` content = 620 − 48 = **572**. `.composer` content = 572 − 7 − 7 − 1 − 1 = **556**.
`.field` (`flex: 1 1 auto`, `flex-basis: auto`) takes its **content size** as its base — and because
`.field` is itself a flex container whose only child `.input` is `width: 100%`, `.field` fills the
whole 556.

| step | value |
|---|---|
| `.field` base | 556 |
| `.add` base | ≈68 |
| `total_basis` | 624 |
| `free_space` = 556 − 624 − 8 (gap) | **−76** |
| `.field` target | 480 |
| `.add` target | 68 |

Placement is **correct**: `.field` spans `[8, 488]`, `.add` spans `[496, 564]`. But `.input` was
measured at 556 and is never re-measured, so it still spans `[8, 564]` — a 556 px box inside a 480 px
flex item, **overhanging by 84 px**, which is the Add button's 68 px plus the 8 px gap. Paint order
is DOM order, so `.add` paints on top of the input's right 68 px.

**The overhang equals the Add button's border-box width plus the gap, at every window width.** It is
structural, not a narrow-window bug.

## Three things that look like fixes and are NOT

- **`min-width: 0` is inert.** Velox implements **no automatic minimum size**;
  `parse_length_value` returns `None` for `"auto"` outright (`layout.rs:2468-2470`), so a flex item's
  minimum main size is unconstrained unless a literal `min-width` is declared. There is no
  content-based floor to defeat. (`min-width` itself IS implemented — `used_min_width` `:1552-1580`,
  `floor_to_min_width` `:1582-1610`, call sites `:3587` and `:4457-4467`. The older claim that
  `grep -c min_width layout.rs` returns 0 is **false**; it returns 8.)
- **`gap` is fully supported** (`layout.rs:3801-3840`; consumed `:4542-4561`, `:4597-4607`, `:4691-4739`,
  `:4812`) and is working.
- **Removing the `.field` wrapper** would fix the symptom but is not available: the wrapper is
  load-bearing so that `.dark .input` has a *strict ancestor* carrying `TodoInput`'s scope id
  (`TodoInput.vx:18-22`, `App.vx:62-69`).

## Objective

A flex item whose resolved main size differs from the size it was measured at must have its subtree
re-laid-out at the resolved size before placement.

## Required behaviour

- When `resolved_main_size != pre_measure_main_size`, re-run the subtree layout at the resolved size.
- When they are equal, do **nothing extra** — this is the common case and must not pay.
- The re-layout must be at the **resolved main size in the main axis**; the cross axis is already
  correct and must not be re-derived.
- Overflow in the cross axis, `align-items`, `position`, and `overflow` semantics must not change.
- Absolutely this must **not** reintroduce the exponential descent. `2.1a`/`2.1c` already established
  that a second descent per flex item is the failure mode that made layout `2^depth`. The re-layout
  must be gated on the delta being non-zero, and the gate must be measured, not asserted.

## Tests

1. **The composer's case.** A row (`display: flex`, `gap: 8px`, width 556) containing a
   `flex: 1 1 auto` wrapper whose single child is `width: 100%`, plus a `flex: 0 0 auto` 68 px
   sibling. Assert the child's rect `.w` equals the wrapper's `.w` — not 556.
2. **Column axis.** The same, with `flex-direction: column`. The bug affects both (`is_column` writes
   `rect.h`).
3. **No-op when sizes match.** A row where free space is zero. Assert the subtree rects are
   bit-identical to the no-change expectation — this pins that the gate works.
4. **No exponential regression.** The existing `velox-dom/tests/flex_probe_memo.rs` depth-18 test must
   stay green and its timing must not regress. Run it before and after and record both numbers.
5. **Perf.** Run `cargo test -p velox-renderer --release --features skia-native --test frame_cost_bench
   -- --ignored --nocapture` before and after. Record `layout_us` at 0/10/200 todos, 3 runs each. The
   Phase 2 gate (≤400 µs @ 10 todos) is already failing at ~551 µs; **this task must not make it
   worse.** If it does, the gate condition is "no regression against the recorded baseline", and you
   must say so explicitly rather than reporting a pass.

## Falsification (required)

- M1: remove the delta gate so every item re-lays-out → M4 (depth-18 timing) and the perf numbers must
  detect it.
- M2: re-layout at the *pre-measure* size instead of the resolved size → test 1 must go RED.
- M3: translate the subtree by the size delta as well as the position delta → the item's children
  land wrong → test 1 must go RED.
- M4: skip the cross-axis branch (`is_column`) → test 2 must go RED.

## Gate

```
cargo test -p velox-dom
cargo test -p velox-dom --features skia-native --test todo_item_pixels
cargo test -p velox-dom --test layout_golden
cargo test -p velox-dom --tests            # every flex_* test
cargo test --workspace --no-fail-fast
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features velox-renderer/skia-native -- -D warnings
```

## Sequencing constraint

**T4 must land first, in the same lane, as its own commit.** T4 also changes `child.rect.x` for
buttons in the same shared tail (`layout.rs:5389-5407`) that the flex branch falls through to. Doing
them together makes the interaction attributable. Re-grep `layout.rs` line numbers at dispatch — they
will have moved.

## Definition of done

The composer's `.input` no longer overhangs `.composer` by the Add button's width, in a screenshot —
not only in a test. A unit test proves the rect arithmetic; a screenshot proves the paint order.
Report both.