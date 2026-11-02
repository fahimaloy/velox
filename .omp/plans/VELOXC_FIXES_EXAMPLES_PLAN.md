# Velox CLI Fixes & Example Overhaul Plan

## Context

The Velox project has several issues that need addressing:

1. **CLI name mismatch**: The binary is named `veloxc` (crates.io package), but `main.rs` sets `#[command(name = "velox")]`, causing help text and docs to reference `velox` instead of `veloxc`.
2. **Dependency resolution broken for crates.io installs**: `velox_git_rev()` falls back to `"unknown"` when installed from crates.io, generating `rev = "unknown"` in scaffolded `Cargo.toml` which Cargo rejects.
3. **Documentation inconsistencies**: Many `.md` files reference `velox` commands instead of `veloxc`.
4. **Outdated examples**: Three examples exist (counter, todo, showcase) but need replacement with 5 well-designed, diverse examples.
5. **README.md uses wrong hero image**: Shows a showcase screenshot instead of the Velox logo.

---

## Approach

### Phase 1: Core CLI Fixes

#### 1.1 Fix CLI binary name in `veloxc/src/bin/main.rs`
- **File**: `veloxc/src/bin/main.rs:6`
- **Change**: `#[command(name = "velox")]` → `#[command(name = "veloxc")]`
- **Effect**: Help text, `veloxc --help`, and all subcommand help will show `veloxc` consistently.

#### 1.2 Fix `velox_git_rev()` fallback for crates.io installs
- **File**: `veloxc/src/lib.rs:112-115`
- **Problem**: `option_env!("VELOX_GIT_REV").unwrap_or("unknown")` — crates.io builds don't have this env var.
- **Solution**: Detect crates.io install at build time via `CARGO_PKG_VERSION` presence without git, or embed the git rev at publish time using `vergen` or similar. Simpler: when `VELOX_GIT_REV` is not set, generate **version** dependencies (crates.io) instead of git dependencies.
- **Implementation**: Modify `generate_cargo_toml()` in `init.rs` to detect "unknown" rev and emit version-based deps:
  ```rust
  let rev = crate::velox_git_rev();
  if rev == "unknown" {
      // Emit version = "X.Y.Z" deps (all 6 crates published)
  } else {
      // Emit git = "...", rev = "..." deps
  }
  ```
- **Affected**: `veloxc/src/commands/init.rs:405-410` (git fallback branch).

#### 1.3 Update template README.md commands
- **File**: `veloxc/templates/project/README.md:19,22`
- **Change**: `velox dev` → `veloxc dev`, `velox build` → `veloxc build`

#### 1.4 Update all documentation files
- **Files to audit/update**:
  - `README.md` (root) — Quick start, CLI reference, examples table
  - `docs/src/getting-started.md` — Install, create project, run dev
  - `docs/src/features/cli.md` — All command references
  - `docs/src/tutorials/counter.md` — Step 1, Step 5, Step 6
  - `docs/src/tutorials/todo.md` — Check for `velox` references
  - `docs/src/tutorials/showcase.md` — Check for `velox` references
  - `docs/src/features/hmr-dev-workflow.md` — Check for `velox` references
  - `veloxc/README.md` — CLI-specific docs
- **Action**: Global replace `velox ` (with space) → `veloxc ` where referring to CLI commands; keep `Velox` for framework name.

---

### Phase 2: Remove Existing Examples

#### 2.1 Delete example directories
- `examples/counter/`
- `examples/todo/`
- `examples/showcase/`

#### 2.2 Remove example references from workspace
- **File**: `Cargo.toml` (root) — Check for `examples/*` in workspace members; remove if present.

#### 2.3 Remove example references from documentation
- `docs/src/SUMMARY.md` — Remove tutorial links for counter, todo, showcase
- `docs/src/tutorials/` — Delete `counter.md`, `todo.md`, `showcase.md`
- `docs/src/assets/` — Delete `counter-hmr.gif`, `showcase-window.png`, `todo-window.png`, `velox-dev-window.png`
- `README.md` — Remove examples table and run commands

---

### Phase 3: Create 5 New Example Projects

The 5 examples will be:

| # | Name | Purpose | Key Features |
|---|------|---------|--------------|
| 1 | **veloxc-init-default** | Default `veloxc init` scaffold (todo app) | Baseline; demonstrates components, theme, dialogs, v-for, signals |
| 2 | **dashboard** | Analytics dashboard | Grid layout, charts (ASCII/rect-based), sidebar nav, real-time updates, dark mode |
| 3 | **markdown-editor** | Split-pane Markdown editor | Textarea + live preview, toolbar, file open/save, syntax highlighting (CSS), resizable panes |
| 4 | **task-board** | Kanban board | Drag-and-drop columns, localStorage persistence, modal forms, optimistic UI |
| 5 | **settings-panel** | System settings UI | Forms, toggles, select, tabs, validation, accordion sections, toast notifications |

