# Interactive Skia Example

This example shows borders, hover styles, and click handling using the Skia renderer.

## Run

```bash
cargo run -p interactive_skia
```

Notes:
- Requires Skia native dependencies for your platform.
- Windowed Skia uses a raster surface presented via `softbuffer` for Wayland/X11.
- Hover the button to see the style change.
- To force Wayland: `WINIT_UNIX_BACKEND=wayland cargo run -p interactive_skia`
