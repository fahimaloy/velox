# Velox HTML/CSS Parity — Remaining Tasks Plan

**Created:** 2026-09-27
**Status updated:** 2026-09-28 — R-1 through R-9 are all closed or waived. **R-10 is
the only remaining task.** The previous version of §2 described a TDD-RED tree
that no longer exists; it has been rewritten. Ignore any earlier copy of this file.
**Branch:** `fix/2A-flex-complete`
**HEAD at last update:** `fc7c6c6`
**Supersedes nothing** — this is the continuation of
`docs/superpowers/plans/2026-09-24-velox-html-css-parity-fix.md`. Read that
plan's Global Constraints; they still bind every task here. Findings and evidence
live in `docs/AUDIT_VELOX_LAYOUT_HTML_CSS_PARITY_2026-09-24.md` (§10 covers
everything discovered after the original audit was written).

---

## 1. Where things stand

**Done and independently review-clean (16 tasks).** The original defect the user
reported — the boilerplate rendering with touching sections, an invisible header,
clipped state text, and a window that broke on resize — is fixed and verified
visually at full screen and at 480×360.

Beyond the original scope, the work also found and fixed: the UA stylesheet was
never applied in any production render path; component-bound props compiled to
empty strings; the `velox init` template had drifted from a duplicate copy of the
examples; the whitespace guard between block boxes; and the absence of
`display: inline` UA defaults for phrasing elements.

**One task deliberately deferred, not failed:** signal batching (plan Task 14B)
was BLOCKED by the todo render proof and deferred by controller ruling, because
the flush could not be placed at a single choke point and a missed call site
renders stale data silently. Its work is preserved in `git stash@{0}` and as
`.superpowers/sdd/2026-09-24-velox-html-css-parity-fix/deferred-batch-redraw.rs.txt`.
Revisit only after R-8 establishes a single render entry point.

**What remains is a different category of problem.** The items below are CSS
features that were never built, not defects against existing ones. Several are
larger than everything done so far. The "make it behave like a browser" work is
substantially complete; this list is unbuilt functionality.

---

## 2. CURRENT STATE (rewritten 2026-09-28 — the previous version was false)

**The tree is GREEN. There is no TDD-RED state, no in-flight fixer, and no failing
test to continue from.** An earlier version of this section claimed
`cargo test -p velox-sfc` was deliberately failing and told you to finish R-1 from
the green step without weakening the test. All of that was true on 2026-09-27 and
is now stale. Measured today: `cargo test -p velox-sfc --lib` → `156 passed; 0
failed` in 0.38s; `cargo test --workspace` → 0 failures, 0 errors.

