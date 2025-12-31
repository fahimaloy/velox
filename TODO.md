manage_todo_list:
  tasks:
    - id: 1
      title: "Upgrade Rust edition to 2024"
      status: "completed"
      notes: "Completed: cargo update and minor compatibility fixes."

    - id: 2
      title: "Restore CI for wgpu tests"
      status: "completed"
      notes: "CI pipeline updated and passing for existing wgpu tests."

    - id: 3
      title: "Skia: scaffolding (create module & bindings)"
      status: "in-progress"
      notes: "Add `velox-renderer/src/skia_gl.rs`, cargo features and initial build integration."

    - id: 4
      title: "Skia: platform context management"
      status: "todo"
      notes: "Implement EGL/GL/Metal/Direct3D handle management and platform abstractions."

    - id: 5
      title: "Skia: surface and swapchain integration"
      status: "todo"
      notes: "Create SkSurface/GrContext and integrate with windowing and present logic."

    - id: 6
      title: "Skia: rendering backend API"
      status: "todo"
      notes: "Expose draw calls, texture upload, readback, and pipeline hooks."

    - id: 7
      title: "Skia: tests and examples"
      status: "todo"
      notes: "Add unit/integration tests and update examples/myapp to exercise Skia path."

    - id: 8
      title: "Renderer: unify backend selection"
      status: "todo"
      notes: "Runtime selection between wgpu and skia; feature gate and config."

    - id: 9
      title: "Perf: measure and optimize Skia path"
      status: "todo"
      notes: "Benchmark common scenes and optimize allocations and flush behavior."

    - id: 10
      title: "Docs: Skia backend RFC & integration guide"
      status: "todo"
      notes: "Add developer docs, migration notes and known limitations."

    - id: 11
      title: "Release: prepare 0.1.3 changelog"
      status: "todo"
      notes: "Document Skia backend addition and notable fixes."

    - id: 12
      title: "Housekeeping: update dependencies and run cargo fmt"
      status: "todo"
      notes: "Run dependency updates, cargo fmt and CI sanity checks."
