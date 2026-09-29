// CSS Style Types and Utilities
//
// This module provides CSS length, color, and computed style types
// used throughout Velox for rendering and layout.

use std::fmt;

/// A CSS length value with unit
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Length {
    /// Pixels (px)
    Px(f32),
    /// Percentage (%)
    Percent(f32),
    /// Root em (rem) - relative to root font size
    Rem(f32),
    /// Em - relative to parent font size
    Em(f32),
    /// Viewport width (vw)
    Vw(f32),
    /// Viewport height (vh)
    Vh(f32),
    /// Dynamic viewport height (dvh) - treated as vh for layout
    Dvh(f32),
    /// Dynamic viewport width (dvw) - treated as vw for layout
    Dvw(f32),
    /// Auto length
    Auto,
    /// Zero (unitless)
    #[default]
    Zero,
}

impl Length {
    /// Parse a CSS length string
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();

        if s == "auto" {
            return Some(Length::Auto);
        }

        if s == "0" {
            return Some(Length::Zero);
        }

        // Try to parse with units
        if let Some(val) = s.strip_suffix("px") {
            return val.trim().parse::<f32>().ok().map(Length::Px);
        }

        if let Some(val) = s.strip_suffix('%') {
            return val.trim().parse::<f32>().ok().map(Length::Percent);
        }

        if let Some(val) = s.strip_suffix("rem") {
            return val.trim().parse::<f32>().ok().map(Length::Rem);
        }

        if let Some(val) = s.strip_suffix("em") {
            return val.trim().parse::<f32>().ok().map(Length::Em);
        }

        if let Some(val) = s.strip_suffix("dvw") {
            return val.trim().parse::<f32>().ok().map(Length::Dvw);
        }

        if let Some(val) = s.strip_suffix("dvh") {
            return val.trim().parse::<f32>().ok().map(Length::Dvh);
        }

        if let Some(val) = s.strip_suffix("vw") {
            return val.trim().parse::<f32>().ok().map(Length::Vw);
        }

        if let Some(val) = s.strip_suffix("vh") {
            return val.trim().parse::<f32>().ok().map(Length::Vh);
        }

        // Try plain number as pixels
        s.parse::<f32>().ok().map(Length::Px)
    }

    /// Convert to pixels given context values
    pub fn to_px(&self, parent_size: f32, root_size: f32, viewport: (f32, f32)) -> f32 {
        match *self {
            Length::Px(v) => v,
            Length::Percent(v) => parent_size * v / 100.0,
            Length::Rem(v) => v * root_size,
            Length::Em(v) => v * parent_size,
            Length::Vw(v) => v * viewport.0 / 100.0,
            Length::Vh(v) => v * viewport.1 / 100.0,
            Length::Dvw(v) => v * viewport.0 / 100.0,
            Length::Dvh(v) => v * viewport.1 / 100.0,
            Length::Auto => 0.0, // Auto needs special handling
            Length::Zero => 0.0,
        }
    }

    /// Check if length is auto
    pub fn is_auto(&self) -> bool {
        matches!(self, Length::Auto)
    }
}

impl fmt::Display for Length {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Length::Px(v) => write!(f, "{}px", v),
            Length::Percent(v) => write!(f, "{}%", v),
            Length::Rem(v) => write!(f, "{}rem", v),
            Length::Em(v) => write!(f, "{}em", v),
            Length::Vw(v) => write!(f, "{}vw", v),
            Length::Vh(v) => write!(f, "{}vh", v),
            Length::Dvw(v) => write!(f, "{}dvw", v),
            Length::Dvh(v) => write!(f, "{}dvh", v),
            Length::Auto => write!(f, "auto"),
            Length::Zero => write!(f, "0"),
        }
    }
}

/// A CSS color value
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Transparent color
    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    /// Black
    pub const BLACK: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    /// White
    pub const WHITE: Color = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    /// Red
    pub const RED: Color = Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    /// Green
    pub const GREEN: Color = Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    };
    /// Blue
    pub const BLUE: Color = Color {
        r: 0,
        g: 0,
        b: 255,
        a: 255,
    };

    /// Modern dark theme colors
    pub const DARK_BG: Color = Color {
        r: 26,
        g: 26,
        b: 26,
        a: 255,
    }; // #1a1a1a
    pub const DARK_TEXT: Color = Color {
        r: 224,
        g: 224,
        b: 224,
        a: 255,
    }; // #e0e0e0
    pub const DARK_CARD: Color = Color {
        r: 38,
        g: 38,
        b: 38,
        a: 255,
    }; // #262626
    pub const ACCENT_BLUE: Color = Color {
        r: 52,
        g: 120,
        b: 246,
        a: 255,
    }; // #3478f6
    pub const ACCENT_GREEN: Color = Color {
        r: 34,
        g: 197,
        b: 94,
        a: 255,
    }; // #22c55e
    pub const BORDER_DARK: Color = Color {
        r: 55,
        g: 65,
        b: 81,
        a: 255,
    }; // #374151

    /// Create a new color
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }

    /// Parse a CSS color string (hex, rgb, rgba, named)
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();

        // Named colors
        let named = match s.to_lowercase().as_str() {
            "transparent" => return Some(Color::TRANSPARENT),
            "black" => Color::BLACK,
            "white" => Color::WHITE,
            "red" => Color::RED,
            "green" => Color::GREEN,
            "blue" => Color::BLUE,
            "yellow" => Color {
                r: 255,
                g: 255,
                b: 0,
                a: 255,
            },
            "cyan" => Color {
                r: 0,
                g: 255,
                b: 255,
                a: 255,
            },
            "magenta" => Color {
                r: 255,
                g: 0,
                b: 255,
                a: 255,
            },
            "silver" => Color {
                r: 192,
                g: 192,
                b: 192,
                a: 255,
            },
            "gray" | "grey" => Color {
                r: 128,
                g: 128,
                b: 128,
                a: 255,
            },
            "maroon" => Color {
                r: 128,
                g: 0,
                b: 0,
                a: 255,
            },
            "olive" => Color {
                r: 128,
                g: 128,
                b: 0,
                a: 255,
            },
            "lime" => Color {
                r: 0,
                g: 255,
                b: 0,
                a: 255,
            },
            "aqua" => Color {
                r: 0,
                g: 255,
                b: 255,
                a: 255,
            },
            "teal" => Color {
                r: 0,
                g: 128,
                b: 128,
                a: 255,
            },
            "navy" => Color {
                r: 0,
                g: 0,
                b: 128,
                a: 255,
            },
            "fuchsia" => Color {
                r: 255,
                g: 0,
                b: 255,
                a: 255,
            },
            "purple" => Color {
                r: 128,
                g: 0,
                b: 128,
                a: 255,
            },
            "orange" => Color {
                r: 255,
                g: 165,
                b: 0,
                a: 255,
            },
            "pink" => Color {
                r: 255,
                g: 192,
                b: 203,
                a: 255,
            },
            _ => {
                // Try hex
                if let Some(hex) = s.strip_prefix('#') {
                    return Self::parse_hex(hex);
                }
                // Try rgb/rgba
                if s.starts_with("rgb") {
                    return Self::parse_rgb(s);
                }
                return None;
            }
        };

        Some(named)
    }

    fn parse_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim();
        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
                Some(Color::new(r, g, b, 255))
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Color::new(r, g, b, 255))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Color::new(r, g, b, a))
            }
            _ => None,
        }
    }

    fn parse_rgb(s: &str) -> Option<Self> {
        // Try rgba first (longer prefix), then rgb — strip_prefix is exact match
        let inner = s
            .strip_prefix("rgba(")
            .or_else(|| s.strip_prefix("rgb("))?
            .trim_end_matches(')')
            .trim();

        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();

        if parts.len() < 3 {
            return None;
        }

        let r = parts[0].parse::<u8>().ok()?;
        let g = parts[1].parse::<u8>().ok()?;
        let b = parts[2].parse::<u8>().ok()?;
        let a = if parts.len() >= 4 {
            // Parse alpha as 0-1 or 0-255
            let a_str = parts[3];
            if let Ok(a_val) = a_str.parse::<f32>() {
                if a_val <= 1.0 {
                    (a_val * 255.0) as u8
                } else {
                    a_val as u8
                }
            } else {
                255
            }
        } else {
            255
        };

        Some(Color::new(r, g, b, a))
    }

    /// Convert to RGBA tuple (0-255)
    pub fn to_rgba(&self) -> (u8, u8, u8, u8) {
        (self.r, self.g, self.b, self.a)
    }

    /// Convert to normalized RGBA (0.0-1.0)
    pub fn to_normalized(&self) -> (f32, f32, f32, f32) {
        (
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a as f32 / 255.0,
        )
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.a == 255 {
            write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            write!(
                f,
                "rgba({}, {}, {}, {})",
                self.r,
                self.g,
                self.b,
                self.a as f32 / 255.0
            )
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Color::BLACK
    }
}

