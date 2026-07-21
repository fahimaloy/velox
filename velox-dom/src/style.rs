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
    pub line_height: Option<f32>,
    pub letter_spacing: Length,
    pub text_align: TextAlign,
    pub text_decoration: TextDecoration,
    pub overflow: Overflow,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    pub visibility: Visibility,
    pub transform: Transform,
    pub box_shadow: Option<BoxShadow>,
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
            line_height: None,
            letter_spacing: Length::default(),
            text_align: TextAlign::default(),
            text_decoration: TextDecoration::default(),
            overflow: Overflow::default(),
            overflow_x: Overflow::default(),
            overflow_y: Overflow::default(),
            visibility: Visibility::default(),
            transform: Transform::default(),
            box_shadow: None,
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
}
