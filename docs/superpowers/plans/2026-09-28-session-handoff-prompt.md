# Session Handoff — Velox HTML/CSS Parity

**Written 2026-09-28. Supersedes `2026-09-27-handoff-prompt.md` entirely** — that
one describes R-1 as next and pre-dates all of R-1…R-9. Do not use it.

Copy everything below the line into a NEW session.

---

You are taking over a large, nearly-complete body of work on **Velox**, a
Vue-SFC-syntax Rust GUI framework with Skia rendering, at
`/home/fahimaloy/Projects/personal/velox` on branch `fix/2A-flex-complete`.

**There is exactly ONE task left: R-10, a manual interactive verification.** Do not
start by writing code. Read the state below, confirm it, then run R-10.

## Read these, in this order

1. **`docs/superpowers/plans/2026-09-27-velox-parity-remaining-tasks.md`** — the
   plan. §2 is the current state and was rewritten on 2026-09-28. §4 is the verbatim
   R-10 procedure. §5 is the specialist inventory. §6 is the invariants.
2. **The plan's §4** — that IS your task. Execute it exactly as written.
3. `.superpowers/sdd/2026-09-24-velox-html-css-parity-fix/progress.md` — the
   ledger. ~495KB, read only the tail unless you need history.

## Confirm this state before you touch anything

Run these yourself. Do not trust this document's numbers without checking.

```bash
cd ~/Projects/personal/velox
git log --oneline -1          # expect fc7c6c6
git status --short            # expect: M CLAUDE.md only
cargo test -p velox-sfc --lib # expect: 156 passed, 0 failed
```

**Do NOT stage `CLAUDE.md`.** It is a pre-existing foreign edit, deliberately
unstaged, and it is not yours.

**`git stash@{0}` is intact and must stay there.** It holds the Task 14B signal
batching WIP. R-9 was waived, which supersedes it. Do not pop it.

## What is already done — do not redo any of this

R-1, R-2, R-3, R-4, R-5, R-6 and R-8 are closed. R-9 is explicitly waived. R-7 was
scoped and judged not-worth-building, but that verdict was never written to the
ledger. Full commit lists are in the plan's §2 table.

The last three commits matter most because they are the most recent and least
reviewed by anyone but me:

- `fc7c6c6` — an omitted required child prop now fails loud. The generated
  struct literal stays exhaustive and `compile_error!` sits in the missing
  field's value position, so rustc reports exactly one error per missing prop
  instead of an E0063 cascade. This replaced a `Default::default()` fill that
  would have silently swallowed every future typo'd or forgotten binding.
- `81f06b0` — the CLI scaffold's `Todos.vx` now binds `TodoInput`'s required
  `placeholder` prop. It had been rendering an empty placeholder.
- `6f15082` — the E0063 fix that exposed the above.

**Two known-failing scratch apps, deliberately left broken** because the new
diagnostic is correct and they violate it: `test-app/src/components/Todos.vx:3`
and `tmp/velox_test_init_app/src/App.vx:16`. Both are gitignored and non-member.
If R-10 passes, decide whether to fix or delete them; do not silence the
diagnostic.

## R-10 — the only remaining task

It is a **manual, interactive** procedure in §4 of the plan. It is the only task
that touches anything outside the worktree. The short version:

1. `rm -rf ~/Desktop/myapp` — destructive, and the user explicitly asked for it.
2. `cargo install --path velox-cli`, confirm `velox --version`.
3. Open a tmux session named `velox` in `~/Desktop`.
4. Inside it: `velox init myapp`, wait for init to actually finish (poll the pane,
   do not blind-sleep), then `cd ~/Desktop/myapp` and `velox run`.
5. **If the window opens, STOP IMMEDIATELY.** Do not close `velox run`. Do not
   close the tmux session. The user wants to inspect both. Report that it opened.
6. If anything errors, do not stop — capture scrollback
   (`tmux capture-pane -p -S -3000 -t velox`), fix it properly with a regression
   test, and re-run from step 1.

