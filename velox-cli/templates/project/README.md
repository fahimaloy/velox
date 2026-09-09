# {{project_name}}

A Velox application — a Rust GUI framework with Vue-like single-file components (`.vx`).

## Getting Started

### Prerequisites

- Rust toolchain (1.70+)
- Velox CLI: `cargo install velox-cli`

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
│   ├── App.vx               # Root component
│   └── components/
│       ├── Todos.vx         # Owns the todo list; renders TodoInput + TodoItem
│       ├── TodoInput.vx     # Text input + Add button (emits input/submit)
│       └── TodoItem.vx      # A single todo row (emits toggle/remove)
├── assets/                  # Static assets (images, fonts)
└── README.md
```

## Features

- Component-based architecture with `.vx` single-file components
- Props passing from parent to child components
- Event emission from child to parent (`@event` handlers)
- Conditional rendering (`v-if`, `v-else-if`, `v-else`)
- List rendering with `v-for`
- Scoped CSS styles per component
- Reactive state with `Signal<T>`
- Fast native rendering (Skia)

## Component Examples

The generated project is a working todo app:

- **App.vx** — root component; hosts the persistent `Todos` component state.
- **Todos.vx** — owns the `todos` array (with default entries) and the input value;
  renders `TodoInput` and a `v-for` list of `TodoItem`.
- **TodoInput.vx** — text input + Add button; emits `input`/`submit` events.
- **TodoItem.vx** — checkbox, text, and remove button; emits `toggle`/`remove` events.

## Documentation

For full documentation, visit: https://velox.dev/docs

## License

MIT