/// Sides type for margin, padding, etc.
#[derive(Debug, Clone, PartialEq)]
pub struct Sides<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

impl<T: Default> Default for Sides<T> {
    fn default() -> Self {
        Self {
            top: T::default(),
            right: T::default(),
            bottom: T::default(),
            left: T::default(),
        }
    }
}

impl<T> Sides<T> {
    pub fn new() -> Self
    where
        T: Default,
    {
        Self::default()
    }

    pub fn all(value: T) -> Self
    where
        T: Copy,
    {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

/// Border style enum
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BorderStyle {
    #[default]
    None,
    Hidden,
    Solid,
    Dashed,
    Dotted,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,
}

impl BorderStyle {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "none" => Some(BorderStyle::None),
            "hidden" => Some(BorderStyle::Hidden),
            "solid" => Some(BorderStyle::Solid),
            "dashed" => Some(BorderStyle::Dashed),
            "dotted" => Some(BorderStyle::Dotted),
            "double" => Some(BorderStyle::Double),
            "groove" => Some(BorderStyle::Groove),
            "ridge" => Some(BorderStyle::Ridge),
            "inset" => Some(BorderStyle::Inset),
            "outset" => Some(BorderStyle::Outset),
            _ => None,
        }
    }
}

/// Border struct
#[derive(Debug, Clone, PartialEq)]
pub struct Border {
    pub width: Sides<Length>,
    pub style: Sides<BorderStyle>,
    pub color: Sides<Color>,
}

impl Default for Border {
    fn default() -> Self {
        Self {
            width: Sides::default(),
            style: Sides::default(),
            color: Sides::all(Color::BLACK),
        }
    }
}

/// Text align enum
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

impl TextAlign {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "left" => Some(TextAlign::Left),
            "center" => Some(TextAlign::Center),
            "right" => Some(TextAlign::Right),
            "justify" => Some(TextAlign::Justify),
            _ => None,
        }
    }
}

/// How an inline-level box is aligned within its line box.
///
/// `Baseline` is the initial value and the only one CSS 2.1 §10.8.1 defines for
/// a line box's own text. `Top`, `Bottom` and `Middle` are real and are honoured
/// in the line box. `Sub` and `Super` are NOT here: they are defined as a shift
/// the font's own metrics supply (`sub` may shift by "the font's own subscript
/// offset"), and the seam reports a run's INK, not the font's subscript offset, so
/// any value here would be a made-up number. An unparseable value returns `None`
/// and the INHERITED value stands -- `Baseline` is the initial value and comes
/// from the property's absence, which is what CSS 2.1 §10.8.1 specifies -- and
/// that is also what keeps a `sub` declaration from silently becoming a
/// `baseline` one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerticalAlign {
    #[default]
    Baseline,
    Top,
    Bottom,
    Middle,
}

impl VerticalAlign {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "baseline" => Some(VerticalAlign::Baseline),
            "top" => Some(VerticalAlign::Top),
            "bottom" => Some(VerticalAlign::Bottom),
            "middle" => Some(VerticalAlign::Middle),
            // A LENGTH or PERCENTAGE is a legal `vertical-align` value in CSS and
            // is meaningful, so it must not be silently read as `baseline`. It is
            // also not something the line box can honour without the strut's
            // coordinates relative to a baseline it does not have yet, so it is
            // rejected here and `parse` returning None leaves the inherited value.
            _ => None,
        }
    }
}

/// Font weight enum
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum FontWeight {
    #[default]
    Normal,
    Bold,
    Bolder,
    Lighter,
    Value(u16),
}

impl FontWeight {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "normal" => Some(FontWeight::Normal),
            "bold" => Some(FontWeight::Bold),
            "bolder" => Some(FontWeight::Bolder),
            "lighter" => Some(FontWeight::Lighter),
            _ => s.parse::<u16>().ok().map(FontWeight::Value),
        }
    }

    pub fn to_number(&self) -> u16 {
        match self {
            FontWeight::Normal => 400,
            FontWeight::Bold => 700,
            FontWeight::Bolder => 900,
            FontWeight::Lighter => 100,
            FontWeight::Value(v) => *v,
        }
    }
}

/// Font style enum.
///
/// Deliberately a local definition rather than a reuse of
/// `velox_style::fonts::FontStyle`, which has the same three variants. Both
/// `velox-renderer` and `velox-style` depend on `velox-dom`, and `velox-style`
/// is only a *dev*-dependency of it, so importing it here would close a
/// dependency cycle. Keep the two definitions in step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

/// Text decoration enum
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TextDecoration {
    #[default]
    None,
    Underline,
    Overline,
    LineThrough,
}

impl TextDecoration {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "none" => Some(TextDecoration::None),
            "underline" => Some(TextDecoration::Underline),
            "overline" => Some(TextDecoration::Overline),
            "line-through" => Some(TextDecoration::LineThrough),
            _ => None,
        }
    }
}

/// Flex wrap enum
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum FlexWrap {
    #[default]
    NoWrap,
    Wrap,
    WrapReverse,
}

impl FlexWrap {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "nowrap" => Some(FlexWrap::NoWrap),
            "wrap" => Some(FlexWrap::Wrap),
            "wrap-reverse" => Some(FlexWrap::WrapReverse),
            _ => None,
        }
    }
}

/// Box sizing enum
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BoxSizing {
    #[default]
    ContentBox,
    BorderBox,
}

impl BoxSizing {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "content-box" => Some(BoxSizing::ContentBox),
            "border-box" => Some(BoxSizing::BorderBox),
            _ => None,
        }
    }
}

/// Visibility enum
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
    Collapse,
}

impl Visibility {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "visible" => Some(Visibility::Visible),
            "hidden" => Some(Visibility::Hidden),
            "collapse" => Some(Visibility::Collapse),
            _ => None,
        }
    }
}

/// Transform operations
#[derive(Debug, Clone, PartialEq)]
pub enum TransformOp {
    TranslateX(Length),
    TranslateY(Length),
    Translate(Length, Length),
    Rotate(f32),
    ScaleX(f32),
    ScaleY(f32),
    Scale(f32, f32),
    SkewX(f32),
    SkewY(f32),
    Skew(f32, f32),
}

/// Transform struct
#[derive(Debug, Clone, PartialEq)]
pub struct Transform {
    pub operations: Vec<TransformOp>,
}

impl Transform {
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::new()
    }
}

impl Transform {
    /// Parse a `transform` value such as `translate(10px, 20px) rotate(45deg) scale(1.5)`.
    ///
    /// Supports the three operations surfaced by Velox: `translate`/`translateX`/`translateY`,
    /// `rotate`, and `scale`/`scaleX`/`scaleY`. `none` and empty values produce an empty transform.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("none") {
            return Some(Self::new());
        }
        let mut operations = Vec::new();
        // Each `<name>(<args>)` is a single operation. Args may contain spaces after commas
        // (`translate(10px, 20px)`), so accumulate whitespace-separated runs into one token
        // until parentheses balance.
        let mut current = String::new();
        let mut depth = 0usize;
        for token in s.split_whitespace() {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(token);
            depth = depth
                .saturating_add(token.matches('(').count())
                .saturating_sub(token.matches(')').count());
            if depth == 0 {
                operations.push(TransformOp::parse(&current)?);
                current.clear();
            }
        }
        if !current.is_empty() {
            operations.push(TransformOp::parse(&current)?);
        }
        Some(Transform { operations })
    }
}

