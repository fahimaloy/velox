# Architecture Overview

Velox is a Rust workspace composed of focused crates that together provide a reactive UI framework:

- velox-core: reactive primitives (signals, effects, lifecycle hooks, watch) forming the state layer.
- velox-sfc: parses `.vx/.vue` Single File Components into a template AST and generates Rust code (`render()` functions and stubs).
- velox-dom: lightweight virtual DOM (`VNode`, `Props`) and a minimal diff algorithm to compute patches.
- velox-style: minimal CSS handling (tag/.class selectors) to compute inline styles for VNodes.
- velox-renderer: feature-gated rendering backends and a stable `Renderer` trait (`backend_name()`, `mount()`); includes an event registry for `on:<event>` handlers.
- velox-cli: a CLI to compile `.vx` files into Rust modules for use in apps and examples.

Data flows top-down:
1) State changes in velox-core signals trigger view recomputation.
2) velox-sfc `render()` (or manual view builders) produce VNode trees.
3) velox-style annotates VNodes with inline styles derived from a stylesheet.
4) velox-dom diff computes patches between old and new trees.
5) velox-renderer mounts VNodes (and later will apply patches to native views); events bubble back via the registry.

Backends:
- wgpu: compiles and tested; serves as a template for a GPU-backed renderer.
- skia: split features — `skia` (API-only stub) and `skia-native` (pulls `skia-safe`; heavy native build).

Examples under `examples/` demonstrate usage; tests across crates validate behavior end-to-end. This modular design isolates responsibilities and makes it easy to evolve components independently.
