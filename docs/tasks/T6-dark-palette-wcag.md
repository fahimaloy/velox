# T6 — rebuild the dark surface/border ramp to WCAG

**Risk:** low (CSS-only) · **Scope:** `velox-cli/templates/**` · **Verification-gated on T1**
**The colours are not the cause of the reported dark-mode failure — but they ARE bad on their own merits.**

## Scope boundary, stated up front

The user reported "dark mode not showing correctly" and "dark mode colour combination is totally
trash". Those are **two different defects**:

- **"not showing correctly"** = T1. The page background stays light because `.dark .app` is dead CSS.
  Nothing about the palette causes it.
- **"totally trash"** = this task. Independent of T1, and real.

**You may land these colour changes at any time. You may NOT report them as visually verified until
T1 has shipped and someone has taken a screenshot of dark mode with these values applied.** A passing
CSS-parsing test proves nothing here.

## Measured state (des-1, WCAG 2.1 relative luminance)

### Dark — text is good, surfaces are broken

| pair | ratio | verdict |
|---|---|---|
| primary `#e8ecec` on card `#161b1f` | 14.57:1 | AAA |
| secondary `#a3aeb2` on card `#161b1f` | 7.64:1 | AAA |
| Add label `#08110f` on fill `#4fd1c5` | 10.26:1 | AAA |
| **card `#161b1f` vs page `#0f1316`** | **1.08:1** | **no figure/ground at all** |
| **border `#242c31` on card** | **1.22:1** | **fails 1.4.11 (needs 3:1)** |
| **border `#242c31` on page** | **1.32:1** | **fails 1.4.11** |
| **border `#333d45` on card** | **1.56:1** | **fails 1.4.11** |
| **muted `#75818a` on card** | **4.35:1** | **fails AA — 4 places** |
| dialog `#181e22` vs page `#0f1316` | 1.11:1 | invisible |
| hover `#1d2429` on card | 1.10:1 | hover is invisible |

The four AA failures are **one token**: `#75818a`, used by `.dark .remove`, `.dark .completed
.todo-text`, `.dark .input::placeholder`, `.dark .empty`.

### Dark is also internally inconsistent

Five surface values for three roles (`#0f1316`, `#161b1f`, `#181e22`, `#1d2429`, `#222a2f`); four
border values for two roles (`#242c31`, `#333d45`, `#414c55`, `#3a2020`). And the **same ghost-button
hover** uses `#333d45` + `#1d2429` in `App.vx:437-438` but `#414c55` + `#222a2f` in
`Modal.vx:325-326` / `Confirm.vx:269-270`. **Light mode has one border and one hover border across all
six files — dark mode is where it rots.**

### Light — sound, with two defects

All text passes except muted `#6b7377` on page `#f6f5f2` at 4.43:1 (marginal AA fail, `.footnote`).
Card-vs-page 1.09:1 is acceptable **only** because the border does the separating — and that border,
`#dedbd3` on white, is **1.38:1**. It is the *only* thing identifying `.input` as a field (there is no
underline), so the input does not announce itself as a field.

## Proposed palette (des-1)

### Dark — rebuild the surface ramp, unify the borders, fix the one failing token

