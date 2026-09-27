//! Text rendering and typography support for Velox
//!
//! Provides text measurement, layout, and rendering with support for:
//! - Font family resolution
//! - Font weight and style matching
//! - Line height calculations
//! - Text alignment
//! - Text decoration (underline, overline, strikethrough)

use velox_style::fonts::{FontStyle, LineHeight};

#[derive(Debug, Clone)]
pub struct TextRenderConfig {
    /// Font family name
    pub font_family: String,
    /// Font size in pixels
    pub font_size: f32,
    /// Font weight (100-900)
    pub font_weight: u16,
    /// Font style (normal, italic)
    pub font_style: FontStyle,
    /// Line height
    pub line_height: LineHeight,
    /// Text color (as rgba)
    pub color: (u8, u8, u8, u8),
    /// Text alignment (0=left, 1=center, 2=right)
    pub text_align: u8,
    /// Maximum width before wrapping (None = no wrap)
    pub max_width: Option<f32>,
    /// Whether to apply antialiasing
    pub antialiased: bool,
    /// Letter spacing in pixels
    pub letter_spacing: f32,
    /// Whether text is bold
    pub bold: bool,
    /// Whether text is italic
    pub italic: bool,
    /// Whether to underline
    pub underline: bool,
    /// Whether to strikethrough
    pub strikethrough: bool,
    /// Whether to overline
    pub overline: bool,
}

impl Default for TextRenderConfig {
    fn default() -> Self {
        Self {
            font_family: "system-ui, sans-serif".to_string(),
            font_size: 16.0,
            font_weight: 400,
            font_style: FontStyle::Normal,
            line_height: LineHeight::Normal,
            color: (0, 0, 0, 255),
            text_align: 0, // left
            max_width: None,
            antialiased: true,
            letter_spacing: 0.0,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            overline: false,
        }
    }
}

impl TextRenderConfig {
    pub fn new(font_family: &str, font_size: f32) -> Self {
        Self {
            font_family: font_family.to_string(),
            font_size,
            ..Default::default()
        }
    }

    pub fn with_color(mut self, r: u8, g: u8, b: u8, a: u8) -> Self {
        self.color = (r, g, b, a);
        self
    }

    pub fn with_weight(mut self, weight: u16) -> Self {
        self.font_weight = weight.clamp(100, 900);
        self.bold = weight >= 700;
        self
    }

    pub fn with_alignment(mut self, align: u8) -> Self {
        self.text_align = align.min(2);
        self
    }

    pub fn with_decoration(mut self, underline: bool, overline: bool, strikethrough: bool) -> Self {
        self.underline = underline;
        self.overline = overline;
        self.strikethrough = strikethrough;
        self
    }

    pub fn with_max_width(mut self, width: f32) -> Self {
        self.max_width = Some(width.max(0.0));
        self
    }
}

/// Text measurement and metrics — unified with Skia snapped measure.
pub struct TextMeasurer;

impl TextMeasurer {
    /// Snapped logical size helper (single rounding point via Viewport).
    #[inline]
    fn snapped_size(font_size: f32, scale: f32) -> f32 {
        if scale.is_finite() && scale > 0.0 && scale != 1.0 {
            (font_size * scale).round() / scale
        } else {
            font_size
        }
    }

    /// Estimate text dimensions without actual rendering
    /// When skia-native is available, delegates to skia FontCache measure; otherwise
    /// uses same 0.5 heuristic as velox-dom fallback (headless parity).
    pub fn measure(text: &str, config: &TextRenderConfig) -> (f32, f32) {
        Self::measure_with_scale(text, config, 1.0)
    }

    /// Scale-aware measure (logical px, snapped).
    pub fn measure_with_scale(text: &str, config: &TextRenderConfig, scale: f32) -> (f32, f32) {
        if text.is_empty() {
            return (0.0, config.font_size);
        }
        let snapped = Self::snapped_size(config.font_size, scale);
        #[cfg(feature = "skia-native")]
        let width =
            crate::skia_render::measure_text(text, snapped, &config.font_family, scale).width;
        #[cfg(not(feature = "skia-native"))]
        let width = snapped * 0.5 * text.chars().count() as f32;
        let height = config.line_height.to_pixels(snapped);
        (width, height)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_config() {
        let config = TextRenderConfig::new("Arial", 16.0)
            .with_weight(700)
            .with_color(255, 0, 0, 255);

        assert_eq!(config.font_size, 16.0);
        assert_eq!(config.font_weight, 700);
        assert!(config.bold);
    }

    #[test]
    fn test_text_measurement() {
        let config = TextRenderConfig::default();
        let (w, h) = TextMeasurer::measure("Hello", &config);

        assert!(w > 0.0);
        assert_eq!(h, 19.2); // 1.2 * 16.0
    }

    /// `test_text_measurement` above asserts `w > 0.0`, which is a shape
    /// assertion: it holds for any non-zero width whatsoever. Falsification
    /// showed it does not notice the non-Skia branch's 0.5 ratio moving to
    /// 0.55, so the formula itself needs a pin or nothing guards it.
    #[cfg(not(feature = "skia-native"))]
    #[test]
    fn the_non_skia_measured_width_is_half_an_em_per_character_at_the_snapped_size() {
        for text in ["", "a", "Hg", "Hello", "not positive"] {
            for size in [1.0f32, 11.0, 16.0, 16.5, 33.0, 47.25] {
                for scale in [1.0f32, 1.25, 1.5, 2.0] {
                    let config = TextRenderConfig::new("Arial", size);
                    let (w, _) = TextMeasurer::measure_with_scale(text, &config, scale);
                    let snapped = if scale.is_finite() && scale > 0.0 && scale != 1.0 {
                        (size * scale).round() / scale
                    } else {
                        size
                    };
                    assert_eq!(
                        w,
                        snapped * 0.5 * text.chars().count() as f32,
                        "non-Skia width for {text:?} at {size}px scale {scale} \
                         is not 0.5em per char at the snapped size"
                    );
                }
            }
        }
    }

    /// The renderer and the layout seam must agree on the width of the same
    /// run at the same size, or layout reserves one width and the renderer
    /// draws another. This is the relationship R-5b will lean on, and it is
    /// only checkable with a real measurer registered.
    #[cfg(feature = "skia-native")]
    #[test]
    fn the_renderer_and_the_layout_seam_measure_a_run_identically() {
        velox_dom::text_wrap::set_skia_measurer(crate::skia_render::measure_text);
        for text in ["Hg", "Wg", "iii", "Hello, world"] {
            for size in [12.0f32, 16.0, 33.0] {
                let config = TextRenderConfig::new("system-ui", size);
                let (w, _) = TextMeasurer::measure_with_scale(text, &config, 1.0);
                let seam = velox_dom::text_wrap::measure_text(text, size, "system-ui", 1.0);
                assert_eq!(
                    w, seam,
                    "renderer and layout seam disagree on {text:?} at {size}px"
                );
                assert_ne!(
                    w,
                    size * 0.5 * text.chars().count() as f32,
                    "a real proportional font must not measure 0.5em per char"
                );
            }
        }
    }

    #[test]
    fn test_line_height() {
        let config = TextRenderConfig {
            line_height: LineHeight::Number(1.5),
            ..Default::default()
        };

        let (_, h) = TextMeasurer::measure("Hello", &config);
        assert_eq!(h, 24.0); // 1.5 * 16.0
    }
}
