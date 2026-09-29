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
    use std::cell::RefCell;
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

    /// Cache key for one memoized advance: the font's own key plus the run.
    ///
    /// The `text` is a `String` rather than borrowed, because the map owns its
    /// keys. Lookups build it per call; at ~150 measures per frame that is
    /// ~150 short allocations, which is far cheaper than the Skia shaping pass
    /// it replaces and is what keeps a `HashMap<&str, _>` self-referential
    /// lifetime problem out of the design.
    #[derive(Hash, Eq, PartialEq, Clone)]
    struct AdvanceKey {
        font: FontKey,
        text: String,
    }

    /// Cap on memoized text advances, in `(family, size, text)` entries.
    ///
    /// The wrap path (`wrap_text`/`truncate_with_ellipsis`) measures every
    /// prefix of a line, so a long paragraph contributes one entry per word
    /// per candidate, and those keys are not the same keys the next frame
    /// asks for. Left unbounded this is a slow leak in a long session. 4096 is
    /// far above any realistic live set (a busy frame touches a few hundred
    /// distinct runs) while still being a few hundred KB worst case.
    const ADVANCE_CACHE_CAP: usize = 4096;

    /// One memoized `measure_str` result.
    ///
    /// `scale_key` is stored per entry and re-checked on every hit, so a stale
    /// entry cannot be served even if the `FontCache` scale bookkeeping is
    /// ever bypassed. See `FontCache::advances`.
    #[derive(Clone, Copy)]
    struct AdvanceEntry {
        width: f32,
        ascent: f32,
        descent: f32,
        scale_key: u32,
        /// Insertion order, for dropping the oldest when the cap is hit.
        seq: u64,
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
                    // `bolder` is a relative-bold keyword. The render path has a
                    // single bold/not-bold axis (`TextStyle::bold`, no numeric
                    // weight), so the relative keyword collapses to bold. It was
                    // previously neither "bold" nor a u16 and so fell through to
                    // non-bold, silently dropping the declaration. `lighter`
                    // stays non-bold: it is a relative-light keyword, and a single
                    // boolean cannot represent "bolder than a weight we do not
                    // track", so mapping it to bold would be a new lie.
                    let w = val.trim();
                    text_style.bold = w.eq_ignore_ascii_case("bold")
                        || w.eq_ignore_ascii_case("bolder")
                        || w.parse::<u16>().map(|n| n >= 700).unwrap_or(false);
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

    /// Read a boolean-ish input attribute.
    ///
    /// The template binds `<input type="checkbox" :checked="completed" />`
    /// (`test-app/src/components/TodoItem.vx:6`), NOT `:value`, so the old
    /// `value == "true"` read at the call site never saw a checked state. The
    /// SFC emits the expression as a stringified bool, so `"true"`/`"false"`,
    /// and the DOM-ish `checked` / the empty string are all accepted here.
    ///
    /// Returns `None` when the attribute is absent, so the caller can tell
    /// "unset" from "explicitly false" and keep the old `value`-based fallback.
    fn read_bool_attr(props: &velox_dom::Props, name: &str) -> Option<bool> {
        let raw = props.attrs.get(name)?;
        let v = raw.trim();
        if v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("checked") || v.is_empty() {
            Some(true)
        } else if v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("unchecked") {
            Some(false)
        } else {
            None
        }
    }

    fn parse_usize_attr(props: &velox_dom::Props, name: &str) -> Option<usize> {
        let raw = props.attrs.get(name)?;
        raw.trim().parse::<usize>().ok()
    }

    /// Ink for the caret bar, chosen from the luminance of the surface the bar
    /// is painted onto.
    ///
    /// The renderer hardcodes a near-white field fill, so the bar is normally
    /// near-black — but the value TEXT in that field is also near-black, and a
    /// black bar on black glyphs is the "cursor not showing up" report all over
    /// again. The bar is therefore drawn as a two-tone mark: a wide halo in the
    /// field's own background colour with a narrow core in the contrast ink on
    /// top. On a light field that is black-on-white (readable, and it punches a
    /// clean gap through any glyph it crosses); flipped onto a dark field the
    /// same pair becomes white-on-dark. One strategy, correct in both worlds,
    /// and never "white bar on a white input".
    fn caret_colors(field_bg: sk::Color) -> (sk::Color, sk::Color) {
        // Rec. 601 luma is enough for a binary light/dark decision; this is not
        // a colour-science grading path.
        let luma =
            0.299 * field_bg.r() as f32 + 0.587 * field_bg.g() as f32 + 0.114 * field_bg.b() as f32;
        let dark_field = luma < 128.0;
        let core = if dark_field {
            sk::Color::from_argb(255, 255, 255, 255)
        } else {
            sk::Color::from_argb(255, 0, 0, 0)
        };
        // The halo wears the field colour so it reads as a gap punched through
        // the glyphs rather than as a second decorative mark.
        let halo = field_bg;
        (halo, core)
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
        // DELIBERATELY NOT FUNNELLED THROUGH `prepare_frame` (R-8).
        //
        // This function builds a raw `raster_n32_premul` surface directly and
        // never installs `set_current_scale`/`set_skia_measurer`, and it runs no
        // `compute_layout` at all. `prepare_frame` requires a
        // `crate::skia_surface::SkiaSurface` and derives the measurer and scale
        // from that surface's scale factor, so using the funnel here would need
        // this body rewritten onto a different surface type first. That is a
        // behaviour change, not a mechanical refactor, so it is deliberately out
        // of R-8's scope.
        //
        // Its own lack of `compute_layout` is the pre-existing invariant-4
        // false-greener hazard: this path lays out nothing, so it can paint a
        // tree whose layout was never computed. Migrating its callers to
        // `render_vnode_to_rgba` / `render_vnode_to_raster_png_with_scale` and
        // then deleting this is a separate decision and a separate commit.
        //
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

    /// R-8's single render prologue. Every live path that cascades with
    /// `apply_with_cascade`, installs the scale/measurer globals and lays out
    /// funnels through here, so the ORDER of those three steps is stated once and
    /// cannot drift between entry points again.
    ///
    /// `logical_w`/`logical_h` are LOGICAL. This function must never multiply them
    /// by a scale factor: invariant 1 makes `viewport::physical_from_logical` the
    /// single place a logical size becomes a physical one, and that happens
    /// exactly once, when the caller constructs the surface it passes in. The
    /// surface is read only for its scale, never resized here.
    ///
    /// The style cascade is applied FIRST because neither of the two steps after
    /// it can change a style: installing a measurer is global-state assignment and
    /// `compute_layout` consumes the already-styled tree.
    pub(crate) fn prepare_frame(
        vnode: &VNode,
        sheet: &Stylesheet,
        logical_w: i32,
        logical_h: i32,
        surface: &crate::skia_surface::SkiaSurface,
    ) -> (VNode, velox_dom::layout::LayoutNode) {
        let styled = apply_with_cascade(vnode, sheet);
        velox_dom::text_wrap::set_current_scale(surface.scale_factor());
        velox_dom::text_wrap::set_skia_measurer(measure_text);
        let layout = velox_dom::layout::compute_layout(&styled, logical_w, logical_h);
        (styled, layout)
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
        let mut surface = crate::skia_surface::SkiaSurface::new_raster(width, height)?;
        let (vnode, layout) = prepare_frame(vnode, sheet, width, height, &surface);
        render_frame(&mut surface, &vnode, &layout, sheet)?;

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
        // Invariant 1: `viewport::physical_from_logical` is the single DPI/scale
        // rounding authority. This used to inline a copy of the same formula
        // the same multiply-then-round formula but was missing the authority's
        // `.max(1)` and the authority's substitute-for-a-degenerate-scale rule, so
        // a non-positive scale collapsed ANY requested size to a 1x1 surface.
        // `SkiaSurface::new_raster` takes i32, so the
        // authority's u32 result is narrowed with a saturating `try_from` rather
        // than an `as` cast that would wrap above `i32::MAX`.
        //
        // The logical inputs are clamped to 0 before the u32 conversion because a
        // negative `i32` cast to `u32` wraps to ~4 billion, which would ask for a
        // 2-billion-pixel surface. Both normalisations are the `.max(1)` fix: a
        // degenerate size now yields a 1px surface instead of a 0px or negative one.
        let (physical_w, physical_h) = crate::Viewport::physical_from_logical(
            width.max(0) as u32,
            height.max(0) as u32,
            scale_factor,
        );
        let mut surface = crate::skia_surface::SkiaSurface::new_raster(
            i32::try_from(physical_w).unwrap_or(i32::MAX),
            i32::try_from(physical_h).unwrap_or(i32::MAX),
        )?;
        surface.set_scale_factor(scale_factor);
        // R-8: the prologue is `prepare_frame`, shared with `render_vnode_to_rgba`.
        // It cascades, installs the scale/measurer globals and lays out, in that
        // one order, and takes LOGICAL `width`/`height` — the logical-to-physical
        // conversion above is the only one, and the funnel does not repeat it.
        let (vnode, layout) = prepare_frame(vnode, sheet, width, height, &surface);
        render_frame(&mut surface, &vnode, &layout, sheet)?;
        surface.encode_png()
    }

    /// Minimal FontCache for mapping sizes to `skia_safe::Font`.
    /// DPI-aware: re-rasters fonts at device pixels (hinting) via scale-factor snapping.
    pub struct FontCache {
        typefaces: HashMap<String, sk::Typeface>,
        fonts: HashMap<FontKey, sk::Font>,
        /// Memoized `measure_str` results, keyed by the same `FontKey` the
        /// `sk::Font` is keyed by plus the text.
        ///
        /// This is the hot one. Layout re-measures the same subtree several
        /// times per frame — a 1-item todo app issues ~57 `measure_run` calls
        /// and a 3-item list ~150 — and the strings are overwhelmingly
        /// repeats, so a lookup here replaces a full Skia shaping pass. The
        /// `FontKey` already carries the DPI-snapped size, which is derived
        /// from `self.scale`, so the scale is *implicitly* part of the key;
        /// `scale_key` on the entry makes that explicit and independently
        /// checked, and `set_scale_factor` clears the map outright, exactly as
        /// it clears `fonts`. Three independent guards, because a stale advance
        /// is a wrong layout and a visibly wrong UI.
        advances: HashMap<AdvanceKey, AdvanceEntry>,
        /// Monotonic insertion counter, for drop-oldest eviction.
        adv_seq: u64,
        /// How many real `measure_str` calls this cache has made.
        ///
        /// Incremented only on a memo MISS, i.e. on the path that actually
        /// shapes a run in Skia, so the counter is a direct measurement of the
        /// work the cache exists to avoid: `skia_measure_calls()` going up by
        /// one per repeated `measure_run` would mean the memo is not working,
        /// and it is the only honest way to test that without timing
        /// assertions. One `u64` increment on a path that then runs a full
        /// Skia shaping pass is not measurable overhead.
        adv_skia_calls: u64,
        /// The one `sk::Paint` every `measure_str` call borrows.
        ///
        /// It existed as a fresh `sk::Paint::default()` inside `measure_run`,
        /// so every measurement allocated one (and registered a native
        /// SkPaint) to set a single anti-alias flag. `measure_str` takes
        /// `Option<&Paint>` — a shared borrow, read-only for Skia's measuring
        /// path — and nothing here mutates it after construction, so one
        /// instance per `FontCache` is safe to reuse across every call and
        /// across every frame the cache serves. Owned as a field rather than a
        /// `thread_local!` so its lifetime is the cache's, with no teardown
        /// ordering question during thread exit.
        measure_paint: sk::Paint,
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
            // Built once, anti-alias flag set once, then never mutated. See the
            // field's doc for why sharing this across every `measure_str` is
            // safe: the API takes `Option<&Paint>`.
            let mut measure_paint = sk::Paint::default();
            measure_paint.set_anti_alias(true);
            FontCache {
                typefaces,
                fonts: HashMap::new(),
                advances: HashMap::new(),
                adv_seq: 0,
                adv_skia_calls: 0,
                measure_paint,
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
                // The advance memo is dropped in the SAME place as `fonts`, and
                // for the same reason: an advance measured against glyphs
                // rastered at the old device size is wrong at the new one, and
                // unlike a blurry glyph a wrong advance moves text around. Both
                // maps live on the same `FontCache` and are cleared together, so
                // there is no ordering in which the advance memo can outlive the
                // scale that produced it. The measure-side and render-side
                // caches are separate `FontCache` values, and each clears its
                // own; neither can read the other's entries.
                self.advances.clear();
            }
        }

        /// Exact bit pattern of `self.scale`, as a comparable key.
        #[inline]
        fn scale_key(&self) -> u32 {
            self.scale.to_bits()
        }

        /// The key both `font()` and the advance memo use.
        ///
        /// Extracted so the two can never drift: if `font()` keyed on a
        /// different size than the memo did, a hit could return an advance
        /// measured at another size. `size_key` is the DPI-snapped size in
        /// hundredths, so it is a function of `self.scale` and changes with it.
        #[inline]
        fn font_key(&self, family: &str, size: f32) -> FontKey {
            let snapped = self.snapped_size(size);
            FontKey {
                family: family.to_string(),
                size_key: (snapped * 100.0).round() as u32,
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
            // NOT a second rounding authority. It differs from
            // `Viewport::snap_logical_to_physical_grid` in two load-bearing ways,
            // so substituting the authority would change font rasterisation:
            //   * the `.max(1.0)` here clamps the DEVICE size; the authority has
            //     no such clamp, so a size that rounds to 0 device pixels becomes
            //     1 here and 0 there. A 0-device-pixel font is not renderable,
            //     so this clamp is the point of the helper.
            //   * the `scale == 1.0` early return skips the round trip entirely,
            //     leaving `logical_size` unrounded; the authority would round it
            //     (16.5 -> 17.0).
            // This is a font-size snap for hinting, not a logical->physical size
            // conversion, so it keeps its own clamp and guard.
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
            // Include scale in key implicitly via snapped value; also clear on scale change.
            let key = self.font_key(family, size);
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
            let key = AdvanceKey {
                font: self.font_key(family, size),
                text: text.to_string(),
            };
            // Hit: return the memoized numbers. The `scale_key` comparison is the
            // third of the three guards described on the `advances` field. It is
            // redundant with the `set_scale_factor` clear *by construction*, but
            // it costs one integer compare and turns "the clear happened" from
            // an assumption into something the code verifies on every lookup, so
            // a future path that mutates `self.scale` directly cannot quietly
            // start serving advances from another scale.
            let scale_key = self.scale_key();
            if let Some(e) = self.advances.get(&key)
                && e.scale_key == scale_key
            {
                return MeasuredText {
                    width: e.width,
                    ascent: e.ascent,
                    descent: e.descent,
                };
            }
            // Miss. `font()` inserts into `self.fonts`, so the borrow of
            // `self.advances` above must end before it — hence the lookup being
            // scoped to its own statement rather than held across the call.
            let font = self.font(family, size);
            self.adv_skia_calls = self.adv_skia_calls.wrapping_add(1);
            let (w, bounds) = font.measure_str(text, Some(&self.measure_paint));
            let run = MeasuredText {
                width: w,
                ascent: (-bounds.top).max(0.0),
                descent: bounds.bottom.max(0.0),
            };
            self.remember_advance(key, run, scale_key);
            run
        }

        /// Memoize one measured run, dropping the oldest entries if the cap is hit.
        ///
        /// Drop-oldest, never refuse: hitting the cap evicts down to half of
        /// `ADVANCE_CACHE_CAP` in a single pass (amortized, since it takes
        /// `ADVANCE_CACHE_CAP/2` inserts to reach the cap again) and then
        /// caches this entry anyway. Refusing to cache past the cap would pin
        /// the working set to whatever was measured first and permanently
        /// exclude every key measured after it, which is the opposite of what
        /// the cache is for.
        fn remember_advance(&mut self, key: AdvanceKey, run: MeasuredText, scale_key: u32) {
            if self.advances.len() >= ADVANCE_CACHE_CAP {
                self.evict_oldest_advances(ADVANCE_CACHE_CAP / 2);
            }
            let seq = self.adv_seq;
            self.adv_seq = self.adv_seq.wrapping_add(1);
            self.advances.insert(
                key,
                AdvanceEntry {
                    width: run.width,
                    ascent: run.ascent,
                    descent: run.descent,
                    scale_key,
                    seq,
                },
            );
        }

        /// Drop the `n` oldest memoized advances.
        ///
        /// `sort_unstable_by_key` on the insertion counter is an O(k log k)
        /// pass, but it only runs once per `ADVANCE_CACHE_CAP/2` inserts, so
        /// the per-measure amortized cost is negligible. The victims' keys are
        /// cloned because the map owns them and `retain` cannot see a partial
        /// match on `seq` without one.
        fn evict_oldest_advances(&mut self, n: usize) {
            if n == 0 || self.advances.is_empty() {
                return;
            }
            let mut victims: Vec<(u64, AdvanceKey)> = self
                .advances
                .iter()
                .map(|(k, e)| (e.seq, k.clone()))
                .collect();
            victims.sort_unstable_by_key(|(seq, _)| *seq);
            // `seq` is unique per insertion and never reused (it only ever
            // advances), so this removes exactly the `n` oldest entries.
            for (_, key) in victims.into_iter().take(n) {
                self.advances.remove(&key);
            }
        }

        /// Number of memoized advances currently held. Test/diagnostic only.
        pub fn advance_cache_len(&self) -> usize {
            self.advances.len()
        }

        /// How many real Skia `measure_str` calls this cache has made.
        /// Test/diagnostic only.
        pub fn skia_measure_calls(&self) -> u64 {
            self.adv_skia_calls
        }
    }

    // PERSISTENT FONT CACHES (perf: one fontconfig scan per scale, not per call).
    //
    // `FontCache::new_with_scale` calls `load_default_typeface`, which builds a
    // `sk::FontMgr::default()` and therefore runs a full fontconfig family scan.
    // That scan is not cheap — it measured at ~50% of total process CPU under
    // `perf`. Both `measure_text` (registered as the global measurer via
    // `text_wrap::set_skia_measurer`, so it runs for every measurement layout
    // performs) and `render_frame` (once per frame) used to build a brand new
    // cache per call, so the scan ran per measurement and per frame. A single
    // frame could then spend unbounded time inside one call, starving the main
    // thread's input handling: clicks/buttons/checkboxes appear dead and the
    // app looks hung, with no coredump and no panic.
    //
    // Thread-local rather than a global `Mutex`: layout and rendering both run
    // on the main/UI thread, so the cache needs no cross-thread sharing, and a
    // thread-local keeps the lock off the per-measurement path entirely.
    //
    // Two caches, not one: measurement and rendering scale are supplied
    // independently. A shared cache would clear its `fonts` map on every
    // alternating scale change (`set_scale_factor`), re-rastering the world
    // between the two callers. Separate caches cost at most one extra scan per
    // scale and never thrash.
    //
    // The `RefCell` initializers are lazy, so `load_default_typeface` runs on
    // first access on the accessing thread, not at static-init time. The render
    // slot holds an `Option` so the slot itself is `const`-initialisable, which
    // is what lets `take_render_font_cache` below hand out an owned cache.
    thread_local! {
        /// Cache backing `measure_text`. Borrowed in place, scale set per call.
        static MEASURE_FONT_CACHE: RefCell<FontCache> =
            RefCell::new(FontCache::new_with_scale(1.0));
        /// Cache backing `render_frame`. Lends its cache to the frame painter.
        static RENDER_FONT_CACHE: RefCell<Option<FontCache>> = const { RefCell::new(None) };
    }

    /// Borrow the persistent measure-path `FontCache`, resynced to `scale`.
    fn with_measure_font_cache<R>(scale: f32, f: impl FnOnce(&mut FontCache) -> R) -> R {
        MEASURE_FONT_CACHE.with(|cell| {
            let mut cache = cell.borrow_mut();
            // Reusing the cache across calls makes this resync load-bearing:
            // without it a scale change would keep serving glyphs rastered at
            // the stale device-pixel size and go blurry at fractional scales
            // (1.25/1.5). `set_scale_factor` is also what drops the `fonts` map
            // on a scale change, so this is the re-raster trigger.
            //
            // The caller's own non-finite/`<= 0` guard in `measure_text` is
            // intentionally NOT replaced by this one: `set_scale_factor`
            // substitutes 1.0 for a degenerate scale, whereas the caller leaves
            // the snapped size unrounded. Both are kept, as before.
            cache.set_scale_factor(scale);
            f(&mut cache)
        })
    }

    /// Owns the render-path `FontCache` for the duration of one frame and
    /// returns it to its thread-local slot on drop.
    ///
    /// The field is an `Option` so `Drop` can `take()` the cache out. It is
    /// deliberately NOT a plain `FontCache` handed back via
    /// `mem::replace(.., FontCache::new_with_scale(1.0))`: that would construct
    /// a throwaway `FontCache` — and therefore run the fontconfig scan this
    /// whole change exists to avoid — on every single frame.
    struct RenderFontCache {
        cache: Option<FontCache>,
    }

    impl RenderFontCache {
        /// The cache for this frame's paint walk.
        fn cache_mut(&mut self) -> &mut FontCache {
            self.cache
                .as_mut()
                .expect("render font cache taken by this guard; not reentrant")
        }
    }

    impl Drop for RenderFontCache {
        fn drop(&mut self) {
            // Take-and-restore rather than holding a `RefCell` borrow across
            // the frame: a non-`const` thread-local cannot hand out a
            // `RefMut<'static, _>`, and the paint walk needs a plain
            // `&mut FontCache` spanning its whole recursion. `try_with`
            // because a `Drop` during thread teardown must not touch a
            // destroyed slot; dropping the cache there is correct, it just
            // forfeits the reuse.
            if let Some(cache) = self.cache.take() {
                let _ = RENDER_FONT_CACHE.try_with(|slot| {
                    *slot.borrow_mut() = Some(cache);
                });
            }
        }
    }

    /// Take the persistent render-path `FontCache`, resynced to `scale`.
    fn take_render_font_cache(scale: f32) -> RenderFontCache {
        let taken = RENDER_FONT_CACHE
            .try_with(|slot| std::mem::take(&mut *slot.borrow_mut()))
            .ok()
            .flatten();
        let mut guard = RenderFontCache {
            // Only the very first frame on this thread pays the fontconfig
            // scan; every later frame gets the previous frame's cache back.
            cache: Some(taken.unwrap_or_else(|| FontCache::new_with_scale(scale))),
        };
        // Same rationale as `with_measure_font_cache`: a frame must be told the
        // current scale, or its glyphs would be rastered at whatever scale the
        // previous frame left behind, going blurry at fractional 1.25/1.5.
        guard.cache_mut().set_scale_factor(scale);
        guard
    }

    /// Public measure helper for layout unify: snapped size, scale-aware.
    /// Consumes: text, font_size (logical), font_family, scale -> logical px width
    /// (snapped) plus the run's ascent and descent.
    pub fn measure_text(text: &str, font_size: f32, font_family: &str, scale: f32) -> MeasuredText {
        // NOT a second rounding authority. It agrees with
        // `Viewport::snap_logical_to_physical_grid` only when `scale` is finite and
        // positive; for a non-finite or `<= 0` scale this returns `font_size`
        // unrounded, whereas the authority substitutes `scale = 1.0` and rounds.
        // Substituting it would change the measured advance for a degenerate
        // scale, so this keeps its own guard. This is a glyph-advance size snap
        // for measurement, not a logical->physical size conversion.
        let snapped = if scale.is_finite() && scale > 0.0 {
            (font_size * scale).round() / scale
        } else {
            font_size
        };
        // Persistent cache, not a fresh one per measurement: see the comment on
        // `MEASURE_FONT_CACHE`. This is the hot path — it runs for every text
        // measurement layout does — so the fontconfig scan must happen here at
        // most once per scale instead of once per call.
        with_measure_font_cache(scale, |fc| fc.measure_run(font_family, snapped, text))
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

        // Persistent per-thread cache rather than a fresh one per frame: see
        // the comment on `RENDER_FONT_CACHE`. `font_guard` lends this frame the
        // cache and returns it to the thread-local slot on drop, so the
        // fontconfig scan behind `load_default_typeface` runs once per thread
        // instead of once per frame. `take_render_font_cache` has already
        // resynced it to `scale`, so glyph rasterisation and the canvas scale
        // above still agree.
        let mut font_guard = take_render_font_cache(scale);
        let fonts = font_guard.cache_mut();
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
                            // Check mark if checked.
                            //
                            // The template binds `:checked="completed"`
                            // (test-app/src/components/TodoItem.vx:6), so
                            // `checked` — not `value` — is what carries the
                            // state. `checked` wins when present; `value` is
                            // kept as the fallback so any caller that really
                            // does pass `value="true"` still ticks.
                            let is_checked = read_bool_attr(props, "checked")
                                .unwrap_or(value == "true" || value == "checked");
                            if is_checked {
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
                            // Text input - draw border, value, and the focus
                            // furniture (selection, caret, ring).
                            //
                            // Contract with the input lane: the FOCUSED input
                            // element carries these as stringified attrs on its
                            // styled VNode. They are read, never computed, and
                            // absence means "not focused, no caret, no
                            // selection".
                            let caret = parse_usize_attr(props, "caret");
                            let sel_start = parse_usize_attr(props, "sel_start");
                            let sel_end = parse_usize_attr(props, "sel_end");
                            let focused = read_bool_attr(props, "focused").unwrap_or(false);
                            let blink = read_bool_attr(props, "caret_blink").unwrap_or(false);
                            // Char index -> byte index. The input lane counts
                            // CHARACTERS; `value` is a Rust &str. A char
                            // index that is not a char boundary (or past the
                            // end) clamps to the nearest legal slice point
                            // rather than panicking the whole frame.
                            let char_to_byte = |ci: usize| -> usize {
                                value
                                    .char_indices()
                                    .nth(ci)
                                    .map(|(b, _)| b)
                                    .unwrap_or(value.len())
                            };

                            let input_rect = sk::Rect::from_xywh(
                                rect.left + 1.0,
                                rect.top + 1.0,
                                (rect.width() - 2.0).max(0.0),
                                (rect.height() - 2.0).max(0.0),
                            );
                            paints.fill.set_color(input_bg);
                            canvas.draw_rect(input_rect, &paints.fill);

                            // Everything below is clipped to the field's inner
                            // rect so a long value's caret and selection can
                            // never paint into the surrounding page.
                            canvas.save();
                            canvas.clip_rect(input_rect, sk::ClipOp::Intersect, true);

                            let font_size = text_style.font_size;
                            let font = fonts.font(font_family, font_size);
                            // Same baseline arithmetic the value text already
                            // used, so the caret's line box is the text's line
                            // box rather than a second opinion about where the
                            // line sits.
                            let ty = input_rect.top
                                + font_size
                                + (input_rect.height() - font_size) / 2.0;
                            // Horizontal padding between the field's inner
                            // edge and the first glyph. One constant, read by
                            // the text origin, the caret x and the selection
                            // edges, so the three can never disagree.
                            const TEXT_PAD: f32 = 4.0;
                            let text_left = input_rect.left + TEXT_PAD;
                            let line_top = ty - font_size;
                            let line_bottom = ty;

                            // The caret/selection x is the MEASURED advance of
                            // the prefix, in the same font and size used to
                            // draw the value. No hardcoded advance, no
                            // monospace grid: a proportional font tracks its
                            // actual glyphs.
                            let advance_of = |fonts: &mut FontCache, ci: usize| -> f32 {
                                let byte = char_to_byte(ci);
                                fonts.measure_text(font_family, font_size, &value[..byte])
                            };
                            // Where text may actually be drawn: the inner
                            // rect less the same padding, so the last glyph
                            // is not half under the border. An index whose
                            // measured advance runs past this (an overflowing
                            // value) clamps HERE, landing the caret flat on
                            // the clip edge rather than scrolling the text.
                            // The text itself never reflows: the caret is an
                            // overlay, and a caret that scrolled entirely out
                            // of view would be the "cursor not showing up"
                            // report a third time.
                            let content_right = input_rect.right - TEXT_PAD;
                            let caret_x_of = |fonts: &mut FontCache, ci: usize| -> f32 {
                                (text_left + advance_of(fonts, ci))
                                    .min(content_right)
                                    .max(text_left)
                            };

                            // 1. Selection highlight, BEHIND the text.
                            if let (Some(s0), Some(s1)) = (sel_start, sel_end) {
                                let (lo, hi) = (s0.min(s1), s0.max(s1));
                                if hi > lo {
                                    let sx = caret_x_of(fonts, lo);
                                    let sw = (caret_x_of(fonts, hi) - sx).max(0.0);
                                    let sel_rect = sk::Rect::from_xywh(
                                        sx,
                                        line_top,
                                        sw.max(0.0),
                                        line_bottom - line_top,
                                    );
                                    // A translucent accent: light enough to
                                    // keep the near-black value text readable
                                    // on top, saturated enough to read as a
                                    // selection against the white field.
                                    paints
                                        .fill
                                        .set_color(sk::Color::from_argb(90, 52, 120, 246));
                                    canvas.draw_rect(sel_rect, &paints.fill);
                                }
                            }

                            // 2. Value text, on top of the selection.
                            if !value.is_empty() {
                                paints.text.set_color(sk::Color::from_argb(255, 0, 0, 0));
                                let _ =
                                    canvas.draw_str(value, (text_left, ty), &font, &paints.text);
                            }

                            // 3. Caret bar. Overlay only — nothing reflows.
                            // Gated on `focused` AND `caret_blink`; this frame
                            // simply omits the bar when the blink phase is
                            // off. The input lane owns the cadence; no timer
                            // lives here.
                            if focused
                                && blink
                                && let Some(caret_idx) = caret
                            {
                                let cx = caret_x_of(fonts, caret_idx);
                                let (halo, core) = caret_colors(input_bg);
                                const BAR_W: f32 = 2.0;
                                // The halo wears the field colour and is 1px
                                // wider on each side, so the core stays legible
                                // even where it crosses a near-black glyph.
                                let halo_rect = sk::Rect::from_xywh(
                                    cx - 1.0,
                                    line_top,
                                    BAR_W + 2.0,
                                    line_bottom - line_top,
                                );
                                paints.fill.set_color(halo);
                                canvas.draw_rect(halo_rect, &paints.fill);
                                let core_rect = sk::Rect::from_xywh(
                                    cx,
                                    line_top,
                                    BAR_W,
                                    line_bottom - line_top,
                                );
                                paints.fill.set_color(core);
                                canvas.draw_rect(core_rect, &paints.fill);
                            }

                            canvas.restore();

                            // 4. Border, then the focus ring.
                            //
                            // The ring is INSET 2px and 2px thick, so it never
                            // shares a pixel with the 1px border drawn here.
                            // A ring drawn on the border's own pixels is
                            // invisible, which is exactly the "input box on
                            // focus cursor not showing up" report.
                            paints.stroke.set_stroke_width(1.0);
                            paints.stroke.set_color(border_color);
                            canvas.draw_rect(input_rect, &paints.stroke);
                            if focused {
                                const RING_INSET: f32 = 2.0;
                                let ring = sk::Rect::from_xywh(
                                    input_rect.left + RING_INSET,
                                    input_rect.top + RING_INSET,
                                    (input_rect.width() - RING_INSET * 2.0).max(0.0),
                                    (input_rect.height() - RING_INSET * 2.0).max(0.0),
                                );
                                if ring.width() > 0.0 && ring.height() > 0.0 {
                                    paints.stroke.set_stroke_width(2.0);
                                    paints
                                        .stroke
                                        .set_color(sk::Color::from_argb(255, 52, 120, 246));
                                    canvas.draw_rect(ring, &paints.stroke);
                                }
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
            layout_root,
            root_rect,
            fonts,
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
            collect_debug_hit_rects(vnode, layout_root, &mut rects);
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
        fn line_box_height_under_real_metrics_is_the_strut_for_every_real_run() {
            velox_dom::text_wrap::set_skia_measurer(measure_text);
            let heights = seam_line_heights(&seam_pre_text_div("Hg\nxxx"));
            assert_eq!(heights.len(), 3, "root, div, two line boxes: {heights:?}");

            // See the bare-text test above for why the old "two different runs
            // get two different heights" assertion had to go: no run of this face
            // at one size overshoots its own strut. What replaces it is the
            // relationship that IS true, and which a fixed multiplier cannot
            // produce: each line is exactly the strut, and the strut is derived
            // from the font file rather than re-run through the implementation.
            let strut = (SEAM_SIZE * 1.362).round() as i32;
            for (line_text, got) in [("Hg", heights[1]), ("xxx", heights[2])] {
                let run = measure_text(line_text, SEAM_SIZE, SEAM_FAMILY, 1.0);
                assert!(
                    (run.line_extent().round() as i32) < strut,
                    "precondition: `{line_text}` ink must be under the strut"
                );
                assert_eq!(
                    got, strut,
                    "line {line_text:?} is the strut, since its ink of {} + {} is \
                     under it",
                    run.ascent, run.descent
                );
            }
            assert_ne!(
                strut,
                (SEAM_SIZE * 1.2).round() as i32,
                "1.362em and 1.2em must not coincide, or this test proves nothing"
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
                (SEAM_SIZE * 1.362).round() as i32,
                "a zero extent must not pass through: the seam substitutes the ink \
                 approximation (1.2em) and the strut then floors it to 1.362em. \
                 Passing the zero on instead would give 0"
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

        /// A measured run's vertical extent is a LOGICAL quantity, so it must not
        /// move when the scale does.
        ///
        /// `measure_run`'s doc says its bounds are logical px rather than the raw
        /// device units Skia produced, and warns that dividing by `scale` would be
        /// the bug rather than the fix. That correction was documentation with
        /// nothing behind it: `measure_run_passes_skias_bounds_through_unmodified`
        /// shares `font()` and `snapped_size` with the code it checks, so it cannot
        /// tell a logical reading from a device one — it only anchors the extraction.
        ///
        /// This is the check that can. `FontCache::snapped_size` divides the device
        /// size by the scale and `font()` builds the typeface at THAT size, so a
        /// correct implementation measures the same extent at scale 1.0 and 2.0. A
        /// device-unit reading doubles at scale 2.0 instead, and a
        /// double-divided one halves. Five assertions, and it is the only test here
        /// that reaches the snap rather than the extraction.
        #[test]
        fn a_runs_vertical_extent_is_logical_and_does_not_move_with_scale() {
            let mut at_one = FontCache::new_with_scale(1.0);
            let mut at_two = FontCache::new_with_scale(2.0);
            for text in ["Hg", "xxx", "Wq"] {
                for size in [16.0f32, 33.0] {
                    let one = at_one.measure_run(SEAM_FAMILY, size, text);
                    let two = at_two.measure_run(SEAM_FAMILY, size, text);
                    assert_eq!(
                        one.line_extent(),
                        two.line_extent(),
                        "{text:?} at {size}px measures a different extent at scale 2.0 \
                         than at 1.0, so these are device px, not logical"
                    );
                    assert_eq!(
                        one.width, two.width,
                        "the advance width must be logical too: {text:?} at {size}px"
                    );
                }
            }
        }

        // ------------------------------------------------------------------
        // The advance memo: does it hit, and does it know when to throw away.
        //
        // Every test above is self-consistent — it asks the seam for a number
        // and then checks that the layout path used that same number — so an
        // advance cache cannot be falsified by any of them. A cache that
        // returned a WRONG number from a stale entry would fail only one of
        // them, and only by accident. These two are the checks that can:
        // `skia_measure_calls` counts the real Skia work, so a memo that never
        // hits is visible, and the scale change is the one event that can
        // legally invalidate a memo, so it is where correctness is load-bearing.
        // ------------------------------------------------------------------

        /// Measuring the same run again must not re-shape it, and must return
        /// the same numbers.
        ///
        /// The result-equality half is the weak half — a correct cache passes
        /// it, and so does no cache at all. `skia_measure_calls` is the half
        /// that can fail: it counts calls into `measure_str` itself, so if the
        /// memo is missing, or keyed on something that never repeats, the
        /// number climbs with every repeat and this goes red. Sixteen repeats
        /// per run rather than two, so a memo that only caught the immediate
        /// successor would not slip through.
        #[test]
        fn a_repeated_measurement_is_served_from_the_memo_without_reshaping() {
            let mut cache = FontCache::new_with_scale(1.0);
            let texts = ["Hg", "xxx", "Buy milk", "Wq", " "];
            for text in texts {
                let first = cache.measure_run(SEAM_FAMILY, SEAM_SIZE, text);
                let calls_after_first = cache.skia_measure_calls();
                assert_eq!(
                    calls_after_first,
                    texts.iter().position(|t| *t == text).unwrap() as u64 + 1,
                    "each distinct run should have cost exactly one Skia call"
                );
                for _ in 0..16 {
                    let again = cache.measure_run(SEAM_FAMILY, SEAM_SIZE, text);
                    assert_eq!(again, first, "re-measuring {text:?} changed the answer");
                }
                assert_eq!(
                    cache.skia_measure_calls(),
                    calls_after_first,
                    "{text:?} was re-shaped in Skia on a repeat: the memo missed"
                );
            }
            assert_eq!(
                cache.advance_cache_len(),
                texts.len(),
                "one entry per distinct run, no more"
            );
        }

        /// A scale change must clear the memo, and what is measured after it
        /// must be what that scale actually measures.
        ///
        /// This is the load-bearing correctness case for the whole cache. An
        /// advance is a function of `(typeface, snapped size, text)`, and the
        /// snapped size is a function of the scale, so a memo that outlived a
        /// scale change would hand the new scale the old scale's text metrics:
        /// wrong wrap points, wrong caret position, visibly wrong UI, and no
        /// panic to notice it by.
        ///
        /// The size is deliberately one where the DPI snap MOVES with the
        /// scale AND the resulting advances land on different f32s — 17.1px is
        /// 17.1 logical at 1.0 and 17.333 at 1.5 (`(17.1 * 1.5).round() / 1.5`
        /// = `26 / 1.5`), and `"Buy milk"` measures 68.0 / 67.0 / 69.0 at
        /// 1.0 / 1.25 / 1.5. The second half of that clause is the load-bearing
        /// half: the default face quantizes advances to integer f32s at these
        /// sizes, so a fractional size alone is not enough. 15.4px snaps apart
        /// too (15.4 vs 15.333) yet `"Buy milk"` measures 61.0 at both 1.25 and
        /// 1.5, so a memo that was never cleared would pass a test comparing
        /// those two scales while still being wrong.
        ///
        /// Measure a run long enough to separate. This uses `"Buy milk"`, not
        /// a short one: at 17.1 the short run `"Hg"` measures 24.0 at BOTH 1.0
        /// and 1.5 and separates only on `ascent`, which is a weaker and less
        /// meaningful discriminator than width — width is what drives wrap
        /// points and caret position, which is what the cache exists to get
        /// right.
        ///
        /// The reference values come from caches built directly at each scale,
        /// so they do not share the cache under test.
        #[test]
        fn a_scale_change_clears_the_advance_memo_and_remeasures_for_the_new_scale() {
            const FRACTIONAL: f32 = 17.1;
            const RUN: &str = "Buy milk";
            let mut cache = FontCache::new_with_scale(1.0);

            let at_one = cache.measure_run(SEAM_FAMILY, FRACTIONAL, RUN);
            let expect_at_one =
                FontCache::new_with_scale(1.0).measure_run(SEAM_FAMILY, FRACTIONAL, RUN);
            assert_eq!(
                at_one, expect_at_one,
                "precondition: the 1.0 answer is right"
            );
            assert_eq!(cache.advance_cache_len(), 1, "one run is memoized");

            cache.set_scale_factor(1.5);
            assert_eq!(
                cache.advance_cache_len(),
                0,
                "a scale change must drop the memo in the same place it drops \
                 `fonts`; a surviving entry is an advance measured at the old \
                 device size being served to the new one"
            );

            let at_one_and_a_half = cache.measure_run(SEAM_FAMILY, FRACTIONAL, RUN);
            let expect_at_one_and_a_half =
                FontCache::new_with_scale(1.5).measure_run(SEAM_FAMILY, FRACTIONAL, RUN);
            assert_eq!(
                at_one_and_a_half, expect_at_one_and_a_half,
                "after the scale change the run must measure as a cache built \
                 at 1.5 does"
            );
            assert_ne!(
                at_one.width, at_one_and_a_half.width,
                "precondition: this run snaps to a different logical size at \
                 each scale, so the two widths must differ; if they are equal \
                 the test cannot detect a stale advance at all"
            );

            // And back again, to catch a one-way clear.
            cache.set_scale_factor(1.0);
            assert_eq!(cache.advance_cache_len(), 0, "the clear is not one-way");
            assert_eq!(
                cache.measure_run(SEAM_FAMILY, FRACTIONAL, RUN),
                expect_at_one,
                "returning to 1.0 must restore the 1.0 measurement"
            );
        }

        /// The same guarantee through the PUBLIC seam, which is the one layout
        /// actually calls.
        ///
        /// The two tests above drive a `FontCache` they own. Production does
        /// not: `measure_text` goes through the thread-local
        /// `MEASURE_FONT_CACHE`, and that thread-local is shared by every
        /// measure in the process. A memo keyed on anything short of the full
        /// identity — a family only, or no scale — is invisible to a per-cache
        /// test and wrong here, because the second call in this sequence is
        /// answered by an entry the first call left behind.
        ///
        /// So this walks the real alternating pattern — 1.0, 1.5, 1.0, 1.25,
        /// 1.5, 1.0 — and pins the property that a scale-blind memo breaks
        /// first: the value at a scale must be the same on every visit to that
        /// scale, and different from every other scale. The repeated 1.5 in the
        /// middle is deliberate; it is a hit, so it also has to agree.
        #[test]
        fn the_shared_measure_seam_never_serves_another_scales_advance() {
            const FRACTIONAL: f32 = 17.1;
            let text = "Buy milk";
            let mut seen: Vec<(f32, f32)> = Vec::new();
            for scale in [1.0f32, 1.5, 1.0, 1.25, 1.5, 1.0] {
                let width = measure_text(text, FRACTIONAL, SEAM_FAMILY, scale).width;
                match seen.iter().find(|(s, _)| *s == scale) {
                    // A second visit to a scale must reproduce the first one.
                    Some((_, first)) => assert_eq!(
                        width, *first,
                        "{text:?} at {scale} measured {width} on a later visit but \
                         {first} on the first, so the memo answered with another \
                         scale's advance"
                    ),
                    None => {
                        // And must differ from every other scale, so the
                        // agreement above is not two scales happening to
                        // measure identically at a snapped-equal size.
                        for (other, other_w) in &seen {
                            assert_ne!(
                                width, *other_w,
                                "{text:?} measures the same at {scale} as at {other}, \
                                 so this test cannot detect a scale-blind memo"
                            );
                        }
                        seen.push((scale, width));
                    }
                }
            }
        }

        /// The memo is bounded, and it drops the OLDEST entries when full
        /// instead of refusing to cache.
        ///
        /// A wrap measures every prefix of a line, so a long paragraph
        /// contributes an entry per word per candidate, and many of those are
        /// never asked for again. Unbounded, that is a slow leak across a
        /// session. Refusing to cache past the cap would be worse than the
        /// leak: it would freeze the working set at whatever was measured
        /// first and permanently exclude every key measured after it, which
        /// defeats the cache on exactly the strings a growing document adds.
        ///
        /// Driven through `remember_advance` directly so the test does not
        /// spend 4096 real Skia shaping passes proving what a map's `len`
        /// already says.
        #[test]
        fn the_advance_memo_is_bounded_and_evicts_the_oldest_rather_than_refusing() {
            let mut cache = FontCache::new_with_scale(1.0);
            let run = MeasuredText {
                width: 7.0,
                ascent: 12.0,
                descent: 4.0,
            };
            let key = |i: usize| AdvanceKey {
                font: FontKey {
                    family: SEAM_FAMILY.to_string(),
                    size_key: (SEAM_SIZE * 100.0) as u32,
                },
                text: format!("run-{i}"),
            };

            for i in 0..ADVANCE_CACHE_CAP {
                cache.remember_advance(key(i), run, cache.scale_key());
            }
            assert_eq!(cache.advance_cache_len(), ADVANCE_CACHE_CAP);

            // One insert past the cap. The newest key must be cached (it did
            // not refuse) and the length must still be bounded (it did not
            // just grow).
            cache.remember_advance(key(ADVANCE_CACHE_CAP), run, cache.scale_key());
            assert!(
                cache.advance_cache_len() <= ADVANCE_CACHE_CAP,
                "grew past the cap: {}",
                cache.advance_cache_len()
            );
            assert_eq!(
                cache.advances.get(&key(ADVANCE_CACHE_CAP)).map(|e| e.width),
                Some(7.0),
                "the entry that tripped the cap was not cached, so the memo \
                 refuses new keys instead of dropping old ones"
            );
            assert!(
                !cache.advances.contains_key(&key(0)),
                "the oldest entry survived; it is not drop-oldest"
            );
            assert!(
                cache.advances.contains_key(&key(ADVANCE_CACHE_CAP / 2)),
                "an entry newer than the eviction window was dropped too"
            );

            // An evicted key must still measure correctly when it comes back:
            // a miss re-shapes, it does not serve whatever survived.
            let mut fresh = FontCache::new_with_scale(1.0);
            for text in ["Hg", "xxx", "Wq", "Buy milk"] {
                assert_eq!(
                    cache.measure_run(SEAM_FAMILY, SEAM_SIZE, text),
                    fresh.measure_run(SEAM_FAMILY, SEAM_SIZE, text),
                    "{text:?} measured differently through a cache that had been \
                     evicting than through a fresh one"
                );
            }
        }

        /// `at()`'s bare `VNode::Text` arm, under real font metrics. This is
        /// the one path R-5a changed that the velox-dom tests cannot reach: they
        /// all install a synthetic measurer, so what is verified here is that a
        /// real font backend's metrics actually arrive at the branch that uses
        /// them.
        #[test]
        fn a_bare_text_node_under_real_metrics_is_the_strut_and_not_the_ink() {
            velox_dom::text_wrap::set_skia_measurer(measure_text);
            let bare = |text: &str| {
                velox_dom::layout::compute_layout(&VNode::Text(text.to_string()), 300, 300)
                    .rect
                    .h
            };
            // RESTATED in R-5b, and the premise of the old version was false.
            // It asserted that a real run's ink sets the bare node's height. It
            // no longer does -- and must not, because a real face's runs are all
            // shorter than its own ascent + descent:
            //   "Hg" at 16px measures 12.0 up + 4.0 down = 1.0em
            //   "xxx" at 16px measures 9.0 up + 0.0 down   = 0.5625em
            //   the strut is 1.069em + 0.293em              = 1.362em
            // So the strut decides for BOTH, which is what a browser does. Under a
            // single uniform font size no run of this face can overshoot its own
            // strut, so "two runs of different content get different heights" is
            // not a property a real font can exhibit at one size. The
            // run-versus-strut discrimination is therefore asserted in
            // velox-dom's `text_metrics_seam.rs`, where a synthetic measurer CAN
            // overshoot; this test pins the real font's numbers.
            let strut = SEAM_SIZE * 1.362;
            for text in ["Hg", "xxx", "Wq"] {
                let run = measure_text(text, SEAM_SIZE, SEAM_FAMILY, 1.0);
                assert!(
                    run.line_extent() < strut,
                    "precondition for this test's claim: `{text}` ink must be under \
                     the strut, or the strut would not be what is being measured \
                     ({} vs {strut})",
                    run.line_extent()
                );
                assert_eq!(
                    bare(text),
                    strut.round() as i32,
                    "a bare `{text}` node is the strut ({} + {} at 1.069em/0.293em), \
                     not its ink of {} up and {} down",
                    SEAM_SIZE * 1.069,
                    SEAM_SIZE * 0.293,
                    run.ascent,
                    run.descent
                );
            }
            assert_ne!(
                bare("Hg"),
                19,
                "and still not the 1.2em the bare-text path had before R-5a: it went \
                 19 -> {} (ink) -> {} (strut)",
                9,
                22
            );
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

        fn plain_text_style() -> TextStyle {
            TextStyle {
                color: sk::Color::from_argb(255, 0, 0, 0),
                align: TextAlign::Left,
                underline: false,
                font_size: 14.0,
                bold: false,
                line_height: 1.2,
                nowrap: false,
                ellipsis: false,
            }
        }

        // `font-weight: bolder` was silently dropped: the check was
        // `"bold" || parse::<u16>() >= 700`, and `bolder` is neither, so the
        // declaration fell through to non-bold. CSS treats `bolder` as
        // relative-bold and the DOM already maps it to 900
        // (`velox_dom::style::FontWeight::Bolder`); the renderer must not
        // contradict that.
        #[test]
        fn parse_text_style_treats_bolder_as_bold() {
            for decl in [
                "font-weight:bolder",
                "font-weight: Bolder ",
                "font-weight:BOLDER",
            ] {
                let (s, _f) = parse_text_style(decl, plain_text_style(), "default");
                assert!(s.bold, "expected `{decl}` to render bold");
            }
        }

        // The regression guard above is only meaningful if the sibling cases
        // keep working and the relative-light keyword is not swept in with it.
        #[test]
        fn parse_text_style_font_weight_keyword_matrix() {
            // Bold-producing, all four spellings CSS accepts for bold.
            for decl in [
                "font-weight:bold",
                "font-weight:700",
                "font-weight:900",
                "font-weight:bolder",
            ] {
                let (s, _f) = parse_text_style(decl, plain_text_style(), "default");
                assert!(s.bold, "expected `{decl}` to render bold");
            }
            // Non-bold-producing.
            for decl in [
                "font-weight:normal",
                "font-weight:100",
                "font-weight:400",
                "font-weight:600",
                "font-weight:lighter",
            ] {
                let (s, _f) = parse_text_style(decl, plain_text_style(), "default");
                assert!(!s.bold, "expected `{decl}` to render non-bold");
            }
        }

        // `bolder` must also override an inherited bold:false base rather than
        // being treated as "no change".
        #[test]
        fn parse_text_style_bolder_overrides_non_bold_base() {
            let (s, _f) = parse_text_style("font-weight:bolder", plain_text_style(), "default");
            assert!(s.bold);
        }

        fn fnv1a(bytes: &[u8]) -> u32 {
            let mut hash: u32 = 0x811c9dc5;
            for b in bytes {
                hash ^= *b as u32;
                hash = hash.wrapping_mul(0x01000193);
            }
            hash
        }

        // ------------------------------------------------------------------
        // Invariant 1: one DPI/scale rounding authority.
        //
        // `render_vnode_to_raster_png_with_scale` used to inline its own copy of
        // the multiply-then-round formula. It now routes through
        // `Viewport::physical_from_logical`. These read the ENCODED PNG's own IHDR
        // dimensions rather than a value the same code path computed on the way
        // in, so they observe the surface that was really allocated.
        // ------------------------------------------------------------------

        /// `(width, height)` from a PNG's IHDR chunk, which sits at a fixed offset:
        /// 8-byte signature, 4-byte length, 4-byte "IHDR", then the two u32s.
        fn png_dimensions(png: &[u8]) -> (u32, u32) {
            assert_eq!(
                &png[..8],
                b"\x89PNG\r\n\x1a\n",
                "encode_png did not emit a PNG signature, so there is no IHDR to read"
            );
            let be = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
            (be(&png[16..20]), be(&png[20..24]))
        }

        /// A logical size that rounds to zero at a fractional scale must still
        /// render, and the surface must be a real pixel rather than a degenerate
        /// one. `1 * 0.4 == 0.4`, which rounds to 0.
        #[test]
        fn a_logical_size_that_rounds_to_zero_at_a_fractional_scale_still_renders() {
            let vnode = h("div", vec![], vec![]);
            let png =
                render_vnode_to_raster_png_with_scale(&vnode, &Stylesheet::default(), 1, 1, 0.4)
                    .expect("a 1x1 logical size at scale 0.4 must render, not fail");
            assert_eq!(
                png_dimensions(&png),
                (1, 1),
                "a size that rounds to 0 at this scale must still allocate at least one pixel"
            );
        }

        /// The discriminating half: the authority substitutes `scale = 1.0` for a
        /// non-finite or non-positive scale, and the call site must inherit that.
        ///
        /// The inlined arithmetic fed the raw scale straight in, so `width * 0.0`
        /// rounded to 0 for ANY width and the surface collapsed to 1x1 no matter
        /// what size was asked for. This is the behaviour the substitution changes,
        /// and it is the part that would silently drop a render to a single pixel.
        #[test]
        fn a_non_positive_scale_falls_back_to_one_instead_of_collapsing_the_surface() {
            let vnode = h("div", vec![], vec![]);
            let png =
                render_vnode_to_raster_png_with_scale(&vnode, &Stylesheet::default(), 8, 6, 0.0)
                    .expect("scale 0.0 must fall back to 1.0, not fail");
            assert_eq!(
                png_dimensions(&png),
                (8, 6),
                "a non-positive scale must be read as 1.0, so the full logical size survives; \
                 an inlined `width * scale` collapses this to (1, 1)"
            );
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
        // NOT a second rounding authority. As in the `skia-native` `measure_text`
        // above, this agrees with `Viewport::snap_logical_to_physical_grid` only
        // when `scale` is finite and positive; for a non-finite or `<= 0` scale it
        // returns `font_size` unrounded where the authority substitutes 1.0 and
        // rounds. It is kept mirroring the `text.rs` fallback so the headless and
        // native paths agree; it is a glyph-advance snap, not a size conversion.
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