| role | current | proposed | reason |
|---|---|---|---|
| page background | `#0f1316` | `#0A0E11` | deepest step; gives the ramp somewhere to climb from |
| surface 1 — cards, composer, input, ghost, toggle | `#161b1f` | `#141A1E` | 1.10:1 off page, paired with a visible border it reads |
| surface 2 — dialog panels | `#181e22` | `#1A2126` | one raised step above surface 1; replaces a two-file-only value |
| surface 3 — hover / raised | `#1d2429` + `#222a2f` | `#20282D` | **collapses two ad-hoc values into one** |
| border — decorative | `#242c31` | `#364147` | 1.68:1 on surface 1, 1.85:1 on page; quiet but visible |
| border — control | `#333d45` | `#64727B` | **3.54:1 on surface 1, 3.28:1 on surface 2, 3.91:1 on page** — clears 1.4.11 for the first time |
| border — control hover | `#333d45` / `#414c55` | `#7F8C94` | 4.3:1; one value for both files |
| text primary | `#e8ecec` | `#EDF2F3` | 15.55:1 on surface 1 |
| text secondary | `#a3aeb2` | `#A8B4B9` | 8.27:1 |
| text muted / placeholder | `#75818a` | `#849299` | **5.48:1 — fixes all four AA failures** |
| accent (fill + accent text) | `#4fd1c5` | `#2DD4BF` | 9.43:1 as text on surface 1, 10.41:1 on page |
| accent hover | `#6fe0d5` | `#5EEAD4` | same hue, one step up |
| on-accent label | `#08110f` | `#04211D` | 9.09:1 on the accent fill |
| danger fill | `#f08a80` | `#FF8A80` | 8.11:1 with its label |
| danger hover | `#f7a49b` | `#FFA9A1` | same hue, one step up |
| on-danger label | `#1a0906` | `#2B0704` | 8.11:1 on the danger fill |
| badge border / fill / text | `#3a2020` / `#2a1614` / `#f08a80` | `#4A2B28` / `#2A1614` / `#FF8A80` | border 1.15:1 → a legible edge |
| scrim | `rgba(3,5,6,0.66)` | `rgba(4,7,9,0.72)` | deeper so the panel reads as lifted |

### Light — four value changes, nothing structural

| role | current | proposed | reason |
|---|---|---|---|
| border — decorative | `#dedbd3` | `#D8D4CB` | 1.48:1; slightly firmer hairlines |
| **border — control** | `#dedbd3` | `#868D92` | **3.37:1 on card, 3.11:1 on page** — the input finally announces itself |
| border — control hover | `#c4bfb4` | `#636A70` | one obvious "live" step, unambiguously above the rest state |
| text muted | `#6b7377` | `#667076` | 5.07:1 on card, **4.69:1 on page** — fixes the footnote |
| hover surface | `#efece5` / `#f6f5f2` | `#EFEDE8` | unifies two values for one interaction |

**Keep:** page `#f6f5f2`, card `#ffffff`, primary `#14181b`, secondary `#5a6469`, accent `#0d6e66`,
accent hover `#0a5750`, danger `#a3342c`, divider `#e3e1db`.

## Requirements

1. **Apply to every template component**, not just `App.vx`. The current split-across-files drift is
   the defect. One value per role, everywhere.
2. **Light and dark must use the SAME role names**, so the two modes stay comparable.
3. **Do not weaken the accent.** `#0d6e66` → `#4fd1c5` in light/dark is a coherent, deliberate flip and
   it works. Keep the hue; the change is depth, not hue.
4. Prefer expressing the ramp as a small named set over magic hexes repeated 40 times. If the
   template CSS language has no custom properties, do not fake them — say so in the report.

## Tests

- A **contrast assertion test**, not a screenshot. For every (foreground, background, size) pair the
  templates declare, compute the WCAG ratio and assert the required threshold: 4.5:1 for body text,
  3:1 for large text and UI boundaries (1.4.11). Enumerate the pairs by parsing the `<style scoped>`
  blocks of all six components — hand-written cases will drift and miss the ones des-1 found by
  inspection.
- This test **must fail on the current values** for at least the four `#75818a` sites and the border
  1.4.11 cases. Prove it.

## Falsification (required)

- M1: revert `#75818a` → the AA test must go RED.
- M2: revert a border to `#242c31` → the 1.4.11 test must go RED.
- M3: reintroduce the split hover values (`#1d2429` in App, `#222a2f` in Modal) → a
  "one value per role" consistency test must go RED. **Add that consistency test**; it is the only
  thing that prevents this specific rot from returning.

## Gate

```
cargo test -p velox-cli
cargo test --workspace --no-fail-fast
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features velox-renderer/skia-native -- -D warnings
```

Plus: **a dark-mode screenshot with T1 applied**, before this task may be called done.