Each example will have:
- Own directory under `examples/<name>/`
- `Cargo.toml` with path deps to workspace crates
- `build.rs` compiling `src/App.vx`
- `src/main.rs` (standard boilerplate)
- `src/App.vx` + components in `src/components/`
- `assets/` for any images/icons
- `README.md` with description, architecture, run instructions
- `docs/src/tutorials/<name>.md` tutorial
- Screenshot at `docs/src/assets/<name>-window.png` (after Phase 4)

#### 3.1 Create Example 1: veloxc-init-default (the default scaffold)
- **Location**: `examples/veloxc-init-default/` (copy of what `veloxc init` produces)
- **Purpose**: Reference implementation of the scaffolded app
- **Components**: App.vx, Todos.vx, TodoInput.vx, TodoItem.vx, Modal.vx, Confirm.vx

#### 3.2 Create Example 2: dashboard
- **Structure**:
  ```
  examples/dashboard/
  ├── Cargo.toml
  ├── build.rs
  ├── src/
  │   ├── main.rs
  │   ├── App.vx          # Layout: sidebar + header + grid
  │   ├── components/
  │   │   ├── Sidebar.vx
  │   │   ├── Header.vx
  │   │   ├── StatCard.vx
  │   │   ├── Chart.vx    # Simple bar/line using rects
  │   │   ├── Table.vx
  │   │   └── ThemeToggle.vx
  │   └── state.rs        # Shared signals for "real-time" data
  └── assets/
  ```
- **Design**: Dark theme default, sidebar navigation, responsive grid (CSS flex/grid), animated stat cards, mock data updates via `setInterval`-style timer.

#### 3.3 Create Example 3: markdown-editor
- **Structure**:
  ```
  examples/markdown-editor/
  ├── Cargo.toml
  ├── build.rs
  ├── src/
  │   ├── main.rs
  │   ├── App.vx          # Split view: editor | preview
  │   ├── components/
  │   │   ├── EditorPane.vx
  │   │   ├── PreviewPane.vx
  │   │   ├── Toolbar.vx
  │   │   ├── ResizeHandle.vx
  │   │   └── FileDialog.vx
  │   └── markdown.rs     # pulldown-cmark integration for rendering
  └── assets/
  ```
- **Design**: Light/dark theme, resizable split pane (CSS flex + drag handle), toolbar with formatting buttons, file open/save via native dialog (if available) or simulated.

#### 3.4 Create Example 4: task-board
- **Structure**:
  ```
  examples/task-board/
  ├── Cargo.toml
  ├── build.rs
  ├── src/
  │   ├── main.rs
  │   ├── App.vx          # Board layout: columns + add-card modals
  │   ├── components/
  │   │   ├── Column.vx
  │   │   ├── Card.vx
  │   │   ├── CardModal.vx
  │   │   ├── ColumnHeader.vx
  │   │   └── Toast.vx
  │   └── storage.rs      # localStorage via web-sys or file fallback
  └── assets/
  ```
- **Design**: Multi-column Kanban, drag-to-reorder (via mouse events + state), cards with title/description/labels, persist to JSON file, toast notifications for actions.

#### 3.5 Create Example 5: settings-panel
- **Structure**:
  ```
  examples/settings-panel/
  ├── Cargo.toml
  ├── build.rs
  ├── src/
  │   ├── main.rs
  │   ├── App.vx          # Sidebar tabs + content panels
  │   ├── components/
  │   │   ├── Sidebar.vx
  │   │   ├── GeneralTab.vx
  │   │   ├── AppearanceTab.vx
  │   │   ├── PrivacyTab.vx
  │   │   ├── ShortcutsTab.vx
  │   │   ├── FormField.vx
  │   │   ├── Toggle.vx
  │   │   ├── Select.vx
  │   │   ├── Accordion.vx
  │   │   └── Toast.vx
  │   └── settings.rs     # Settings state + persistence
  └── assets/
  ```
- **Design**: macOS/System Settings inspired, accordion sections, form validation, keyboard shortcuts display, toast confirmations.

#### 3.6 Add tutorial documentation for each
- **Files**: `docs/src/tutorials/veloxc-init-default.md`, `dashboard.md`, `markdown-editor.md`, `task-board.md`, `settings-panel.md`
- **Pattern**: Follow `counter.md` structure — prerequisites, step-by-step, explanation, exercises.
- **Update**: `docs/src/SUMMARY.md` to list all 5 tutorials.

---

### Phase 4: Build, Run & Capture Screenshots

#### 4.1 Build each example
```bash
cd examples/<name> && cargo build --release
```
- Run from repository root with workspace context for path deps.

#### 4.2 Run each example and capture maximized window screenshot
- **Tool**: Use `computer` eval to launch, maximize, screenshot.
- **Sequence per example**:
  1. `cargo run --release` (or `veloxc run` from example dir)
  2. Wait for window to appear
  3. Maximize window
  4. Wait for render (500ms)
  5. Screenshot → save to `examples/<name>/screenshot.png` and `docs/src/assets/<name>-window.png`

