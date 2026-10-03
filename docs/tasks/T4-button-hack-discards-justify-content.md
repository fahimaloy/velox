# T4 — the button hack must not discard `justify-content`

**Risk:** low · **Scope:** `velox-dom/**` · **Sequence: FIRST in the layout lane, before T2**

## Root cause

`velox-dom/src/layout.rs:5389-5407`:

```rust
if tag == "button"
    && children.len() == 1
    && let Some(child) = laid_children.get_mut(0)
{
    let content_h = (rect_h - pt - pb - bt - bb).max(0);
    let child_h = child.rect.h;
    let offset_y = ((content_h - child_h).max(0)) / 2;
    child.rect.y = elem_y + bt + pt + offset_y;

    let align = style_lookup_str(style, "text-align").unwrap_or_else(|| "left".to_string());
    let child_w = child.rect.w;
    let offset_x = match align.as_str() {
        "center" => ((content_w - child_w).max(0)) / 2,
        "right"  =>  (content_w - child_w).max(0),
        _ => 0,
    };
    child.rect.x = content_x + offset_x;
}
```

The flex pass already computed the correct horizontal centring at `layout.rs:4722`:

```rust
"center" => main_start + extra_space / 2.0,
```

This block then **unconditionally discards it**, deriving `x` from `text-align` alone. With no
`text-align` — the default — `offset_x = 0` and the child is pinned to the content box's left edge.

On the 26 px `.remove` button (1 px border, `padding: 0`, content 24×24, glyph box 10×22) the `×` ink
span starts 1.0 px into a 24 px content box whose centre is at 12 — **≈11 px left of centre**. On a
26 px control that is unmissable.

## Three corroborating facts that pin the mechanism

1. `.ghost` (`App.vx:374-385`) sets **both** `justify-content: center` **and** `text-align: center`, so
   it renders centred.
2. `.toggle` and `.check` have **three** children, because the SFC leaves template indentation as
   whitespace text nodes (`layout.rs:3852-3857` skips them at layout time but they are still in
   `children`). `children.len() == 1` is false, the hack is skipped, and their flex centring survives.
   **This is exactly why only the single-line `×` is wrong**, and it matches the screenshot precisely.
3. The flex branch (`layout.rs:3791`) **falls through** to this shared tail — the `else` is the
   block/inline branch at `:5051`, the flex branch does not `return`. So `justify-content: center` is
   ignored on *any* single-child `<button>` that lacks `text-align: center`.

`docs/AUDIT_VELOX_LAYOUT_HTML_CSS_PARITY_2026-09-24.md:145-148` (**F-17**) names this same hack as a
year-old smell — **but its diagnosis differs and its recommended fix contradicts this task.** Read it as
context, not as a prior writeup of this bug:

- It cites `layout.rs:2143-2222`; the hack is at `:5389-5407`. The line numbers are ~3,200 stale.
- Its **Impact** is "non-button elements with 1 child get centered inadvertently" — **that is no longer
  true.** The code is `if tag == "button" && children.len() == 1`, so the tag guard exists.
- It says nothing about `justify-content` being discarded.
- Its **Fix** is "remove the hack; rely on flex/button stylesheet defaults" — which would also remove
  the vertical centring browsers give buttons. This task deliberately keeps it.

**Net:** the audit corroborates the hack is known and disliked; it does **not** record this defect, and
adopting its fix would regress vertical centring. (Verified at source while reconciling `fix-29`.)

## Objective

`justify-content` must not be silently discarded for a single-child `<button>`.

## Required behaviour

The hack exists to centre a button's single child **vertically** — browsers do that by default
(`align-items: center` on the UA button box). The **horizontal** override is the defect.

Resolve the horizontal position as:

1. `text-align` if it is **explicitly set** (and that is what the author asked for), otherwise
2. the position the flex/block flow already computed — i.e. **do not touch `child.rect.x`**.

`"left"` is the default, so the discriminator must be "is `text-align` present at all", not "is it
equal to `center`". `text-align` is in `INHERITABLE` (`velox-style/src/lib.rs:769`), so an inherited
value from an ancestor also counts as explicit — decide and document whether that is what you want.

## Tests

1. `<button style="justify-content:center">×</button>` → the glyph is horizontally centred.
2. The same with `text-align: center` → still centred (no regression).
3. `<button style="justify-content:center; text-align:right">` → right-aligned (`text-align` wins).
4. `<button style="justify-content:flex-end">` → right-aligned (flex wins when no `text-align`).
5. **Non-flex** single-child button with no `justify-content` → unchanged from today (this fix must
   not start moving things in boxes where nothing asked it to).
6. Multi-child button → the hack is skipped, and must stay skipped.

## Falsification (required)

- M1: restore the unconditional `child.rect.x = content_x + offset_x` → test 1 must go RED.
- M2: check `align == "center"` rather than "is `text-align` present" → test 3 must go RED (right
  alignment would be lost). This mutation is the subtle one.
- M3: only apply the fix when `display == "flex"` → test 5 must go RED.
- M4: remove the vertical centring while fixing the horizontal → any existing vertical test must go
  RED; if none exists, **say so**, because that means the vertical half is untested and the "falsify
  every guard" rule was not applied to it.

## Gate

```
cargo test -p velox-dom
cargo test -p velox-dom --features skia-native --test todo_item_pixels
cargo test -p velox-dom --test layout_golden
cargo test --workspace --no-fail-fast
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features velox-renderer/skia-native -- -D warnings
```

## Template-side mitigation (do this too, it is one line)

`.remove` in `TodoItem.vx:154-168` relies on `justify-content: center` alone. Adding
`text-align: center` restores correct rendering **today** without the engine fix, because the hack
overwrites rather than adds. Ship it as a template change in the same commit — it makes the template
robust and it is what the sibling `.ghost` rule already does. But the engine fix is the real fix:
without it, every future author who writes `justify-content: center` on a single-child button gets the
same defect, and no template can prevent that.

## Sequencing

Same lane as T2, **this one first**, its own commit. T2 re-lays-out flex subtrees and T4 adjusts the
same shared tail; separating them keeps the interaction attributable. Re-grep `layout.rs` at dispatch —
line numbers will have moved.