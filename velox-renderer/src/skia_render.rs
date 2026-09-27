//! Lightweight Skia raster renderer helpers (Phase 1 minimal implementation).
//!
//! Provides a small helper to render a `VNode` into a raster PNG byte buffer.
//! This file is intentionally minimal for Phase 1: it draws element background
//! rectangles (via inline `style` attr parsing) and placeholders for text.
//!
#![allow(unused)]

use velox_dom::VNode;
use velox_dom::text_wrap::MeasuredText;
use velox_style::{Stylesheet, apply_with_cascade};

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
        bold: bool,
        line_height: f32,
        nowrap: bool,
        ellipsis: bool,
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
        let value = value.trim();
        let hex = value.strip_prefix('#');
        if let Some(hex) = hex {
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
        }
        // Try rgb(r, g, b) and rgba(r, g, b, a) formats
        if let Some(inner) = value
            .strip_prefix("rgb(")
            .or_else(|| value.strip_prefix("rgba("))
        {
            let inner = inner.trim_end_matches(')').trim();
            let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
            if parts.len() >= 3
                && let (Ok(r), Ok(g), Ok(b)) = (
                    parts[0].parse::<u8>(),
                    parts[1].parse::<u8>(),
                    parts[2].parse::<u8>(),
                )
            {
                let a = if parts.len() >= 4 {
                    parts[3]
                        .parse::<f32>()
                        .map(|v| (v * 255.0) as u8)
                        .unwrap_or(255)
                } else {
                    255
                };
                return Some(sk::Color::from_argb(a, r, g, b));
            }
        }
        // Try named colors
        let named = match value.trim().to_lowercase().as_str() {
            "red" => Some(sk::Color::from_argb(255, 255, 0, 0)),
            "green" | "lime" => Some(sk::Color::from_argb(255, 0, 128, 0)),
            "blue" => Some(sk::Color::from_argb(255, 0, 0, 255)),
            "white" => Some(sk::Color::from_argb(255, 255, 255, 255)),
            "black" => Some(sk::Color::from_argb(255, 0, 0, 0)),
            "yellow" => Some(sk::Color::from_argb(255, 255, 255, 0)),
            "cyan" => Some(sk::Color::from_argb(255, 0, 255, 255)),
            "magenta" | "fuchsia" => Some(sk::Color::from_argb(255, 255, 0, 255)),
            "gray" | "grey" => Some(sk::Color::from_argb(255, 128, 128, 128)),
            "orange" => Some(sk::Color::from_argb(255, 255, 165, 0)),
            "purple" => Some(sk::Color::from_argb(255, 128, 0, 128)),
            "transparent" => Some(sk::Color::from_argb(0, 0, 0, 0)),
            _ => None,
        };
        if named.is_some() {
            return named;
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
        value
            .strip_suffix("px")
            .and_then(|px| px.trim().parse::<f32>().ok())
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
        Some(ClipInsets {
            top,
            right,
            bottom,
            left,
        })
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
                    bg = parse_color_hex(val).or_else(|| {
                        velox_dom::style::Color::parse(val)
                            .map(|c| sk::Color::from_argb(c.a, c.r, c.g, c.b))
                    });
                } else if key == "border" {
                    border = parse_border_value(val);
                } else if key == "border-radius" {
                    if let Some(px) = parse_px_value(val) {
                        radius = Some(px);
                    }
                } else if key == "overflow" {
                    let v = val.to_ascii_lowercase();
                    overflow_hidden = v == "hidden" || v == "scroll" || v == "auto";
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
                        if let Some(value) = part.strip_prefix("blur(")
                            && let Some(px) = parse_px_value(value.trim())
                        {
                            filters.blur_sigma = Some(px.max(0.0));
                        } else if let Some(value) = part.strip_prefix("brightness(")
                            && let Some(f) = parse_float_value(value.trim())
                        {
                            filters.brightness = Some(f.max(0.0));
                        }
                    }
                } else if key == "z-index"
                    && let Ok(z) = val.parse::<i32>()
                {
                    z_index = z;
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
                if let Some((k, v)) = d.split_once(':')
                    && k.trim() == "z-index"
                    && let Ok(z) = v.trim().parse::<i32>()
                {
                    return z;
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
                    if let Some(color) = parse_color_hex(val).or_else(|| {
                        velox_dom::style::Color::parse(val)
                            .map(|c| sk::Color::from_argb(c.a, c.r, c.g, c.b))
                    }) {
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
                } else if key == "font-weight" {
                    text_style.bold = val.trim().eq_ignore_ascii_case("bold")
                        || val.trim().parse::<u16>().map(|w| w >= 700).unwrap_or(false);
                } else if key == "white-space" {
                    let v = val.trim().to_ascii_lowercase();
                    text_style.nowrap = v == "nowrap" || v == "pre";
                } else if key == "text-overflow" {
                    text_style.ellipsis = val.trim().eq_ignore_ascii_case("ellipsis");
                } else if key == "line-height" {
                    if let Ok(lh) = val.trim().parse::<f32>() {
                        text_style.line_height = lh;
                    } else if let Some(px) = parse_px_value(val.trim()) {
                        text_style.line_height = px / text_style.font_size;
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
        if let Some(sigma) = filters.blur_sigma
            && sigma > 0.0
        {
            paint.set_image_filter(sk::image_filters::blur((sigma, sigma), None, None, None));
        }
        if let Some(brightness) = filters.brightness {
            let b = brightness.max(0.0);
            let matrix: [f32; 20] = [
                b, 0.0, 0.0, 0.0, 0.0, 0.0, b, 0.0, 0.0, 0.0, 0.0, 0.0, b, 0.0, 0.0, 0.0, 0.0, 0.0,
                1.0, 0.0,
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
        let limit = if max_width <= 0.0 {
            f32::INFINITY
        } else {
            max_width
        };
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

    /// Truncate `text` to fit `max_width` px, appending a horizontal ellipsis
    /// (`…`) when it overflows. Used for `text-overflow: ellipsis` on
    /// single-line (`white-space: nowrap` / `pre`) text boxes.
    fn truncate_with_ellipsis(
        text: &str,
        max_width: f32,
        fonts: &mut FontCache,
        family: &str,
        size: f32,
    ) -> String {
        const ELLIPSIS: &str = "\u{2026}";
        if max_width <= 0.0 || text.is_empty() {
            return String::new();
        }
        if fonts.measure_text(family, size, text) <= max_width {
            return text.to_string();
        }
        // Reserve room for the ellipsis itself before fitting prefix chars.
        let avail = (max_width - fonts.measure_text(family, size, ELLIPSIS)).max(0.0);
        let mut cur = String::new();
        for ch in text.chars() {
            let candidate = format!("{cur}{ch}");
            if fonts.measure_text(family, size, &candidate) <= avail {
                cur = candidate;
            } else {
                break;
            }
        }
        format!("{cur}{ELLIPSIS}")
    }

    fn collect_debug_hit_rects(
        vnode: &VNode,
        layout: &velox_dom::layout::LayoutNode,
        out: &mut Vec<velox_dom::layout::Rect>,
    ) {
        match vnode {
            VNode::Text(_) => {}
            VNode::Element {
                tag,
                props,
                children,
                ..
            } => {
                if crate::events::is_hoverable(tag, props) {
                    out.push(layout.rect);
                }
                for child_layout in &layout.children {
                    if child_layout.display_none {
                        continue;
                    }
                    if let Some(src_idx) = child_layout.source_index
                        && let Some(child) = children.get(src_idx)
                    {
                        collect_debug_hit_rects(child, child_layout, out);
                    }
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
            ImageCache {
                images: HashMap::new(),
            }
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
        sheet: &Stylesheet,
        width: i32,
        height: i32,
    ) -> Result<Vec<u8>, String> {
        // Apply stylesheet declarations to inline style attrs before drawing,
        // so backgrounds/colors from the sheet are actually painted.
        let styled = apply_with_cascade(vnode, sheet);
        let vnode = &styled;
        let mut surface = sk::surfaces::raster_n32_premul((width, height))
            .ok_or_else(|| "skia: failed to create raster surface".to_string())?;
        let canvas = surface.canvas();
        canvas.clear(sk::Color::TRANSPARENT);

        let mut fonts = FontCache::new_with_scale(1.0);
        let mut images = ImageCache::new();
        let default_family = fonts.default_family();
        let default_text_style = TextStyle {
            color: sk::Color::from_argb(255, 0, 0, 0),
            align: TextAlign::Left,
            underline: false,
            font_size: 14.0,
            bold: false,
            line_height: 1.2,
            nowrap: false,
            ellipsis: false,
        };
        let mut paints = RenderPaints::new();

        #[allow(clippy::too_many_arguments)]
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
                VNode::Element {
                    props, children, ..
                } => {
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
                            paints
                                .stroke
                                .set_color(color_with_opacity(border.color, opacity));
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
                            canvas.draw_image_rect(img, None, rect, &paints.image);
                        }
                    }

                    // Naive child layout: stack children vertically
                    let child_count = children.len().max(1);
                    let child_h = rect.height() / (child_count as f32);
                    let did_clip =
                        apply_clips(canvas, rect, clip_rrect, overflow_hidden, clip_inset);
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
                    let mut font = fonts.font(font_family, font_size);
                    if text_style.bold {
                        font.set_embolden(true);
                    }
                    let line_height = font_size * text_style.line_height;
                    let layout_rect =
                        sk::Rect::from_xywh(rect.left, rect.top, rect.width(), rect.height());
                    let lines = if text_style.ellipsis {
                        // Single-line truncated with an ellipsis to the text box width.
                        let single = truncate_with_ellipsis(
                            t.as_str(),
                            layout_rect.width(),
                            fonts,
                            font_family,
                            font_size,
                        );
                        let single_w = fonts.measure_text(font_family, font_size, &single);
                        vec![(single, single_w)]
                    } else {
                        layout_text_lines(
                            t.as_str(),
                            container_rect.width(),
                            fonts,
                            font_family,
                            font_size,
                        )
                    };
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
                            TextAlign::Center => {
                                align_rect.left + (align_rect.width() - line_w) * 0.5
                            }
                            TextAlign::Right => {
                                (align_rect.right - line_w - padding).max(align_rect.left + padding)
                            }
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

    /// Render `vnode` into a raw RGBA8888 byte buffer (premultiplied, opaque
    /// alpha) of size `width * height * 4`. Useful for pixel-level assertions
    /// in tests without decoding a PNG.
    pub fn render_vnode_to_rgba(
        vnode: &VNode,
        sheet: &Stylesheet,
        width: i32,
        height: i32,
    ) -> Result<Vec<u8>, String> {
        let styled = apply_with_cascade(vnode, sheet);
        let vnode = &styled;
        let mut surface = crate::skia_surface::SkiaSurface::new_raster(width, height)?;
        velox_dom::text_wrap::set_current_scale(surface.scale_factor());
        velox_dom::text_wrap::set_skia_measurer(measure_text);
        let layout = velox_dom::layout::compute_layout(vnode, width, height);
        render_frame(&mut surface, vnode, &layout, sheet)?;

        let info = sk::ImageInfo::new(
            (width, height),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Premul,
            None,
        );
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        if !surface.read_pixels(&info, &mut rgba, (width * 4) as usize, (0, 0)) {
            return Err("skia: read_pixels failed".to_string());
        }
        Ok(rgba)
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
        velox_dom::text_wrap::set_current_scale(scale_factor);
        velox_dom::text_wrap::set_skia_measurer(measure_text);
        // The scale helper owns the same one-pass cascade as the unscaled
        // helpers. Layout and paint both consume this already-styled tree.
        let styled = apply_with_cascade(vnode, sheet);
        let vnode = &styled;
        let layout = velox_dom::layout::compute_layout(vnode, width, height);
        render_frame(&mut surface, vnode, &layout, sheet)?;
        surface.encode_png()
    }

    /// Minimal FontCache for mapping sizes to `skia_safe::Font`.
    /// DPI-aware: re-rasters fonts at device pixels (hinting) via scale-factor snapping.
    pub struct FontCache {
        typefaces: HashMap<String, sk::Typeface>,
        fonts: HashMap<FontKey, sk::Font>,
        default_family: String,
        scale: f32,
    }

    impl FontCache {
        /// Attempt to load a system font or bundled fallback fonts. Defaults to scale 1.0.
        pub fn new() -> Self {
            Self::new_with_scale(1.0)
        }

        /// Create a FontCache that re-rasters at `scale` device pixels.
        pub fn new_with_scale(scale: f32) -> Self {
            let s = if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            };
            let default_family = "default".to_string();
            let mut typefaces = HashMap::new();
            if let Some(tf) = load_default_typeface() {
                typefaces.insert(default_family.clone(), tf);
            }
            FontCache {
                typefaces,
                fonts: HashMap::new(),
                default_family,
                scale: s,
            }
        }

        /// Update scale (e.g. on ScaleFactorChanged) — clears cache so glyphs
        /// are re-rastered at new device pixels (prevents blur).
        pub fn set_scale_factor(&mut self, scale: f32) {
            let s = if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            };
            if (self.scale - s).abs() > f32::EPSILON {
                self.scale = s;
                self.fonts.clear();
            }
        }

        pub fn scale_factor(&self) -> f32 {
            self.scale
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

        /// Snap a logical font size to the nearest physical pixel row so that
        /// `(logical * scale).round()` is integer device pixels. This ensures
        /// glyph hinting lands on device pixels at fractional scales (1.25/1.5)
        /// instead of 0.25px subpixel blur after `canvas.scale(scale)`.
        #[inline]
        fn snapped_size(&self, logical_size: f32) -> f32 {
            if self.scale == 1.0 {
                return logical_size;
            }
            let device = (logical_size * self.scale).round().max(1.0);
            device / self.scale
        }

        /// Return a `skia_safe::Font` at the requested `size` and `family`.
        /// Size is DPI-snapped to device pixels for `self.scale`.
        pub fn font(&mut self, family: &str, size: f32) -> sk::Font {
            let snapped = self.snapped_size(size);
            let size_key = (snapped * 100.0).round() as u32;
            // Include scale in key implicitly via snapped value; also clear on scale change.
            let key = FontKey {
                family: family.to_string(),
                size_key,
            };
            if let Some(font) = self.fonts.get(&key) {
                return font.clone();
            }
            let font = if let Some(tf) = self.get_or_load_family(family) {
                sk::Font::new(tf, snapped)
            } else {
                let mut f = sk::Font::default();
                f.set_size(snapped);
                f
            };
            self.fonts.insert(key, font.clone());
            font
        }

        /// Measure the width (in px) of `text` rendered at `size` using the cached typeface.
        /// Measurement is at DPI-snapped size so layout and render agree (no wrap mismatch).
        pub fn measure_text(&mut self, family: &str, size: f32, text: &str) -> f32 {
            self.measure_run(family, size, text).width
        }

        /// Measure `text`'s advance width *and* its vertical extent.
        ///
        /// `font.measure_str` already returned the ink bounds and this code threw
        /// them away (`_bounds`); they are the vertical answer. Skia reports those
        /// bounds with the baseline at y = 0, so the run's reach above the
        /// baseline is `-top` and its reach below is `bottom`.
        ///
        /// A run with no ink — a blank one, or a space — measures a zero-height
        /// rectangle, which is a true statement about ink and a useless one about
        /// a line box, so that case is left for the seam to answer from the
        /// documented approximation rather than propagated. `.max(0.0)` also maps
        /// a NaN bound to 0.0, which lands in that same case.
        ///
        /// Both numbers are in LOGICAL px, not device px. `font()` builds the
        /// `sk::Font` at `snapped_size(size)`, which is `device / self.scale`, so
        /// `measure_str` reports at the logical size the caller asked for — the
        /// same units `measure_heuristic` and `velox_dom`'s own snap point work in,
        /// and the same units the width has always been in. Dividing these by
        /// `scale` would be the bug, not the fix: it would scale every line box by
        /// the device factor. There is one rounding authority in this project and it
        /// is not here, so nothing below rounds.
        pub fn measure_run(&mut self, family: &str, size: f32, text: &str) -> MeasuredText {
            let font = self.font(family, size);
            let mut p = sk::Paint::default();
            p.set_anti_alias(true);
            let (w, bounds) = font.measure_str(text, Some(&p));
            MeasuredText {
                width: w,
                ascent: (-bounds.top).max(0.0),
                descent: bounds.bottom.max(0.0),
            }
        }
    }

    /// Public measure helper for layout unify: snapped size, scale-aware.
    /// Consumes: text, font_size (logical), font_family, scale -> logical px width
    /// (snapped) plus the run's ascent and descent.
    pub fn measure_text(text: &str, font_size: f32, font_family: &str, scale: f32) -> MeasuredText {
        let snapped = if scale.is_finite() && scale > 0.0 {
            (font_size * scale).round() / scale
        } else {
            font_size
        };
        let mut fc = FontCache::new_with_scale(scale);
        fc.measure_run(font_family, snapped, text)
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
            if let Ok(bytes) = fs::read(p)
                && let Some(tf) = font_mgr.new_from_data(&bytes, None)
            {
                return Some(tf);
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
        let preferred_families = [
            "DejaVu Sans",
            "Noto Sans",
            "Sans",
            "Arial",
            "Liberation Sans",
        ];
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

    /// Render a VNode tree into an existing `SkiaSurface` using a precomputed layout.
    /// `layout` must be the result of `compute_layout(vnode, logical_w, logical_h)` where
    /// `logical_w,logical_h` were obtained via `logical_size(physical, scale)` — single rounding point.
    /// Hit-test and render must share the same `Viewport` (physical/logical/scale) so that
    /// fractional scales (1.25/1.5) have identical snapped edges and clicks hit the rendered pixel.
    ///
    /// `sheet` is retained for the renderer API, but paint-time code reads the
    /// already-cascaded inline `style` attributes on `vnode`; it performs no
    /// second style application or paint-time sheet lookup.
    pub fn render_frame(
        surface: &mut crate::skia_surface::SkiaSurface,
        vnode: &VNode,
        layout_root: &velox_dom::layout::LayoutNode,
        _sheet: &Stylesheet,
    ) -> Result<(), String> {
        let scale = surface.scale_factor().max(1.0);
        // Ensure layout text measure uses same Skia snapped scale (unified).
        velox_dom::text_wrap::set_current_scale(scale);
        velox_dom::text_wrap::set_skia_measurer(measure_text);

        let canvas = surface.canvas();
        canvas.clear(sk::Color::TRANSPARENT);
        canvas.save();
        // R-H3: single rounding physical=(logical*scale).round() keeping both; don't round-trip.
        // We do canvas.scale(scale) here so logical rects map to device pixels; font re-raster
        // below snaps sizes to physical pixels to avoid 0.25px blur at 1.25/1.5.
        canvas.scale((scale, scale));

        let mut fonts = FontCache::new_with_scale(scale);
        let mut images = ImageCache::new();
        let default_text_style = TextStyle {
            color: sk::Color::from_argb(255, 0, 0, 0),
            align: TextAlign::Left,
            underline: false,
            font_size: 14.0,
            bold: false,
            line_height: 1.2,
            nowrap: false,
            ellipsis: false,
        };
        let default_family = fonts.default_family();
        let mut paints = RenderPaints::new();

        #[allow(clippy::too_many_arguments)]
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
                VNode::Element {
                    props,
                    children,
                    tag,
                    ..
                } => {
                    // Check visibility:hidden
                    if let Some(s) = props.attrs.get("style")
                        && (s.contains("visibility: hidden") || s.contains("visibility:hidden"))
                    {
                        return;
                    }
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
                            paints
                                .stroke
                                .set_color(color_with_opacity(border.color, opacity));
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
                            canvas.draw_image_rect(img, None, rect, &paints.image);
                        }
                    }

                    // Handle <input> elements - draw a text field
                    if tag == "input" {
                        let input_type = props
                            .attrs
                            .get("type")
                            .map(|s| s.as_str())
                            .unwrap_or("text");
                        let value = props.attrs.get("value").map(|s| s.as_str()).unwrap_or("");

                        // Draw input background (white or light gray)
                        let input_bg = sk::Color::from_argb(255, 255, 255, 255);
                        let border_color = sk::Color::from_argb(255, 200, 200, 200);

                        let rect = sk::Rect::from_xywh(
                            layout.rect.x as f32,
                            layout.rect.y as f32,
                            layout.rect.w as f32,
                            layout.rect.h as f32,
                        );

                        if input_type == "checkbox" {
                            // Draw checkbox
                            let size = rect.width().min(rect.height()).min(18.0);
                            let check_rect = sk::Rect::from_xywh(
                                rect.left + (rect.width() - size) / 2.0,
                                rect.top + (rect.height() - size) / 2.0,
                                size,
                                size,
                            );
                            // Background
                            paints.fill.set_color(input_bg);
                            canvas.draw_rect(check_rect, &paints.fill);
                            // Border
                            paints.stroke.set_stroke_width(1.0);
                            paints.stroke.set_color(border_color);
                            canvas.draw_rect(check_rect, &paints.stroke);
                            // Check mark if checked
                            if value == "true" || value == "checked" {
                                paints
                                    .fill
                                    .set_color(sk::Color::from_argb(255, 52, 120, 246));
                                let inset = size * 0.2_f32;
                                let inner = sk::RRect::new_rect_xy(
                                    sk::Rect::from_xywh(
                                        check_rect.left + inset,
                                        check_rect.top + inset,
                                        size - inset * 2.0,
                                        size - inset * 2.0,
                                    ),
                                    2.0,
                                    2.0,
                                );
                                canvas.draw_rrect(inner, &paints.fill);
                            }
                        } else {
                            // Text input - draw border and value
                            let input_rect = sk::Rect::from_xywh(
                                rect.left + 1.0,
                                rect.top + 1.0,
                                (rect.width() - 2.0).max(0.0),
                                (rect.height() - 2.0).max(0.0),
                            );
                            paints.fill.set_color(input_bg);
                            canvas.draw_rect(input_rect, &paints.fill);
                            paints.stroke.set_stroke_width(1.0);
                            paints.stroke.set_color(border_color);
                            canvas.draw_rect(input_rect, &paints.stroke);
                            // Draw value text
                            if !value.is_empty() {
                                paints.text.set_color(sk::Color::from_argb(255, 0, 0, 0));
                                let font_size = text_style.font_size;
                                let font = fonts.font(font_family, font_size);
                                let ty = input_rect.top
                                    + font_size
                                    + (input_rect.height() - font_size) / 2.0;
                                let _ = canvas.draw_str(
                                    value,
                                    (input_rect.left + 4.0, ty),
                                    &font,
                                    &paints.text,
                                );
                            }
                        }
                    }

                    // Render children in order using their layout nodes
                    let rect = sk::Rect::from_xywh(
                        layout.rect.x as f32,
                        layout.rect.y as f32,
                        layout.rect.w as f32,
                        layout.rect.h as f32,
                    );
                    let did_clip =
                        apply_clips(canvas, rect, clip_rrect, overflow_hidden, clip_inset);
                    // Scroll offset is applied in the layout itself (children rects
                    // are shifted by events::apply_scroll_offsets so render and
                    // hit-testing agree) — content renders at content_y - scroll_y.
                    let mut ordered: Vec<(i32, usize)> = layout
                        .children
                        .iter()
                        .enumerate()
                        .filter_map(|(i, ln)| {
                            if ln.display_none {
                                return None;
                            }
                            Some((ln.z_index, i))
                        })
                        .collect();
                    ordered.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
                    for (_, layout_idx) in ordered {
                        if let Some(child_layout) = layout.children.get(layout_idx)
                            && let Some(src_idx) = child_layout.source_index
                            && let Some(child) = children.get(src_idx)
                        {
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
                    // Paint scrollbar thumb when scrollable && max > 0.
                    // (Headless-provable via render_vnode_to_rgba pixel tests.)
                    if layout.scrollable && layout.max_scroll_y > 0 {
                        let scrollbar_w = 8.0;
                        let track_x = rect.right - scrollbar_w - 2.0;
                        let track_y = rect.top + 2.0;
                        let track_h = (rect.height() - 4.0).max(0.0);
                        // Thumb size ~ viewport^2 / scrollHeight, floored at 20px
                        // (or track_h for tiny containers — clamp(min, max) must
                        // never see min > max) and capped at the track height.
                        let min_thumb = 20.0_f32.min(track_h);
                        let thumb_h = ((rect.height() * rect.height())
                            / (layout.scroll_height as f32).max(1.0))
                        .clamp(min_thumb, track_h);
                        let max_y = layout.max_scroll_y as f32;
                        let thumb_y = if max_y > 0.0 {
                            track_y + (layout.scroll_y as f32 / max_y) * (track_h - thumb_h)
                        } else {
                            track_y
                        };
                        let mut sb_paint = sk::Paint::default();
                        sb_paint.set_anti_alias(true);
                        sb_paint.set_color(sk::Color::from_argb(120, 100, 100, 100));
                        let thumb_rect =
                            sk::Rect::from_xywh(track_x, thumb_y, scrollbar_w, thumb_h);
                        let rrect = sk::RRect::new_rect_xy(thumb_rect, 4.0, 4.0);
                        canvas.draw_rrect(rrect, &sb_paint);
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
                    let mut font = fonts.font(font_family, font_size);
                    if text_style.bold {
                        font.set_embolden(true);
                    }
                    let line_height = font_size * text_style.line_height;
                    let layout_rect = sk::Rect::from_xywh(
                        layout.rect.x as f32,
                        layout.rect.y as f32,
                        layout.rect.w as f32,
                        layout.rect.h as f32,
                    );
                    let lines = if text_style.ellipsis {
                        // Single-line truncated with an ellipsis to the text box width.
                        let single = truncate_with_ellipsis(
                            t.as_str(),
                            layout_rect.width(),
                            fonts,
                            font_family,
                            font_size,
                        );
                        let single_w = fonts.measure_text(font_family, font_size, &single);
                        vec![(single, single_w)]
                    } else {
                        layout_text_lines(
                            t.as_str(),
                            container_rect.width(),
                            fonts,
                            font_family,
                            font_size,
                        )
                    };
                    let align_rect = if layout_rect.width() >= container_rect.width() - 0.5 {
                        container_rect
                    } else {
                        layout_rect
                    };
                    let text_bottom =
                        (layout.rect.y as f32) + (layout.rect.h as f32).max(line_height);
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
                            TextAlign::Center => {
                                align_rect.left + (align_rect.width() - line_w) * 0.5
                            }
                            TextAlign::Right => {
                                (align_rect.right - line_w - padding).max(align_rect.left + padding)
                            }
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
                    log::debug!("hit rect: x={} y={} w={} h={}", r.x, r.y, r.w, r.h);
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

        // ------------------------------------------------------------------
        // The measurement seam, against a real font backend.
        //
        // These need `--features skia-native`; without it the crate's fallback
        // measurer is registered instead and there is no font to measure.
        // ------------------------------------------------------------------

        const SEAM_FAMILY: &str = "system-ui";
        const SEAM_SIZE: f32 = 16.0;

        /// Heights of the text line boxes `vnode` lays out, in order.
        fn seam_line_heights(vnode: &VNode) -> Vec<i32> {
            let laid = velox_dom::layout::compute_layout(vnode, 300, 300);
            let mut out = vec![laid.rect.h];
            for child in &laid.children {
                if child.children.is_empty() {
                    out.push(child.rect.h);
                } else {
                    for grandchild in &child.children {
                        out.push(grandchild.rect.h);
                    }
                }
            }
            out
        }

        /// A div whose only child is one text node, wrapping only at explicit
        /// newlines, so each line's content is known exactly.
        fn seam_pre_text_div(text: &str) -> VNode {
            let style = format!("width:240px;font-size:{SEAM_SIZE}px;white-space:pre");
            h(
                "div",
                vec![("style", style.as_str())],
                vec![VNode::Text(text.to_string())],
            )
        }

        #[test]
        fn measured_run_vertical_metrics_come_from_the_glyphs_not_a_constant() {
            let caps = measure_text("H", SEAM_SIZE, SEAM_FAMILY, 1.0);
            let lower = measure_text("x", SEAM_SIZE, SEAM_FAMILY, 1.0);
            let mixed = measure_text("Hg", SEAM_SIZE, SEAM_FAMILY, 1.0);

            // The load-bearing assertion. A fabricated `descent = 0.4em` gives
            // every run the same descent, including a capital, which has none.
            assert_eq!(
                caps.descent,
                0.0,
                "a capital has no descender in any real face; got {} \
                 (0.4em would be {})",
                caps.descent,
                SEAM_SIZE * 0.4
            );
            assert!(
                mixed.descent > 0.0,
                "a run containing a descender must report one, got {}",
                mixed.descent
            );
            assert!(
                mixed.descent > caps.descent,
                "descent must depend on the glyphs, not the font"
            );

            // Likewise a fabricated `ascent = 0.8em` gives every run the same
            // ascent, but cap height is taller than x-height in every real face.
            assert!(
                caps.ascent > lower.ascent,
                "cap height ({}) must exceed x-height ({}); 0.8em would make both {}",
                caps.ascent,
                lower.ascent,
                SEAM_SIZE * 0.8
            );
            assert!(
                mixed.ascent > lower.ascent,
                "a run with a capital must reach higher than one without"
            );

            // In a Latin face the ascent dominates the descent.
            assert!(
                mixed.ascent > mixed.descent,
                "ascent {} must exceed descent {} for Latin text",
                mixed.ascent,
                mixed.descent
            );
            assert!(
                mixed.line_extent() > lower.line_extent(),
                "the total extent must follow the content: {} vs {}",
                mixed.line_extent(),
                lower.line_extent()
            );
            assert!(
                mixed.width != SEAM_SIZE * 0.5 * 2.0,
                "width {} is exactly the 0.5em-per-char heuristic, so this is not \
                 a real proportional font",
                mixed.width
            );
        }

        #[test]
        fn line_box_height_under_real_metrics_follows_the_run_in_it() {
            velox_dom::text_wrap::set_skia_measurer(measure_text);
            let heights = seam_line_heights(&seam_pre_text_div("Hg\nxxx"));
            assert_eq!(heights.len(), 3, "root, div, two line boxes: {heights:?}");

            for (line_text, got) in [("Hg", heights[1]), ("xxx", heights[2])] {
                let run = measure_text(line_text, SEAM_SIZE, SEAM_FAMILY, 1.0);
                assert_eq!(
                    got,
                    run.line_extent().round() as i32,
                    "line {line_text:?} should be the round of its own run's extent \
                     {} + {} = {}",
                    run.ascent,
                    run.descent,
                    run.line_extent()
                );
            }
            assert_ne!(
                heights[1], heights[2],
                "two lines of different content must get different heights: {heights:?}"
            );
            let nominal = (SEAM_SIZE * 1.2).round() as i32;
            assert!(
                heights.iter().all(|h| *h != nominal),
                "no line box may still be the fixed 1.2em multiplier: {heights:?}"
            );
        }

        #[test]
        fn a_blank_run_does_not_collapse_its_line_box_under_real_metrics() {
            velox_dom::text_wrap::set_skia_measurer(measure_text);
            // A space really has no ink, so Skia reports a zero-height rectangle
            // for it. Passing that on as a line box would be a zero-height line.
            let blank = measure_text(" ", SEAM_SIZE, SEAM_FAMILY, 1.0);
            assert_eq!(
                blank.line_extent(),
                0.0,
                "precondition: a blank run really does measure zero ink"
            );
            let heights = seam_line_heights(&seam_pre_text_div(" "));
            assert!(
                heights.iter().all(|h| *h > 0),
                "a line box collapsed to {heights:?}"
            );
            assert_eq!(
                heights[1],
                (SEAM_SIZE * 1.2).round() as i32,
                "with no usable vertical measurement the documented approximation applies"
            );
        }

        /// `measure_run` passes Skia's bounds through unmodified.
        ///
        /// Every other test in this module is self-consistent: it asks the seam
        /// what a run measures and then checks that the layout path used that
        /// same number, so a uniform error applied to the measured VALUE — a
        /// stray scale factor, a flipped sign, the descent taken from
        /// `bounds.top` — is invisible to all of them. Falsification confirmed
        /// exactly that: scaling `measure_run`'s ascent by 0.7 left all 30 of
        /// these tests green.
        ///
        /// So this one does not go through `measure_run`'s callers. It takes the
        /// typeface, calls `measure_str` itself, and requires the production
        /// path to have extracted precisely the same numbers. The duplication is
        /// deliberate and is the whole point: independence from the code under
        /// test. The typeface and DPI snap are shared with production because
        /// those are not what is under test.
        #[test]
        fn measure_run_passes_skias_bounds_through_unmodified() {
            let mut cache = FontCache::new_with_scale(1.0);
            for text in ["Hg", "H", "x", "W", "Ag", " "] {
                for size in [16.0f32, 33.0] {
                    let (w, bounds) = {
                        let font = cache.font(SEAM_FAMILY, size);
                        let mut paint = sk::Paint::default();
                        paint.set_anti_alias(true);
                        font.measure_str(text, Some(&paint))
                    };
                    let run = cache.measure_run(SEAM_FAMILY, size, text);
                    assert_eq!(run.width, w, "advance width for {text:?} at {size}px");
                    assert_eq!(
                        run.ascent,
                        (-bounds.top).max(0.0),
                        "ascent for {text:?} at {size}px: Skia puts the baseline at y=0 \
                         with the top negative above it, so the ascent is -top"
                    );
                    assert_eq!(
                        run.descent,
                        bounds.bottom.max(0.0),
                        "descent for {text:?} at {size}px: below the baseline is positive"
                    );
                }
            }
        }

        /// `at()`'s bare `VNode::Text` arm, under real font metrics. This is
        /// the one path R-5a changed that the velox-dom tests cannot reach: they
        /// all install a synthetic measurer, so what is verified here is that a
        /// real font backend's metrics actually arrive at the branch that uses
        /// them.
        #[test]
        fn a_bare_text_node_is_as_tall_as_the_run_in_it_under_real_metrics() {
            velox_dom::text_wrap::set_skia_measurer(measure_text);
            let bare = |text: &str| {
                velox_dom::layout::compute_layout(&VNode::Text(text.to_string()), 300, 300)
                    .rect
                    .h
            };
            let caps = bare("Hg");
            let x_only = bare("xxx");
            assert_ne!(
                caps, x_only,
                "two runs at {SEAM_SIZE}px must get different heights from real \
                 metrics, or the bare-text path is not using them ({caps} vs {x_only})"
            );
            for (text, actual) in [("Hg", caps), ("xxx", x_only)] {
                let run = measure_text(text, SEAM_SIZE, SEAM_FAMILY, 1.0);
                assert_eq!(
                    actual,
                    run.line_extent().round() as i32,
                    "a bare `{text}` node must be as tall as the run it holds, \
                     which measures {} up and {} down",
                    run.ascent,
                    run.descent
                );
                assert_ne!(
                    actual, 19,
                    "`{text}` must not get the old 1.2em height; routing this path \
                     through the seam is the point of the change"
                );
            }
        }

        #[test]
        fn a_nan_vertical_bound_cannot_escape_the_seam() {
            // `f32::max` maps a NaN bound to 0.0, which is the same
            // zero-extent case the seam already handles. Asserted here because
            // the sanitising happens in the measurer, far from the guard that
            // consumes it, and the two must stay in step.
            assert_eq!(f32::NAN.max(0.0), 0.0);
            assert_eq!((-f32::NAN).max(0.0), 0.0);
        }

        #[test]
        #[ignore = "requires skia-native feature and GPU hardware"]
        fn render_overflow_hidden_clips_children() {
            let vnode = h(
                "div",
                vec![(
                    "style",
                    "background-color:#FFFFFF;overflow:hidden;width:40px;height:40px",
                )],
                vec![h(
                    "div",
                    vec![("style", "background-color:#FF0000;width:40px;height:80px")],
                    vec![],
                )],
            );

            let mut surface =
                crate::skia_surface::SkiaSurface::new_raster(64, 64).expect("surface");
            let layout = velox_dom::layout::compute_layout(&vnode, 64, 64);
            render_frame(&mut surface, &vnode, &layout, &Stylesheet::default()).expect("render");
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
        #[ignore = "requires skia-native feature and GPU hardware"]
        fn render_z_index_overlap_checksum() {
            let vnode = h(
                "div",
                vec![("style", "background-color:#FFFFFF;width:64px;height:64px")],
                vec![
                    h(
                        "div",
                        vec![(
                            "style",
                            "background-color:#FF0000;width:40px;height:40px;z-index:1",
                        )],
                        vec![],
                    ),
                    h(
                        "div",
                        vec![(
                            "style",
                            "background-color:#0000FF;width:40px;height:40px;margin-top:-20px;z-index:0",
                        )],
                        vec![],
                    ),
                ],
            );

            let mut surface =
                crate::skia_surface::SkiaSurface::new_raster(64, 64).expect("surface");
            let layout = velox_dom::layout::compute_layout(&vnode, 64, 64);
            render_frame(&mut surface, &vnode, &layout, &Stylesheet::default()).expect("render");
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

        #[test]
        fn truncate_ellipsis_fits_short_text_unchanged() {
            let mut fc = FontCache::new();
            let family = fc.default_family();
            let size = 14.0;
            let text = "Hi";
            let out = truncate_with_ellipsis(text, 500.0, &mut fc, &family, size);
            assert_eq!(out, "Hi");
        }

        #[test]
        fn truncate_ellipsis_appends_ellipsis_on_overflow() {
            let mut fc = FontCache::new();
            let family = fc.default_family();
            let size = 14.0;
            let text = "This is a very long line of text that will definitely not fit";
            let out = truncate_with_ellipsis(text, 60.0, &mut fc, &family, size);
            assert!(out.ends_with('\u{2026}'), "expected ellipsis, got: {out:?}");
            let w = fc.measure_text(&family, size, &out);
            assert!(w <= 60.0, "truncated width {w} exceeds 60px: {out:?}");
            assert!(out.len() < text.len(), "expected truncation, got: {out:?}");
        }

        #[test]
        fn truncate_ellipsis_handles_zero_width() {
            let mut fc = FontCache::new();
            let family = fc.default_family();
            let out = truncate_with_ellipsis("anything", 0.0, &mut fc, &family, 14.0);
            assert!(out.is_empty());
        }

        #[test]
        fn parse_text_style_detects_ellipsis_and_nowrap() {
            let base = TextStyle {
                color: sk::Color::from_argb(255, 0, 0, 0),
                align: TextAlign::Left,
                underline: false,
                font_size: 14.0,
                bold: false,
                line_height: 1.2,
                nowrap: false,
                ellipsis: false,
            };
            let (s, _f) =
                parse_text_style("white-space:nowrap;text-overflow:ellipsis", base, "default");
            assert!(s.nowrap, "expected nowrap");
            assert!(s.ellipsis, "expected ellipsis");
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

    /// Heuristic fallback when skia-native not compiled — still snapped, but uses
    /// fixed 0.5 ratio so divergence test (0.6) triggers while wrap parity holds
    /// via same fallback in both crates headless.
    ///
    /// UNREACHABLE TODAY: `lib.rs` gates this entire file behind
    /// `feature = "skia-native"`, so this `cfg(not(...))` module is in no current
    /// build. The non-Skia width that *is* compiled is `text.rs`'s
    /// `measure_with_scale` fallback branch, which evaluates the same expression
    /// below. This copy is kept correct rather than deleted so ungating the module
    /// cannot hand layout a half-updated seam.
    ///
    /// There is no font backend here, so the vertical half is the approximation
    /// `FontMetrics::heuristic_vertical` documents, not a measurement. It is
    /// deliberately not zero: a zero ascent and descent would collapse every line
    /// box on this path to zero height, which is a silent, catastrophic wrong
    /// answer on the path that is easiest to reach in a test.
    pub fn measure_text(
        text: &str,
        font_size: f32,
        _font_family: &str,
        scale: f32,
    ) -> MeasuredText {
        let snapped = if scale.is_finite() && scale > 0.0 {
            (font_size * scale).round() / scale
        } else {
            font_size
        };
        // Use slightly different ratio than old 0.6 to prove divergence (>0.5) but
        // stable for wrap parity when skia not available.
        let (ascent, descent) = velox_dom::layout::FontMetrics::heuristic_vertical(snapped);
        MeasuredText {
            width: snapped * 0.5 * text.chars().count() as f32,
            ascent,
            descent,
        }
    }
}

pub use skia_impl::measure_text;
pub use skia_impl::render_vnode_to_raster_png;
pub use skia_impl::render_vnode_to_raster_png_with_scale;
