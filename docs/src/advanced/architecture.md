# Architecture

Velox is a Rust workspace composed of focused crates that together form a reactive UI framework. Each crate owns one responsibility, and they fit together in a clear top-down data flow.

## The crate graph

| Crate | Responsibility |
|:---|:---|
| [`velox-core`](https://docs.rs/velox-core) | Reactive primitives: signals, effects, lifecycle hooks, and `watch` — the state layer. |
| `velox-sfc` | Parses `.vx`/`.vue` Single File Components into a template AST and generates Rust code (`render()` functions and stubs). |
| `velox-dom` | Lightweight virtual DOM (`VNode`, `Props`), a minimal diff algorithm, and a flexbox layout engine. |
| `velox-style` | CSS handling (tag/`.class` selectors) that computes inline styles for `VNode`s. |
| `velox-renderer` | Feature-gated rendering backends behind a stable `Renderer` trait (`backend_name()`, `mount()`), plus the event registry for `on:<event>` handlers. |
| `veloxc` | The CLI that compiles `.vx` files into Rust modules and scaffolds new projects. |

```
velox/
├── velox-core/      # Reactive state management
├── velox-sfc/       # Single File Component compiler
├── velox-dom/       # Virtual DOM + layout engine
├── velox-style/     # CSS styling system
├── velox-renderer/  # Rendering backends (Skia)
├── veloxc/          # CLI tools
└── examples/        # Example applications
```

## Data flow

Data flows top-down, from state to pixels:

1. **State** — a change in a `velox-core` signal triggers view recomputation via effects.
2. **View** — the generated `render()` from `velox-sfc` (or a manual view builder) produces a `VNode` tree.
3. **Style** — `velox-style` annotates the `VNode`s with inline styles derived from the stylesheet.
4. **Layout & diff** — `velox-dom` computes the flexbox layout against the viewport and, where a diffing consumer exists, patches between old and new trees.
5. **Render** — `velox-renderer` mounts the `VNode`s and paints every frame; events bubble back up through the event registry.

## Platform layering

The native window stack is layered deliberately:

- **winit** owns the OS window, the event loop, and input events (resize, scale-factor changes, keyboard, mouse).
- **Skia** rasterizes every frame into a premultiplied RGBA raster surface (`raster_n32_premul`).
- **softbuffer** presents that surface to the screen, with Wayland and X11 compatibility on Linux.

This split keeps rendering deterministic and portable. See [Rendering Pipeline](rendering-pipeline.md) for how a frame is painted and what it costs.

## Backends

Rendering backends are feature-gated:

- **`skia`** — an API-only stub surface for tests and headless checks.
- **`skia-native`** — pulls in `skia-safe` for the real renderer (a heavier native build). Windowed rendering currently uses a raster surface presented via softbuffer; GPU-backed surfaces are experimental on Linux and fall back to raster unless EGL window surfaces are implemented.

This modular design isolates responsibilities and makes it easy to evolve components independently.