#### 4.3 Screenshot specs
- Format: PNG
- Naming: `<name>-window.png` in `docs/src/assets/`, `screenshot.png` in example dir
- Window title should match example name

---

### Phase 5: Update Root README.md & Documentation Site

#### 5.1 Update root README.md
- **Hero section** (lines 1-12): Replace showcase screenshot with `velox-logo.svg` (already at root)
- **Examples section** (lines 501-519): Replace with 5-example table + screenshots
  ```markdown
  ## Examples

  Five example apps ship in `examples/`:

  | App | What it demonstrates | Screenshot |
  |-----|----------------------|------------|
  | veloxc-init-default | Default scaffold: components, theme, dialogs, v-for | ![veloxc-init-default](docs/src/assets/veloxc-init-default-window.png) |
  | dashboard | Analytics dashboard: grid, charts, sidebar, real-time | ![dashboard](docs/src/assets/dashboard-window.png) |
  | markdown-editor | Split-pane editor: live preview, toolbar, resize | ![markdown-editor](docs/src/assets/markdown-editor-window.png) |
  | task-board | Kanban board: drag-drop, persistence, modals | ![task-board](docs/src/assets/task-board-window.png) |
  | settings-panel | Settings UI: forms, tabs, accordion, toasts | ![settings-panel](docs/src/assets/settings-panel-window.png) |
  ```
- **Quick start**: Ensure all `velox` → `veloxc` command references
- **CLI reference**: Already uses `veloxc` in tables but check prose

#### 5.2 Update docs website (mdBook)
- **File**: `docs/src/SUMMARY.md` — Add 5 tutorials under "Tutorials"
- **Assets**: Ensure all 5 screenshots in `docs/src/assets/`
- **Deploy**: `.github/workflows/deploy-docs.yml` should pick up changes automatically

#### 5.3 Update veloxc/README.md
- Sync with root README.md CLI reference
- Ensure all commands use `veloxc`

---

### Phase 6: Verification

#### 6.1 Run full test suite
```bash
cargo test --workspace
```
- All existing tests must pass
- New examples should compile (add as workspace members or test separately)

#### 6.2 Verify each example builds and runs
```bash
for ex in veloxc-init-default dashboard markdown-editor task-board settings-panel; do
  cd examples/$ex && cargo build --release && cargo run --release &
  # verify window opens, screenshot captured
  cd ../..
done
```

#### 6.3 Verify documentation builds
```bash
cd docs && mdbook build
```
- Check output in `docs/book/` for broken links, missing images

---

## Critical Files & Anchors

| File | Symbol/Region | Reason |
|------|---------------|--------|
| `veloxc/src/bin/main.rs:6` | `#[command(name = "velox")]` | CLI name fix |
| `veloxc/src/lib.rs:112-115` | `velox_git_rev()` | Git rev fallback |
| `veloxc/src/commands/init.rs:348-412` | `generate_cargo_toml()` | Dependency generation logic |
| `veloxc/templates/project/README.md:19,22` | Command examples | Template doc fix |
| `README.md:1-12,501-519` | Hero + Examples section | Root README overhaul |
| `docs/src/SUMMARY.md` | Tutorial list | Doc nav update |
| `examples/` | Directory | Example cleanup/creation |

---

## Verification Checklist

- [ ] `veloxc --help` shows `veloxc` as command name (not `velox`)
- [ ] `veloxc init test-app` from clean directory generates `Cargo.toml` with valid deps (version-based, no `rev = "unknown"`)
- [ ] `cd test-app && cargo build` succeeds
- [ ] All 5 examples build: `cargo build --release` in each
- [ ] All 5 examples run and show maximized window
- [ ] 5 screenshots captured and saved to correct locations
- [ ] Root README.md renders correctly with logo + 5 screenshots
- [ ] `cd docs && mdbook build` succeeds with all tutorials linked
- [ ] `cargo test --workspace` passes

---

## Assumptions & Contingencies

| Assumption | If False → Fallback |
|------------|---------------------|
| All 6 Velox crates published on crates.io at same version | Use git deps with known-good rev; document `--local` workaround |
| `computer` eval can launch GUI apps and screenshot on this Linux env | Use `xvfb-run` + headless Skia if display unavailable; document limitation |
| `mdbook` installed for doc build | Install via `cargo install mdbook` in verification step |
| Drag-and-drop in task-board works with current Velox event system | Simplify to click-to-move if drag events not fully supported |
| `pulldown-cmark` available for markdown-editor | Use simple string replacement for demo; note limitation in tutorial |

---

## Non-Goals (Explicitly Out of Scope)

- Adding new Velox framework features (reconciler, transitions, etc.)
- Changing the `.vx` template syntax
- Modifying the renderer or layout engine
- CI/CD pipeline changes beyond what's needed for doc deploy
- VS Code extension updates