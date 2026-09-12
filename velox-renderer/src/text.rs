//! Text rendering and typography support for Velox
//!
//! Provides text measurement, layout, and rendering with support for:
//! - Font family resolution
//! - Font weight and style matching
//! - Line height calculations
//! - Text alignment
//! - Text decoration (underline, overline, strikethrough)

use velox_style::fonts::{FontStyle, LineHeight};

/// Text measurement and layout result
#[derive(Debug, Clone)]
pub struct TextLayout {
    /// Lines of text with their bounds
    pub lines: Vec<TextLine>,
    /// Total width of the text
    pub width: f32,
    /// Total height of the text
    pub height: f32,
    /// Baseline position from top
    pub baseline: f32,
}

/// A single line of text
#[derive(Debug, Clone)]
pub struct TextLine {
    /// The text content
    pub text: String,
    /// Width of this line
    pub width: f32,
    /// Height of this line (usually same as font size + line height)
    pub height: f32,
    /// Y offset from text block top
    pub y_offset: f32,
    /// Baseline position within this line
    pub baseline: f32,
}

/// Text rendering configuration
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

/// Text measurement and metrics
pub struct TextMeasurer;

impl TextMeasurer {
    /// Estimate text dimensions without actual rendering
    /// This is a placeholder that should be replaced with actual font metrics
    pub fn measure(text: &str, config: &TextRenderConfig) -> (f32, f32) {
        if text.is_empty() {
            return (0.0, config.font_size);
        }

        // Very rough approximation: average character width = 0.5 * font_size
        let char_width = config.font_size * 0.5;
        let width = text.chars().count() as f32 * char_width;
        let height = config.line_height.to_pixels(config.font_size);

        (width, height)
    }

    /// Layout text with wrapping
    pub fn layout(text: &str, config: &TextRenderConfig) -> TextLayout {
        let line_height = config.line_height.to_pixels(config.font_size);

        if config.max_width.is_none() {
            // Single line, no wrapping
            let (width, _) = Self::measure(text, config);
            return TextLayout {
                lines: vec![TextLine {
                    text: text.to_string(),
                    width,
                    height: line_height,
                    y_offset: 0.0,
                    baseline: config.font_size * 0.8, // rough baseline
                }],
                width,
                height: line_height,
                baseline: config.font_size * 0.8,
            };
        }

        // Multi-line with wrapping — safe to unwrap: None case handled above
        let max_width = config.max_width.unwrap();
        let _char_width = config.font_size * 0.5;
        let mut lines = Vec::new();
        let mut current_line = String::new();
        let mut y_offset = 0.0;

        for word in text.split_whitespace() {
            let test_line = if current_line.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", current_line, word)
            };

            let (line_width, _) = Self::measure(&test_line, config);

            if line_width > max_width && !current_line.is_empty() {
                // Save current line and start new one
                let (w, _) = Self::measure(&current_line, config);
                lines.push(TextLine {
                    text: current_line,
                    width: w,
                    height: line_height,
                    y_offset,
                    baseline: config.font_size * 0.8,
                });
                y_offset += line_height;
                current_line = word.to_string();
            } else {
                current_line = test_line;
            }
        }

        // Add final line
        if !current_line.is_empty() {
            let (w, _) = Self::measure(&current_line, config);
            lines.push(TextLine {
                text: current_line,
                width: w,
                height: line_height,
                y_offset,
                baseline: config.font_size * 0.8,
            });
        }

        let total_height = if lines.is_empty() {
            line_height
        } else {
            let last = &lines[lines.len() - 1];
            last.y_offset + last.height
        };

        let total_width = lines.iter().map(|l| l.width).fold(0.0, f32::max);

        TextLayout {
            lines,
            width: total_width,
            height: total_height,
            baseline: config.font_size * 0.8,
        }
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
