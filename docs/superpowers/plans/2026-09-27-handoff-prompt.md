# Handoff Prompt — Velox HTML/CSS Parity, Remaining Tasks

Copy everything below into a NEW session.

---

You are continuing a large, mostly-complete body of work on **Velox**, a Vue-SFC-syntax
Rust GUI framework with Skia rendering, at `/home/fahimaloy/Projects/personal/velox`.

Use the **`subagent-driven-development`** skill for the whole task. It is mandatory,
not optional: fresh implementer per task, independent reviewer per task, scoped
re-review per fix round, max 5 rounds, a ledger entry for every completion and ruling,
never two implementers in parallel, and never fix a review finding yourself in the
controller session. Read that skill before your first dispatch.

## Read these first, in this order

1. **`docs/superpowers/plans/2026-09-27-velox-parity-remaining-tasks.md`** — your
   plan. It contains the task sequence R-1…R-10, the full specification of the final
   task, the in-flight state description, the specialist-session inventory, and eight
   invariants that must survive everything. Start here and follow it.
2. **`docs/superpowers/plans/2026-09-24-velox-html-css-parity-fix.md`** — the
   *Global Constraints* section still binds every task. It also contains the original
   14 tasks, all of which are complete.
3. **`docs/AUDIT_VELOX_LAYOUT_HTML_CSS_PARITY_2026-09-24.md`** — 29 findings with
   evidence. **§10 is the one you need**: it covers everything discovered *after* the
   original audit, which is most of what remains.
4. **`.superpowers/sdd/2026-09-24-velox-html-css-parity-fix/progress.md`** — the
   ledger. It is the recovery map. Every prior ruling, deferred minor, and
   adjudicated finding is recorded there with its reasoning. Do not re-litigate
   anything already ruled there; read it first so you do not repeat a decision that
   was already made deliberately.

## The goal

Make Velox behave like regular HTML/CSS, so that every element, style, component,
prop, layout rule, overflow behaviour, resize behaviour and responsive rule behaves
the way a browser's would, and every `.vx` component's behaviour matches what Vue
does.

**The originally reported bug is already fixed and verified.** The boilerplate that
rendered with touching sections, an invisible header, clipped state text, and a
window that broke on resize now renders correctly at full screen and at 480×360.
Sixteen tasks are done and independently review-clean.

**What remains is unbuilt functionality, not defects.** The remaining items — inline
formatting context, `max-width`, `position:absolute`, root-level `v-if`/`:class`,
key-based list reconciliation — are CSS features that were never built. Some are
larger than all the completed work put together. Expect this to be feature
construction, not debugging.

## READ THIS BEFORE YOU TOUCH ANYTHING: the tree is in TDD-RED state

**`cargo test -p velox-sfc` currently fails. This is expected. Do not "fix" it by
weakening the test, and do not revert it.**

- `HEAD` is `55e0903` — a clean, review-clean commit.
- The working tree has **+85 uncommitted lines** in
  `velox-sfc/tests/template_codegen_tests.rs`, written by a fixer that was stopped
  mid-task (its session is retained — see the inventory in the plan).
- Those lines add two tests:
  - `resolve_mode_loop_rooted_binding_is_read_from_the_loop_item` — **this is the
    one failing. 27 pass, 1 fails.** It is the deliberate TDD red step.
  - `state_mode_loop_rooted_binding_output_is_unchanged` — passes.
- **The implementation has not been written.** The red test is the setup; writing
  the implementation is the next step (R-1).
- **Untracked scratch to delete, never commit:** `velox-sfc/tests/zz_dump17.rs`
  (a 1630-byte dump helper).

Your first action is to verify this state matches reality, then continue R-1 from the
green step.

## Task sequence

R-1 Resolve-mode `v-for` bindings resolve (IN FLIGHT — see above; also the last item
declared BLOCKING for the parity gate)
→ R-2 `v-model` on a loop-item field
→ R-3 root-level `v-if` and `:class` binding reactivity + Vue class merging
→ R-4 `max-width` and `position:absolute`
→ R-5 inline formatting context (largest remaining; must precede any `float` or
  `inline-block` work)
→ R-6 key-based list reconciliation
→ R-7 per-window resize-hook registry (replaces an accepted tradeoff)
→ R-8 a single render entry point (precondition; makes R-9 possible)
→ R-9 revisit signal batching — ONLY if R-8 actually landed
→ **R-10 FINAL end-to-end manual verification — see below**

R-3, R-4, R-5, R-6 and R-7 need briefs written before dispatch. R-1 and R-2 have
briefs already in `.superpowers/sdd/2026-09-24-velox-html-css-parity-fix/`.

## R-10 — THE FINAL TASK. Do this last, and do it exactly.

This is a manual, interactive procedure. It is the only task that touches anything
outside the worktree. The user specified it; follow it exactly.

1. Baseline: `git status --short` in the repo — expect nothing but the two
   intentionally untracked docs (`docs/AUDIT_VELOX_LAYOUT_HTML_CSS_PARITY_2026-09-24.md`
   and `docs/superpowers/plans/`).
2. `rm -rf ~/Desktop/myapp`  (destructive, explicitly requested by the user)
3. `cargo install --path velox-cli` then `velox --version`
4. `tmux kill-session -t velox 2>/dev/null` only if a stale session exists, then
   `tmux new-session -d -s velox -c ~/Desktop`
