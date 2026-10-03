---
name: velox-html-parity
description: Use when adding a new HTML element, CSS property, or UA stylesheet rule to velox. Knows the parity audit findings, the Blink html.css reference values, and the invariants that must be preserved.
---

# Velox HTML/CSS Parity

Use when adding or modifying any element default, CSS property support, or UA stylesheet rule. Velox must match HTML/CSS/Vue defaults so authors never hand-write what a browser provides.

## Source of truth

- **Blink `html.css`** — the reference for UA defaults. The remediation plan's Task 4.2 has the full value table.
- **WHATWG HTML §15.3** — form control defaults (box-sizing, padding, white-space).
- **`velox-style/src/ua.css`** — velox's current UA stylesheet (9 rules / 11 tags).
- **`velox-dom/src/layout.rs:2042-2067`** — `INLINE_BY_DEFAULT_TAGS` + `default_display_for_tag`.
- **`velox-style/tests/cascade.rs`** — existing sync test that fails if `ua.css` and `INLINE_BY_DEFAULT_TAGS` diverge. **Keep this green.**

## Current state (measured, from remediation plan N5)

`ua.css` has **9 rules covering 11 tags**: `html, body, button, p, ul, ol, u, var, wbr`.

`h1` and `h2` are **already correct** in `ua.css:4-5`. The gap starts at `h3`.

### Already-correct elements (no action needed)

| Element | velox behavior | Status |
|---------|---------------|--------|
| `h1`    | `font-size:2em; margin:0.67em 0; font-weight:bold; display:block` | ✅ correct |
| `h2`    | `font-size:1.5em; margin:0.83em 0; font-weight:bold` | ✅ correct |
| 27 inline-by-default tags (`img`, `a`, `span`, `code`, `label`, `output`, `data`, `time`, …) | match Blink | ✅ correct |

### Deliberate deviations (DO NOT "fix")

| Element | Browser | velox | Reason | Record in |
|---------|---------|-------|--------|-----------|
| `meter`, `progress` | `inline-block` | `inline` | velox has no `inline-block` layout; `inline` is closer than `block` | `docs/HTML_PARITY.md` (Task 4.5) |

These are documented at `layout.rs:2029-2033` and `ua.css:15-18`. The sync test in `cascade.rs` must be extended to assert every entry has a justification.

### Missing — Tier 1 (near-universal, visibly wrong, no layout engine needed)

Add to `ua.css`. These are pure CSS — no layout algorithm needed:

```css
h3 { font-size: 1.17em; margin: 1em 0; font-weight: bold; }
h4 { font-size: 1em; margin: 1.33em 0; font-weight: bold; }
h5 { font-size: 0.83em; margin: 1.67em 0; font-weight: bold; }
h6 { font-size: 0.67em; margin: 2.33em 0; font-weight: bold; }
pre { font-family: monospace; white-space: pre; margin-block: 1em; }
blockquote { margin-block: 1em; margin-inline: 40px; }
hr { display: block; border-style: inset; border-width: 1px; margin-block: 0.5em; margin-inline: auto; }
fieldset { display: block; border: 2px groove; padding-inline: 0.75em; margin-inline: 2px; }
figure { display: block; margin-block: 1em; margin-inline: 40px; }
dl { display: block; margin-block: 1em; }
dd { display: block; margin-inline-start: 40px; }
dt { display: block; }
address { display: block; font-style: italic; }
sub, sup { vertical-align: sub/super; font-size: smaller; }
mark { background-color: Mark; color: MarkText; }
big { font-size: larger; }
small { font-size: smaller; }
s, del, strike { text-decoration: line-through; }
i, cite, em, dfn { font-style: italic; }
strong, b { font-weight: bolder; }
tt, code, kbd, samp { font-family: monospace; }
center { display: block; text-align: center; }
```

**`pre` is the highest priority** — without `white-space: pre`, preformatted text silently wraps, corrupting its entire content.

### Missing — Tier 2 (form controls, partial)

Per WHATWG §15.3.10 and Blink `html.css`. Note: velox's `button`/`input`/`select`/`textarea` are `block` not `inline-block` — this is a deliberate documented deviation (Task 4.5). But these rules are independent of that deviation and should still be added:

```css
input, button { display: inline-block; }        /* if velox adds inline-block */
input:is([type=radio],[type=checkbox],[type=reset],[type=button],
         [type=submit],[type=color],[type=search]),
select, button { box-sizing: border-box; }
button { padding-block: 1px; padding-inline: 6px;
         text-align: center; white-space: nowrap; }
textarea { white-space: pre-wrap; display: inline-block; }
label { cursor: default; }
input:not([type=file]) { cursor: text; }
```

### Missing — Tier 3 (pseudo-elements)

Blink ships `:focus-visible { outline: auto 1px -webkit-focus-ring-color }` and a `::selection` highlight. Velox has neither. A focus ring is an accessibility affordance — add it.

### Missing — Tier 4 (tables — needs table layout algorithm first)

