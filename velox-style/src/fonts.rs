//! Font management system for Velox
//!
//! Handles font family resolution, font matching, and font loading
//! with support for system fonts and custom font files.

/// Font weight values (100-900)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum FontWeight {
    Thin = 100,       // 100
    ExtraLight = 200, // 200
    Light = 300,      // 300
    #[default]
    Normal = 400, // 400 (default)
    Medium = 500,     // 500
    SemiBold = 600,   // 600
    Bold = 700,       // 700
    ExtraBold = 800,  // 800
    Black = 900,      // 900
}

impl FontWeight {
    pub fn from_number(n: u16) -> Self {
        match n {
            100 => FontWeight::Thin,
            200 => FontWeight::ExtraLight,
            300 => FontWeight::Light,
            400 => FontWeight::Normal,
            500 => FontWeight::Medium,
            600 => FontWeight::SemiBold,
            700 => FontWeight::Bold,
            800 => FontWeight::ExtraBold,
            900 => FontWeight::Black,
            n if n < 100 => FontWeight::Thin,
            n if n < 200 => FontWeight::Thin,
            n if n < 300 => FontWeight::ExtraLight,
            n if n < 400 => FontWeight::Light,
            n if n < 500 => FontWeight::Normal,
            n if n < 600 => FontWeight::Medium,
            n if n < 700 => FontWeight::SemiBold,
            n if n < 800 => FontWeight::Bold,
            n if n < 900 => FontWeight::ExtraBold,
            _ => FontWeight::Black,
        }
    }

    pub fn to_number(&self) -> u16 {
        *self as u16
    }
}

/// Font style (normal or italic)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

impl FontStyle {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "normal" => Some(FontStyle::Normal),
            "italic" => Some(FontStyle::Italic),
            "oblique" => Some(FontStyle::Oblique),
            _ => None,
        }
    }
}

/// Line height value (can be number, length, or percentage)
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LineHeight {
    /// Number multiplied by font size
    Number(f32),
    /// Absolute length in pixels
    Pixels(f32),
    /// Relative to parent (percentage)
    Percentage(f32),
    /// Browser default (1.2x font size)
    #[default]
    Normal,
}

impl LineHeight {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();

        if s.eq_ignore_ascii_case("normal") {
            return Some(LineHeight::Normal);
        }

        if let Some(num_str) = s.strip_suffix("px")
            && let Ok(v) = num_str.trim().parse::<f32>()
        {
            return Some(LineHeight::Pixels(v));
        }

        if let Some(pct_str) = s.strip_suffix('%')
            && let Ok(v) = pct_str.trim().parse::<f32>()
        {
            return Some(LineHeight::Percentage(v / 100.0));
        }

        // Try plain number
        if let Ok(v) = s.parse::<f32>()
            && v > 0.0
        {
            return Some(LineHeight::Number(v));
        }

        None
    }

    /// Calculate actual line height in pixels given font size
    pub fn to_pixels(&self, font_size: f32) -> f32 {
        match self {
            LineHeight::Number(n) => *n * font_size,
            LineHeight::Pixels(px) => *px,
            LineHeight::Percentage(pct) => *pct * font_size,
            LineHeight::Normal => 1.2 * font_size,
        }
    }
}

/// Font descriptor for matching and loading
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct FontDescriptor {
    pub family: String,
    pub weight: FontWeight,
    pub style: FontStyle,
    pub size: u32, // in pixels, rounded
}

impl FontDescriptor {
    pub fn new(family: impl Into<String>, size: f32) -> Self {
        Self {
            family: family.into(),
            weight: FontWeight::Normal,
            style: FontStyle::Normal,
            size: size.round() as u32,
        }
    }

    pub fn with_weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_style(mut self, style: FontStyle) -> Self {
        self.style = style;
        self
    }
}

/// Font family list with fallbacks
#[derive(Debug, Clone)]
pub struct FontFamily {
    /// Ordered list of font family names to try
    families: Vec<String>,
}

impl FontFamily {
    pub fn new(family_str: &str) -> Self {
        let families = family_str
            .split(',')
            .map(|f| {
                let trimmed = f.trim();
                let unquoted = trimmed.trim_matches('"').trim_matches('\'');
                unquoted.to_string()
            })
            .filter(|f| !f.is_empty())
            .collect::<Vec<_>>();

        Self {
            families: if families.is_empty() {
                vec!["sans-serif".to_string()]
            } else {
                families
            },
        }
    }