impl TransformOp {
    fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let open = s.find('(')?;
        let close = s.rfind(')')?;
        if close < open {
            return None;
        }
        let name = s[..open].trim().to_ascii_lowercase();
        let args: Vec<&str> = s[open + 1..close].split(',').map(|a| a.trim()).collect();

        match name.as_str() {
            "translate" => {
                let x = Length::parse(args.first()?)?;
                let y = if args.len() > 1 {
                    Length::parse(args[1])?
                } else {
                    Length::Zero
                };
                Some(TransformOp::Translate(x, y))
            }
            "translatex" => Some(TransformOp::TranslateX(Length::parse(args.first()?)?)),
            "translatey" => Some(TransformOp::TranslateY(Length::parse(args.first()?)?)),
            "rotate" => {
                let raw = args.first()?.trim();
                let deg_raw = raw.strip_suffix("deg").unwrap_or(raw).trim();
                let deg = deg_raw.parse::<f32>().ok()?;
                Some(TransformOp::Rotate(deg))
            }
            "scale" => {
                let x = args.first()?.trim().parse::<f32>().ok()?;
                let y = if args.len() > 1 {
                    args[1].trim().parse::<f32>().ok()?
                } else {
                    x
                };
                Some(TransformOp::Scale(x, y))
            }
            "scalex" => Some(TransformOp::ScaleX(
                args.first()?.trim().parse::<f32>().ok()?,
            )),
            "scaley" => Some(TransformOp::ScaleY(
                args.first()?.trim().parse::<f32>().ok()?,
            )),
            _ => None,
        }
    }
}

/// Box shadow struct
#[derive(Debug, Clone, PartialEq)]
pub struct BoxShadow {
    pub offset_x: Length,
    pub offset_y: Length,
    pub blur_radius: Length,
    pub spread_radius: Length,
    pub color: Color,
    pub inset: bool,
}

impl BoxShadow {
    /// Parse a `box-shadow` value such as `2px 2px 4px rgba(0,0,0,0.5)` or
    /// `inset 0 2px 4px #000`. Returns `None` for `none` / empty values.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("none") {
            return None;
        }
        let mut inset = false;
        let mut parts: Vec<&str> = Vec::new();
        for tok in s.split_whitespace() {
            if tok.eq_ignore_ascii_case("inset") {
                inset = true;
            } else {
                parts.push(tok);
            }
        }
        // offset-x and offset-y are required.
        let offset_x = Length::parse(parts.first()?)?;
        let offset_y = Length::parse(parts.get(1)?)?;
        let mut blur: Option<Length> = None;
        let mut spread: Option<Length> = None;
        let mut color = Color::BLACK;
        for tok in &parts[2..] {
            if let Some(l) = Length::parse(tok) {
                if blur.is_none() {
                    blur = Some(l);
                } else if spread.is_none() {
                    spread = Some(l);
                }
            } else if let Some(c) = Color::parse(tok) {
                color = c;
            }
        }
        Some(BoxShadow {
            offset_x,
            offset_y,
            blur_radius: blur.unwrap_or(Length::Zero),
            spread_radius: spread.unwrap_or(Length::Zero),
            color,
            inset,
        })
    }
}

/// CSS transition timing function
#[derive(Debug, Clone, PartialEq, Default)]
pub enum TimingFunction {
    #[default]
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
}

impl TimingFunction {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "linear" => Some(TimingFunction::Linear),
            "ease" => Some(TimingFunction::Ease),
            "ease-in" => Some(TimingFunction::EaseIn),
            "ease-out" => Some(TimingFunction::EaseOut),
            "ease-in-out" => Some(TimingFunction::EaseInOut),
            _ => {
                let s = s.trim();
                if let Some(inner) = s.strip_prefix("cubic-bezier(") {
                    let inner = inner.trim_end_matches(')');
                    let parts: Vec<f32> = inner
                        .split(',')
                        .map(|p| p.trim().parse::<f32>())
                        .collect::<Result<Vec<_>, _>>()
                        .ok()?;
                    if parts.len() == 4 {
                        return Some(TimingFunction::CubicBezier(
                            parts[0], parts[1], parts[2], parts[3],
                        ));
                    }
                }
                None
            }
        }
    }
}

/// A single CSS transition (e.g. `opacity 0.2s ease-in 0.1s`)
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    /// The CSS property being transitioned, or `all`.
    pub property: String,
    /// Transition duration in seconds.
    pub duration: f32,
    /// Timing function applied over the duration.
    pub timing_function: TimingFunction,
    /// Transition delay in seconds.
    pub delay: f32,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            property: "all".to_string(),
            duration: 0.0,
            timing_function: TimingFunction::Linear,
            delay: 0.0,
        }
    }
}

impl Transition {
    /// Parse a single transition declaration value, e.g. `opacity 0.2s ease-in 0.1s`.
    /// Returns `None` for `none` / empty values.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.is_empty() || value.eq_ignore_ascii_case("none") {
            return None;
        }
        let mut t = Transition::default();
        let mut seen_time = false;
        for tok in value.split_whitespace() {
            if let Some(num) = parse_time_seconds(tok) {
                if !seen_time {
                    t.duration = num;
                } else {
                    t.delay = num;
                }
                seen_time = true;
            } else if let Some(tf) = TimingFunction::parse(tok) {
                t.timing_function = tf;
            } else {
                t.property = tok.trim_matches(',').to_string();
            }
        }
        if t.property.is_empty() {
            t.property = "all".to_string();
        }
        Some(t)
    }
}

fn parse_time_seconds(s: &str) -> Option<f32> {
    if let Some(v) = s.strip_suffix("ms") {
        return v.trim().parse::<f32>().ok().map(|ms| ms / 1000.0);
    }
    if let Some(v) = s.strip_suffix('s') {
        return v.trim().parse::<f32>().ok();
    }
    None
}

/// Display enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Display {
    #[default]
    Block,
    Inline,
    InlineBlock,
    Flex,
    Grid,
    None,
}

impl Display {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "block" => Some(Display::Block),
            "inline" => Some(Display::Inline),
            "inline-block" => Some(Display::InlineBlock),
            "flex" => Some(Display::Flex),
            "grid" => Some(Display::Grid),
            "none" | "hidden" => Some(Display::None),
            _ => None,
        }
    }
}

/// Position enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

impl Position {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "static" => Some(Position::Static),
            "relative" => Some(Position::Relative),
            "absolute" => Some(Position::Absolute),
            "fixed" => Some(Position::Fixed),
            "sticky" => Some(Position::Sticky),
            _ => None,
        }
    }
}

/// Flex direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexDirection {
    #[default]
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl FlexDirection {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "row" => Some(FlexDirection::Row),
            "row-reverse" => Some(FlexDirection::RowReverse),
            "column" => Some(FlexDirection::Column),
            "column-reverse" => Some(FlexDirection::ColumnReverse),
            _ => None,
        }
    }
}

/// Justify content
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JustifyContent {
    #[default]
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl JustifyContent {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "flex-start" | "start" => Some(JustifyContent::FlexStart),
            "flex-end" | "end" => Some(JustifyContent::FlexEnd),
            "center" => Some(JustifyContent::Center),
            "space-between" => Some(JustifyContent::SpaceBetween),
            "space-around" => Some(JustifyContent::SpaceAround),
            "space-evenly" => Some(JustifyContent::SpaceEvenly),
            _ => None,
        }
    }
}

