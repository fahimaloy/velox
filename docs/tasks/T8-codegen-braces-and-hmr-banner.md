# T8 — remove the redundant codegen brace; stop the HMR banner from lying

**Risk:** low · **Scope:** T8a `velox-cli/**` · **Scope:** T8b `velox-sfc/**`
**Split into two commits. T8b is blocked by T1 (both own `velox-sfc/`).**

Both problems were reported together in one `velox dev` run but have **no shared cause and no shared
fix**. Do not let one be described as a consequence of the other.

---

## T8a — "App started (HMR enabled)" is printed unconditionally

### Root cause

The banner is gated on `spawn()` succeeding, not on HMR being available:

```rust
// velox-cli/src/commands/dev.rs:1383-1387
match run.spawn() {
    Ok(c) => {
        println!("{}", green("App started (HMR enabled)"));
        Some(c)
    }
```

The bind failure happens on a **different thread**, and nothing carries the result back:

1. `dev.rs:1056` — `HmrListener::start(DEFAULT_HMR_PORT)` spawns a **thread**.
2. `dev.rs:971-980` — the thread binds. On failure it `eprintln!`s and `return`s. **The error goes to
   stderr on the thread and stops there.**
3. `HmrListener` is `{ slot, shutdown, handle }` (`dev.rs:958-962`, `:1006-1010`). There is **no**
   `bound: bool`, no `bind_error`, no `is_listening()`. `slot()` (`:1013-1015`) exposes only the
   connected-stream slot.
4. `dev.rs:1179-1180` — `spawn_app(&project, release, bin, hmr_port)` passes only the **port number**.

**So `dev_current` structurally cannot know the bind failed.** The lie is architectural, not a
missing `if`. The two prints are ~2 s apart only because the build takes that long; they are causally
unrelated.

**The same hole has a second symptom:** `send_hmr_reload(hmr.slot())` (`dev.rs:1235`) will print
`[velox] No HMR client connected — skipping reload` (`dev.rs:1284`) on **every** save, because the slot
stays `None` forever.

### Secondary finding: the port is not configurable

`velox-renderer/src/hmr.rs:23` `pub const DEFAULT_HMR_PORT: u16 = 31313;`, hardcoded at
`dev.rs:1056` (bind), `:1057`, `:1305` (banner). The **app** side honours `VELOX_HMR_PORT`
(`hmr.rs:46-56`), but `dev.rs:1378` always sets it to the same constant, so **there is no override
path**. Repo-wide grep for `hmr-port` / `--port` in `velox-cli` returns nothing.

The user-visible consequence: if any listener survives, HMR is permanently wedged for that project
and the only signal is a contradictory banner.

### What the code makes easy

**Bind synchronously.** Move `TcpListener::bind` out of the thread and into `HmrListener::start`,
returning `Option<Self>` / `Result<Self, io::Error>` and handing the **already-bound** listener to the
thread. The failure becomes a plain return value — no atomics, no channel, no race, and the port is
provably free before the banner is printed. This is strictly better than an `Arc<AtomicBool>` and the
code structure invites it.

Do **not** use an `is_bound()` probe — it is a race, since the thread may not have run yet.

### Required behaviour

1. The banner must reflect reality: `App started (HMR enabled)` vs `App started (HMR unavailable)`.
2. `print_banner` (`:1302-1306`) hardcodes the port and must reflect an unavailable port too
   (`HMR: port 31313 (unavailable)`), or move its HMR line after the bind attempt.
3. **Add a way to choose a different port** — `--hmr-port <N>`, and pass it to both the listener and
   the app's `VELOX_HMR_PORT`. Without this, a wedged port is unrecoverable without finding and
   killing an unknown process.
4. On bind failure, do **not** silently proceed as if fine — but also do not abort. Continuing without
   HMR is correct; lying about it is not.

### What is NOT the cause

**A SIGINT cannot leave the app child holding port 31313.** The *listener* lives in the `velox dev`
process (`dev.rs:972`); the app is the HMR **client** (`hmr.rs:73`). `AppChild` cleanup is irrelevant
to this symptom.