```css
table { display: table; border-spacing: 2px; border-collapse: separate; border-color: gray; }
thead { display: table-header-group; vertical-align: middle; }
tbody { display: table-row-group; vertical-align: middle; }
tfoot { display: table-footer-group; vertical-align: middle; }
tr { display: table-row; }
td, th { display: table-cell; padding: 1px; }
th { font-weight: bold; text-align: center; }
caption { display: table-caption; text-align: center; }
```

**Wait for Task 4.5's table layout algorithm** before adding these — without the layout engine, the UA rules are decorative only.

## Properties: parsed vs rendered vs unparsed

From remediation plan N1 (verified against velox source):

### Parsed AND rendered (130 properties)

These work. Don't touch without a specific reason.

### Parsed but SILENTLY DROPPED (3 properties — HIGH, fix with Task 4.7)

| Property | Parsed at | Stored in | Read by renderer? | Verified |
|----------|-----------|-----------|-------------------|----------|
| `overflow-x` | `style.rs` | (field) | **NO** | nothing reads it |
| `overflow-y` | `style.rs` | (field) | **NO** | nothing reads it |
| `background-image` | `style.rs` | (field) | **NO** | nothing reads it |
| `letter-spacing` | `style.rs` | (field) | **NO** | nothing reads it |
| `visibility` | `style.rs` | (field) | **NO** | **no reader looks up `visibility`, so `visibility: hidden` hides nothing** |
| `transform` | `style.rs` | (field) | **NO** | read at `layout.rs:3695` ONLY to force a stacking context — no visual effect |
| `box-shadow` | `style.rs` | (field) | **NO** | nothing reads it |
| `transition` | `style.rs` | `transitions: Vec<Transition>` | **NO** | nothing reads it |
| `border-style` | `style.rs` | (field) | **NO** | only the `border`/`border-width` shorthands are read |
| `border-color` | `style.rs` | (field) | **NO** | only the `border`/`border-width` shorthands are read |

**This list is 10 long, not 3.** N1 named only `transition`, `transform`, and `box-shadow`; seven more existed. The table above was regenerated from the shipped constant, so trust the constant, not this list.

**Rule:** if a property is parsed, the framework owes the author an effect — or an explicit "not implemented" signal. Silently dropping it is a bug.

**Shipped fix (`de1e12f`, Task 4.7a):** `velox-dom/src/style.rs` `pub const PARSED_BUT_UNRENDERED: &[(&str, &str)]`, immediately above `set_property`, each entry citing its `set_property` arm line and why nothing reads the field. `veloxc/src/commands/lint.rs` iterates it and warns per name.

**Do not reinstate the original prescription** — adding `unimplemented: Vec<&'static str>` to `ComputedStyle` **cannot work**, because `ComputedStyle` has **zero production callers**: the live path carries declarations in the merged style *string* and the renderer never builds a `ComputedStyle`. Storing state there is dead on arrival. The dev-server one-time notice was also **not** implemented; `velox lint` is the whole mechanism.

Removing a name from `PARSED_BUT_UNRENDERED` is the definition of done for implementing it.

### Unparsed entirely (no `set_property` arm)

`content`, `cursor`, `user-select`, `float`, `clear`, `aspect-ratio`, `writing-mode`.

- **`cursor` and `user-select`** are UI-visible and were explicitly requested — add parsing + rendering.
- **`float`/`clear`** — defensible to omit; css-flexbox-1 §3 says they have no effect on flex items. Document in `docs/HTML_PARITY.md`.
- **`writing-mode`, `aspect-ratio`** — imply layout modes velox does not have. Document as non-goals.

## Invariants to preserve

1. **`ua.css` and `INLINE_BY_DEFAULT_TAGS` must stay in sync.** The test in `velox-style/tests/cascade.rs` already fails if they diverge. **Add new tags to BOTH in the same commit.**

2. **Every entry in the per-tag display table (Task 4.3) must have a justification.** Replace the flat `INLINE_BY_DEFAULT_TAGS.contains(tag)` with `ua_display_for_tag(tag: &str) -> Display` that carries a doc-comment per entry citing the Blink/HTML default it mirrors.

3. **Do not silently ignore a parsed declaration.** See Task 4.7 above.

4. **Record every deliberate deviation in `docs/HTML_PARITY.md`.** A deviation that's written down is a decision. One that's only in a source comment is a trap for the next maintainer.

## When adding a new element

1. Add UA rule to `ua.css`.
2. Add tag to `INLINE_BY_DEFAULT_TAGS` (or per-tag table after Task 4.3).
3. Add row to the parity test in `velox-dom/tests/default_parity.rs` (Task 4.1).
4. If the element needs layout behavior beyond `inline`/`block`, implement it.
5. If it's a deliberate deviation from browser default, document in `docs/HTML_PARITY.md` with reason.

## When adding a new CSS property

1. Decide: implement it, or add it to the `unimplemented` list with a warning.
2. If implementing: add `set_property` arm in `style.rs`, store in `ComputedStyle`, read in renderer.
3. If not implementing: add name to `unimplemented` list. `velox lint` warns. Dev server notices. **Do not silently drop.**
4. Add test to `velox-dom/tests/declaration_honesty.rs` proving the property is either rendered or reported.
