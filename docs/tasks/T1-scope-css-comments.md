# T1 — `scope_css` must skip CSS comments

**Risk:** low · **Scope:** `velox-sfc/**` · **Blocks visual verification of T6**

## Root cause

`velox-sfc/src/codegen.rs:307-368` `scope_css(css: &str, scope_id: &str) -> String` walks the CSS
character by character into a `prelude` buffer, flushing on `{`. It special-cases exactly four
things, all visible in the `'{'` arm:

1. `trimmed.starts_with('@')` → emit the prelude unscoped (at-rule)
2. inside `@keyframes` → emit unscoped
3. `trimmed.is_empty()` → emit as-is
4. `inside_style` (a nested selector inside a declaration block) → emit as-is

It does **not** recognise `/* … */`. A comment between two rules is therefore accumulated into the
front of the **next** rule's prelude. `scope_single_selector` (`codegen.rs:388-414`) then splits that
prelude on whitespace and appends the scope attribute to **every token**:

```
authored   .dark .app
emitted    [data-v-APP] .dark[data-v-APP] .app[data-v-APP]
```

`/*` and `*/` both survive into the output, so the CSS lexer still closes the comment in place — what
remains is a stray **leading attribute-only compound selector**. That turns a two-part descendant
selector into a three-part one requiring a scope-tagged ancestor *strictly above* the `.dark`
carrier. The carrier is the tree root, whose ancestor list is empty (`velox-style/src/lib.rs:898`), so
`match_prefix` (`lib.rs:662-694`) computes candidates `(below + 1).max(0)..ancestors.len()` =
`1..1` = empty. The rule can never match, in any context.

## Blast radius — 4 rules in `App.vx`, 21 across all six components

**Counted at source (2026-09-02, `awk '/<style/,/<\/style>/' | grep -c '/\*'`):** one dead rule per
comment, because `scope_css` clears the prelude on `{` so only the *next* rule absorbs it.

| component | comments in `<style>` = dead rules |
|---|---|
| `src/App.vx` | **4** |
| `src/components/Confirm.vx` | 3 |
| `src/components/Modal.vx` | 3 |
| `src/components/TodoInput.vx` | 3 |
| `src/components/TodoItem.vx` | 5 |
| `src/components/Todos.vx` | 3 |
| **total** | **21** |

In `App.vx` the four casualties are `.app` (comment at style-line 2), `.toggle` (:66), `.rule` (:110)
and **`.dark .app` (:145, immediately before the `.dark` block)**. The other three are cosmetic; the
fourth is the dark-mode symptom.

### The dark-mode symptom is exactly this, and T6 cannot be verified until this lands

`.dark` rides on the root `.app` element, so `.dark .app` matches the root and `.dark .card` matches the
cards. With `.dark .app` dead the **page keeps its light background while the cards turn dark** — which
is precisely what the screenshot shows (white page, dark cards, dark `Add` button). That is the
reported "dark mode not showing correctly".

T6's palette work is a **separate, still-real** defect (its own dark card-on-page ratio is 1.08:1). Both
must land, and dark mode may not be called correct until it is re-screenshotted after **both**.

Every rule that immediately follows a comment in a scoped block is corrupted:

- `App.vx` `.app`, `.toggle`, `.rule`, **`.dark .app`**
- `Todos.vx` `.todos`, `.composer`, **`.dark .composer`**
- `TodoItem.vx` `.row`, `.check`, `.completed .check`, `.completed .check:hover`, **`.dark .todo-item`**
- `TodoInput.vx` `.input`, **`.dark .input`**
- `Modal.vx` `.overlay`, **`.dark .overlay`**
- `Confirm.vx` `.overlay`, `.head`, **`.dark .overlay`**

Re-grep the templates for the exact current set; the list drifts.

**This is not a template defect.** Any velox app whose `<style scoped>` block contains a CSS comment
gets the same corruption. `<style scoped>` is a documented, user-facing feature.

## Objective

A CSS comment between two rules must not change how either rule is scoped.

## Required behaviour

`scope_css` must treat `/* … */` as whitespace — or, at minimum, must never let comment text
contribute tokens to a selector prelude. It must handle:

1. A comment between two rules (the reported bug).
2. A comment **inside** a selector prelude: `.foo /* x */ .bar { }`.
3. A comment **inside a declaration block**: `.foo { color: red; /* note */ background: blue; }` —
   the comment must not leak into the *next* rule either.
4. A comment at the very start of the input.
5. An **unterminated** comment at end of input — must not hang or panic.
6. A comment containing `{`, `}`, or `[data-v-` — must not be mistaken for structure.
7. **A `/*` that is not a comment**: a string literal (`content: "/*"`, `url(/*)`) or a `url()` /
   quoted attribute must not start comment mode. This is the classic CSS-tokeniser trap and is the
   one way a naive fix makes things worse.

Do **not** hand-roll a full CSS lexer unless you can also satisfy (7) — the parser already exists
(`velox-style`), so consider whether the comment can be stripped with an existing, tested tokenizer,
and whether stripping before scoping is safe for (7).

## Tests — `velox-sfc/tests/scope_combinator.rs`

The existing file covers `scope_css` but **never passes a comment to it**. That is the coverage gap
that let this ship. Add a table covering all seven cases above.

For case 1 specifically, assert the emitted selector is exactly `.dark[data-v-x] .app[data-v-x]` —
i.e. **two** compounds, not three. A weaker assertion ("output contains `data-v-x`") passes under the
bug; that is exactly the kind of vacuous test to avoid here.

## Falsification (required before the commit)

Mutations that must each turn the suite RED:

- M1: restore the old behaviour — treat `/*` as an ordinary character in the prelude.
- M2: strip comments **only when they sit between a `}` and the next prelude start** — must still be
  caught by cases 2, 3 and 6.
- M3: strip comments by `split("/*").next()` — must be caught by cases 5 and 7.
- M4: strip comments *after* scoping instead of before — must be caught by case 1.

If any mutation does not compile, that is **not** a pass — it is an invalid sweep. Two prior sweeps in
this repo were void for exactly that reason.

## Gate

```
cargo test -p velox-sfc
cargo test --workspace --no-fail-fast     # must not regress; report exact counts
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features velox-renderer/skia-native -- -D warnings
```

Also emit, and attach to the report, the **`scope_css` output for `App.vx`'s real `<style scoped>`
body** before and after, so the 18-rule list can be confirmed fixed rather than argued about.

## Notes

- Do not "fix" this by deleting the comments from the templates. The template comments carry real
  reasoning (they explain why the dark block must come last). The engine is wrong; the template is
  right.
- There is an unrelated but adjacent bug found by recon and **out of scope here**: the legacy
  `draw_node` painter (`velox-renderer/src/skia_render.rs:1143-1410`) is reachable only from the
  headless PNG helper and runs no layout. Do not touch it.