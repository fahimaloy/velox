//! Lightweight Skia raster renderer helpers (Phase 1 minimal implementation).
//!
//! Provides a small helper to render a `VNode` into a raster PNG byte buffer.
//! This file is intentionally minimal for Phase 1: it draws element background
//! rectangles (via inline `style` attr parsing) and placeholders for text.
//!
#![allow(unused)]

use velox_dom::VNode;
use velox_style::Stylesheet;

#[cfg(feature = "skia-native")]
pub mod skia_impl {
    use super::*;
    use skia_safe as sk;

    /// Render `vnode` into a PNG-encoded raster image.
    ///
    /// This is a minimal proof-of-concept renderer used in Phase 1. It:
    /// - Creates a CPU raster `Surface`
    /// - Draws element backgrounds parsed from a `style` attr (`background-color:#RRGGBB`)
    /// - Draws simple placeholders for text nodes
    /// - Returns PNG bytes
    pub fn render_vnode_to_raster_png(
        vnode: &VNode,
        _sheet: &Stylesheet,
        width: i32,
        height: i32,
    ) -> Result<Vec<u8>, String> {
        let mut surface = sk::surfaces::raster_n32_premul((width, height))
            .ok_or_else(|| "skia: failed to create raster surface".to_string())?;
        let canvas = surface.canvas();
        canvas.clear(sk::Color::WHITE);

        fn draw_node(canvas: &sk::Canvas, node: &VNode, x: f32, y: f32, w: f32, h: f32) {
            match node {
                VNode::Element { props, children, .. } => {
                    // Parse simple inline style `background-color:#RRGGBB`
                    if let Some(s) = props.attrs.get("style") {
                        for decl in s.split(';') {
                            let d = decl.trim();
                            if d.is_empty() { continue; }
                            if let Some((k, v)) = d.split_once(':') {
                                if k.trim() == "background-color" {
                                    let v = v.trim();
                                    if let Some(hex) = v.strip_prefix('#') {
                                        if hex.len() == 6 {
                                            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
                                            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
                                            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
                                            let mut p = sk::Paint::default();
                                            p.set_anti_alias(true);
                                            p.set_color(sk::Color::from_argb(255, r, g, b));
                                            let rrect = sk::Rect::from_xywh(x, y, w, h);
                                            canvas.draw_rect(rrect, &p);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Naive child layout: stack children vertically
                    let child_count = children.len().max(1);
                    let child_h = h / (child_count as f32);
                    for (i, ch) in children.iter().enumerate() {
                        draw_node(canvas, ch, x, y + i as f32 * child_h, w, child_h);
                    }
                }
                VNode::Text(t) => {
                    // Draw simple text using a default typeface and font size.
                    let mut p = sk::Paint::default();
                    p.set_anti_alias(true);
                    p.set_color(sk::Color::from_argb(255, 0, 0, 0));
                    let mut font = sk::Font::default();
                    font.set_size(14.0);
                    let txt = t.as_str();
                    // Position text baseline a few px below y
                    let tx = x + 2.0;
                    let ty = y + 12.0;
                    // Use draw_str if available; fall back to drawing a rect if not.
                    #[allow(unused_must_use)]
                    {
                        // Many skia-safe versions provide `draw_str` on Canvas.
                        let _ = canvas.draw_str(txt, (tx, ty), &font, &p);
                    }
                }
            }
        }

        draw_node(canvas, vnode, 0.0, 0.0, width as f32, height as f32);

        let image = surface.image_snapshot();
        #[allow(deprecated)]
        let data = image
            .encode_to_data(skia_safe::EncodedImageFormat::PNG)
            .ok_or_else(|| "skia: failed to encode image".to_string())?;
        Ok(data.as_bytes().to_vec())
    }
}

#[cfg(not(feature = "skia-native"))]
pub mod skia_impl {
    use super::*;

    pub fn render_vnode_to_raster_png(
        _vnode: &VNode,
        _sheet: &Stylesheet,
        _width: i32,
        _height: i32,
    ) -> Result<Vec<u8>, String> {
        Err("skia-native feature not enabled".into())
    }
}

pub use skia_impl::render_vnode_to_raster_png;