5. `tmux send-keys -t velox 'cd ~/Desktop' Enter`, then `velox init myapp`. **Wait for
   init to actually finish** — poll the pane, do not blind-sleep.
6. `tmux send-keys -t velox 'cd ~/Desktop/myapp' Enter`, then `velox run` Enter.
7. Watch the build and the window open. `tmux capture-pane -p -t velox` to read
   output; use `-S -3000` for scrollback when an error occurs.

**Stop conditions:**

- **The window opens successfully → STOP IMMEDIATELY.** Do not close `velox run`.
  Do not close the tmux session. The user will inspect both manually. Just report
  that the window opened and that tmux session `velox` is left running with the app
  live.
- **Any error occurs → do NOT stop.** Capture the full error, diagnose the root
  cause, fix it properly with a regression test, and re-run the whole procedure from
  step 1.

**Do not close the tmux session under any circumstances.** The user wants to inspect
the running app and the session themselves.

**Before blaming the code for a missing window:** a GUI window needs a display and a
compositor, and this project has a known failure mode — an EPIPE crash when no
compositor is available, where the renderer degrades to a no-op instead of opening a
window. So before concluding a regression:

- Run with `VELOX_DEBUG_COMPOSITOR=1` inside the tmux session to make the backend
  choice visible (this exists for exactly this purpose).
- Check whether `$DISPLAY` / `$WAYLAND_DISPLAY` are set in that environment.
- If there genuinely is no compositor, that is an **environment limitation, not a
  regression** — report it as such. Do not fabricate a code fix. Confirm by checking
  whether any previously-working window still opens.

## Specialised sessions available for reuse

| Alias | Holds | Use for |
|-------|-------|---------|
| fix-2 | `velox-sfc/src/template_codegen.rs`, `template_codegen_tests.rs`, Task 15/17 briefs | R-1, R-2, R-3, R-6 |
| ora-1 | `velox-sfc` (2290 lines), `velox-dom/src/layout.rs`, velox-style UA work; the reviewer that caught the false State-mode warning, the vacuous todo test, and the untested UA defaults | reviewer for SFC and layout work |
| fix-5 | `velox-dom/src/layout.rs`, `velox-dom/tests/layout_tests.rs`, `velox-style/src/ua.css`, `velox-style/tests/cascade.rs` | R-4, R-5 |
| ora-2 | velox-renderer + velox-core (was cancelled as hung mid-review) | R-7, R-8, R-9 |

**Prefer reuse when the domain matches.** velox-sfc work goes to fix-2 with ora-1
reviewing; velox-dom/velox-style work goes to fix-5. Passing a `task_id` to resume a
session saves real time; a session given only a "resume" prompt has no memory of its
own requirements, so always hand over the brief by file path.

**Never dispatch two implementers in parallel.** That pattern repeatedly triggered
request-credit exhaustion and cost a full task. One in-flight task at a time.

## Non-negotiable invariants

1. Single rounding authority: `velox-renderer/src/viewport.rs`. Never add a second
   DPI/scale rounding site.
2. Single style-cascade application site: `style_vnode_with_hover` in
   `velox-renderer/src/lib.rs`. Order is UA < author < inline.
3. State-mode generated output stays byte-identical unless a task explicitly targets
   State mode. The four goldens in `velox-sfc/tests/testdata/` are the guard, and
   regenerating one to make a test pass is forbidden.
4. No visual change without a headless-provable regression test, using
   `render_vnode_to_rgba` or `render_vnode_to_raster_png_with_scale`.
   **Never `render_vnode_to_raster_png`** — it does not run `compute_layout` and
   passes regardless of whether layout is correct. An entire class of false-green
   results in the previous session traced back to this one mistake.
5. A test that reimplements the behavior it claims to verify is **not evidence**. Two
   separate review findings were exactly this defect.
6. Never force-add anything under `.superpowers/` — git-ignored scratch, meant to be
   deleted at the end. This happened once; the cleanup commit is `76b5d2e`.
7. Falsify every behavioral claim before believing it. The implementers in the
   previous session falsified their own tests empirically every time a claim could
   have been wrong, and that is the standard to hold.
8. A report that overstates is a finding, not a nuisance. One implementer claimed a
   test "pinned" behavior it did not actually cover.

## What NOT to claim as done

State these limits explicitly in your final report; they are real and unfixed:

- `:key` compiles and resolves, but does **not** reorder or diff a list. Vue's
  list-key semantics are unimplemented (that is R-6).
- The resize-hook teardown is global, not per-window. That is an accepted tradeoff
  with a recorded cost, replaced by R-7.
- Signal batching is deferred, not implemented. It is preserved in `git stash@{0}`
  and `deferred-batch-redraw.rs.txt`, and R-8 is its precondition (R-9).
- `progress`/`meter` claim `display: inline` where browsers say `inline-block`, and
  `input`/`button`/`select`/`textarea` get no display declaration at all. Both are
  deliberate, commented deviations caused by inline-block layout being unimplemented.
  The real fix is R-5.
- There is no real inline layout yet: no inline boxes sharing a line box, no baseline
  alignment, no line breaking across inline elements. `float` is meaningless until
  R-5 lands.

Do not let any of these be reported as working. Every one of them was, at some point
in the previous session, within reach of being overstated.