    pub fn families(&self) -> &[String] {
        &self.families
    }

    /// Get the primary font family name
    pub fn primary(&self) -> &str {
        &self.families[0]
    }

    /// Get fallback names
    pub fn fallbacks(&self) -> &[String] {
        if self.families.len() > 1 {
            &self.families[1..]
        } else {
            &[]
        }
    }
}

impl Default for FontFamily {
    fn default() -> Self {
        Self::new("system-ui, -apple-system, sans-serif")
    }
}

impl From<&str> for FontFamily {
    fn from(s: &str) -> Self {
        FontFamily::new(s)
    }
}

/// Generic font family fallback
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenericFamily {
    Serif,
    SansSerif,
    Monospace,
    Cursive,
    Fantasy,
    SystemUI,
}

impl GenericFamily {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "serif" => Some(GenericFamily::Serif),
            "sans-serif" | "sans serif" => Some(GenericFamily::SansSerif),
            "monospace" => Some(GenericFamily::Monospace),
            "cursive" => Some(GenericFamily::Cursive),
            "fantasy" => Some(GenericFamily::Fantasy),
            "system-ui" => Some(GenericFamily::SystemUI),
            _ => None,
        }
    }

    /// Get platform-specific font names for this generic family
    pub fn system_fonts(&self) -> &'static [&'static str] {
        match self {
            GenericFamily::Serif => &["Georgia", "Times New Roman", "serif"],
            GenericFamily::SansSerif => &[
                "system-ui",
                "-apple-system",
                "BlinkMacSystemFont",
                "Segoe UI",
                "Roboto",
                "Oxygen",
                "Ubuntu",
                "Cantarell",
                "sans-serif",
            ],
            GenericFamily::Monospace => &[
                "SF Mono",
                "Monaco",
                "Inconsolata",
                "Fira Mono",
                "Roboto Mono",
                "monospace",
            ],
            GenericFamily::Cursive => &["Comic Sans MS", "Brush Script MT", "cursive"],
            GenericFamily::Fantasy => &["Impact", "Charcoal", "fantasy"],
            GenericFamily::SystemUI => &[
                "system-ui",
                "-apple-system",
                "BlinkMacSystemFont",
                "sans-serif",
            ],
        }
    }
}

/// Font metrics (baseline, ascent, descent, etc.)
#[derive(Debug, Clone, Copy)]
pub struct FontMetrics {
    /// Distance from baseline to top of font
    pub ascent: f32,
    /// Distance from baseline to bottom of font
    pub descent: f32,
    /// Gap between lines (internal leading)
    pub line_gap: f32,
    /// Total line height = ascent + descent + line_gap
    pub line_height: f32,
    /// Underline position (usually negative)
    pub underline_position: f32,
    /// Underline thickness
    pub underline_thickness: f32,
}

impl FontMetrics {
    pub fn new(ascent: f32, descent: f32, line_gap: f32) -> Self {
        let line_height = ascent + descent + line_gap;
        Self {
            ascent,
            descent,
            line_gap,
            line_height,
            underline_position: descent / 2.0,
            underline_thickness: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_weight() {
        assert_eq!(FontWeight::Normal.to_number(), 400);
        assert_eq!(FontWeight::Bold.to_number(), 700);
        assert_eq!(FontWeight::from_number(400), FontWeight::Normal);
        assert_eq!(FontWeight::from_number(850), FontWeight::ExtraBold);
    }

    #[test]
    fn test_line_height() {
        let lh = LineHeight::parse("1.5").unwrap();
        assert_eq!(lh.to_pixels(16.0), 24.0);

        let lh = LineHeight::parse("20px").unwrap();
        assert_eq!(lh.to_pixels(16.0), 20.0);

        let lh = LineHeight::Normal;
        assert_eq!(lh.to_pixels(16.0), 19.2);
    }

    #[test]
    fn test_font_family_parsing() {
        let ff = FontFamily::new("Arial, Helvetica, sans-serif");
        assert_eq!(ff.primary(), "Arial");
        assert_eq!(ff.fallbacks(), ["Helvetica", "sans-serif"]);
    }

    #[test]
    fn test_generic_family() {
        let gf = GenericFamily::from_str("sans-serif").unwrap();
        assert!(!gf.system_fonts().is_empty());
    }
}
