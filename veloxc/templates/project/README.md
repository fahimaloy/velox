# {{project_name}}

A Velox application — a Rust GUI framework with Vue-like single-file components (`.vx`).

## Getting Started

### Prerequisites

- Rust toolchain (1.70+)
- Velox CLI: `cargo install veloxc`

### Development

```bash
# Install dependencies
cargo build

# Run the app (dev hot-reload)
velox dev

# Build for production
velox build --release
```

### Project Structure

```
{{project_name}}/
├── Cargo.toml
├── build.rs
├── src/
│   ├── main.rs              # Application entry point
│   ├── App.vx               # Root component: page chrome, theme toggle, dialogs
│   └── components/
│       ├── Todos.vx         # Owns the todo list, the draft text and the theme flag
│       ├── TodoInput.vx     # Text input (the owning Todos component handles input)
│       ├── TodoItem.vx      # A single todo row; owning Todos handles row actions
│       ├── Modal.vx         # Reusable dialog — title, message, cancel/confirm
│       └── Confirm.vx       # Reusable destructive question — cancel/accept, danger styling
├── assets/                  # Static assets (images, fonts)
└── README.md
```

## Features

- Component-based architecture with `.vx` single-file components
- Props passing from parent to child components
- Renderer-dispatched input and click handlers owned by the component state
- Conditional rendering (`v-if`, `v-else-if`, `v-else`)
- List rendering with `v-for`
- Scoped CSS styles per component
- Reactive state with `Signal<T>`
- A working light/dark theme, toggled from the icon button in the header
- Two reusable dialog components that ship with the template, not app-specific code
- Fast native rendering (Skia)

## Component Examples

The generated project is a working todo app:

- **App.vx** — root component; hosts the persistent `Todos` state, the header,
  the theme toggle and both dialogs.
- **Todos.vx** — owns the `todos` list, the draft text and the theme flag; renders
  the composer, a `v-for` list of `TodoItem`, and the empty state.
- **TodoInput.vx** — the text field; the owning `Todos` component handles input.
- **TodoItem.vx** — one row: a check control, the task text (struck through when
  done) and a remove control; the owning `Todos` component handles both clicks.
- **Modal.vx** / **Confirm.vx** — drop-in dialogs. Render one behind a `v-if`,
  bind `:title` / `:body` (or `:message`) and the button labels, and listen for
  `dismiss` / `confirm` (or `accept`). Both are ordinary components with ordinary
  scoped styles, so a caller restyles them by appending its own rules.

## Styling notes

There is no `var()`, no `@media`, no `border-color` alone and no HTML comments in
a `<template>`. The theme class has to sit on a wrapper element inside each
component, because scoped CSS cannot reach across a component boundary. Each
style block lists its light rules first and its `.dark` overrides last.

## Documentation

For full documentation, visit: https://velox.dev/docs

## License

MIT