/// Align items
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignItems {
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
    #[default]
    Stretch,
}

impl AlignItems {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "flex-start" | "start" => Some(AlignItems::FlexStart),
            "flex-end" | "end" => Some(AlignItems::FlexEnd),
            "center" => Some(AlignItems::Center),
            "baseline" => Some(AlignItems::Baseline),
            "stretch" => Some(AlignItems::Stretch),
            _ => None,
        }
    }
}

/// Align self (for individual flex children)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignSelf {
    #[default]
    Auto,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
    Stretch,
}

impl AlignSelf {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "auto" => Some(AlignSelf::Auto),
            "flex-start" | "start" => Some(AlignSelf::FlexStart),
            "flex-end" | "end" => Some(AlignSelf::FlexEnd),
            "center" => Some(AlignSelf::Center),
            "baseline" => Some(AlignSelf::Baseline),
            "stretch" => Some(AlignSelf::Stretch),
            _ => None,
        }
    }

    /// Resolve to AlignItems if not Auto, otherwise use parent's align_items
    pub fn resolve(&self, parent_align: AlignItems) -> AlignItems {
        match self {
            AlignSelf::Auto => parent_align,
            AlignSelf::FlexStart => AlignItems::FlexStart,
            AlignSelf::FlexEnd => AlignItems::FlexEnd,
            AlignSelf::Center => AlignItems::Center,
            AlignSelf::Baseline => AlignItems::Baseline,
            AlignSelf::Stretch => AlignItems::Stretch,
        }
    }
}

/// Overflow
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
    Scroll,
    Auto,
}

impl Overflow {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "visible" => Some(Overflow::Visible),
            "hidden" => Some(Overflow::Hidden),
            "scroll" => Some(Overflow::Scroll),
            "auto" => Some(Overflow::Auto),
            _ => None,
        }
    }
}

/// White-space handling per CSS Text
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WhiteSpace {
    #[default]
    Normal,
    Nowrap,
    Pre,
    PreWrap,
    PreLine,
}

impl WhiteSpace {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "normal" => Some(WhiteSpace::Normal),
            "nowrap" => Some(WhiteSpace::Nowrap),
            "pre" => Some(WhiteSpace::Pre),
            "pre-wrap" => Some(WhiteSpace::PreWrap),
            "pre-line" => Some(WhiteSpace::PreLine),
            _ => None,
        }
    }
}

/// Text-overflow handling
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

impl TextOverflow {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "clip" => Some(TextOverflow::Clip),
            "ellipsis" => Some(TextOverflow::Ellipsis),
            _ => None,
        }
    }
}

/// ComputedStyle struct (moved from velox-style)
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    pub display: Display,
    pub position: Position,
    pub z_index: Option<i32>,
    pub opacity: f32,
    pub width: Length,
    pub height: Length,
    pub min_width: Length,
    pub min_height: Length,
    pub max_width: Length,
    pub max_height: Length,
    pub margin: Sides<Length>,
    pub padding: Sides<Length>,
    pub border: Border,
    pub border_radius: Sides<Length>,
    pub box_sizing: BoxSizing,
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub align_self: AlignSelf,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Length,
    pub gap: Length,
    pub row_gap: Length,
    pub column_gap: Length,
    pub background_color: Color,
    pub background_image: Option<String>,
    pub color: Color,
    pub font_size: Length,
    pub font_family: String,
    pub font_weight: FontWeight,
    /// Inherited (CSS 2.1 §10.8.1), so it is in `velox_style`'s `INHERITABLE`.
    /// Reaches the cascade — it used to be dropped here because `set_property`
    /// had no arm for it — but no renderer reads it yet; see
    /// `PARSED_BUT_UNRENDERED`.
    pub font_style: FontStyle,
    pub line_height: Option<f32>,
    pub letter_spacing: Length,
    pub text_align: TextAlign,
    /// Inherited (CSS 2.1 §10.8.1), so it is in `velox_style`'s `INHERITABLE`.
    pub vertical_align: VerticalAlign,
    pub text_decoration: TextDecoration,
    pub overflow: Overflow,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    pub visibility: Visibility,
    pub transform: Transform,
    pub box_shadow: Option<BoxShadow>,
    pub white_space: WhiteSpace,
    pub text_overflow: TextOverflow,
    /// Declared CSS transitions, applied in order.
    pub transitions: Vec<Transition>,
}