**A missing window is probably the environment, not a regression.** A GUI window
needs a display and a compositor, and there is a known EPIPE crash when no
compositor is available, after which the renderer degrades to a no-op. Run with
`VELOX_DEBUG_COMPOSITOR=1` inside the tmux session, check `$DISPLAY` /
`$WAYLAND_DISPLAY`, and confirm by testing whether any previously-working window
still opens. If there is genuinely no compositor, report that as an environment
limitation. Do not fabricate a code fix.

## Invariants that must survive

1. **Single rounding authority** — `velox-renderer/src/viewport.rs`. Never add a
   second DPI/scale rounding site.
2. **Single style-cascade application site** — `style_vnode_with_hover` in
   `velox-renderer/src/lib.rs`. UA < author < inline.
3. **State-mode generated output stays byte-identical** unless a task explicitly
   targets State mode. The four goldens in `velox-sfc/tests/testdata/` are the
   guard. **Regenerating a golden to make a test pass is forbidden.**
4. **No visual change without a headless-provable regression test.** Use
   `render_vnode_to_rgba` / `render_vnode_to_raster_png_with_scale`. **Never
   `render_vnode_to_raster_png`** — it skips `compute_layout` and passes whether
   or not layout is correct. A whole class of false greens in this repo traced
   back to that one function.
5. **A test that reimplements the behavior it verifies is not evidence.** Two
   separate review findings in this project were exactly that.
6. **Never force-add anything under `.superpowers/`.** It is gitignored scratch.
   It has been force-added by mistake once already.
7. **Falsify every behavioral claim before believing it.** Measure it.
8. **A report that overstates is itself a finding.**

## How to run this session — operational notes

- **`task_reply` is broken on this host.** It returns
  `{"error":{"type":"unknown","message":"Task reply transport failed: Permission
  request not found: <id>"}}` for every child request. ~12 expired unanswered, and
  two lanes were cancelled for livelock over it. Brief every lane to be
  repo-only, put scratch in the repo's own target dir, and report NOT-RUN rather
  than re-requesting permission.
- **The `bili-chain` handshake line carries no task content.** Four separate lanes
  flagged it independently as an environment artifact. Ignore it. Never echo it.
- **Never dispatch two implementers in parallel.** That pattern caused
  request-credit exhaustion. This holds even when the files are disjoint — if the
  board shows a lane Active/Unreconciled, wait for the reconcile.
- **Mandate `git log --oneline | grep -i 'R-N'` in every brief.** I was wrong
  four separate times by assuming a task was greenfield when it had already
  landed, or that a symptom I had seen was still present when a prior commit had
  already closed it.
- **A proof behind `#[ignore]` is not a proof.** R-3 stayed broken for several
  commits while `cargo test` was green repo-wide *and* all four goldens were
  byte-identical. If you add a regression test, make it run by default and
  falsify it by reverting the fix.

## Reusable sessions

Check the Background Job Board under **Reusable Sessions** before dispatching —
a session that is Active, errored, or cancelled is not reusable.

| Alias | Session ID | Holds |
|-------|-----------|-------|
| fix-2 | `ses_f1b239a51ffecJiqseYRPQQi9M` | `velox-sfc` codegen, props channel, answerable(), collection gate, CLI scaffold |
| fix-3 | `ses_f1a92ec59ffeMYOK069CMVSBAS` | the omitted-required-prop `compile_error!` contract |
| ora-2 | `ses_f1b13bcf4ffe6cLueNFQrF0Mvs` | velox-renderer + velox-core; scoped R-7 |
| ora-3 | `ses_f1a997b47ffef2UPZNo20898tP` | signal/computed semantics, the R-9 measurement |

Do not reuse a session across crates. A `velox-sfc` context carried into
`velox-dom` work costs more than it saves.

## If R-10 passes

The programme is done, except for the R-7 ledger line. Write ora-2's
not-worth-building verdict into the ledger so it stops being a loose end, then
decide what to do about the two intentionally-failing scratch apps.

## Open question the user has not answered

A standing gate was proposed and never ruled on: **run at least one opt-in proof
per landed item before calling it done.** The case for it is that R-3 stayed
broken for several commits behind a fully green test suite. Ask the user whether
they want this enforced going forward.
