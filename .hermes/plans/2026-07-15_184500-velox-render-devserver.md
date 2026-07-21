# Velox: Fix Rendering + Professional Dev Server

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Make the initialized Velox app (`velox dev`) render its full UI (background, counter, conditional text, 3 buttons, component children) correctly, and rebuild the dev server into a Vite-like, professional, interactive experience.

**Architecture:** The pipeline is `velox-sfc` (`.vx` → Rust `VNode` tree + `STYLE` string) → `velox-style::Stylesheet::parse` + `apply_styles_with_hover` (attaches inline `style` attrs) → `velox-dom::layout::compute_layout` → `velox-renderer::run_window_vnode_skia` (Skia draw). The rendering bug is in **(a)** the SFC codegen for `v-if`/`v-else` (malformed `push` expression + desynced `source_index` in layout) and **(b)** the stylesheet parser not matching rules for `.app { background: #1a1a2e }`, so the brown default clear color shows through. The dev server is a bare `cargo run` loop in `velox-cli/src/commands/dev.rs` that needs a full rewrite (colors, progress, error panel, click-to-reload, build status).

**Tech Stack:** Rust, Cargo, clap (CLI), winit + softbuffer + skia-safe (renderer), cssparser (styles), `velox-sfc`/`velox-dom`/`velox-style`/`velox-renderer`/`velox-cli` workspace crates.

---

## Part 0 — Reproduction (do this first, no code change)

### Task 0: Reproduce the broken render and capture a baseline

**Objective:** Confirm the exact broken state before fixing, so every fix has a verify step.

**Files:**
- Read: `velox-cli/templates/project/src/App.vx`
- Run: `cd /home/fahimaloy/Projects/personal/velox && cargo install --path velox-cli --force` (already done in session; rerun if crates changed)
- Run: `cd /tmp && rm -rf test_inited_app && velox init test_inited_app && cd test_inited_app && velox dev` then inspect the window.

**Step 1:** Start `velox dev` in `tmp/test_inited_app`. Observe: brown window, "Velox App" text top-left only, no card/counter/buttons.

**Step 2:** Confirm the generated tree is malformed:
```bash
grep -n "v-if\|v_else\|__children.push((" tmp/test_inited_app/target/debug/build/*/out/app.rs | head -40
```
Expected: see `__children.push((if ((resolve("positive") ...` — the extra parens and the `(if ...)` form pushed as a value (the bug).

**Step 3:** Record the baseline. Take a screenshot (Hermes desktop) of the broken window and the terminal output for comparison later.

**Verification:** You can articulate the two symptoms: (1) styles not applied → brown bg + unstyled title; (2) siblings after `v-if` not drawn.

---

## Part A — Fix the SFC `v-if`/`v-else` codegen

### Task A1: Add a failing unit test for conditional child emission

**Objective:** Lock the desired codegen shape: a `v-if`/`v-else` pair must emit a Rust **block** that yields one `VNode` and is pushed as `__children.push({ ... })`, with no dangling parens, and the sibling order preserved.

**Files:**
- Modify: `velox-sfc/src/codegen_unit_tests.rs` (create if missing) — add test module.
- The crate already has `velox-sfc/src/codegen_unit_tests.rs`; append a test that calls the public codegen entry (`velox_sfc::generate` or `template_to_rust`) on a fixture template.

**Step 1:** Write failing test:
```rust
#[test]
fn v_if_else_emits_block_push() {
    let tpl = r#"<template>
      <div class="app">
        <p class="a">{{ x }}</p>
        <p v-if="ok" class="b">yes</p>
        <p v-else class="c">no</p>
        <p class="d">{{ y }}</p>
      </div>
    </template>"#;
    let rust = velox_sfc::generate(tpl, None).unwrap();
    // Must NOT contain the malformed "(if (" push form:
    assert!(!rust.contains("__children.push((if "), "v-if must not be pushed as a parenthesized value: {}", rust);
    // Must contain a block-push that yields a VNode:
    assert!(rust.contains("__children.push({"), "expected block push for conditional: {}", rust);
    // And must still push the unconditional siblings a and d:
    assert!(rust.contains("\"a\"") && rust.contains("\"d\""), "siblings dropped: {}", rust);
}
```
(Adjust the public API name to whatever `velox-sfc/src/lib.rs` actually exposes for "compile template to rust string" — search `lib.rs` for `pub fn`.)