impl ComputedStyle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply_inline_style(&mut self, style: &str) {
        for decl in style.split(';') {
            let d = decl.trim();
            if d.is_empty() {
                continue;
            }
            if let Some((k, v)) = d.split_once(':') {
                self.set_property(k.trim(), v.trim());
            }
        }
    }

    /// Properties that `set_property` accepts and stores, but which produce **no
    /// user-visible effect** — the declaration is valid CSS, survives the
    /// cascade, and is then dropped.
    ///
    /// `ComputedStyle` itself has no production consumer, so the readers that
    /// matter are the three ad-hoc style-string parsers on the live path:
    /// `layout::table_for`, the box painter, and the text painter. Each honours
    /// a *different* subset, so "a field exists" proves nothing; a property is
    /// honest only if at least one of those readers looks it up.
    ///
    /// Adding a `set_property` arm for one of these? Either implement it, or
    /// leave it here — but the honest state is: in the armed set AND in this
    /// table, until a reader exists. A property is never in both states: see
    /// `velox-cli/tests/lint_css_tests.rs::table_entries_all_have_a_set_property_arm`,
    /// which fails the moment the two drift apart.
    ///
    /// This table is the *only* thing `velox lint` reports. It deliberately
    /// does not report "anything unmatched": the cascade filters unknown
    /// declarations out silently, so flagging genuinely-unknown properties
    /// would contradict the spec — and would fire on every vendor prefix,
    /// custom property and deliberately-declined property.
    ///
    /// Format: `(property, what was checked and found not to render)`.
    pub const PARSED_BUT_UNRENDERED: &[(&str, &str)] = &[
        (
            "overflow-x",
            "arm at style.rs:1662 sets `overflow_x`; no reader looks up `overflow-x`",
        ),
        (
            "overflow-y",
            "arm at style.rs:1667 sets `overflow_y`; no reader looks up `overflow-y`",
        ),
        (
            "background-image",
            "arm at style.rs:1588 sets `background_image`; the box painter only reads `background`/`background-color`",
        ),
        (
            "font-style",
            "arm at style.rs:1621 sets `font_style`; the cascade now propagates it, but NO reader applies it — the renderer's `TextRenderConfig.font_style` is only ever set by `Default`, never copied from here",
        ),
        (
            "letter-spacing",
            "arm at style.rs:1633 sets `letter_spacing`; cascade-inheritable, but no reader applies it",
        ),
        (
            "visibility",
            "arm at style.rs:1674 sets `visibility`; NO reader looks up `visibility`, so `visibility: hidden` hides nothing",
        ),
        (
            "transform",
            "arm at style.rs:1681 sets `transform`; read at layout.rs:3695 ONLY to force a stacking context — no visual effect",
        ),
        (
            "box-shadow",
            "arm at style.rs:1688 sets `box_shadow`; the string name appears nowhere in the renderer",
        ),
        (
            "transition",
            "arm at style.rs:1693 pushes to `transitions`; no reader plays a transition",
        ),
        (
            "border-style",
            "arm at style.rs:1720 sets `border.style.*`; only `border`/`border-width` are read, so `border-style` alone does nothing",
        ),
        (
            "border-color",
            "arm at style.rs:1728 sets `border.color.*`; only `border`/`border-width` are read, so `border-color` alone does nothing",
        ),
    ];

    pub fn set_property(&mut self, prop: &str, value: &str) {
        let prop_lower = prop.to_lowercase();

        match prop_lower.as_str() {
            // Display
            "display" => {
                if let Some(d) = Display::parse(value) {
                    self.display = d;
                }
            }

            // Position
            "position" => {
                if let Some(p) = Position::parse(value) {
                    self.position = p;
                }
            }
            "z-index" | "zindex" => {
                if let Ok(z) = value.parse::<i32>() {
                    self.z_index = Some(z);
                }
            }
            "opacity" => {
                if let Ok(o) = value.parse::<f32>() {
                    self.opacity = o.clamp(0.0, 1.0);
                }
            }

            // Box model - dimensions
            "width" => {
                if let Some(l) = Length::parse(value) {
                    self.width = l;
                }
            }
            "height" => {
                if let Some(l) = Length::parse(value) {
                    self.height = l;
                }
            }
            "min-width" => {
                if let Some(l) = Length::parse(value) {
                    self.min_width = l;
                }
            }
            "min-height" => {
                if let Some(l) = Length::parse(value) {
                    self.min_height = l;
                }
            }
            "max-width" => {
                if let Some(l) = Length::parse(value) {
                    self.max_width = l;
                }
            }
            "max-height" => {
                if let Some(l) = Length::parse(value) {
                    self.max_height = l;
                }
            }

            // Box model - spacing
            "margin" => {
                if let Some(sides) = parse_sides_shorthand(value, Length::parse) {
                    self.margin = sides;
                }
            }
            "margin-top" => {
                if let Some(l) = Length::parse(value) {
                    self.margin.top = l;
                }
            }
            "margin-right" => {
                if let Some(l) = Length::parse(value) {
                    self.margin.right = l;
                }
            }
            "margin-bottom" => {
                if let Some(l) = Length::parse(value) {
                    self.margin.bottom = l;
                }
            }
            "margin-left" => {
                if let Some(l) = Length::parse(value) {
                    self.margin.left = l;
                }
            }
            "padding" => {
                if let Some(sides) = parse_sides_shorthand(value, Length::parse) {
                    self.padding = sides;
                }
            }
            "padding-top" => {
                if let Some(l) = Length::parse(value) {
                    self.padding.top = l;
                }
            }
            "padding-right" => {
                if let Some(l) = Length::parse(value) {
                    self.padding.right = l;
                }
            }
            "padding-bottom" => {
                if let Some(l) = Length::parse(value) {
                    self.padding.bottom = l;
                }
            }
            "padding-left" => {
                if let Some(l) = Length::parse(value) {
                    self.padding.left = l;
                }
            }

            // Positioning offsets
            "top" => {
                if let Some(l) = Length::parse(value) {
                    self.top = l;
                }
            }
            "right" => {
                if let Some(l) = Length::parse(value) {
                    self.right = l;
                }
            }
            "bottom" => {
                if let Some(l) = Length::parse(value) {
                    self.bottom = l;
                }
            }
            "left" => {
                if let Some(l) = Length::parse(value) {
                    self.left = l;
                }
            }

            // Flex
            "flex-direction" => {
                if let Some(fd) = FlexDirection::parse(value) {
                    self.flex_direction = fd;
                }
            }
            "flex-wrap" => {
                if let Some(fw) = FlexWrap::parse(value) {
                    self.flex_wrap = fw;
                }
            }
            "justify-content" => {
                if let Some(jc) = JustifyContent::parse(value) {
                    self.justify_content = jc;
                }
            }
            "align-items" => {
                if let Some(ai) = AlignItems::parse(value) {
                    self.align_items = ai;
                }
            }
            "align-self" => {
                if let Some(as_) = AlignSelf::parse(value) {
                    self.align_self = as_;
                }
            }
            "flex-grow" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.flex_grow = v;
                }
            }
            "flex-shrink" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.flex_shrink = v;
                }
            }
            "flex-basis" => {
                if let Some(l) = Length::parse(value) {
                    self.flex_basis = l;
                }
            }
            "gap" | "grid-gap" => {
                if let Some(l) = Length::parse(value) {
                    self.gap = l;
                    self.row_gap = l;
                    self.column_gap = l;
                }
            }
            "row-gap" => {
                if let Some(l) = Length::parse(value) {
                    self.row_gap = l;
                }
            }
            "column-gap" => {
                if let Some(l) = Length::parse(value) {
                    self.column_gap = l;
                }
            }

            // Background
            "background-color" => {
                if let Some(c) = Color::parse(value) {
                    self.background_color = c;
                }
            }
            "background-image" => {
                self.background_image = Some(value.to_string());
            }
            "background" => {
                // Minimal shorthand: color-only tokens (e.g. `background: #1a1a2e`).
                // Image/gradient tokens are out of scope and ignored here.
                for token in value.split_whitespace() {
                    if let Some(c) = Color::parse(token) {
                        self.background_color = c;
                        break;
                    }
                }
            }

            // Typography
            "color" => {
                if let Some(c) = Color::parse(value) {
                    self.color = c;
                }
            }
            "font-size" => {
                if let Some(l) = Length::parse(value) {
                    self.font_size = l;
                }
            }
            "font-family" => {
                self.font_family = value.to_string();
            }
            "font-weight" => {
                if let Some(fw) = FontWeight::parse(value) {
                    self.font_weight = fw;
                }
            }
            "font-style" => {
                if let Some(fs) = FontStyle::parse(value) {
                    self.font_style = fs;
                }
            }
            "line-height" => {
                if let Ok(lh) = value.parse::<f32>() {
                    self.line_height = Some(lh);
                } else if let Some(Length::Px(px)) = Length::parse(value) {
                    self.line_height = Some(px);
                }
            }
            "letter-spacing" => {
                if let Some(l) = Length::parse(value) {
                    self.letter_spacing = l;
                }
            }
            "text-align" => {
                if let Some(ta) = TextAlign::parse(value) {
                    self.text_align = ta;
                }
            }
            "vertical-align" => {
                if let Some(va) = VerticalAlign::parse(value) {
                    self.vertical_align = va;
                }
            }
            "text-decoration" => {
                if let Some(td) = TextDecoration::parse(value) {
                    self.text_decoration = td;
                }
            }

            // Overflow
            "overflow" => {
                if let Some(o) = Overflow::parse(value) {
                    self.overflow = o;
                    self.overflow_x = o;
                    self.overflow_y = o;
                }
            }
            "overflow-x" => {
                if let Some(o) = Overflow::parse(value) {
                    self.overflow_x = o;
                }
            }
            "overflow-y" => {
                if let Some(o) = Overflow::parse(value) {
                    self.overflow_y = o;
                }
            }

            // Visibility
            "visibility" => {
                if let Some(v) = Visibility::parse(value) {
                    self.visibility = v;
                }
            }

            // Transform
            "transform" => {
                if let Some(t) = Transform::parse(value) {
                    self.transform = t;
                }
            }

            // Box shadow
            "box-shadow" => {
                self.box_shadow = BoxShadow::parse(value);
            }

            // Transitions (comma-separated list of transition declarations)
            "transition" => {
                self.transitions.clear();
                for part in value.split(',') {
                    let part = part.trim();
                    if part.is_empty() || part.eq_ignore_ascii_case("none") {
                        continue;
                    }
                    if let Some(t) = Transition::parse(part) {
                        self.transitions.push(t);
                    }
                }
            }

            // Border shorthand
            "border" => {
                if let Some(sides) = parse_border_shorthand(value) {
                    self.border = sides;
                }
            }
            "border-width" => {
                if let Some(sides) = parse_sides_shorthand(value, Length::parse) {
                    self.border.width.top = sides.top;
                    self.border.width.right = sides.right;
                    self.border.width.bottom = sides.bottom;
                    self.border.width.left = sides.left;
                }
            }
            "border-style" => {
                if let Some(sides) = parse_sides_shorthand(value, BorderStyle::parse) {
                    self.border.style.top = sides.top;
                    self.border.style.right = sides.right;
                    self.border.style.bottom = sides.bottom;
                    self.border.style.left = sides.left;
                }
            }
            "border-color" => {
                if let Some(sides) = parse_sides_shorthand(value, Color::parse) {
                    self.border.color.top = sides.top;
                    self.border.color.right = sides.right;
                    self.border.color.bottom = sides.bottom;
                    self.border.color.left = sides.left;
                }
            }

            // Border radius
            "border-radius" => {
                if let Some(sides) = parse_sides_shorthand(value, Length::parse) {
                    self.border_radius = sides;
                }
            }

            // Box sizing
            "box-sizing" => {
                if let Some(bs) = BoxSizing::parse(value) {
                    self.box_sizing = bs;
                }
            }
            "white-space" => {
                if let Some(ws) = WhiteSpace::parse(value) {
                    self.white_space = ws;
                }
            }
            "text-overflow" => {
                if let Some(to) = TextOverflow::parse(value) {
                    self.text_overflow = to;
                }
            }

            _ => {}
        }
    }

    /// Check if this element creates a stacking context
    pub fn creates_stacking_context(&self) -> bool {
        self.position != Position::Static
            || self.z_index.is_some()
            || self.opacity < 1.0
            || !self.transform.is_empty()
    }

    /// Check if display is none
    pub fn is_display_none(&self) -> bool {
        self.display == Display::None
    }

    /// Check if visibility is hidden
    pub fn is_hidden(&self) -> bool {
        self.visibility == Visibility::Hidden
    }
}

