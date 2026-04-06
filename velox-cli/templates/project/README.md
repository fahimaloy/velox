# {{project_name}}

A Velox application.

## Getting Started

### Prerequisites

- Rust toolchain (1.70+)
- Velox CLI installed globally

### Installation

```bash
# Install dependencies
cargo build

# Run in development mode
velox dev

# Build for production
velox build
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
│       ├── mod.rs           # Component module declarations
│       └── Counter.vx       # Example child component
├── assets/                  # Static assets (images, fonts)
└── README.md
```

### Development

Start the development server with hot reload:

```bash
velox dev
```

### Building

Create a production build:

```bash
velox build --release
```

## Features

- Component-based architecture with `.vx` single-file components
- Props passing from parent to child components
- Event emission from child to parent (`emit` / `@event`)
- Conditional rendering (`v-if`, `v-else-if`, `v-else`)
- List rendering with `v-for`
- Scoped CSS styles per component
- Reactive state with `Signal<T>`
- Fast native rendering

## Component Examples

The generated project includes educational examples demonstrating:

- **App.vx** - Root component showing component composition, conditional rendering, and list management
- **Counter.vx** - Child component with props, internal state, and parent event emission

## Documentation

For full documentation, visit: https://velox.dev/docs

## License

MIT