**Step 2:** Run `cargo test -p velox-sfc v_if_else_emits_block_push`.
Expected: FAIL (`!contains` assertion triggers because current code emits `__children.push((if ...`).

**Step 3:** Leave failing; move to implementation.

### Task A2: Fix `emit_children_with_state` conditional branch

**Objective:** Emit conditional chains as `__children.push({ if (cond) { inner } else { inner } });` so the value is a block (yields `VNode`), not a parenthesized `if`-expression — and so layout's `source_index` stays aligned.

**Files:**
- Modify: `velox-sfc/src/template_codegen.rs:1047-1060` (the `cond`/`out.push_str("__children.push({})")` section inside `emit_children_with_state`).

**Step 1:** Replace the current:
```rust
let mut cond = String::new();
cond.push_str(&format!(r#"(if ({}) {{ {} }}"#, expr_if.trim(), inner_if));
for part in chain_parts.iter() { cond.push(' '); cond.push_str(part); }
if let Some(e) = else_part { cond.push(' '); cond.push_str(&e); }
else { cond.push_str(r#" else { text("") }"#); }
cond.push(')');
out.push_str(&format!("__children.push({});\n", cond));
```
with:
```rust
let mut cond = String::new();
cond.push_str(&format!(r#"{{ if ({}) {{ {} }}"#, expr_if.trim(), inner_if));
for part in chain_parts.iter() { cond.push(' '); cond.push_str(part); }
if let Some(e) = else_part { cond.push(' '); cond.push_str(&e); }
else { cond.push_str(r#" else { text("") }"#); }
cond.push_str(" }");
out.push_str(&format!("__children.push({});\n", cond));
```
This makes the pushed value a block expression `{ if ... {} else {} }` (valid `VNode`), eliminating the redundant outer `( )` that the current code wraps around the `if`.

**Step 2:** Run `cargo test -p velox-sfc v_if_else_emits_block_push`.
Expected: PASS.

