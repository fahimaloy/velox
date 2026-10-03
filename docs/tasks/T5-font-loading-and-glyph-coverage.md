# T5 — ship real font assets, fix the candidate list, and stop shipping unrenderable glyphs

**Risk:** medium · **Scope:** `velox-renderer/**`, `velox-style/**` · **Independent of the layout lane**

## Root cause

`velox-renderer/src/skia_render.rs:1980-2009` loads a typeface by trying six absolute system paths,
then two `include_bytes!` bundles.

### Finding 1 — the bundled fallbacks are 14-byte text stubs

`velox-renderer/assets/DejaVuSans.ttf` and `velox-renderer/assets/NotoSans-Regular.ttf` both contain
the literal ASCII `<BINARY FILE>\n`. `FontMgr::new_from_data` on that always returns `None`, so **the
entire `bundles` loop at `skia_render.rs:2005-2009` is dead code.** The doc comment "Attempt to load a
system font or bundled fallback fonts" at `:1554` is therefore misleading.

### Finding 2 — the candidate list misses the real DejaVu on this machine

The winner here is `/usr/share/fonts/google-noto/NotoSans-Regular.ttf` (candidate #4). The first three
candidates do not exist — the real DejaVu on this box is at
`/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf`, **which is not in the list**. No `symbols`
family is ever consulted.

### Finding 3 — the winning face has none of the three glyphs

Reading the real cmaps:

| face | `☀` U+2600 | `☾` U+263E | `×` U+00D7 | `✓` U+2713 |
|---|---|---|---|---|
| `NotoSans-Regular.ttf` (wins here) | **missing** | **missing** | present | **missing** |
| `FreeSans.ttf` | missing | missing | present | missing |
| real `DejaVuSans.ttf` (not on the list) | present | present | present | present |

`×` is Latin-1 Supplement and is in every candidate; the three symbols are not.

### Finding 4 — there is no glyph fallback, so a miss is a tofu box

The only text draw is `canvas.draw_str` (`skia_render.rs:2811`), which forwards to
`drawSimpleText`: a single typeface, no shaping, **no fallback**. There is no `Paragraph`, no `Shaper`,
no `SkUnicode`, no glyph-run building anywhere in the workspace — `grep -rn
'Paragraph|Shaper|charToGlyph|glyph_id|unichar' velox-renderer/src` returns **zero**.

Unmapped codepoints become glyph 0 = `.notdef`, a hollow rectangle. That is the ▯ in the screenshot.
Noto Sans's `.notdef` has ink only **above** the baseline (0 → 0.714 em), which is why the tofu also
sits ~10 px low in the button.

**This is environment-dependent.** On a machine where the real DejaVu path exists, `☀`/`☾` render
correctly from the same binary.

### Finding 5 — `font-family` is completely inert

`get_or_load_family` (`skia_render.rs:1651-1661`) permanently writes the default face under any
requested family name on a miss:

```rust
if let Some(default_tf) = self.typefaces.get(&self.default_family) {
    let tf = default_tf.clone();
    self.typefaces.insert(family.to_string(), tf.clone());
    return Some(tf);
}
```

`new_with_scale` seeds exactly one entry, `"default"` (`:1566-1570`). And `parse_font_family`
(`:512-520`) keeps only the first comma-separated name. So
`"Inter", "Noto Sans", "DejaVu Sans", sans-serif` → `"Inter"` → Noto Sans. Every family in every app
resolves to the same face.

`GenericFamily` (`velox-style/src/fonts.rs:217-265`) has **zero consumers** outside its own parser and
one `pub use` re-export at `velox-style/src/lib.rs:18`.

### Finding 6 — the baseline uses `font_size` as an ascent proxy

`skia_render.rs:2791` sets `ty = rect.y + font_size`, ignoring the real ascent the code already
measures at `skia_render.rs:1765-1766` (`ascent: (-bounds.top).max(0.0)`). Noto Sans's real hhea ascent
is 1.069 em, so a browser would place the baseline ≈2.8 px higher.

## Objective — three separable parts, ship them separately

**T5a — make the fallback real.** Replace the two stub assets with genuine font files, or delete the
bundles and rely on system paths, whichever is the honest choice for a framework. **Do not commit a
file whose bytes are `<BINARY FILE>`.** If you commit real fonts, state the licence and file size in
the report — a 700 KB font in the repo is a real cost and must be a conscious decision, not a
side-effect of making a test pass.

**T5b — make the candidate list correct.** Add the paths that actually exist on mainstream Linux
distros (including `/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf`), and — more importantly —
prefer a face that actually carries the glyphs, or add a `NotoSansSymbols2` path.

**T5c — decide the `font-family` contract, honestly.** Either:
- make `get_or_load_family` attempt a real lookup instead of poisoning the cache with the default
  face (needs `FontMgr` access — `FontMgr::default()` is already constructed at `:1984`); or
- if `font-family` is genuinely a declared non-goal for now, **stop parsing it into a cache key that
  implies it works**, and record it in the deviation docs alongside the other deliberate non-goals.

The current state — an attribute that parses, caches under its own name, and silently does nothing —
is exactly the "silent lie" failure class this repo already has a task for. Do not leave it there.

## Tests

1. **The bundles are real.** A test that opens `velox-renderer/assets/*.ttf` and asserts
   `FontMgr::new_from_data` returns `Some` for at least one. This test **fails today** and must go
   RED if the stubs return.
2. **A glyph-coverage test.** For each character the template actually uses (`☀ ☾ × ✓` and any others
   you find), assert the loaded typeface can render it — via `FontMgr::match_family_style_character` or
   by rasterising and checking the ink is not `.notdef`. This is the test that would have caught
   this, and it is the one that keeps the template honest if the system font changes.
3. **No cache poisoning.** `get_or_load_family("Nonexistent Family")` must not poison subsequent
   lookups of that name.

## Falsification (required)

- M1: restore the 14-byte stubs → test 1 must go RED.
- M2: revert `get_or_load_family` to the poisoning version → test 3 must go RED.
- M3: drop the `dejavu-sans-fonts` candidate path → the glyph-coverage test must still pass **only if**
   the loaded face really has the glyphs; if it only passes because a different candidate covered it,
   that is a coincidence, not coverage — say so explicitly in the report.

## Gate

```
cargo test -p velox-renderer
cargo test -p velox-style
cargo test --workspace --no-fail-fast
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features velox-renderer/skia-native -- -D warnings
```

## Scope discipline

**Do not implement full glyph fallback** (per-codepoint font substitution across a font stack). That
is a renderer feature, it is a real design task, and it is not what makes the app wrong today. What
makes it wrong today is: stub assets, a candidate list that misses the good font, and a template that
assumes glyph coverage it never verified. Fix those; record the rest.

**Do not** "fix" the tofu by replacing `☀`/`☾`/`✓` with ASCII characters in the template. That hides
the defect, makes the UI worse, and leaves any future author of any app exposed to the same tofu.
(Contrast T7, which legitimately does fix *comments* that assert false things — those are lies about
the system, not design choices.)

## Sequencing

Independent of the layout lane. **Must precede T3** if T3 takes option (b), since that adds a field to
the same file.