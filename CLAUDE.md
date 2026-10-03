# Velox

Vue SFC-syntax Rust GUI framework with Skia rendering.

## Architecture

```
.vx SFC File
    │
    ▼
velox-sfc (compiler)
    ├── grammar.pest → pest parser
    ├── parse_sfc() → SFC AST
    ├── template_codegen.rs → template → VNode
    └── codegen.rs → SFC → Rust code
    │
    ▼
Generated Rust in OUT_DIR/
    │
    ▼
velox-renderer (Skia + softbuffer)
    ├── lib.rs: run_window_vnode_skia()
    ├── skia_render.rs → RGBA pixels
    └── presenter.rs → SoftbufferPresenter
```

## Crates

| Crate | Purpose |
|-------|---------|
| velox-core | Reactive primitives (ref, signal, lifecycle hooks) |
| velox-sfc | .vx file parsing and code generation |
| velox-dom | Layout computation (compute_layout) |
| velox-style | CSS processing and style application |
| velox-renderer | Skia rendering and window management |
| velox-cli | Build CLI and dev server |

## Key Commands

```bash
cd /home/fahimaloy/Projects/personal/velox

# Build all
cargo build

# Run tests
cargo test

# Run specific crate tests
cargo test -p velox-sfc

# Build example
cargo build -p velox-example-todo

# Run (requires display/compositor)
cargo run -p velox-example-todo
```

## Key Files

- `velox-sfc/src/codegen.rs` - SFC to Rust codegen
- `velox-sfc/src/template_codegen.rs` - Template to VNode codegen
- `velox-renderer/src/lib.rs` - Window + Skia rendering loop
- `velox-renderer/src/presenter.rs` - Softbuffer presenter
- `velox-core/src/signal.rs` - Reactive primitives
- `velox-core/src/lifecycle.rs` - Lifecycle hooks

## .vx Syntax

```vue
<script>
  let count = ref!(0);

  fn on_increment() {
    count.set(*count + 1);
  }
</script>

<template>
  <button @click={on_increment}>
    Count: {count}
  </button>
</template>

<style>
  button {
    padding: 8px 16px;
    background: #4a90d9;
    color: white;
    border-radius: 4px;
  }
</style>
```

## Coding Conventions

- Reactive primitives: `ref!()`, `signal!()`, `define_emits!()`, `on_mounted!()`
- Template interpolation: `{variable}`
- Event handlers: `@click={handler}`
- Attribute binding: `:src={expr}`
- Conditional: `v-if`, `v-else`
- Lists: `v-for`

## Known Issues

- EPIPE crash when no compositor available (headless)
- CSS scoping not fully implemented
- Some CSS properties missing (box-shadow, text-shadow)

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **velox** (3976 symbols, 10263 relationships, 342 execution flows).

> Index stale? Run `node .gitnexus/run.cjs analyze --index-only` from the project root — it auto-selects an available runner. No `.gitnexus/run.cjs` yet? Bootstrap with `npx`, `bunx`, or `pnpm dlx` — e.g. `bunx gitnexus@latest analyze` (npm 11 npx crash; #1939).

## Always Do

- **MUST run impact before editing.** Use `impact({target: "symbolName", direction: "upstream"})` or `node .gitnexus/run.cjs impact "symbolName" --direction upstream --repo .`; report callers, processes, and risk. Never substitute grep for graph analysis.
- **MUST analyze graph changes before committing.** Use `detect_changes({scope: "all"})` (MCP) or `node .gitnexus/run.cjs detect-changes --scope all --repo .` (CLI fallback). `partial: true` or `truncated: true` is not a clean check — a zero means unseen, not unaffected; re-run it. For regression review: `detect_changes({scope: "compare", base_ref: "main"})` or `node .gitnexus/run.cjs detect-changes --scope compare --base-ref "main" --repo .`.
- MUST warn on HIGH/CRITICAL `risk` pre-edit; never use `riskSharedAxes` to waive a HIGH/CRITICAL `risk` warning. Compare File/symbol: MCP File omits axes; Graph-RAG expands File.
- **MUST treat `risk: UNKNOWN` as unresolved, not as low.** An empty caller set is not evidence the symbol is unused — it can also mean the callers are not resolvable by the index (plain-object property access, dynamic dispatch, cross-language calls). `impact` pairs `UNKNOWN` with a `riskNote` saying so. Confirm with a text search before treating the symbol as safe to change or delete; do not proceed on the strength of a zero.
- **MUST use `query({search_query: "concept"})` for concepts/flows, `context({name: "symbolName"})` for a named symbol, or `impact` for blast radius, on read-only callers, dependencies, imports, or execution flow.** Graph first; text search only for empty/`UNKNOWN`/literals.
- For security review, `explain({target: "fileOrSymbol"})` lists taint findings (source→sink flows; needs `analyze --pdg`).

## Never Do

- NEVER edit a function, class, or method before MCP/CLI impact analysis.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis, and never read `UNKNOWN` as an all-clear — it means the walk could not answer, which is the one verdict that requires confirming by other means.
- NEVER rename symbols with find-and-replace — use `rename` which understands the call graph.
- NEVER commit before MCP/CLI graph change analysis.

## Resources

| Resource | Use for |
| --- | --- |
| `gitnexus://repo/velox/context` | Codebase overview, check index freshness |
| `gitnexus://repo/velox/clusters` | All functional areas |
| `gitnexus://repo/velox/processes` | All execution flows |
| `gitnexus://repo/velox/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
| --- | --- |
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