- **HEAD is `fc7c6c6`** ("velox-sfc: an omitted required child prop fails loud,
  and says which"), not `55e0903`. That is eight commits behind.
- **Working tree is clean of task work.** The only modified file is `CLAUDE.md`, a
  pre-existing foreign edit that was deliberately left unstaged — do not stage it.
- Untracked: `AGENTS.md`, the audit doc, and the plan docs. All intended.
- `velox-sfc/tests/zz_dump17.rs` is **already deleted.** Nothing to clean up.
- `git stash@{0}` is **intact and must stay there.** It holds the Task 14B signal
  batching WIP, which R-9's waiver supersedes. Do not pop it.

### Task dispositions

| Task | State | Evidence |
|------|-------|----------|
| R-1 | **CLOSED** | `17b4249`, `e72f43b`, `3762347`, `1843971`; plus the E0063 chain `6f15082`/`81f06b0`/`fc7c6c6` |
| R-2 | **CLOSED as a documented limitation** | `9a4b6a8` — a `v-model` on a loop item is read-only by design, and the warning now names the two routes that work |
| R-3 | **CLOSED** | `31cddf4`, `7aed12f3` (its 13 proofs now run by default) |
| R-4 | **CLOSED** | `9879ab6..be36f93`, review clean, 5 fix rounds |
| R-5 | **CLOSED** (R-5a review-clean at `a823179`; R-5b `be36f93..9a46187` after 3 fix rounds) | largest task in the programme |
| R-6 | **CLOSED** | `519c0a6` — `:key` stated as a non-goal, and reordering proven to need no reconciler |
| R-7 | **⚠️ SCOPED, NOT CLOSED — verify before treating as done** | ora-2 judged a per-window hook registry not-worth-building. **There is no commit and no ledger line recording this.** Treat as open until confirmed. |
| R-8 | **LANDED** | `b0011fe` — one render prologue funnel, dead wgpu loop deleted |
| R-9 | **WAIVED on measurement** | 2 of 13 handlers do >1 signal mutation, both the same `add_todo` shape. A single frame-boundary in an already frame-rate-limited GUI buys nothing measurable. No benchmark or profiler evidence ever existed for it. |
| R-10 | **NOT STARTED — the only remaining task** | manual, interactive; spec in §4 |

**R-10's gate is now satisfied** ("do not start until R-1 through R-9 are done or
explicitly waived"). R-9 is explicitly waived. R-7 is the one loose end; see below.

### Carry-forward items a new session must not rediscover the hard way

1. **R-7 is unrecorded.** If R-10 passes, the programme is done and R-7 can stay
   closed. Write the ora-2 verdict into the ledger so it stops being a loose end.
2. **Two gitignored scratch apps now fail loud by design** — they bind fewer
   props than their children declare, and the new `compile_error!` names each one.
   Found, reported, deliberately not fixed: `test-app/src/components/Todos.vx:3`
   and `tmp/velox_test_init_app/src/App.vx:16`.
3. **The CLI scaffold is exercised by NO test in the repo** and is the only in-repo
   user of the typed `PropsArg` literal. Its breakage is what exposed the E0063.
4. **`task_reply` is broken on this host** — it returns
   `{"error":{"type":"unknown","message":"Task reply transport failed: Permission
   request not found: <id>"}}` for every child request, and ~12 expired
   unanswered. Brief every lane to be repo-only and to report NOT-RUN rather than
   re-requesting. This cost two cancelled lanes.
5. **The `bili-chain` handshake line carries no task content.** Three separate
   lanes flagged it independently as an environment artifact. Ignore it; never
   echo it.
6. **Carried R-8 concerns** (landed, but not fully closed): the render funnel is
   not single — three commented exception classes remain; the measurer is
   process-global, so a later call can inherit it and mask a bug; and the 15
   `#[ignore]`d GPU skia tests were NOT-RUN.
7. **Standing gate, proposed and never answered by the user:** run at least one
   opt-in proof per landed item before calling it done. R-3 stayed broken for
   several commits while `cargo test` was green repo-wide *and* all four goldens
   were byte-identical.

---

## 3. Task sequence

Process rules for every task: fresh implementer per task, independent reviewer
per task, scoped re-review per fix round, max 5 rounds, ledger every completion
and ruling, never run two implementers in parallel, never fix a review finding
in the controller session. Full detail is in the
`subagent-driven-development` skill.

### R-1 — Resolve-mode `v-for` bindings must actually resolve  *(CLOSED)*

**The last item declared BLOCKING for the parity gate.** In Resolve mode a
bound attribute inside `v-for` emits `resolve("todo.id")`, but no loop-rooted key
is ever registered, so it falls through to `String::new()` — `:key` compiles and
then evaluates to an empty string.

Brief: `.superpowers/sdd/2026-09-24-velox-html-css-parity-fix/task-17-resolve-mode-brief.md`

**Core design constraint:** `make_resolve` is built once, outside any loop, and
returns a `String` for a key name. Inside `for todo in &todos { … resolve("todo.id") }`
it cannot see the loop's `todo`. Preferred fix is to emit the direct expression
(`todo.id.to_string()`) for loop-rooted bindings, since inside a `v-for` the item
is a real Rust binding and there is nothing to resolve — which is what State mode
already does. Shadowing the resolver inside the loop is the accepted fallback.
Redesigning `make_resolve` is rejected as out of scope.

**Also required:** the bound-attribute diagnostic must STOP firing for a binding
that now genuinely resolves. A warning on working code is the exact defect Task 15
fixed for State mode. If any loop binding still cannot resolve, diagnose it — do
not ship a silently-empty path.

**Verify:** `cargo test -p velox-sfc --test template_codegen_tests -v`,
`cargo test -p velox-sfc`, all three example proof suites, `cargo build --workspace`,
`cargo fmt --check`. The four goldens in `velox-sfc/tests/testdata/` must still
match **unmodified** — regenerating a golden to pass is forbidden.

### R-2 — `v-model` bound to a loop-item field (E-1)  *(CLOSED as a documented limitation)*

Same class as R-1: `v-model="todo.text"` on a loop item. If it falls out of R-1's
code path for free, take it; otherwise do it as its own task. Do not expand R-1
to cover it.

### R-3 — Root-level `v-if` and `:class` bindings (G-5)  *(CLOSED)*

`make_resolve` recognizes only interpolation keys, so a `v-if` on the root element
never toggles, and a bound `:class` **replaces** the static class list instead of
adding to it — the opposite of Vue's class-merging semantics. Both are silent:
the code compiles and renders wrong.

Brief needed. Scope: root-level `v-if` reactivity, and `:class` merging with the
static `class` attribute rather than replacing it.

### R-4 — `max-width` and `position:absolute` (G-4)  *(CLOSED)*

Both parse and are silently ignored by layout. `position:absolute` has a
half-built path at `velox-dom/src/layout.rs:305-470`. Independent of the inline
formatting context, so it can land before R-5. `float` is deliberately NOT in
this task — it is meaningless without R-6.

### R-5 — Inline formatting context (the largest remaining item)  *(CLOSED)*

Today `display: inline` exists only as a classification used by the whitespace
guard. There is no real inline layout: no inline boxes sharing a line box, no
baseline alignment, no line breaking across inline elements. `inline-block` and
`float` are both meaningless until this lands.

**This must land before any `float` or `inline-block` work.** R-4's `max-width`
and `position:absolute` do not depend on it and may go first.

Carry-forward from Task 16: the negative-control test
`unstyled_inline_level_tags_keep_their_collapsing_whitespace` currently proves
line-box survival, not a visible horizontal gap. With real inline layout that
test must be revisited to assert the pixel-level claim.

### R-6 — Key-based list reconciliation (G-1 follow-on, T-20)  *(CLOSED — `:key` stated as a non-goal)*

`:key` now compiles and resolves. It does **not** reorder or diff a list — Vue's
list-key semantics are not implemented. This task adds them. Depends on R-1.

### R-7 — Per-window resize-hook registry (T-21)  *(SCOPED, NOT CLOSED — verify)*

Replaces an accepted tradeoff. `run_all_destroy_hooks` clears the whole thread-local
registry, so tearing down one window can clear another's hooks. That was chosen
over the opposite bug (leaked hooks firing against a surviving window) because the
leak hit the in-tree HMR full-reload path while simultaneous same-thread Skia
windows have no in-tree caller. Low priority; no user-visible symptom today.

### R-8 — A single render entry point (precondition, not user-visible)  *(LANDED)*

Every render path — the winit event loops, `render_vnode_to_rgba`,
`render_vnode_to_raster_png_with_scale`, and any future consumer of a
`make_view`-produced VNode — currently reaches the layout pipeline through
structurally different call sites. That is why the deferred signal-batching change
could not be made safe: "did every path flush?" is answerable only by enumeration
today, and a missed path renders stale data silently.

Deliverable: one entry point every path funnels through, so correctness is
answerable by construction. Then R-9 becomes possible.

### R-9 — Revisit signal batching (conditional, only after R-8)  *(WAIVED on measurement — see §2)*

Only attempt if R-8 actually landed. Recover the preserved work from
`git stash@{0}` / `deferred-batch-redraw.rs.txt`. Its hard gate is that
`cargo test -p velox-example-todo` and `-p velox-example-counter` must pass; a
regression there is a finding about the change, not grounds for adjusting the
example.

### R-11 — `velox init` from outside a workspace emits an unbuildable scaffold  *(OPEN — found 2026-09-28 during R-10)*

`generate_cargo_toml(name, project_dir)` at `velox-cli/src/commands/init.rs:264`
has three branches:

1. `VELOX_PATH` set → path deps (:265-279)
2. `find_velox_workspace()` walking up from CWD finds one → path deps (:284-317)
3. **no workspace found → git deps pinned to `crate::velox_git_rev()`** (:318-323)

Branch 3 pins every dependency to the *building CLI's own local commit*. Run
`velox init` from outside a velox checkout (the normal user case) with a CLI
built from unpushed commits, and the scaffold is guaranteed unbuildable:

```
error: failed to get `velox-core` as a dependency of package `myapp v0.1.0`
Caused by: Unable to update https://github.com/fahimaloy/velox?rev=fc7c6c6
Caused by: revspec 'fc7c6c6' not found; class=Reference (4); code=NotFound (-3)
```

Measured: `git ls-remote origin | grep -c fc7c6c6` → `0`. The error names cargo
and never mentions that the cause is an unpinned local rev, so triage is
misleading. The code never checks the rev is reachable on the remote.

Workaround until fixed: `velox init <name> --local <path-to-velox>` (sets
`VELOX_PATH`, takes branch 1). §4 step 4 now uses this.

Suggested fix direction (needs a real implementer + a regression test): branch 3
should pin to a rev/branch that actually exists on the remote, or refuse and tell
the user to pass `--local`, rather than silently emitting a dead rev. Note
`velox init` currently has NO test coverage in the repo and is the only in-repo
user of the typed `PropsArg` literal.

### R-10 — FINAL: end-to-end manual verification  *(in progress 2026-09-28)*

**Do not start R-10 until R-1 through R-9 are done or explicitly waived** — that
gate is satisfied. R-11 is independent of R-10 and can land in either order. Its
full specification is §4 below. It is a manual,
interactive procedure — not a headless test — and it is the only task that
touches anything outside the worktree.

---

## 4. R-10 — FINAL TASK SPECIFICATION (verbatim procedure)

The user specified this procedure. Execute it exactly.

```bash
# 0. Baseline: confirm the tree is clean and everything committed
cd ~/Projects/personal/velox
git status --short          # expect: nothing but the two intentionally untracked docs
git log --oneline -1

# 1. Remove the previous test project  (DESTRUCTIVE — explicitly requested)
rm -rf ~/Desktop/myapp

# 2. Locally install the velox CLI
cargo install --path velox-cli
# *** DO NOT trust `velox --version` to confirm this. *** On this host there are
# TWO binaries and BOTH report 0.1.0:
#   ~/.local/bin/velox  (dated 2026-09-09, stale)  -- EARLIER on PATH, so it WINS
#   ~/.cargo/bin/velox  (freshly built by the line above)
# `cargo install` writes to ~/.cargo/bin, so a bare `velox` silently runs the stale
# one. Confirmed by content: the stale binary contains 0 references to
# "components/Todos.vx" and omits Todos.vx/TodoInput.vx from the scaffold, while
# the fresh one contains 2. Verify by CONTENT, not version:
strings ~/.cargo/bin/velox | grep -c 'components/Todos.vx'   # must be >= 1
# Then pin it for the whole procedure:
export PATH="$HOME/.cargo/bin:$PATH"
command -v velox        # must print /home/fahimaloy/.cargo/bin/velox

# 3. New tmux session named `velox`  (so the window keeps running after you stop)
tmux kill-session -t velox 2>/dev/null   # only if a stale one exists
tmux new-session -d -s velox -c ~/Desktop

# 4. Scaffold and run, all inside that tmux session
tmux send-keys -t velox 'cd ~/Desktop' Enter
sleep 1
# *** --local IS REQUIRED. *** Without it `generate_cargo_toml`
# (velox-cli/src/commands/init.rs:264) finds no workspace above ~/Desktop and falls
# to its third branch (:318-323), which pins every dependency to
# `crate::velox_git_rev()` -- the *building CLI's local commit*. That rev is
# unpushed, so the build dies with:
#     error: failed to get `velox-core` as a dependency of package `myapp`
#     Unable to update https://github.com/fahimaloy/velox?rev=<local-sha>
#     revspec '<local-sha>' not found
# --local sets VELOX_PATH and takes the first branch (:265-279), emitting path
# dependencies, which is what a from-a-local-checkout verification needs.
tmux send-keys -t velox 'velox init myapp --local ~/Projects/personal/velox' Enter
#   WAIT for init to finish before continuing — poll for a sentinel, do not blind-sleep
#   VERIFY the deps before running, or you will waste a build on the same failure:
#   grep velox-core ~/Desktop/myapp/Cargo.toml     # must show path =, NOT git =
tmux send-keys -t velox 'cd ~/Desktop/myapp' Enter
tmux send-keys -t velox 'velox run' Enter

# 5. Watch the build and the window open
#    tmux capture-pane -pt velox    (repeat; the window is drawn on the host display)
```

**STOP CONDITIONS — this is the important part:**

- **The window opens successfully** → **stop immediately.** Do not close
  `velox run`. Do not close the tmux session. The user will inspect both manually.
  Report that the window opened and that the tmux session `velox` is left running
  with the app live.
- **Any error occurs** → do **not** stop. Capture the full error
  (`tmux capture-pane -p -S -3000 -t velox` to get scrollback), diagnose the root
  cause, fix it properly with a regression test, and re-run the whole procedure
  from step 1.

**Environment vs code — read before blaming the code.** A GUI window needs a
display and a compositor. The project has a known failure mode: an EPIPE crash
when no compositor is available, in which case the renderer degrades to a no-op
instead of opening a window. Before treating a missing window as a defect:

- Run with `VELOX_DEBUG_COMPOSITOR=1` inside the tmux session to make the
  backend choice visible (added for exactly this purpose).
- Check `$DISPLAY` / `$WAYLAND_DISPLAY` are set in that environment.
- If there genuinely is no compositor, that is an environment limitation, not a
  regression — report it as such rather than fabricating a code fix. Confirm by
  checking whether any previously-working window still opens.

**Do not close the tmux session under any circumstances.** The user explicitly
wants to inspect the running app and the session themselves.

---

## 5. Reusable specialist sessions

Inventory as of 2026-09-28. Aliases are only reusable if they appear on the
Background Job Board under **Reusable Sessions**; anything listed as Active,
errored, or cancelled is not.

| Alias | Session ID | Specialist | Holds | Use for |
|-------|-----------|-----------|-------|---------|
| fix-2 | `ses_f1b239a51ffecJiqseYRPQQi9M` | fixer | `velox-sfc/src/template_codegen.rs`, `codegen.rs`, the props channel, the answerable() predicate, the R-1e-1 collection gate, the CLI scaffold | any further `velox-sfc` codegen work |
| fix-3 | `ses_f1a92ec59ffeMYOK069CMVSBAS` | fixer | the omitted-required-prop `compile_error!` contract and `velox-sfc/tests/props_arg_compiles.rs` | props-arg diagnostics |
| ora-2 | `ses_f1b13bcf4ffe6cLueNFQrF0Mvs` | oracle | velox-renderer + velox-core; scoped R-7 | render architecture, R-7 |
| ora-3 | `ses_f1a997b47ffef2UPZNo20898tP` | oracle | signal/computed semantics, the R-9 batching measurement, the Option A props ruling | signal semantics, design trade-offs |

**Gone from the board:** `ora-1` and `fix-5` were cancelled or errored and are NOT
reusable. An earlier version of this table listed them; ignore that.

**Prefer reuse when the domain matches.** Do not reuse a session across crates — a
`velox-sfc` context carried into `velox-dom` work costs more than it saves.
**Never dispatch two implementers in parallel**; that pattern repeatedly triggered
request-credit exhaustion in a previous session. If the board shows a lane as
Active/Unreconciled, you cannot dispatch a second implementer even for disjoint
files — wait for the reconcile.

---

## 6. Invariants that must survive everything

1. **Single rounding authority** — `velox-renderer/src/viewport.rs`. Never add a
   second DPI/scale rounding site.
2. **Single style-cascade application site** — `style_vnode_with_hover` in
   `velox-renderer/src/lib.rs`. UA < author < inline.
3. **State-mode generated output stays byte-identical** unless a task explicitly
   targets State mode. The four goldens are the guard.
4. **No visual change without a headless-provable regression test.** Use
   `render_vnode_to_rgba` / `render_vnode_to_raster_png_with_scale`.
   **Never `render_vnode_to_raster_png`** — it does not run `compute_layout` and
   passes regardless of whether layout is correct. An entire class of false-green
   results in the previous session traced back to this.
5. **A test that reimplements the behavior it verifies is not evidence.** Two
   separate review findings in the previous session were exactly this defect.
6. **Never force-add anything under `.superpowers/`** — it is git-ignored scratch
   (`*` in its own `.gitignore`) and is meant to be deleted at the end. This
   happened once already; the cleanup commit is `76b5d2e`.
7. **Every behavioral claim gets falsified before it is believed.** Implementers
   in the previous session falsified tests empirically in every case where a claim
   could have been wrong, and that is the standard to hold.
8. **A report that overstates is a finding.** One implementer claimed a test "pinned"
   behavior it did not cover; the reviewer caught it.
