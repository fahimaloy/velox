# CLI Reference

`veloxc` is the command-line tool that scaffolds, builds, lints, and runs Velox applications — install it with `cargo install veloxc`. Every command on this page is verified against the CLI source; `veloxc --help` lists them, and `veloxc <subcommand> --help` is the authoritative flag reference.

## Command summary

| Command | Purpose |
|:---|:---|
| `velox init <name>` | Create a new Velox project with scaffolding |
| `velox build [input]` | Build the current project, or compile a `.vx` component to Rust |
| `velox run` | Build and run the current project |
| `velox dev` | Start the dev server with file watching and hot reload |
| `velox lint [target]` | Check `.vx` files for issues |
| `velox add component <name>` | Generate a new component `.vx` file |
| `velox version` | Show version and system info |

---

## `velox init <name>`

Creates a complete, working project — not a stub. The scaffold is a todo list app with components, assets, and a build script, so everything is on screen from the first `velox dev`.

| Flag | Short | Default | Meaning |
|:---|:---|:---|:---|
| `--template <name>` | `-t` | `default` | Project template. The flag is accepted; today every template value produces the same scaffold. |
| `--local <path>` | *(none)* | *(none)* | Use the Velox workspace at `<path>` for **path** dependencies instead of git. |

The scaffold writes:

```text
<name>/
├── Cargo.toml          # velox-core/dom/style, velox-renderer (skia-native), veloxc build-dep
├── build.rs            # compiles src/App.vx + imports via veloxc::build_cmd
├── README.md
├── assets/
│   ├── velox-logo.svg
│   └── velox-logo.png
└── src/
    ├── main.rs
    ├── App.vx
    └── components/
        ├── Todos.vx  TodoInput.vx  TodoItem.vx
        └── Confirm.vx  Modal.vx
```

Dependency resolution, in order: `--local <path>` wins; otherwise the CLI walks up from the current directory looking for a Velox workspace (a `velox-core/Cargo.toml`), or uses the checkout the CLI itself was built in — either way the scaffold gets relative **path** dependencies. When no workspace is found it falls back to **git** dependencies pinned to the commit the CLI was built at, so a scaffold always compiles against tested code.

```bash
velox init counter
```

```text
✅ Created Velox project: counter
📦 To get started:
   cd counter
   velox dev
   velox build
   velox run
✅ Created Velox project at: counter

📖 Next steps:
   cd counter
   velox dev
```

> Note: the scaffold's printed next steps say `velox dev`, but the binary on your `PATH` is `veloxc` — type `veloxc dev` unless you have aliased it.

> Warning: the git-pinned fallback assumes the CLI knows its own commit. A **crates.io-installed** `veloxc` does not — it falls back to `"unknown"` and writes `rev = "unknown"`, which Cargo rejects with ``revspec 'unknown' not found``. Scaffold from a clone (`cargo install --path veloxc`), pass `--local <path>`, or drop the `git = …` / `rev = …` keys from every `velox-*` line in the generated `Cargo.toml`.

---

## `velox build [input]`

Two modes, chosen by whether a positional `input` is given:

| Mode | Invocation | Behavior |
|:---|:---|:---|
| Project build | `velox build` | `cargo build` for the current project |
| Component compile | `velox build src/App.vx` | Compile the `.vx` file **and all imported components** to Rust source |

| Flag | Short | Meaning |
|:---|:---|:---|
| `--out-dir <dir>` | `-o` | Output directory when compiling a `.vx` file (default `target/velox-gen`) |
| `--release` | *(none)* | Release mode when building the current project |

The component compile is recursive: every component imported from `<script setup>` is parsed and compiled, one `.rs` file per component in the output directory. The root component is wrapped in a `pub mod` block (so `include!` in `main.rs` works), imported components are declared with `#[path]` attributes, and child-component styles are merged into the root stylesheet — scoped ones stay scoped. When run from a build script, `cargo:rerun-if-changed` directives are emitted for every `.vx` file read, so Cargo re-runs the build script when a component changes.

```bash
velox build src/App.vx -o target/velox-gen
```

```text
[velox] Generated: target/velox-gen/app.rs
ℹ️  The generated .rs file is source code, not an executable.
```

---

## `velox run`

Builds first when `--release` is passed, then runs the project. The app inherits your terminal.

| Flag | Meaning |
|:---|:---|
| `--release` | Build in release mode before running |

```bash
velox run            # debug build (existing binary is reused by cargo)
velox run --release  # builds in release mode first, then runs
```

---

## `velox dev`