/// Viewport-filling predicate for `ComputedStyle`, expanded to handle
/// `100% | 100vw | 100dvw | 100vh | 100dvh | min-height`.
/// `is_root_index` should be true when `source_index == 0` (first VNode always fills).
pub fn is_viewport_filling(style: &ComputedStyle, is_root_index: bool) -> bool {
    if is_root_index {
        return true;
    }
    let is_100 = |l: Length| match l {
        Length::Percent(v) => (v - 100.0).abs() < 0.01,
        Length::Vw(v) => (v - 100.0).abs() < 0.01,
        Length::Dvw(v) => (v - 100.0).abs() < 0.01,
        Length::Vh(v) => (v - 100.0).abs() < 0.01,
        Length::Dvh(v) => (v - 100.0).abs() < 0.01,
        _ => false,
    };
    let has_100pct_w = is_100(style.width) && matches!(style.width, Length::Percent(_));
    let has_vw = matches!(
        style.width,
        Length::Vw(v) | Length::Dvw(v) if (v - 100.0).abs() < 0.01
    );
    let has_viewport_h = is_100(style.height)
        && matches!(
            style.height,
            Length::Percent(_) | Length::Vh(_) | Length::Dvh(_)
        )
        || matches!(
            style.min_height,
            Length::Percent(v) | Length::Vh(v) | Length::Dvh(v) if (v - 100.0).abs() < 0.01
        );
    // strict combo retained as sufficient condition; root handles implicit fill.
    (has_100pct_w || has_vw) && has_viewport_h
}

/// Root-is-viewport-filling helper: index == 0 ⇒ true (first VNode always fills).
pub fn root_is_viewport_filling(index: usize) -> bool {
    index == 0
}

// Document-level defaults — UA sheet (velox-style::ua) supplies element-specific
// margins/padding/display; ComputedStyle::default() remains the zero baseline.
impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::default(),
            position: Position::default(),
            z_index: None,
            opacity: 1.0,
            width: Length::default(),
            height: Length::default(),
            min_width: Length::default(),
            min_height: Length::default(),
            max_width: Length::default(),
            max_height: Length::default(),
            margin: Sides::default(),
            padding: Sides::default(),
            border: Border::default(),
            border_radius: Sides::default(),
            box_sizing: BoxSizing::default(),
            top: Length::default(),
            right: Length::default(),
            bottom: Length::default(),
            left: Length::default(),
            flex_direction: FlexDirection::default(),
            flex_wrap: FlexWrap::default(),
            justify_content: JustifyContent::default(),
            align_items: AlignItems::default(),
            align_self: AlignSelf::default(),
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Length::Auto,
            gap: Length::default(),
            row_gap: Length::default(),
            column_gap: Length::default(),
            background_color: Color::default(),
            background_image: None,
            color: Color::default(),
            font_size: Length::Px(16.0), // Default font size
            font_family: "sans-serif".to_string(),
            font_weight: FontWeight::default(),
            font_style: FontStyle::default(),
            line_height: None,
            letter_spacing: Length::default(),
            text_align: TextAlign::default(),
            vertical_align: VerticalAlign::default(),
            text_decoration: TextDecoration::default(),
            overflow: Overflow::default(),
            overflow_x: Overflow::default(),
            overflow_y: Overflow::default(),
            visibility: Visibility::default(),
            transform: Transform::default(),
            box_shadow: None,
            white_space: WhiteSpace::default(),
            text_overflow: TextOverflow::default(),
            transitions: Vec::new(),
        }
    }
}

// Shorthand parsing functions
fn parse_sides_shorthand<T: Copy + Default>(
    value: &str,
    parser: impl Fn(&str) -> Option<T>,
) -> Option<Sides<T>> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    match parts.len() {
        1 => {
            let v = parser(parts[0])?;
            Some(Sides::all(v))
        }
        2 => {
            let v = parser(parts[0])?;
            let h = parser(parts[1])?;
            Some(Sides {
                top: v,
                right: h,
                bottom: v,
                left: h,
            })
        }
        3 => {
            let top = parser(parts[0])?;
            let h = parser(parts[1])?;
            let bottom = parser(parts[2])?;
            Some(Sides {
                top,
                right: h,
                bottom,
                left: h,
            })
        }
        4 => {
            let top = parser(parts[0])?;
            let right = parser(parts[1])?;
            let bottom = parser(parts[2])?;
            let left = parser(parts[3])?;
            Some(Sides {
                top,
                right,
                bottom,
                left,
            })
        }
        _ => None,
    }
}

fn parse_border_shorthand(value: &str) -> Option<Border> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    // Parse width
    let mut width_parts = Vec::new();
    for part in &parts {
        if let Some(w) = Length::parse(part) {
            width_parts.push(w);
        } else {
            break;
        }
    }

    // Parse style
    let mut style_parts = Vec::new();
    for part in &parts[width_parts.len()..] {
        if let Some(s) = BorderStyle::parse(part) {
            style_parts.push(s);
        } else {
            break;
        }
    }

    // Parse color
    let color_start = width_parts.len() + style_parts.len();
    let mut color_parts = Vec::new();
    for part in &parts[color_start..] {
        if let Some(c) = Color::parse(part) {
            color_parts.push(c);
        } else {
            break;
        }
    }

    // If no width specified, default to medium
    let default_width = Length::Px(3.0);
    let width_sides = if width_parts.len() == 1 {
        Sides::all(width_parts[0])
    } else if width_parts.len() == 2 {
        Sides {
            top: width_parts[0],
            right: width_parts[1],
            bottom: width_parts[0],
            left: width_parts[1],
        }
    } else if width_parts.len() == 3 {
        Sides {
            top: width_parts[0],
            right: width_parts[1],
            bottom: width_parts[2],
            left: width_parts[1],
        }
    } else if width_parts.len() == 4 {
        Sides {
            top: width_parts[0],
            right: width_parts[1],
            bottom: width_parts[2],
            left: width_parts[3],
        }
    } else {
        Sides::all(default_width)
    };

    // If no style specified, default to solid
    let default_style = BorderStyle::Solid;
    let style_sides = if style_parts.len() == 1 {
        Sides::all(style_parts[0])
    } else if style_parts.len() == 2 {
        Sides {
            top: style_parts[0],
            right: style_parts[1],
            bottom: style_parts[0],
            left: style_parts[1],
        }
    } else if style_parts.len() == 3 {
        Sides {
            top: style_parts[0],
            right: style_parts[1],
            bottom: style_parts[2],
            left: style_parts[1],
        }
    } else if style_parts.len() == 4 {
        Sides {
            top: style_parts[0],
            right: style_parts[1],
            bottom: style_parts[2],
            left: style_parts[3],
        }
    } else {
        Sides::all(default_style)
    };

    // If no color specified, default to black
    let default_color = Color::BLACK;
    let color_sides = if color_parts.len() == 1 {
        Sides::all(color_parts[0])
    } else if color_parts.len() == 2 {
        Sides {
            top: color_parts[0],
            right: color_parts[1],
            bottom: color_parts[0],
            left: color_parts[1],
        }
    } else if color_parts.len() == 3 {
        Sides {
            top: color_parts[0],
            right: color_parts[1],
            bottom: color_parts[2],
            left: color_parts[1],
        }
    } else if color_parts.len() == 4 {
        Sides {
            top: color_parts[0],
            right: color_parts[1],
            bottom: color_parts[2],
            left: color_parts[3],
        }
    } else {
        Sides::all(default_color)
    };

    Some(Border {
        width: width_sides,
        style: style_sides,
        color: color_sides,
    })
}

