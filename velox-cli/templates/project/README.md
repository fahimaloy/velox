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
├── src/
│   ├── main.rs       # Application entry point
│   └── App.vx        # Main component
├── assets/           # Static assets (images, fonts)
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

- ⚡ Hot Module Reload
- 🎨 Scoped CSS
- 📦 Component-based architecture
- 🚀 Fast native rendering

## Documentation

For full documentation, visit: https://velox.dev/docs

## License

MIT
