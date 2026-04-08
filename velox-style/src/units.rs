//! CSS Units and Values
//!
//! This module provides types for CSS length units, colors, and other values
//! with proper parsing and computation support.

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
    pub const TRANSPARENT: Color = Color { r: 0, g: 0, b: 0, a: 0 };
    /// Black
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };
    /// White
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };
    /// Red
    pub const RED: Color = Color { r: 255, g: 0, b: 0, a: 255 };
    /// Green
    pub const GREEN: Color = Color { r: 0, g: 255, b: 0, a: 255 };
    /// Blue
    pub const BLUE: Color = Color { r: 0, g: 0, b: 255, a: 255 };
    
    /// Modern dark theme colors
    pub const DARK_BG: Color = Color { r: 26, g: 26, b: 26, a: 255 };         // #1a1a1a
    pub const DARK_TEXT: Color = Color { r: 224, g: 224, b: 224, a: 255 };    // #e0e0e0
    pub const DARK_CARD: Color = Color { r: 38, g: 38, b: 38, a: 255 };       // #262626
    pub const ACCENT_BLUE: Color = Color { r: 52, g: 120, b: 246, a: 255 };   // #3478f6
    pub const ACCENT_GREEN: Color = Color { r: 34, g: 197, b: 94, a: 255 };   // #22c55e
    pub const BORDER_DARK: Color = Color { r: 55, g: 65, b: 81, a: 255 };     // #374151
    
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
            "yellow" => Color { r: 255, g: 255, b: 0, a: 255 },
            "cyan" => Color { r: 0, g: 255, b: 255, a: 255 },
            "magenta" => Color { r: 255, g: 0, b: 255, a: 255 },
            "silver" => Color { r: 192, g: 192, b: 192, a: 255 },
            "gray" | "grey" => Color { r: 128, g: 128, b: 128, a: 255 },
            "maroon" => Color { r: 128, g: 0, b: 0, a: 255 },
            "olive" => Color { r: 128, g: 128, b: 0, a: 255 },
            "lime" => Color { r: 0, g: 255, b: 0, a: 255 },
            "aqua" => Color { r: 0, g: 255, b: 255, a: 255 },
            "teal" => Color { r: 0, g: 128, b: 128, a: 255 },
            "navy" => Color { r: 0, g: 0, b: 128, a: 255 },
            "fuchsia" => Color { r: 255, g: 0, b: 255, a: 255 },
            "purple" => Color { r: 128, g: 0, b: 128, a: 255 },
            "orange" => Color { r: 255, g: 165, b: 0, a: 255 },
            "pink" => Color { r: 255, g: 192, b: 203, a: 255 },
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
        let inner = s.trim_start_matches("rgb(").trim_start_matches("rgba(")
            .trim_end_matches(')').trim();
        
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
            write!(f, "rgba({}, {}, {}, {})", self.r, self.g, self.b, self.a as f32 / 255.0)
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Color::BLACK
    }
}

/// CSS display property
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

/// CSS position property
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

/// CSS flex direction
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

/// CSS justify-content values
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
            "flex-start" => Some(JustifyContent::FlexStart),
            "flex-end" => Some(JustifyContent::FlexEnd),
            "center" => Some(JustifyContent::Center),
            "space-between" => Some(JustifyContent::SpaceBetween),
            "space-around" => Some(JustifyContent::SpaceAround),
            "space-evenly" => Some(JustifyContent::SpaceEvenly),
            _ => None,
        }
    }
}

/// CSS align-items values
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
            "flex-start" => Some(AlignItems::FlexStart),
            "flex-end" => Some(AlignItems::FlexEnd),
            "center" => Some(AlignItems::Center),
            "baseline" => Some(AlignItems::Baseline),
            "stretch" => Some(AlignItems::Stretch),
            _ => None,
        }
    }
}

/// CSS align-self values
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
            "flex-start" => Some(AlignSelf::FlexStart),
            "flex-end" => Some(AlignSelf::FlexEnd),
            "center" => Some(AlignSelf::Center),
            "baseline" => Some(AlignSelf::Baseline),
            "stretch" => Some(AlignSelf::Stretch),
            _ => None,
        }
    }
}

/// CSS overflow values
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