Starts the dev server: file watching, rebuild on save, and the HMR channel that tells the running app to restart itself.

| Flag | Short | Default | Meaning |
|:---|:---|:---|:---|
| `--watch <dir>` | `-w` | `.` | Watch directory (project root; builds with `cargo run`) |
| `--release` | *(none)* | *(none)* | Release mode build |

There is no positional directory argument — the watch root is passed with `--watch`/`-w`. The server watches `src/` and `assets/` (roots that do not exist are skipped), binds `127.0.0.1:31313` for hot reload, and serves keyboard commands on stdin.

```bash
velox dev
```

The full workflow — what updates live, what restarts, the banner, the shortcuts, and inotify troubleshooting — is on [Dev Workflow & HMR](hmr-dev-workflow.md).

---

## `velox lint [target] [--fix]`

Checks `.vx` files for issues. Three kinds of findings:

| Finding | Example | Severity |
|:---|:---|:---|
| Parse errors | Unclosed `{{`, multiple root elements | **Error** — fails the command |
| Script-idiom warnings | `Cell`/`RefCell` usage where `Ref` is expected | Warning — advisory only |
| Parsed-but-unrendered CSS | `visibility: hidden`, `box-shadow`, `transition` | Warning — with the reason |

The target defaults to `src` and may be a single `.vx` file or a directory (walked recursively, `.vx` files only). CSS warnings are reported by file and rule index; unknown declarations are not flagged — the cascade filters those out silently, so vendor prefixes and custom properties stay quiet.

`--fix` applies the two auto-fixes — trailing whitespace on every line, and exactly one final newline. Parse errors are **not** auto-fixable: non-parseable files are reported as errors and left untouched.

```bash
velox lint src/App.vx
velox lint            # every .vx under src/
velox lint --fix      # …and fix whitespace issues
```

```text
📊 Lint results: 8 files, 0 errors
```

---

## `velox add component <name>`

Generates a new, self-contained component `.vx` file in `src/components/`. The name is normalized: `my-counter`, `side_bar`, and `My Component` all produce a PascalCase struct (`SideBar`), a matching PascalCase filename (`SideBar.vx`), and a kebab-case CSS class (`side-bar`). The name must start with a letter, and the file is never overwritten.

| Flag | Short | Default | Meaning |
|:---|:---|:---|:---|
| `--slots <NAME,NAME>` | `-s` | `header` | Named slots to declare, comma-separated. The implicit `default` slot is always included. |

Slot names are folded to the form the SFC parser registers — `footerBar` and `main_content` are scaffolded as `footer-bar` and `main-content` — and `default` cannot be requested explicitly.

```bash
velox add component side-bar --slots header,footer
```

```text
✅ Created component: src/components/SideBar.vx
   Slots: default, header, footer
   Fill one from a parent: <SideBar><template v-slot:header>…</template></SideBar>
   (shorthand: <SideBar><template #header>…</template></SideBar>)
   Import it from a parent component, e.g.:
   import SideBar from './components/SideBar.vx'
```

---

## `velox version`

```bash
velox version
```

```text
velox 0.1.2
Edition: 2021
Platform: Linux
```

> Note: `Edition: 2021` is a fixed string from the CLI itself, not read from your project's manifest.

---

## Exit codes

| Code | Meaning |
|:---|:---|
| `0` | Success — also the code the app exits with when it receives an HMR `FullReload` |
| `1` | Any command failure: `cargo build`/`run` failure, SFC parse errors, lint findings (`Lint failed with N errors`), missing project, invalid names |
| `2` | Invalid usage (unknown flag, missing subcommand) — standard clap behavior |

A failing `velox lint` exits non-zero, so it drops straight into CI: `velox lint src/` as a step fails the pipeline on the first parse error.

---

## Environment variables

| Variable | Set by | Read by | Meaning |
|:---|:---|:---|:---|
| `VELOX_PATH` | `velox init --local` | `velox init` | Workspace to link as path dependencies |
| `VELOX_HMR` | `velox dev` (spawn) | the running app | `1` enables the HMR client; `0` disables it |
| `VELOX_HMR_PORT` | `velox dev` (spawn) | the running app | Port of the dev server's HMR listener (default `31313`) |
| `VELOX_HEADLESS` | you | the renderer | `1` forces offscreen rendering (no presentation) |

---

## See also

- [Dev Workflow & HMR](hmr-dev-workflow.md) — `velox dev` end to end.
- [Renderer](renderer.md) — the HMR channel and rendering backends the CLI wires up.
- [Template Syntax](template-syntax.md) — what `velox lint` checks and what `velox build` compiles.
