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
    use std::collections::HashMap;

    #[derive(Clone, Copy)]
    struct BorderSpec {
        width: f32,
        color: sk::Color,
    }

    #[derive(Hash, Eq, PartialEq, Clone)]
    struct FontKey {
        family: String,
        size_key: u32,
    }

    #[derive(Clone, Copy)]
    enum TextAlign {
        Left,
        Center,
        Right,
    }

    #[derive(Clone, Copy)]
    struct TextStyle {
        color: sk::Color,
        align: TextAlign,
        underline: bool,
        font_size: f32,
    }

    #[derive(Clone, Copy)]
    struct ClipInsets {
        top: f32,
        right: f32,
        bottom: f32,
        left: f32,
    }

    #[derive(Clone, Copy, Default)]
    struct FilterSpec {
        blur_sigma: Option<f32>,
        brightness: Option<f32>,
    }

    fn parse_color_hex(value: &str) -> Option<sk::Color> {
        let hex = value.strip_prefix('#')?;
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(sk::Color::from_argb(255, r, g, b));
        }
        if hex.len() == 8 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            return Some(sk::Color::from_argb(a, r, g, b));
        }
        None
    }

    fn parse_border_value(value: &str) -> Option<BorderSpec> {
        let mut width: Option<f32> = None;
        let mut color: Option<sk::Color> = None;
        let mut is_solid = false;

        for part in value.split_whitespace() {
            if let Some(px) = part.strip_suffix("px") {
                if let Ok(v) = px.parse::<f32>() {
                    width = Some(v);
                }
            } else if part.eq_ignore_ascii_case("solid") {
                is_solid = true;
            } else if let Some(col) = parse_color_hex(part) {
                color = Some(col);
            }
        }

        if !is_solid {
            return None;
        }

        Some(BorderSpec {
            width: width.unwrap_or(1.0),
            color: color.unwrap_or_else(|| sk::Color::from_argb(255, 0, 0, 0)),
        })
    }

    fn parse_px_value(value: &str) -> Option<f32> {
        value.strip_suffix("px").and_then(|px| px.trim().parse::<f32>().ok())
    }

    fn parse_float_value(value: &str) -> Option<f32> {
        value.trim().parse::<f32>().ok()
    }

    fn parse_font_family(value: &str) -> Option<String> {
        let first = value.split(',').next()?.trim();
        let trimmed = first.trim_matches('"').trim_matches('\'').trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    fn parse_clip_inset(value: &str) -> Option<ClipInsets> {
        let value = value.trim();
        if !value.starts_with("inset(") || !value.ends_with(')') {
            return None;
        }
        let inner = value.trim_start_matches("inset(").trim_end_matches(')');
        let inner = inner.split("round").next().unwrap_or(inner).trim();
        let mut parts: Vec<f32> = Vec::new();
        for part in inner.split_whitespace() {
            if let Some(px) = parse_px_value(part) {
                parts.push(px);
            } else {
                return None;
            }
        }
        let (top, right, bottom, left) = match parts.len() {
            1 => (parts[0], parts[0], parts[0], parts[0]),
            2 => (parts[0], parts[1], parts[0], parts[1]),
            3 => (parts[0], parts[1], parts[2], parts[1]),
            4 => (parts[0], parts[1], parts[2], parts[3]),
            _ => return None,
        };
        Some(ClipInsets { top, right, bottom, left })
    }

    fn parse_style_attr(
        style: &str,
    ) -> (
        Option<sk::Color>,
        Option<BorderSpec>,
        Option<f32>,
        bool,
        Option<ClipInsets>,
        f32,
        FilterSpec,
        i32,
    ) {
        let mut bg = None;
        let mut border = None;
        let mut radius = None;
        let mut overflow_hidden = false;
        let mut clip_inset = None;
        let mut opacity = 1.0f32;
        let mut filters = FilterSpec::default();
        let mut z_index = 0i32;

        for decl in style.split(';') {
            let d = decl.trim();
            if d.is_empty() {
                continue;
            }
            if let Some((k, v)) = d.split_once(':') {
                let key = k.trim();
                let val = v.trim();
                if key == "background-color" || key == "background" {
                    bg = parse_color_hex(val);
                } else if key == "border" {
                    border = parse_border_value(val);
                } else if key == "border-radius" {
                    if let Some(px) = parse_px_value(val) {
                        radius = Some(px);
                    }
                } else if key == "overflow" {
                    overflow_hidden = val.eq_ignore_ascii_case("hidden");
                } else if key == "clip-path" {
                    clip_inset = parse_clip_inset(val);
                } else if key == "opacity" {
                    if let Some(alpha) = parse_float_value(val) {
                        opacity = alpha.clamp(0.0, 1.0);
                    }
                } else if key == "filter" {
                    for part in val.split(')') {
                        let part = part.trim();
                        if part.is_empty() {
                            continue;
                        }
                        if let Some(value) = part.strip_prefix("blur(") {
                            if let Some(px) = parse_px_value(value.trim()) {
                                filters.blur_sigma = Some(px.max(0.0));
                            }
                        } else if let Some(value) = part.strip_prefix("brightness(") {
                            if let Some(f) = parse_float_value(value.trim()) {
                                filters.brightness = Some(f.max(0.0));
                            }
                        }
                    }
                } else if key == "z-index" {
                    if let Ok(z) = val.parse::<i32>() {
                        z_index = z;
                    }
                }
            }
        }

        (
            bg,
            border,
            radius,
            overflow_hidden,
            clip_inset,
            opacity,
            filters,
            z_index,
        )
    }

    fn z_index_for_props(props: &velox_dom::Props) -> i32 {
        if let Some(style) = props.attrs.get("style") {
            for decl in style.split(';') {
                let d = decl.trim();
                if d.is_empty() {
                    continue;
                }
                if let Some((k, v)) = d.split_once(':') {
                    if k.trim() == "z-index" {
                        if let Ok(z) = v.trim().parse::<i32>() {
                            return z;
                        }
                    }
                }
            }
        }
        0
    }

    fn parse_text_style(style: &str, base: TextStyle, family: &str) -> (TextStyle, String) {
        let mut text_style = base;
        let mut font_family = family.to_string();
        for decl in style.split(';') {
            let d = decl.trim();
            if d.is_empty() {
                continue;
            }
            if let Some((k, v)) = d.split_once(':') {
                let key = k.trim();
                let val = v.trim();
                if key == "color" {
                    if let Some(color) = parse_color_hex(val) {
                        text_style.color = color;
                    }
                } else if key == "text-align" {
                    text_style.align = match val.to_ascii_lowercase().as_str() {
                        "center" => TextAlign::Center,
                        "right" => TextAlign::Right,
                        _ => TextAlign::Left,
                    };
                } else if key == "text-decoration" {
                    let val_l = val.to_ascii_lowercase();
                    if val_l.contains("underline") {
                        text_style.underline = true;
                    } else if val_l == "none" {
                        text_style.underline = false;
                    }
                } else if key == "font-size" {
                    if let Some(px) = parse_px_value(val).or_else(|| parse_float_value(val)) {
                        text_style.font_size = px.max(1.0);
                    }
                } else if key == "font-family" {
                    if let Some(family) = parse_font_family(val) {
                        font_family = family;
                    }
                }
            }
        }
        (text_style, font_family)
    }

    fn inset_rect(rect: sk::Rect, inset: ClipInsets) -> sk::Rect {
        let left = rect.left + inset.left;
        let top = rect.top + inset.top;
        let width = (rect.width() - inset.left - inset.right).max(0.0);
        let height = (rect.height() - inset.top - inset.bottom).max(0.0);
        sk::Rect::from_xywh(left, top, width, height)
    }

    fn apply_clips(
        canvas: &sk::Canvas,
        rect: sk::Rect,
        rrect: Option<sk::RRect>,
        overflow_hidden: bool,
        clip_inset: Option<ClipInsets>,
    ) -> bool {
        let needs_clip = rrect.is_some() || overflow_hidden || clip_inset.is_some();
        if !needs_clip {
            return false;
        }
        canvas.save();
        if let Some(rrect) = rrect {
            canvas.clip_rrect(rrect, sk::ClipOp::Intersect, true);
        } else if overflow_hidden {
            canvas.clip_rect(rect, sk::ClipOp::Intersect, true);
        }
        if let Some(inset) = clip_inset {
            let clip_rect = inset_rect(rect, inset);
            canvas.clip_rect(clip_rect, sk::ClipOp::Intersect, true);
        }
        true
    }

    fn text_x_for_align(container: sk::Rect, text_w: f32, align: TextAlign) -> f32 {
        let padding = 2.0;
        match align {
            TextAlign::Left => container.left + padding,
            TextAlign::Center => container.left + (container.width() - text_w) * 0.5,
            TextAlign::Right => (container.right - text_w - padding).max(container.left + padding),
        }
    }

    fn color_with_opacity(color: sk::Color, opacity: f32) -> sk::Color {
        let a = ((color.a() as f32) * opacity).round().clamp(0.0, 255.0) as u8;
        sk::Color::from_argb(a, color.r(), color.g(), color.b())
    }

    fn apply_filters_to_paint(paint: &mut sk::Paint, filters: FilterSpec) {
        if let Some(sigma) = filters.blur_sigma {
            if sigma > 0.0 {
                paint.set_image_filter(sk::image_filters::blur((sigma, sigma), None, None, None));
            }
        }
        if let Some(brightness) = filters.brightness {
            let b = brightness.max(0.0);
            let matrix: [f32; 20] = [
                b, 0.0, 0.0, 0.0, 0.0,
                0.0, b, 0.0, 0.0, 0.0,
                0.0, 0.0, b, 0.0, 0.0,
                0.0, 0.0, 0.0, 1.0, 0.0,
            ];
            paint.set_color_filter(sk::color_filters::matrix_row_major(&matrix, None));
        }
    }

    fn layout_text_lines(
        text: &str,
        max_width: f32,
        fonts: &mut FontCache,
        family: &str,
        size: f32,
    ) -> Vec<(String, f32)> {
        let limit = if max_width <= 0.0 { f32::INFINITY } else { max_width };
        let mut lines = Vec::new();
        for para in text.split('\n') {
            if para.trim().is_empty() {
                lines.push((String::new(), 0.0));
                continue;
            }
            let mut current = String::new();
            let mut current_w = 0.0;
            for word in para.split_whitespace() {
                let candidate = if current.is_empty() {
                    word.to_string()
                } else {
                    format!("{} {}", current, word)
                };
                let candidate_w = fonts.measure_text(family, size, &candidate);
                if candidate_w <= limit || current.is_empty() {
                    current = candidate;
                    current_w = candidate_w;
                } else {
                    lines.push((current, current_w));
                    current = word.to_string();
                    current_w = fonts.measure_text(family, size, &current);
                }
            }
            if !current.is_empty() {
                lines.push((current, current_w));
            }
        }
        lines
    }

    fn collect_debug_hit_rects(
        vnode: &VNode,
        layout: &velox_dom::layout::LayoutNode,
        out: &mut Vec<velox_dom::layout::Rect>,
    ) {
        match vnode {
            VNode::Text(_) => {}
            VNode::Element { tag, props, children, .. } => {
                if crate::events::is_hoverable(tag, props) {
                    out.push(layout.rect);
                }
                for (child, child_layout) in children.iter().zip(&layout.children) {
                    collect_debug_hit_rects(child, child_layout, out);
                }
            }
        }
    }

    struct RenderPaints {
        fill: sk::Paint,
        stroke: sk::Paint,
        text: sk::Paint,
        underline: sk::Paint,
        image: sk::Paint,
    }

    impl RenderPaints {
        fn new() -> Self {
            let mut fill = sk::Paint::default();
            fill.set_anti_alias(true);
            let mut stroke = sk::Paint::default();
            stroke.set_anti_alias(true);
            stroke.set_style(skia_safe::paint::Style::Stroke);
            let mut text = sk::Paint::default();
            text.set_anti_alias(true);
            let mut underline = sk::Paint::default();
            underline.set_anti_alias(true);
            underline.set_stroke_width(1.0);
            let mut image = sk::Paint::default();
            image.set_anti_alias(true);
            RenderPaints {
                fill,
                stroke,
                text,
                underline,
                image,
            }
        }
    }

    struct ImageCache {
        images: HashMap<String, sk::Image>,
    }

    impl ImageCache {
        fn new() -> Self {
            ImageCache { images: HashMap::new() }
        }

        fn load(&mut self, src: &str) -> Option<sk::Image> {
            if let Some(img) = self.images.get(src) {
                return Some(img.clone());
            }
            let bytes = std::fs::read(src).ok()?;
            let data = sk::Data::new_copy(&bytes);
            let image = sk::Image::from_encoded(data)?;
            self.images.insert(src.to_string(), image.clone());
            Some(image)
        }
    }

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

        let default_text_style = TextStyle {
            color: sk::Color::from_argb(255, 0, 0, 0),
            align: TextAlign::Left,
            underline: false,
            font_size: 14.0,
        };

        let mut fonts = FontCache::new();
        let mut images = ImageCache::new();
        let default_family = fonts.default_family();
        let default_text_style = TextStyle {
            color: sk::Color::from_argb(255, 0, 0, 0),
            align: TextAlign::Left,
            underline: false,
            font_size: 14.0,
        };
        let mut paints = RenderPaints::new();

        fn draw_node(
            canvas: &sk::Canvas,
            node: &VNode,
            rect: sk::Rect,
            container_rect: sk::Rect,
            text_style: TextStyle,
            font_family: &str,
            fonts: &mut FontCache,
            paints: &mut RenderPaints,
            images: &mut ImageCache,
            inherited_opacity: f32,
        ) {
            match node {
                VNode::Element { props, children, .. } => {
                    let mut clip_rrect = None;
                    let mut overflow_hidden = false;
                    let mut clip_inset = None;
                    let mut child_text_style = text_style;
                    let mut child_family = font_family.to_string();
                    let mut opacity = inherited_opacity;
                    let mut filters = FilterSpec::default();
                    if let Some(s) = props.attrs.get("style") {
                        let (bg, border, radius, overflow, inset, alpha, filter_spec, _z) =
                            parse_style_attr(s);
                        let rect = rect;
                        let rrect = radius.map(|r| sk::RRect::new_rect_xy(rect, r, r));
                        if let Some(rrect) = rrect {
                            clip_rrect = Some(rrect);
                        }
                        overflow_hidden = overflow;
                        clip_inset = inset;
                        let (style, family) = parse_text_style(s, text_style, font_family);
                        child_text_style = style;
                        child_family = family;
                        opacity = (opacity * alpha).clamp(0.0, 1.0);
                        filters = filter_spec;
                        if let Some(bg) = bg {
                            paints.fill.set_color(color_with_opacity(bg, opacity));
                            if let Some(rrect) = rrect {
                                canvas.draw_rrect(rrect, &paints.fill);
                            } else {
                                canvas.draw_rect(rect, &paints.fill);
                            }
                        }

                        if let Some(border) = border {
                            paints.stroke.set_stroke_width(border.width);
                            paints.stroke.set_color(color_with_opacity(border.color, opacity));
                            if let Some(rrect) = rrect {
                                canvas.draw_rrect(rrect, &paints.stroke);
                            } else {
                                canvas.draw_rect(rect, &paints.stroke);
                            }
                        }
                    }

                    if let Some(src) = props.attrs.get("src") {
                        paints.image.set_image_filter(None);
                        paints.image.set_color_filter(None);
                        paints.image.set_alpha_f(opacity);
                        apply_filters_to_paint(&mut paints.image, filters);
                        if let Some(img) = images.load(src) {
                            canvas.draw_image_rect(
                                img,
                                None,
                                rect,
                                &paints.image,
                            );
                        }
                    }

                    // Naive child layout: stack children vertically
                    let child_count = children.len().max(1);
                    let child_h = rect.height() / (child_count as f32);
                    let rect = rect;
                    let did_clip = apply_clips(canvas, rect, clip_rrect, overflow_hidden, clip_inset);
                    let mut ordered: Vec<(i32, usize, &VNode)> = children
                        .iter()
                        .enumerate()
                        .map(|(i, ch)| {
                            let z = match ch {
                                VNode::Element { props, .. } => z_index_for_props(props),
                                _ => 0,
                            };
                            (z, i, ch)
                        })
                        .collect();
                    ordered.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
                    for (_, original_idx, ch) in ordered.iter() {
                        let child_rect = sk::Rect::from_xywh(
                            rect.left,
                            rect.top + *original_idx as f32 * child_h,
                            rect.width(),
                            child_h,
                        );
                        draw_node(
                            canvas,
                            ch,
                            child_rect,
                            rect,
                            child_text_style,
                            &child_family,
                            fonts,
                            paints,
                            images,
                            opacity,
                        );
                    }
                    if did_clip {
                        canvas.restore();
                    }
                }
                VNode::Text(t) => {
                    paints
                        .text
                        .set_color(color_with_opacity(text_style.color, inherited_opacity));
                    let font_size = text_style.font_size;
                    let font = fonts.font(font_family, font_size);
                    let lines = layout_text_lines(
                        t.as_str(),
                        container_rect.width(),
                        fonts,
                        font_family,
                        font_size,
                    );
                    let line_height = font_size * 1.2;
                    let layout_rect = sk::Rect::from_xywh(
                        rect.left,
                        rect.top,
                        rect.width(),
                        rect.height(),
                    );
                    let align_rect = if layout_rect.width() >= container_rect.width() - 0.5 {
                        container_rect
                    } else {
                        layout_rect
                    };
                    let text_bottom = rect.top + rect.height().max(line_height);
                    for (idx, (line, line_w)) in lines.into_iter().enumerate() {
                        let ty = rect.top + font_size + (idx as f32) * line_height;
                        if ty > text_bottom {
                            break;
                        }
                        let padding = if align_rect.width() >= container_rect.width() - 0.5 {
                            2.0
                        } else {
                            0.0
                        };
                        let tx = match text_style.align {
                            TextAlign::Left => align_rect.left + padding,
                            TextAlign::Center => align_rect.left + (align_rect.width() - line_w) * 0.5,
                            TextAlign::Right => (align_rect.right - line_w - padding).max(align_rect.left + padding),
                        };
                        #[allow(unused_must_use)]
                        {
                            let _ = canvas.draw_str(line.as_str(), (tx, ty), &font, &paints.text);
                        }
                        if text_style.underline {
                            paints
                                .underline
                                .set_color(color_with_opacity(text_style.color, inherited_opacity));
                            let uy = ty + 1.0;
                            canvas.draw_line((tx, uy), (tx + line_w, uy), &paints.underline);
                        }
                    }
                }
            }
        }

        let root_rect = sk::Rect::from_xywh(0.0, 0.0, width as f32, height as f32);
        draw_node(
            canvas,
            vnode,
            root_rect,
            root_rect,
            default_text_style,
            &default_family,
            &mut fonts,
            &mut paints,
            &mut images,
            1.0,
        );

        let image = surface.image_snapshot();
        #[allow(deprecated)]
        let data = image
            .encode_to_data(skia_safe::EncodedImageFormat::PNG)
            .ok_or_else(|| "skia: failed to encode image".to_string())?;
        Ok(data.as_bytes().to_vec())
    }

    /// Render `vnode` into a PNG-encoded raster image with a scale factor applied.
    pub fn render_vnode_to_raster_png_with_scale(
        vnode: &VNode,
        sheet: &Stylesheet,
        width: i32,
        height: i32,
        scale_factor: f32,
    ) -> Result<Vec<u8>, String> {
        let physical_w = ((width as f32) * scale_factor).round() as i32;
        let physical_h = ((height as f32) * scale_factor).round() as i32;
        let mut surface = crate::skia_surface::SkiaSurface::new_raster(physical_w, physical_h)?;
        surface.set_scale_factor(scale_factor);
        render_frame(&mut surface, vnode, sheet)?;
        surface.encode_png()
    }

    /// Minimal FontCache for mapping sizes to `skia_safe::Font`.
    pub struct FontCache {
        typefaces: HashMap<String, sk::Typeface>,
        fonts: HashMap<FontKey, sk::Font>,
        default_family: String,
    }

    impl FontCache {
        /// Attempt to load a system font or bundled fallback fonts.
        pub fn new() -> Self {
            let default_family = "default".to_string();
            let mut typefaces = HashMap::new();
            if let Some(tf) = load_default_typeface() {
                typefaces.insert(default_family.clone(), tf);
            }
            FontCache { typefaces, fonts: HashMap::new(), default_family }
        }

        pub fn default_family(&self) -> String {
            self.default_family.clone()
        }

        fn get_or_load_family(&mut self, family: &str) -> Option<sk::Typeface> {
            if let Some(tf) = self.typefaces.get(family) {
                return Some(tf.clone());
            }
            if let Some(default_tf) = self.typefaces.get(&self.default_family) {
                let tf = default_tf.clone();
                self.typefaces.insert(family.to_string(), tf.clone());
                return Some(tf);
            }
            None
        }

        /// Return a `skia_safe::Font` at the requested `size` and `family`.
        pub fn font(&mut self, family: &str, size: f32) -> sk::Font {
            let size_key = (size * 100.0).round() as u32;
            let key = FontKey { family: family.to_string(), size_key };
            if let Some(font) = self.fonts.get(&key) {
                return font.clone();
            }
            let font = if let Some(tf) = self.get_or_load_family(family) {
                sk::Font::new(tf, size)
            } else {
                let mut f = sk::Font::default();
                f.set_size(size);
                f
            };
            self.fonts.insert(key, font.clone());
            font
        }

        /// Measure the width (in px) of `text` rendered at `size` using the cached typeface.
        pub fn measure_text(&mut self, family: &str, size: f32, text: &str) -> f32 {
            let font = self.font(family, size);
            let mut p = sk::Paint::default();
            p.set_anti_alias(true);
            let (w, _bounds) = font.measure_str(text, Some(&p));
            w
        }
    }

    fn load_default_typeface() -> Option<sk::Typeface> {
        use std::fs;

        const CANDIDATES: &[&str] = &[
            "/usr/share/fonts/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/gnu-free/FreeSans.ttf",
        ];

        let font_mgr = sk::FontMgr::default();
        for p in CANDIDATES {
            if let Ok(bytes) = fs::read(p) {
                if let Some(tf) = font_mgr.new_from_data(&bytes, None) {
                    return Some(tf);
                }
            }
        }

        let bundles: &[&[u8]] = &[
            include_bytes!("../assets/DejaVuSans.ttf"),
            include_bytes!("../assets/NotoSans-Regular.ttf"),
        ];
        for b in bundles {
            if let Some(tf) = font_mgr.new_from_data(b, None) {
                return Some(tf);
            }
        }
        let preferred_families = ["DejaVu Sans", "Noto Sans", "Sans", "Arial", "Liberation Sans"];
        for family in preferred_families {
            let mut set = font_mgr.match_family(family);
            if set.count() == 0 {
                continue;
            }
            if let Some(tf) = set.match_style(sk::FontStyle::default()) {
                return Some(tf);
            }
            if let Some(tf) = set.new_typeface(0) {
                return Some(tf);
            }
        }

        if font_mgr.count_families() > 0 {
            let family = font_mgr.family_name(0);
            let mut set = font_mgr.match_family(&family);
            if let Some(tf) = set.match_style(sk::FontStyle::default()) {
                return Some(tf);
            }
            if let Some(tf) = set.new_typeface(0) {
                return Some(tf);
            }
        }

        let fallback_mgr = sk::FontMgr::new();
        fallback_mgr.legacy_make_typeface(None, sk::FontStyle::default())
    }

    /// Render a VNode tree into an existing `SkiaSurface`.
    pub fn render_frame(
        surface: &mut crate::skia_surface::SkiaSurface,
        vnode: &VNode,
        _sheet: &Stylesheet,
    ) -> Result<(), String> {
        // Compute layout using the existing velox-dom layout system.
        let scale = surface.scale_factor().max(1.0);
        let width_i = ((surface.width as f32) / scale).round().max(1.0) as i32;
        let height_i = ((surface.height as f32) / scale).round().max(1.0) as i32;
        let layout_root = velox_dom::layout::compute_layout(vnode, width_i, height_i);

        let canvas = surface.canvas();
        canvas.clear(sk::Color::WHITE);
        canvas.save();
        canvas.scale((scale, scale));

        let mut fonts = FontCache::new();
        let mut images = ImageCache::new();
        let default_text_style = TextStyle {
            color: sk::Color::from_argb(255, 0, 0, 0),
            align: TextAlign::Left,
            underline: false,
            font_size: 14.0,
        };
        let default_family = fonts.default_family();
        let mut paints = RenderPaints::new();

        fn render_with_layout(
            canvas: &sk::Canvas,
            node: &VNode,
            layout: &velox_dom::layout::LayoutNode,
            container_rect: sk::Rect,
            fonts: &mut FontCache,
            text_style: TextStyle,
            font_family: &str,
            paints: &mut RenderPaints,
            images: &mut ImageCache,
            inherited_opacity: f32,
        ) {
            match node {
                VNode::Element { props, children, .. } => {
                    let mut clip_rrect = None;
                    let mut overflow_hidden = false;
                    let mut clip_inset = None;
                    let mut child_text_style = text_style;
                    let mut child_family = font_family.to_string();
                    let mut opacity = inherited_opacity;
                    let mut filters = FilterSpec::default();
                    if let Some(s) = props.attrs.get("style") {
                        let (bg, border, radius, overflow, inset, alpha, filter_spec, _z) =
                            parse_style_attr(s);
                        let rect = sk::Rect::from_xywh(
                            layout.rect.x as f32,
                            layout.rect.y as f32,
                            layout.rect.w as f32,
                            layout.rect.h as f32,
                        );
                        let rrect = radius.map(|r| sk::RRect::new_rect_xy(rect, r, r));
                        if let Some(rrect) = rrect {
                            clip_rrect = Some(rrect);
                        }
                        overflow_hidden = overflow;
                        clip_inset = inset;
                        let (style, family) = parse_text_style(s, text_style, font_family);
                        child_text_style = style;
                        child_family = family;
                        opacity = (opacity * alpha).clamp(0.0, 1.0);
                        filters = filter_spec;
                        if let Some(bg) = bg {
                            paints.fill.set_color(color_with_opacity(bg, opacity));
                            if let Some(rrect) = rrect {
                                canvas.draw_rrect(rrect, &paints.fill);
                            } else {
                                canvas.draw_rect(rect, &paints.fill);
                            }
                        }
                        if let Some(border) = border {
                            paints.stroke.set_stroke_width(border.width);
                            paints.stroke.set_color(color_with_opacity(border.color, opacity));
                            if let Some(rrect) = rrect {
                                canvas.draw_rrect(rrect, &paints.stroke);
                            } else {
                                canvas.draw_rect(rect, &paints.stroke);
                            }
                        }
                    }

                    if let Some(src) = props.attrs.get("src") {
                        paints.image.set_image_filter(None);
                        paints.image.set_color_filter(None);
                        paints.image.set_alpha_f(opacity);
                        apply_filters_to_paint(&mut paints.image, filters);
                        if let Some(img) = images.load(src) {
                            let rect = sk::Rect::from_xywh(
                                layout.rect.x as f32,
                                layout.rect.y as f32,
                                layout.rect.w as f32,
                                layout.rect.h as f32,
                            );
                            canvas.draw_image_rect(
                                img,
                                None,
                                rect,
                                &paints.image,
                            );
                        }
                    }

                    // Render children in order using their layout nodes
                    let rect = sk::Rect::from_xywh(
                        layout.rect.x as f32,
                        layout.rect.y as f32,
                        layout.rect.w as f32,
                        layout.rect.h as f32,
                    );
                    let did_clip = apply_clips(canvas, rect, clip_rrect, overflow_hidden, clip_inset);
                    let mut ordered: Vec<(i32, usize)> = children
                        .iter()
                        .enumerate()
                        .map(|(i, ch)| {
                            let z = match ch {
                                VNode::Element { props, .. } => z_index_for_props(props),
                                _ => 0,
                            };
                            (z, i)
                        })
                        .collect();
                    ordered.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
                    for (_, idx) in ordered {
                        if let Some(child) = children.get(idx) {
                            if let Some(child_layout) = layout.children.get(idx) {
                                render_with_layout(
                                    canvas,
                                    child,
                                    child_layout,
                                    rect,
                                    fonts,
                                    child_text_style,
                                    &child_family,
                                    paints,
                                    images,
                                    opacity,
                                );
                            }
                        }
                    }
                    if did_clip {
                        canvas.restore();
                    }
                }
                VNode::Text(t) => {
                    paints
                        .text
                        .set_color(color_with_opacity(text_style.color, inherited_opacity));
                    let font_size = text_style.font_size;
                    let font = fonts.font(font_family, font_size);
                    let lines = layout_text_lines(
                        t.as_str(),
                        container_rect.width(),
                        fonts,
                        font_family,
                        font_size,
                    );
                    let line_height = font_size * 1.2;
                    let layout_rect = sk::Rect::from_xywh(
                        layout.rect.x as f32,
                        layout.rect.y as f32,
                        layout.rect.w as f32,
                        layout.rect.h as f32,
                    );
                    let align_rect = if layout_rect.width() >= container_rect.width() - 0.5 {
                        container_rect
                    } else {
                        layout_rect
                    };
                    let text_bottom = (layout.rect.y as f32)
                        + (layout.rect.h as f32).max(line_height);
                    for (idx, (line, line_w)) in lines.into_iter().enumerate() {
                        let ty = layout.rect.y as f32 + font_size + (idx as f32) * line_height;
                        if ty > text_bottom {
                            break;
                        }
                        let padding = if align_rect.width() >= container_rect.width() - 0.5 {
                            2.0
                        } else {
                            0.0
                        };
                        let tx = match text_style.align {
                            TextAlign::Left => align_rect.left + padding,
                            TextAlign::Center => align_rect.left + (align_rect.width() - line_w) * 0.5,
                            TextAlign::Right => (align_rect.right - line_w - padding).max(align_rect.left + padding),
                        };
                        #[allow(unused_must_use)]
                        {
                            let _ = canvas.draw_str(line.as_str(), (tx, ty), &font, &paints.text);
                        }
                        if text_style.underline {
                            paints
                                .underline
                                .set_color(color_with_opacity(text_style.color, inherited_opacity));
                            let uy = ty + 1.0;
                            canvas.draw_line((tx, uy), (tx + line_w, uy), &paints.underline);
                        }
                    }
                }
            }
        }

        let root_rect = sk::Rect::from_xywh(
            layout_root.rect.x as f32,
            layout_root.rect.y as f32,
            layout_root.rect.w as f32,
            layout_root.rect.h as f32,
        );
        render_with_layout(
            canvas,
            vnode,
            &layout_root,
            root_rect,
            &mut fonts,
            default_text_style,
            &default_family,
            &mut paints,
            &mut images,
            1.0,
        );
        let debug_overlay = std::env::var("VELOX_DEBUG_HIT_RECTS")
            .ok()
            .as_deref()
            .map(|v| v == "1")
            .unwrap_or(false);
        let debug_log = std::env::var("VELOX_DEBUG_HIT_RECTS_LOG")
            .ok()
            .as_deref()
            .map(|v| v == "1")
            .unwrap_or(false);
        if debug_overlay || debug_log {
            let mut rects = Vec::new();
            collect_debug_hit_rects(vnode, &layout_root, &mut rects);
            if debug_log {
                for r in &rects {
                    eprintln!("[skia debug] hit rect: x={} y={} w={} h={}", r.x, r.y, r.w, r.h);
                }
            }
            if debug_overlay {
                let mut paint = sk::Paint::default();
                paint.set_anti_alias(true);
                paint.set_style(skia_safe::paint::Style::Stroke);
                paint.set_stroke_width(1.0);
                paint.set_color(sk::Color::from_argb(200, 255, 0, 0));
                for r in rects {
                    let rect = sk::Rect::from_xywh(r.x as f32, r.y as f32, r.w as f32, r.h as f32);
                    canvas.draw_rect(rect, &paint);
                }
            }
        }
        canvas.restore();

        // Present/flush if GPU-backed
        let _ = surface.present();
        Ok(())
    }

    #[cfg(all(test, feature = "skia-native", unix))]
    mod tests {
        use super::*;
        use velox_dom::h;
        use velox_style::Stylesheet;

        #[test]
        #[ignore]
        fn render_overflow_hidden_clips_children() {
            let vnode = h(
                "div",
                vec![("style", "background-color:#FFFFFF;overflow:hidden;width:40px;height:40px")],
                vec![h(
                    "div",
                    vec![("style", "background-color:#FF0000;width:40px;height:80px")],
                    vec![],
                )],
            );

            let mut surface =
                crate::skia_surface::SkiaSurface::new_raster(64, 64).expect("surface");
            render_frame(&mut surface, &vnode, &Stylesheet::default()).expect("render");
            let path = "target/skia_overflow_clip.png";
            surface.save_png(path).expect("save png");
            let png = std::fs::read(path).expect("read png");

            let checksum = fnv1a(&png);
            println!("overflow-hidden checksum: 0x{checksum:08x}");
            // Update this checksum after regenerating the raster output.
            const EXPECTED_OVERFLOW_CHECKSUM: u32 = 0xf74653e7;
            assert_eq!(checksum, EXPECTED_OVERFLOW_CHECKSUM);
        }

        #[test]
        #[ignore]
        fn render_z_index_overlap_checksum() {
            let vnode = h(
                "div",
                vec![("style", "background-color:#FFFFFF;width:64px;height:64px")],
                vec![
                    h(
                        "div",
                        vec![("style", "background-color:#FF0000;width:40px;height:40px;z-index:1")],
                        vec![],
                    ),
                    h(
                        "div",
                        vec![
                            ("style", "background-color:#0000FF;width:40px;height:40px;margin-top:-20px;z-index:0"),
                        ],
                        vec![],
                    ),
                ],
            );

            let mut surface =
                crate::skia_surface::SkiaSurface::new_raster(64, 64).expect("surface");
            render_frame(&mut surface, &vnode, &Stylesheet::default()).expect("render");
            let path = "target/skia_z_index.png";
            surface.save_png(path).expect("save png");
            let png = std::fs::read(path).expect("read png");

            let checksum = fnv1a(&png);
            println!("z-index checksum: 0x{checksum:08x}");
            // Update this checksum after regenerating the raster output.
            const EXPECTED_Z_INDEX_CHECKSUM: u32 = 0x0c864983;
            assert_eq!(checksum, EXPECTED_Z_INDEX_CHECKSUM);
        }

        #[test]
        fn render_debug_hit_rects_collects_clickable() {
            let vnode = h(
                "div",
                vec![],
                vec![
                    h("div", vec![("class", "btn")], vec![]),
                    h("div", vec![], vec![]),
                ],
            );
            let layout = velox_dom::layout::compute_layout(&vnode, 100, 50);
            let mut rects = Vec::new();
            collect_debug_hit_rects(&vnode, &layout, &mut rects);
            assert_eq!(rects.len(), 1);
        }

        fn fnv1a(bytes: &[u8]) -> u32 {
            let mut hash: u32 = 0x811c9dc5;
            for b in bytes {
                hash ^= *b as u32;
                hash = hash.wrapping_mul(0x01000193);
            }
            hash
        }
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

    pub fn render_vnode_to_raster_png_with_scale(
        _vnode: &VNode,
        _sheet: &Stylesheet,
        _width: i32,
        _height: i32,
        _scale_factor: f32,
    ) -> Result<Vec<u8>, String> {
        Err("skia-native feature not enabled".into())
    }
}

pub use skia_impl::render_vnode_to_raster_png;
pub use skia_impl::render_vnode_to_raster_png_with_scale;
