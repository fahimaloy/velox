# T7 — delete the false comments; un-stale `template_scope_coverage.rs`

**Risk:** trivial · **Scope:** `velox-cli/**`, `velox-sfc/tests/**`

This is small, but it is not cosmetic bookkeeping: **each of these is a comment asserting something
about the system that is false**, and a false comment is how the next session wastes a day.

## F1 — `App.vx:78-85` claims the glyphs are present

The comment asserts `U+2600 ☀` and `U+263E ☾` are "both present in the only two faces the renderer
can load". **True for real DejaVu Sans, false for Noto Sans** — and Noto Sans is the one that wins on
this machine (`/usr/share/fonts/google-noto/NotoSans-Regular.ttf`, candidate #4). It contains
neither. It also does not contain `✓` U+2713.

Replace the assertion with a statement that is true, and that says **why the glyphs are a risk**:
velox has exactly one face, no glyph fallback, and which face that is depends on the host. Do not
just delete the comment — the information that "these characters depend on the host font" is worth
having.

## F2 — `TodoItem.vx:48-50` makes the same claim for `✓`

Same fix. The comment says both `✓` U+2713 and `×` U+00D7 are present in both loadable faces. False
for `✓`.

## F3 — `App.vx:812` is a stale citation

The comment cites `velox-style/src/lib.rs:812` `apply_styles_with_hover`. The function starts at
**`:786`** and the cascade loop is at **`:840-841`**. Re-grep before writing the new number — line
numbers move, and a citation that has drifted is worse than no citation because it certifies
unrelated code.

## F4 — `velox-cli/tests/template_scope_coverage.rs` is stale

The test asserts `.btn-add` is used in `TodoInput.vx` and `.todo-item.completed .todo-text` exists.
**Neither exists any more.** The test fails today.

**This failure was previously mis-diagnosed** as "the tree changed mid-gate, so the result is void."
It is not; it is a real staleness failure. Correct the diagnosis in any report that repeats the old
one.

Decide, and say which you did:
- **Update** the expectations to the current selectors, if the underlying contract (every `class` used
  in a template must be declared in that component's scoped CSS) is still right — it is a good
  contract and worth keeping.
- **Or** delete it, if the contract no longer holds. Do not leave it failing and do not delete it to
  make the suite green without saying why.

## F5 — `velox-sfc/tests/scope_combinator.rs` never feeds a comment to `scope_css`

This is the coverage gap that let **T1** ship: every existing case is comment-free. T1 adds the
comment cases. Do not duplicate that work here — cross-reference T1 and make sure this file's
existing cases still pass unchanged.

## Also (from T4, one line)

Add `text-align: center` to `.remove` in `TodoItem.vx`. It restores correct rendering today and makes
the template robust. It is **not** the fix — T4 is — but the sibling `.ghost` rule already does it, so
its absence is an oversight, not a style choice.

## Falsification

- M1: delete the contrast/consistency assertions added by T6 and re-run this task's scope — no effect
  expected, which is the point: this task must not depend on colour.
- M2 (the real one): re-introduce `.btn-add` usage in `TodoInput.vx` → the corrected
  `template_scope_coverage.rs` must go RED. If it does not, the corrected test is not testing the
  contract and must be rewritten.

## Gate

```
cargo test -p velox-cli
cargo test -p velox-sfc
cargo test --workspace --no-fail-fast
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features velox-renderer/skia-native -- -D warnings
```

## Standing rule

**A comment that asserts a fact about the system is a test that nobody runs.** When you find a false
one, either fix it to be true or delete it — and if you delete it, check whether the *reasoning* it
carried was load-bearing and belongs somewhere real (like `docs/RECONCILER.md` or the plan).
---

## T7h — `template_scope_coverage.rs` has its own broken comment stripper

`velox-cli/tests/template_scope_coverage.rs:218-239` defines a private `strip_css_comments` that
duplicates logic T1 fixed properly in `velox-sfc/src/codegen.rs`. It has **two** defects the production
one does not:

1. **String-blind** — a `/*` inside `"…"` or `url(…)` is treated as a comment opener, so
   `content: "/*"` silently truncates the rest of the sheet.
2. **UTF-8-destroying** — `out.push(bytes[i] as char)` walks *bytes* and casts each to `char`, so every
   non-ASCII byte becomes a Latin-1 codepoint. Any non-ASCII CSS (the template components are full of
   `×`, `☀`, `—`) is mangled.

It cannot import the production helper because that one is private. **Fix: make it `pub` and delete the
duplicate** — one stripper, one behaviour, and the test then exercises the shipped code. If exposing it
is judged unacceptable, keep the duplicate but fix defects 1 and 2 and add a comment naming the
production function it must stay in sync with.