**Step 3:** Run the full sfc unit tests: `cargo test -p velox-sfc`.
Expected: all pass (no regression in other v-if/v-for codegen paths — `emit_children_with`, `emit_children`, `emit_node_with_ctx_state` are separate functions; verify they don't share the same bug by grepping for `__children.push((if ` across the file; if found, apply the same block fix there).

**Step 4:** Commit:
```bash
git add velox-sfc/src/template_codegen.rs velox-sfc/src/codegen_unit_tests.rs
git commit -m "fix(sfc): emit v-if/v-else as block push to preserve child order"
```

---

## Part B — Fix stylesheet application (brown background)

### Task B1: Add failing test that `Stylesheet::parse` matches `.app { background: #1a1a2e }`

**Objective:** Prove (or disprove) that the parser produces a rule for the `.app` selector with a `background` declaration, and that `apply_styles_with_hover` attaches it.

**Files:**
- Modify: `velox-style/tests/style_parse_tests.rs` (append test).

**Step 1:** Write test:
```rust
#[test]
fn class_rule_applies_background() {
    use velox_dom::{h, Props};
    let css = ".app { background: #1a1a2e; color: #e6edf3; }";
    let ss = velox_style::Stylesheet::parse(css);
    assert_eq!(ss.rules.len(), 1, "expected one rule");
    let node = h("div", Props::new().set("class", "app"), vec![]);
    let styled = velox_style::apply_styles(&node, &ss);
    if let velox_dom::VNode::Element { props, .. } = &styled {
        let s = props.attrs.get("style").expect("style attr applied");
        assert!(s.contains("background") && s.contains("#1a1a2e"),
            "background not applied: {}", s);
    } else { panic!("not element"); }
}
```

**Step 2:** Run `cargo test -p velox-style class_rule_applies_background`.
Expected: This tells us the truth. Two outcomes:
- If **PASS**: the parser works; the brown bg is caused elsewhere (skip to Task B3 — check `apply_styles_with_hover` is actually called with the parsed `STYLE` in `run_window_vnode_skia`, and that `make_view` returns the sheet). In that case root-cause is the `source_index`/layout skip, not styles.
- If **FAIL**: parser/apply bug; continue to B2.

### Task B2: Fix stylesheet parsing so `.app { background: X }` yields a `background` declaration

**Objective:** Ensure `background:` shorthand and class selectors are captured into `Rule.decls` with key `background` (the renderer's `parse_style_attr` only reads `background`/`background-color`).

**Files:**
- Modify: `velox-style/src/lib.rs` — `parse_prelude` (lines 58-67), `parse_selector_list` (127-170), `DeclarationParser::parse_value` (108-118), and `Rule` decl keying (line 80 `decls.insert(name, value)`).

**Step 1:** Inspect the failing assertion output from B1. Likely culprits:
- The selector prelude parser reads the **whole** selector with `to_css` but may include trailing whitespace/whitespace tokens that break `parse_selector_list`'s `.app` detection. If `rules.len()==0`, the block never parsed → check `DeclarationListParser` usage and the `flatten()` error handling.
- If `rules.len()==1` but the `style` attr is empty after apply: the `background` key isn't in `decls` → ensure `DeclarationParser::parse_value` returns `("background", "#1a1a2e")` (it lowercases? no — it returns `name` as-is from cssparser which is **lowercased by cssparser** already, good). Verify `apply_rec` matches `.app` via `matches_selector` (Class kind + class attr split_whitespace) — that path is correct, so the bug is upstream in parse.

**Step 2:** Apply minimal fix indicated by the test output (do NOT rewrite the whole parser). Common fix: in `parse_prelude`, trim and strip internal whitespace between `.` and class; or in `parse_selector_list`, also handle leading/trailing spaces and the `background` shorthand is already a fine key.

**Step 3:** Run `cargo test -p velox-style class_rule_applies_background`.
Expected: PASS.

**Step 4:** Commit:
```bash
git add velox-style/src/lib.rs velox-style/tests/style_parse_tests.rs
git commit -m "fix(style): parse class rules and background shorthand into declarations"
```

### Task B3: Confirm `apply_styles_with_hover` runs on every frame with the real sheet

**Objective:** Guarantee the render path feeds the parsed `STYLE` into the style applier (it already does in `run_window_vnode_skia` lines 691, 763, 812, 855 — `apply_styles_with_hover(&vnode_tagged, &sheet, ...)`), and that `sheet` is `Stylesheet::parse(app::STYLE)` in the generated `main.rs` (it is, per `MakeView`). No code change expected; this is a verification gate.

**Files:**
- Read: `tmp/test_inited_app/src/main.rs` (already shown in session: `let sheet = Stylesheet::parse(app::STYLE);` ✓), `velox-renderer/src/lib.rs:691` (✓).

**Step 1:** Run:
```bash
cargo test -p velox-style
cargo test -p velox-renderer app_render
```
Expected: PASS (style tests) and the `app_render_test.rs` (which uses the same `App.vx`-style CSS) passes, confirming the renderer reads `background`.

**Step 2:** Rebuild the dev app and visually confirm the navy `#1a1a2e` background now fills the window and the title is centered/padded. If still brown → the bug is in `presenter.rs` color conversion (ABGR), not styles — see Task B4.

### Task B4 (only if B3 still brown): Verify Skia→softbuffer color conversion

**Objective:** Ensure RGBA from Skia maps correctly to softbuffer's ABGR little-endian format.

**Files:**
- Read: `velox-renderer/src/presenter.rs:104-113` (already seen: `*pixel = (a<<24)|(b<<16)|(g<<8)|r;` — correct for ABGR). Also `skia_surface.rs` uses `raster_n32_premul` (premultiplied). **Premultiplied alpha** means for opaque pixels (a=255) RGB is unchanged, so `#1a1a2e` → r=0x1a,g=0x1a,b=0x2e. The conversion is correct. If a stray alpha<255 existed, premultiply would darken — but our colors are opaque. So B4 is likely a no-op; treat as verification only.

**Step 1:** If still brown after B2/B3, add a tiny test that encodes `#1a1a2e` through `parse_style_attr` + `parse_color_hex` and asserts the ARGB value, and a manual `save_png` of the `app.rs` tree to inspect pixels.

---

## Part C — Verify full UI renders (counter, buttons, events)

### Task C1: Render the inited app and confirm all elements present

**Objective:** End-to-end: background navy, title "Velox App", counter "0", "not positive" text, three buttons "+1/-1/Reset", and clicking changes counter.

**Files:**
- Run: `cd /tmp/test_inited_app && velox dev` (rebuilds on start). Screenshot the window.

**Step 1:** Verify each element is visible and styled (navy bg, card `#16213e`, blue buttons `#3478f6`).

**Step 2:** Click "+1" → counter shows "1", text switches to "positive" (green). Confirms (a) buttons render, (b) click events route via `make_on_event` + `on:click` attr, (c) `v-if`/`v-else` now toggles.

**Step 3:** If any element still missing, re-grep the generated `app.rs` for `source_index` alignment: the layout's `source_index` (set in `velox-dom/src/layout.rs:1647` for text, `1814` for elements) must index into the **decorated** children vec used by `render_frame`. Render uses `children.get(src_idx)` (skia_render.rs:987-988) — confirm `src_idx` always < `children.len()`. The only way it desyncs is if apply_styles reorders/duplicates children; it does not (it maps 1:1). So after A2 the alignment holds.

**Step 4:** Commit nothing new (verification). Move on.

### Task C2: Render the TodoItem component in the template

**Objective:** Confirm child components (`<TodoItem>`) resolve and render (the inited `App.vx` currently does NOT import TodoItem — but `init` ships `src/components/TodoItem.vx` unused). Wire it so the starter app exercises composition.

**Files:**
- Modify: `velox-cli/templates/project/src/App.vx` (and `tmp/test_inited_app/src/App.vx` for live test) — add a `<TodoItem :todo="'Learn Velox'" :completed="false" />` usage and the component import/resolution.

**Step 1:** Add usage inside `.card`:
```html
<TodoItem :todo="'Buy milk'" :completed="false" @toggle="noop" @remove="noop" />
```
and ensure the component resolver (`velox-sfc/src/component_resolver.rs`) maps `TodoItem` → `src/components/TodoItem.vx`. If resolution isn't automatic, add the import in generated `main.rs`/build.rs include.

**Step 2:** Rebuild, screenshot, confirm a todo row renders.

**Step 3:** Commit:
```bash
git add velox-cli/templates/project/src/App.vx
git commit -m "feat(templates): wire TodoItem component into starter app"
```

---

## Part D — Rewrite the Dev Server (Vite-like)

### Task D1: Add a colored, structured startup banner + config summary

**Objective:** Professional first impression: gradient/colored banner, project name, local URL placeholder, wagon of watched paths.

**Files:**
- Modify: `velox-cli/src/commands/dev.rs:9-18` (`dev_current`).

**Step 1:** Replace the plain `println!` block with a styled banner using ANSI codes (no extra deps; `ansi_term`/`colored` are NOT in Cargo.toml — use raw `\x1b[...m` or add `colored` to `velox-cli/Cargo.toml` if preferred). Keep it dependency-light:
```rust
fn ansi(code: &str, s: &str) -> String { format!("\x1b[{}m{}\x1b[0m", code, s) }
// banner: bold cyan title, dim watching path, green "ready" after first build
```

**Step 2:** Print after `spawn_app` succeeds:
```
  ⚡ Velox dev server
  ➤ Project: test_inited_app
  ➤ Watching: src (and components)
  ➤ Build: debug
```
Expected: visible in terminal on `velox dev`.

**Step 3:** Commit:
```bash
git add velox-cli/src/commands/dev.rs
git commit -m "feat(dev): professional colored startup banner"
```

### Task D2: Capture build output, parse errors, and show them inline (don't bury in cargo noise)

**Objective:** Instead of `Stdio::inherit` dumping raw `cargo` output, capture stdout/stderr, detect compile errors, and print a clean red "✗ Build failed" panel with the error, plus a green "✓ Compiled in 1.2s" on success.

**Files:**
- Modify: `velox-cli/src/commands/dev.rs` — `spawn_app` (43-57) and `dev_current` loop (20-41).

**Step 1:** Change `spawn_app` to capture output:
```rust
use std::process::{Command, Stdio};
use std::io::Read;
fn spawn_app(watch_dir: &Path) -> Result<Option<Child>> {
    let mut cmd = Command::new("cargo");
    cmd.arg("run").current_dir(watch_dir)
       .stdin(Stdio::null())
       .stdout(Stdio::piped())
       .stderr(Stdio::piped());
    // spawn; on exit, read stdout+stderr, scan for "error[" / "error:" to classify
}
```
Implement a helper `classify_build(child) -> BuildStatus { Compiling, Success(duration), Failed(errors) }` that reads both pipes (use `child.wait_with_output()` if you don't need live kill — but we DO need to kill on change, so spawn + poll + read on exit). Simpler: spawn with piped stderr, in the loop use `try_wait()`; when it exits, read captured stderr.

**Step 2:** On change: print `ansi("33","⟳ Rebuilding...")`, kill old child, spawn, wait for exit, classify, print result.

**Step 3:** On success print `✓ Compiled in {secs}s`. On failure print a red box with the `error[E...]` lines extracted (keep last ~20 lines).

**Step 4:** Run `velox dev`, then introduce a deliberate syntax error in `App.vx`, save, confirm the red error panel shows instead of a crash. Fix it, confirm green success.

**Step 5:** Commit:
```bash
git add velox-cli/src/commands/dev.rs
git commit -m "feat(dev): capture build output and show clean success/error panels"
```

### Task D3: Interactive controls (reload / clear / quit) + on-window error overlay

**Objective:** Vite-like UX: `r` reload, `c` clear, `q` quit, and when the app binary exits with an error, show the error in-terminal AND keep the server alive (don't kill the whole `velox dev`). Also support the in-app `R`/`Q` keys already wired in the renderer (they exit the binary; the dev loop restarts it — keep that).

**Files:**
- Modify: `velox-cli/src/commands/dev.rs` loop.

**Step 1:** Add a blocking stdin reader thread (or use `termion`/`crossterm`? NOT in deps — use a simple `std::io::stdin` line reader in a spawned thread that sends signals via an `Arc<Mutex<DevCommand>>`). Keep it minimal:
```rust
enum DevCmd { Reload, Quit, Clear }
// thread reads lines: "r"/"reload" -> Reload, "q" -> Quit, "c" -> Clear
```
On `Reload`: kill + respawn. On `Quit`: kill + `break`. On `Clear`: clear screen (`\x1b[2J\x1b[1;1H`) and reprint banner.

**Step 2:** When app child exits non-zero, print `✗ App crashed` + last stderr, but **do not exit** `velox dev` — keep watching for the next save to rebuild. This mirrors Vite's resilience.

**Step 3:** Print a footer hint: `  r: reload   c: clear   q: quit`.

**Step 4:** Run, press `r`/`c`/`q`, confirm behavior.

**Step 5:** Commit:
```bash
git add velox-cli/src/commands/dev.rs
git commit -m "feat(dev): interactive r/c/q controls and crash-resilient loop"
```

### Task D4: Hot-reload polish — only rebuild when `.vx`/`.rs` change, debounce, and show file + time

**Objective:** Vite rebuilds on save, not on `target/` churn. Current `has_change` already ignores `target`/`.git` (dev.rs:7,65). Add: debounce (wait 200ms stable), and print `↻ src/App.vx changed — rebuilding` with the relative path.

**Files:**
- Modify: `velox-cli/src/commands/dev.rs` `has_change` (59-89) + loop.

**Step 1:** Track the changed file path (modify `walk` to return `Option<PathBuf>` of the first changed file) and print it.

**Step 2:** Add debounce: after detecting change, sleep 200ms and re-check `modified()` to coalesce rapid saves.

**Step 3:** Run, edit `App.vx`, save → see `↻ src/App.vx changed — rebuilding (12ms)`.

**Step 4:** Commit:
```bash
git add velox-cli/src/commands/dev.rs
git commit -m "feat(dev): debounced rebuild with changed-file reporting"
```

### Task D5: Build/run command parity + `--open`/port niceties (optional, time-permitting)

**Objective:** Make `velox build --release` and `velox run` print the same professional banners, and add a `--release` flag to `dev` (`velox dev --release`) reusing `spawn_app` with `cargo run --release`.

**Files:**
- Modify: `velox-cli/src/bin/main.rs` (add `--release` to `Dev` subcommand, already has `Run { release }`), `velox-cli/src/commands/dev.rs` (`spawn_app` takes a `release: bool`).

**Step 1:** Thread `release` through `dev_current` → `spawn_app`.

**Step 2:** Run `velox dev --release` once to confirm it compiles in release and serves.

**Step 3:** Commit:
```bash
git add velox-cli/src/bin/main.rs velox-cli/src/commands/dev.rs
git commit -m "feat(dev): --release support and consistent banners"
```

---

## Part E — Final verification

### Task E1: Full workspace test + clean render

**Objective:** Everything green and the app looks right.

**Files:**
- Run: `cargo test -p velox-sfc -p velox-style -p velox-dom -p velox-renderer -p velox-cli` (or `cargo test --workspace`).
Expected: all pass.

**Step 2:** `cd /tmp/test_inited_app && velox dev` → screenshot: navy bg, card, counter, 3 buttons, centered title. Click +1 → counter increments, text turns green.

**Step 3:** `velox build --release && velox run --release` → same correct render.

**Step 4:** Confirm dev server shows: banner, `↻ file changed`, `✓ Compiled in Xs`, interactive `r/c/q`.

**Step 5:** Commit final:
```bash
git add -A
git commit -m "feat: fix rendering pipeline and ship Vite-like dev server"
```

---

## Risks / Tradeoffs / Open Questions

- **Root-cause uncertainty on "brown":** Task B1 is a *discovery* gate. If styles already parse, the brown is from layout `source_index` skip or the `v-if` push dropping siblings — and B2 becomes a no-op while A2 + C3 carry the fix. Follow the test, don't assume.
- **Premultiplied alpha:** Skia `raster_n32_premul` + softbuffer ABGR is correct for opaque colors; if transparent elements appear wrong later, revisit `presenter.rs` with an unpremultiply step — out of scope now.
- **No TTY coloring lib:** Plan uses raw ANSI to avoid adding `colored`/`crossterm` deps. If you'd rather have a robust TTY lib, add `crossterm` to `velox-cli/Cargo.toml` (note: increases build time; the session already fought a 3m compile).
- **Stdin control thread:** Reading stdin while the app child also reads stdin — we set child `stdin(Stdio::null())`, so the dev loop owns the terminal stdin. Good.
- **`v-if` block push:** Emitting `{ if ... {} else {} }` as the pushed value is valid Rust (block expr yields last expr = `VNode`). Verified by the unit test in A1.
- **Child components:** Starter `App.vx` doesn't currently use `TodoItem`; Task C2 wires it. If the resolver needs the component registered in `build.rs`, do that in `velox-cli/templates/project/build.rs`.

## Files likely to change

- `velox-sfc/src/template_codegen.rs` (A2) — v-if/v-else block push
- `velox-sfc/src/codegen_unit_tests.rs` (A1) — new test
- `velox-style/src/lib.rs` (B2) — parser fix IF B1 fails
- `velox-style/tests/style_parse_tests.rs` (B1) — new test
- `velox-cli/templates/project/src/App.vx` (C2) — component usage
- `velox-cli/src/commands/dev.rs` (D1–D5) — full dev server rewrite
- `velox-cli/src/bin/main.rs` (D5) — `--release` flag
