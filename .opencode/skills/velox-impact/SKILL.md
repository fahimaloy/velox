---
name: velox-impact
description: Use when about to edit, rename, delete, or move any symbol in the velox workspace. Runs gitnexus impact analysis and reports callers, risk, and execution flows. Mandatory before any edit per AGENTS.md.
---

# Velox Impact Analysis

**Run this before editing ANY symbol.** This is mandatory per `AGENTS.md`. Never substitute grep for graph analysis.

## How to run

### Via MCP (preferred — faster, richer output)

```
impact({target: "symbolName", direction: "upstream"})
```

For concepts/flows:
```
query({search_query: "concept or flow name"})
```

For a named symbol's full context:
```
context({name: "symbolName"})
```

### Via CLI fallback

```bash
# run from the repository root
node .gitnexus/run.cjs impact "symbolName" --direction upstream --repo .
```

For change detection (after editing, before committing):
```bash
node .gitnexus/run.cjs detect-changes --scope all --repo .
```

For regression review against main:
```bash
node .gitnexus/run.cjs detect-changes --scope compare --base-ref "main" --repo .
```

## What to report

From the impact result, extract and report:

1. **Callers** — every symbol that calls the target. List them with file:line.
2. **Risk level** — `LOW`, `MEDIUM`, `HIGH`, `CRITICAL`, or `UNKNOWN`.
3. **Risk note** — if `UNKNOWN`, say so explicitly: "gitnexus could not resolve callers — confirm with `grep -rn` before treating as safe."
4. **Execution flows** — the processes/flows that traverse this symbol.

## Rules

- **MUST warn on HIGH/CRITICAL risk pre-edit.** Do not use `riskSharedAxes` to waive a HIGH/CRITICAL warning.
- **MUST treat `risk: UNKNOWN` as unresolved, not as low.** An empty caller set is not evidence the symbol is unused — it can mean callers are not resolvable by the index (plain-object property access, dynamic dispatch, cross-language calls). Confirm with `grep -rn "symbolName" velox-*/src/` before treating as safe.
- **`partial: true` or `truncated: true` is not a clean check** — a zero means unseen, not unaffected. Re-run it.
- **NEVER rename symbols with find-and-replace** — use gitnexus `rename` which understands the call graph.
- **NEVER commit before running `detect_changes --scope all`** and confirming the result is clean (not partial/truncated).

## Key velox symbols to be careful with

These are high-risk to change without impact analysis:

| Symbol | Location | Why risky |
|--------|----------|-----------|
| `flush_queue` | `velox-core/src/signal.rs` | Touches every reactive path. **Note the real 1.1 bug was NOT the double-borrow the plan claimed** — `IS_FLUSHING: Cell<bool>` already guarded it. The defect was a *panic inside an effect body* leaving `IS_FLUSHING` stuck `true`, which silently kills all reactivity for the process |
| `NEXT_EFFECT_ID` | `velox-core/src/signal.rs` | Replaced `ptr_id` (`925fb69`). `ptr_id` was `eff.as_ptr() as usize`, and tombstones in `STOPPED_EFFECTS` were never pruned, so a recycled address inherited a dead effect's stopped flag. **Prune predicate is `weak.strong_count() > 0`** — NOT "still in `EFFECT_STORAGE`", because stop/`Drop` remove from storage in the same operation that records the tombstone |
| `compute_layout` | `velox-dom/src/layout.rs` | Entry point for all layout. **Add NO identity field to `VNode`** if you implement 2.2: it duplicates the existing `"key"` attribute and `VNode` is pure data. Thread a `u64` alongside, and key the cache on content — never on `source_index` (child ordinal, shifts on sibling insert) |
| `at()` | `velox-dom/src/layout.rs` | Recursive layout; **7 call sites, 2 descents per content-basis level** ⇒ `2^depth`. Probe and target share node + `ContainingBlock` but differ in `avail_w`/`avail_h`, so **a layout cache cannot dedupe them** — a lane proposing one as the fix has not read the two descents apart |
| `set_property` | `velox-dom/src/style.rs` | All CSS parsing flows through here. The honest-parity mechanism is now `pub const PARSED_BUT_UNRENDERED: &[(&str, &str)]`, immediately above `set_property`, consumed by `velox lint` (`de1e12f`). **`ComputedStyle` has zero production callers** — do not hang a fix off it |
| `scope_css` / `scope_selector_list` | `velox-sfc/src/codegen.rs` | **Task 0.1's diagnosis was wrong** — child hashing already worked (`feee06b`). `impact` before touching: a fix here was rejected because the failing test was legitimate behaviour |
| `apply_with_cascade` | `velox-style/src/lib.rs` | The real Task 2.3 cost: `ua.rules.clone()` + `author.rules.clone()` **every frame**. `ua_sheet()` is already a `OnceLock`; the plan's "ua_sheet is per-frame" premise was false. The live path writes declarations into the style **string** here |
| `make_view` | app-side closure in `velox-cli/templates/project/src/main.rs` | Returns a freshly-parsed `Stylesheet` **each frame**; 8 call sites in the renderer, not 2. Hoisting the parse is a *prerequisite* for caching the merge, not an independent win |
| `changed_file` | `velox-cli/src/commands/dev.rs` | 400 ms polling watcher → `notify` is Task 3.1, **never started**. Task 3.4 (`8ccdaa1`) fixed the *reload* path only, and the "150 ms blocking sleep at :467" premise was false — real sleeps are at the reload/build sites |
| `raster_n32_premul` | `velox-renderer/src/skia_surface.rs` | **Deliberate.** Task 5.8 = Option A, stay on the CPU raster surface; 949 tests assert EXACT BYTES and a `GrDirectContext` would break them. Its GPU resource cache defaults to 256 MB with no limit configured. Read the comment at the call site (`39b406d`) before "optimizing" this |

## Index freshness

The gitnexus index covers **3976 symbols, 10263 relationships, 342 execution flows**. If you suspect it's stale:

```bash
node .gitnexus/run.cjs analyze --index-only --repo .
```

## After editing

Run before committing:
```bash
node .gitnexus/run.cjs detect-changes --scope all --repo .
```

A clean result (not `partial` or `truncated`) is required. If it reports changes, review them and make sure they match what you intended.