/// Core auto-margin distribution (CSS 2.1 §10.3.3).
///
/// Given the containing block width (`avail_w`), the element's border-box
/// outer width *excluding margins* (`outer_w`), the numerically resolved
/// side margins (`ml`/`mr` — values on auto sides are ignored), and the
/// per-side auto flags, spreads the free space `avail_w - outer_w - fixed
/// margins` across the auto margins:
///
/// - both sides auto: free space splits equally between them (their used
///   values are equal, per spec);
/// - one side auto: it absorbs the remainder after the fixed opposite margin;
/// - free space may be negative and auto margins resolve negative — an
///   over-wide box centers with negative margins, as in browsers. No
///   clamping.
pub fn resolve_auto_margins_core(
    avail_w: f32,
    outer_w: f32,
    ml: f32,
    mr: f32,
    ml_auto: bool,
    mr_auto: bool,
) -> (f32, f32) {
    let fixed = (if ml_auto { 0.0 } else { ml }) + (if mr_auto { 0.0 } else { mr });
    let free = avail_w - outer_w - fixed;
    match (ml_auto, mr_auto) {
        (true, true) => {
            let each = free / 2.0;
            (each, each)
        }
        (true, false) => (free, mr),
        (false, true) => (ml, free),
        (false, false) => (ml, mr),
    }
}