That said, `AppChild` **is** cleaned on every path, and you should not "fix" it: `dev.rs:785-820` is an
RAII `Option<Child>` whose `Drop` calls `shutdown()` → `kill()` + `wait()`; it is dropped at `:1108`
(`q`), `:1237` (rebuild), every other `break` (`:1199`, `:1206`, `:1343`, `:1350`), and on normal
return (`:1258-1260`). `HmrListener` has a `Drop` (`:1018-1025`) that sets the flag and **joins** the
thread, so the socket is released deterministically.

There is **no signal handler anywhere in `velox-cli`** — grep for `ctrlc`, `SIGINT`, `sigaction`,
`signal_hook` returns zero. On Ctrl-C the OS default sends SIGINT to the whole foreground process
group; all three processes die and the kernel closes the socket. **That is correct behaviour — do not
add a signal handler on the assumption it is a leak.**

### Tests

1. Bind to a port already in use → the banner says "unavailable", never "enabled".
2. Bind succeeds → the banner says "enabled".
3. `--hmr-port N` → both the listener and `VELOX_HMR_PORT` use `N`.
4. Assert the *invariant*, not the string: after `dev_current` returns, **the port must be free** —
   this catches a regression that adds a leak.

---

## T8b — the redundant `{ … }` around every `v-if` child

### Root cause

`velox-sfc/src/template_codegen.rs:2847-2860`, inside `emit_children_with_mode`:

```rust
let mut cond = String::new();
cond.push_str(&format!(r#"{{ if {} {{ {} }}"#, expr_if.trim(), inner_if));   // :2848
for part in chain_parts.iter() { cond.push(' '); cond.push_str(part); }
if let Some(e) = else_part { cond.push(' '); cond.push_str(&e); }
else { cond.push_str(r#" else { text("") }"#); }
cond.push_str(" }");                                                         // :2859
out.push_str(&format!("__children.push({});\n", cond));                      // :2860
```

**Three brace levels, only ONE is superfluous.** `rustc` points at column 21 — the **outer** `{`
(`4 + "__children".len()=10 + "." + "push".len()=4 + "("` = col 21):

```
push( { if COND { { let __props=…; …; Comp::render_with_callbacks(…) } } else { text("") } } )
      ^^^^^^^^^^^^^^^^^^^^^^^ superfluous — rustc: app.rs:1324:21
                          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ REQUIRED — template_codegen.rs:2158
```

The **inner** block is the `if` arm's own, and its content is the component render block from
`:2157-2158` (`let __props = …; let __callbacks = …; Comp::render_with_callbacks(…)`). **An expression
cannot hold `let` bindings.** Removing it would not compile.

**Why the outer one exists — and it is NOT for multi-statement bindings.** Its own test says so
(`velox-sfc/src/codegen_unit_tests.rs:154-165`):

```rust
// The conditional must NOT be pushed as a parenthesized value (the bug):
//   __children.push((if (...) { ... } else { ... }))
assert!(!rust.contains("__children.push((if "), ...);
// It must instead be pushed as a block that yields a single VNode:
//   __children.push({ if (...) { ... } else { ... })
```

Parens were the actual bug; the block was an over-correction. **Dropping the outer brace preserves the
paren fix.** `__children.push(if c { … } else { … })` is valid Rust.

### The discriminator is single-line, not component-vs-element

**It does not fire on every `v-if`.** `rustc`'s `UnusedBraces` has a gate
(`compiler/rustc_lint/src/unused.rs`, `check_unused_delims_expr`):

```rust
&& !cx.sess().source_map().is_multiline(value.span)
```

Any `v-if` whose body contains an element **with children** spans multiple lines and is suppressed. That
is why exactly 4 of the 9 `__children.push({` sites in the generated `app.rs` warn — verified against
`target/debug/build/myapp-8fce0a90703b6945/out/app.rs`:

| line | target | body | warns |
|---|---|---|---|
| 1274 | `<span class="glyph">` | 1274-1284 multi-line | no |
| 1297 | `<button class="ghost">` | 1297-1300 multi-line | no |
| 1311 | `<Todos>` (no v-if) | 1 line, 2 stmts | no |
| **1324** | `<Confirm>` | 1 line | **yes** |
| **1325** | `<Modal>` | 1 line | **yes** |
| 1364 | `<span class="glyph">` | multi-line | no |
| 1387 | `<button class="ghost">` | multi-line | no |
| **1414** | `<Confirm>` | 1 line | **yes** |
| **1415** | `<Modal>` | 1 line | **yes** |

1324/1325 are the `render_with` copies; 1414/1415 the `render_with_state` copies (the extra
`set_emit_dispatch(…)` statements come from `emit_dispatch_registrations`, `:2125-2132`). A component
render has no element children, so its block never breaks onto a second line.

**This is why the count is exactly 4 and reproduces both pairs.** A `v-if` on a *childless* plain
element (e.g. `<hr v-if>`) would also warn — it is not component-specific.

`v-for` + `v-if` (`:2321-2327`, `:2994-3000`) and `v-if` in slot content (`slots_map_expr`,
`:3964-3997`) never add the wrap and are unaffected.

### Tests that break — exactly two, both in `src/`, not `tests/`

Repo-wide grep for `push({ if` (excluding `target/`) returns **only** these two lines:

1. `velox-sfc/src/codegen_unit_tests.rs:163-167` `fn v_if_else_emits_block_push` — asserts
   `rust.contains("__children.push({")`. After the fix the count is 0 (its template is a bare `<div>`
   with `<p v-if>`/`<p v-else>`, no components, no `v-for`). The sibling paren assertion at `:156-160`
   keeps passing.
2. `velox-sfc/src/codegen_unit_tests.rs:198-203` `fn v_if_else_resolves_to_single_branch` — asserts
   `rust.matches("__children.push({ if").count() == 2`. Becomes 0.

**Rewrite, do not delete.** The invariant test 2 encodes is real and load-bearing: *"one conditional
push per render fn, so layout `source_index` does not desync."* Rewrite as
`rust.matches("__children.push(if").count() == 2`.

**What does NOT break** (this is why the fix is cheap):
`tests/fixtures/list_parent.rs:239,251`; `rows_state_child.rs:259,285`;
`rows_props_child.rs:283,309,359`; `tests/testdata/*.golden`; `tests/v_if_tests.rs`,
`v_else_tests.rs`, `v_else_if_tests.rs` (`v_else_if_tests.rs:14` asserts `rs.contains("else {")`, which
still holds). All of those are `v-for`-key or `v-for`-props blocks from `:2136` / `:2298` / `:2308`, not
v-if pushes.

### Minimal fix

Delete the wrap at `template_codegen.rs:2848` (emit `r#"if {} {{ {} }}"#`) and the matching
`cond.push_str(" }")` at `:2859`. `:2860` is unchanged.

### Required

Add a **compile-warning gate**: a scaffolded app must build with **zero warnings**. That is what makes
this task done — not "the brace is gone". Four warnings in generated code that ships to every user of
`velox init` is a real defect; the absence of a test asserting warning-freedom is how it recurs.

---

## Falsification (required, per commit)

- T8a: M1 — restore the unconditional `println!("App started (HMR enabled)")` → the banner test must go
  RED. M2 — make `--hmr-port` a no-op → the port test must go RED. M3 — remove `HmrListener`'s
  `Drop` join → the "port free after return" test must go RED.
- T8b: M1 — restore the outer brace → the "zero warnings" gate must go RED. M2 — also remove the inner
  brace → **must fail to compile**; that is a valid result here, but record it as "did not compile"
  rather than "not caught".

## Gate

```
cargo test -p velox-cli
cargo test -p velox-sfc
cargo test --workspace --no-fail-fast
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features velox-renderer/skia-native -- -D warnings
```

Plus, for T8b: **a real `velox init` + `cargo build` of the scaffolded app with zero warnings.** A
workspace green does not prove it — the warnings only appear in *generated* code.

## Sequencing

- T8a may run any time `velox-cli/` is free.
- **T8b is blocked by T1** — both own `velox-sfc/`. Never two lanes on one crate's codegen.
- T7 also touches `velox-cli/` and `velox-sfc/tests/`; sequence T8a and T7 so they do not overlap.