/// Resolve `margin: auto` for a block-level box (CSS 2.1 §10.3.3).
///
/// Given the containing block width (`avail_w`), the computed style and the
/// declared width in px (`None` when `width: auto`), returns the resolved
/// `(margin_left, margin_right)` pair in px. When neither side is auto the
/// declared margins resolve against `avail_w` unchanged. When `width` is
/// auto the box fills the containing block and auto margins resolve to 0.
///
/// Resolution basis: percentage paddings/borders resolve against `avail_w`
/// (the containing block width), `rem` against the default 16px root size.
/// `em`/viewport-unit paddings are approximated because the pure helper has
/// no font-size/viewport context; the layout path, which has full context,
/// feeds exact px values into [`resolve_auto_margins_core`] instead.
pub fn resolve_auto_margins(
    avail_w: f32,
    style: &ComputedStyle,
    declared_w: Option<f32>,
) -> (f32, f32) {
    let px = |l: Length| l.to_px(avail_w, 16.0, (avail_w, avail_w));
    let ml = px(style.margin.left);
    let mr = px(style.margin.right);
    let ml_auto = style.margin.left.is_auto();
    let mr_auto = style.margin.right.is_auto();
    if !ml_auto && !mr_auto {
        return (ml, mr);
    }
    let Some(dw) = declared_w else {
        return (
            if ml_auto { 0.0 } else { ml },
            if mr_auto { 0.0 } else { mr },
        );
    };
    // Border-box: declared width already includes padding+border, so no
    // double subtraction (reconciled with content_size_for, F-04).
    let outer_w = match style.box_sizing {
        BoxSizing::BorderBox => dw,
        BoxSizing::ContentBox => {
            dw + px(style.padding.left)
                + px(style.padding.right)
                + px(style.border.width.left)
                + px(style.border.width.right)
        }
    };
    resolve_auto_margins_core(avail_w, outer_w, ml, mr, ml_auto, mr_auto)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_length_parsing() {
        assert_eq!(Length::parse("10px"), Some(Length::Px(10.0)));
        assert_eq!(Length::parse("50%"), Some(Length::Percent(50.0)));
        assert_eq!(Length::parse("1.5rem"), Some(Length::Rem(1.5)));
        assert_eq!(Length::parse("2em"), Some(Length::Em(2.0)));
        assert_eq!(Length::parse("auto"), Some(Length::Auto));
        assert_eq!(Length::parse("0"), Some(Length::Zero));
    }

    #[test]
    fn test_color_parsing() {
        assert_eq!(Color::parse("#ff0000"), Some(Color::RED));
        assert_eq!(Color::parse("#f00"), Some(Color::RED));
        assert_eq!(Color::parse("red"), Some(Color::RED));
        assert_eq!(Color::parse("rgb(255, 0, 0)"), Some(Color::RED));
        assert_eq!(Color::parse("rgba(255, 0, 0, 1.0)"), Some(Color::RED));
    }

    #[test]
    fn test_display_hidden_alias() {
        assert_eq!(Display::parse("none"), Some(Display::None));
        assert_eq!(Display::parse("hidden"), Some(Display::None));
    }

    #[test]
    fn test_position_parsing() {
        assert_eq!(Position::parse("relative"), Some(Position::Relative));
        assert_eq!(Position::parse("absolute"), Some(Position::Absolute));
        assert_eq!(Position::parse("fixed"), Some(Position::Fixed));
        assert_eq!(Position::parse("static"), Some(Position::Static));
    }

    #[test]
    fn test_overflow_parsing() {
        assert_eq!(Overflow::parse("hidden"), Some(Overflow::Hidden));
        assert_eq!(Overflow::parse("scroll"), Some(Overflow::Scroll));
        assert_eq!(Overflow::parse("auto"), Some(Overflow::Auto));
        assert_eq!(Overflow::parse("visible"), Some(Overflow::Visible));
    }

    #[test]
    fn test_set_property_position_overflow_zindex() {
        let mut cs = ComputedStyle::new();
        cs.set_property("position", "absolute");
        cs.set_property("overflow", "hidden");
        cs.set_property("z-index", "10");
        assert_eq!(cs.position, Position::Absolute);
        assert_eq!(cs.overflow, Overflow::Hidden);
        assert_eq!(cs.z_index, Some(10));
    }

    #[test]
    fn test_set_property_transform() {
        let mut cs = ComputedStyle::new();
        cs.set_property(
            "transform",
            "translate(10px, 20px) rotate(45deg) scale(1.5)",
        );
        assert_eq!(
            cs.transform.operations,
            vec![
                TransformOp::Translate(Length::Px(10.0), Length::Px(20.0)),
                TransformOp::Rotate(45.0),
                TransformOp::Scale(1.5, 1.5),
            ]
        );
        // translateX / scaleY / none
        cs.set_property("transform", "translateX(5px) scaleY(2)");
        assert_eq!(
            cs.transform.operations,
            vec![
                TransformOp::TranslateX(Length::Px(5.0)),
                TransformOp::ScaleY(2.0)
            ]
        );
        cs.set_property("transform", "none");
        assert!(cs.transform.is_empty());
    }

    #[test]
    fn test_set_property_box_shadow() {
        let mut cs = ComputedStyle::new();
        cs.set_property("box-shadow", "2px 3px 4px rgba(0,0,0,0.5)");
        let bs = cs.box_shadow.as_ref().expect("box shadow should be set");
        assert_eq!(bs.offset_x, Length::Px(2.0));
        assert_eq!(bs.offset_y, Length::Px(3.0));
        assert_eq!(bs.blur_radius, Length::Px(4.0));
        assert!(!bs.inset);
        // inset + color
        cs.set_property("box-shadow", "inset 0 2px #000");
        let bs = cs.box_shadow.as_ref().expect("inset shadow set");
        assert!(bs.inset);
        assert_eq!(bs.color, Color::BLACK);
        // none clears
        cs.set_property("box-shadow", "none");
        assert!(cs.box_shadow.is_none());
    }

    #[test]
    fn test_set_property_transition() {
        let mut cs = ComputedStyle::new();
        cs.set_property("transition", "opacity 0.2s ease-in 0.1s, transform 300ms");
        assert_eq!(cs.transitions.len(), 2);
        assert_eq!(cs.transitions[0].property, "opacity");
        assert_eq!(cs.transitions[0].duration, 0.2);
        assert_eq!(cs.transitions[0].timing_function, TimingFunction::EaseIn);
        assert_eq!(cs.transitions[0].delay, 0.1);
        assert_eq!(cs.transitions[1].property, "transform");
        assert_eq!(cs.transitions[1].duration, 0.3);
        // transition: none clears
        cs.set_property("transition", "none");
        assert!(cs.transitions.is_empty());
    }

    #[test]
    fn test_margin_shorthand_preserves_auto() {
        // `margin: 0 auto` => top/bottom = 0, left/right = auto
        let mut cs = ComputedStyle::new();
        cs.set_property("margin", "0 auto");
        assert_eq!(cs.margin.top, Length::Zero);
        assert_eq!(cs.margin.bottom, Length::Zero);
        assert_eq!(cs.margin.left, Length::Auto);
        assert_eq!(cs.margin.right, Length::Auto);

        // `margin: 10px auto` => vertical margins preserved, horizontal auto
        let mut cs = ComputedStyle::new();
        cs.set_property("margin", "10px auto");
        assert_eq!(cs.margin.top, Length::Px(10.0));
        assert_eq!(cs.margin.bottom, Length::Px(10.0));
        assert_eq!(cs.margin.left, Length::Auto);
        assert_eq!(cs.margin.right, Length::Auto);

        // 4-value shorthand: top right bottom left
        let mut cs = ComputedStyle::new();
        cs.set_property("margin", "1px auto 3px auto");
        assert_eq!(cs.margin.top, Length::Px(1.0));
        assert_eq!(cs.margin.bottom, Length::Px(3.0));
        assert_eq!(cs.margin.left, Length::Auto);
        assert_eq!(cs.margin.right, Length::Auto);

        // longhand auto overrides shorthand value
        let mut cs = ComputedStyle::new();
        cs.set_property("margin", "8px");
        cs.set_property("margin-left", "auto");
        assert_eq!(cs.margin.top, Length::Px(8.0));
        assert_eq!(cs.margin.left, Length::Auto);
    }

    #[test]
    fn test_background_shorthand_parses_color() {
        let mut cs = ComputedStyle::new();
        cs.set_property("background", "#1a1a2e");
        assert_eq!(cs.background_color, Color::new(0x1a, 0x1a, 0x2e, 255));

        // bare named color token; non-color tokens are ignored
        let mut cs = ComputedStyle::new();
        cs.set_property("background", "red no-repeat");
        assert_eq!(cs.background_color, Color::RED);

        // unknown values must not panic or clear defaults unexpectedly
        let mut cs = ComputedStyle::new();
        cs.set_property("background", "url(img.png)");
        assert_eq!(cs.background_color, Color::default());
    }

    #[test]
    fn test_border_shorthand_width_style_color() {
        let mut cs = ComputedStyle::new();
        cs.set_property("border", "1px solid #fff");
        assert_eq!(cs.border.width.top, Length::Px(1.0));
        assert_eq!(cs.border.width.right, Length::Px(1.0));
        assert_eq!(cs.border.style.top, BorderStyle::Solid);
        assert_eq!(cs.border.color.top, Color::new(255, 255, 255, 255));

        // partial shorthand keeps defaults for missing components
        let mut cs = ComputedStyle::new();
        cs.set_property("border", "4px solid");
        assert_eq!(cs.border.width.top, Length::Px(4.0));
        assert_eq!(cs.border.style.top, BorderStyle::Solid);
        assert_eq!(cs.border.color.top, Color::BLACK);
    }

    #[test]
    fn test_resolve_auto_margins_both_sides() {
        let mut cs = ComputedStyle::new();
        cs.set_property("margin", "0 auto");
        let (ml, mr) = resolve_auto_margins(800.0, &cs, Some(200.0));
        assert_eq!(ml, 300.0);
        assert_eq!(mr, 300.0);
    }

    #[test]
    fn test_resolve_auto_margins_single_side_subtracts_fixed_opposite() {
        let mut cs = ComputedStyle::new();
        cs.set_property("margin-left", "auto");
        cs.set_property("margin-right", "40px");
        let (ml, mr) = resolve_auto_margins(800.0, &cs, Some(200.0));
        assert_eq!(ml, 560.0); // 800 - 200 - 40
        assert_eq!(mr, 40.0);
    }

    #[test]
    fn test_resolve_auto_margins_content_box_subtracts_padding_border_once() {
        let mut cs = ComputedStyle::new();
        cs.set_property("margin", "0 auto");
        cs.set_property("padding", "0 20px");
        cs.set_property("border", "2px solid");
        // outer = 200 + 2*20 + 2*2 = 244; free = 556 -> 278 each
        let (ml, mr) = resolve_auto_margins(800.0, &cs, Some(200.0));
        assert_eq!(ml, 278.0);
        assert_eq!(mr, 278.0);
    }

    #[test]
    fn test_resolve_auto_margins_border_box_no_double_subtraction() {
        let mut cs = ComputedStyle::new();
        cs.set_property("box-sizing", "border-box");
        cs.set_property("margin", "0 auto");
        cs.set_property("padding", "0 20px");
        // declared 200 already contains padding; free = 600 -> 300 each
        let (ml, mr) = resolve_auto_margins(800.0, &cs, Some(200.0));
        assert_eq!(ml, 300.0);
        assert_eq!(mr, 300.0);
    }

    #[test]
    fn test_resolve_auto_margins_declared_none_resolves_zero() {
        let mut cs = ComputedStyle::new();
        cs.set_property("margin", "0 auto");
        let (ml, mr) = resolve_auto_margins(800.0, &cs, None);
        assert_eq!((ml, mr), (0.0, 0.0));
    }

    #[test]
    fn test_resolve_auto_margins_negative_free_space_resolves_negative() {
        // CSS 2.1 §10.3.3: auto margins resolve from the constraint equation
        // with no clamping — an over-wide box centers with negative margins.
        let mut cs = ComputedStyle::new();
        cs.set_property("margin", "0 auto");
        let (ml, mr) = resolve_auto_margins(800.0, &cs, Some(900.0));
        assert_eq!((ml, mr), (-50.0, -50.0)); // (800 - 900) / 2

        // one auto side absorbs the negative leftover: 800 - 900 - 0
        let mut cs = ComputedStyle::new();
        cs.set_property("margin-left", "auto");
        let (ml, mr) = resolve_auto_margins(800.0, &cs, Some(900.0));
        assert_eq!((ml, mr), (-100.0, 0.0));
    }

    #[test]
    fn test_resolve_auto_margins_without_auto_returns_declared() {
        let mut cs = ComputedStyle::new();
        // 2-value shorthand: top/bottom = 10px, left/right = 20px
        cs.set_property("margin", "10px 20px");
        let (ml, mr) = resolve_auto_margins(800.0, &cs, Some(200.0));
        assert_eq!((ml, mr), (20.0, 20.0));
    }

    #[test]
    fn test_resolve_auto_margins_percent_padding_resolves_against_avail() {
        let mut cs = ComputedStyle::new();
        cs.set_property("margin", "0 auto");
        cs.set_property("padding", "0 10%");
        // outer = 200 + 2*80 = 360; free = 440 -> 220 each
        let (ml, mr) = resolve_auto_margins(800.0, &cs, Some(200.0));
        assert_eq!(ml, 220.0);
        assert_eq!(mr, 220.0);
    }
}
