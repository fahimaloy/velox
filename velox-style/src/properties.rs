//! CSS Properties Module
//!
//! Comprehensive CSS properties support for Velox styling system.

use crate::units::*;
use std::collections::HashMap;

/// Represents all CSS box sides (margin, padding, border)
#[derive(Debug, Clone, Copy, PartialEq)]
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

impl<T: Copy> Sides<T> {
    pub fn new(top: T, right: T, bottom: T, left: T) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    pub fn all(value: T) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

/// Border style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

/// Border properties
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    pub width: Length,
    pub style: BorderStyle,
    pub color: Color,
}

impl Default for Border {
    fn default() -> Self {
        Self {
            width: Length::Px(0.0),
            style: BorderStyle::None,
            color: Color::BLACK,
        }
    }
}

/// Text alignment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
            "center" | "centre" => Some(TextAlign::Center),
            "right" => Some(TextAlign::Right),
            "justify" => Some(TextAlign::Justify),
            _ => None,
        }
    }
}

/// Font weight
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum FontWeight {
    #[default]
    Normal,
    Bold,
    Bolder,
    Lighter,
    Value(u16), // 100-900
}

impl FontWeight {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_lowercase();
        match s.as_str() {
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
            FontWeight::Bolder => 700,
            FontWeight::Lighter => 300,
            FontWeight::Value(v) => *v,
        }
    }
}

/// Text decoration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

/// Flex wrap
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

/// Computed CSS styles for a single element
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    // Layout
    pub display: Display,
    pub position: Position,
    pub z_index: Option<i32>,
    pub opacity: f32,

    // Box model
    pub width: Length,
    pub height: Length,
    pub min_width: Length,
    pub min_height: Length,
    pub max_width: Length,
    pub max_height: Length,
    pub margin: Sides<Length>,
    pub padding: Sides<Length>,
    pub border: Sides<Border>,
    pub border_radius: Sides<Length>,
    pub box_sizing: BoxSizing,

    // Positioning offsets
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,

    // Flex properties
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

    // Background
    pub background_color: Color,
    pub background_image: Option<String>,

    // Typography
    pub color: Color,
    pub font_size: Length,
    pub font_family: String,
    pub font_weight: FontWeight,
    pub line_height: Option<f32>,
    pub letter_spacing: Length,
    pub text_align: TextAlign,
    pub text_decoration: TextDecoration,

    // Overflow
    pub overflow: Overflow,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,

    // Visibility
    pub visibility: Visibility,

    // Transform
    pub transform: Transform,

    // Box shadow (simplified - just a single shadow)
    pub box_shadow: Option<BoxShadow>,
}

/// Box sizing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

/// Visibility
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

/// Transform
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

/// Transform operations
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransformOp {
    Translate(Length, Length),
    TranslateX(Length),
    TranslateY(Length),
    Rotate(f32), // degrees
    Scale(f32, f32),
    ScaleX(f32),
    ScaleY(f32),
    Skew(f32, f32), // degrees
}

/// Box shadow
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    pub offset_x: Length,
    pub offset_y: Length,
    pub blur_radius: Length,
    pub spread_radius: Length,
    pub color: Color,
    pub inset: bool,
}

/// Parse 1-4 value shorthand for sides
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

/// Parse CSS border shorthand "width style color" (any order)
fn parse_border_shorthand(value: &str) -> Option<Sides<Border>> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let mut width = None;
    let mut style = None;
    let mut color = None;

    for part in parts {
        if let Some(w) = Length::parse(part) {
            width = Some(w);
        } else if let Some(s) = BorderStyle::parse(part) {
            style = Some(s);
        } else if let Some(c) = Color::parse(part) {
            color = Some(c);
        }
    }

    let border = Border {
        width: width.unwrap_or(Length::Px(3.0)), // Default medium
        style: style.unwrap_or(BorderStyle::None),
        color: color.unwrap_or(Color::BLACK),
    };

    Some(Sides::all(border))
}

impl ComputedStyle {
    pub fn new() -> Self {
        Self {
            display: Display::default(),
            position: Position::default(),
            z_index: None,
            opacity: 1.0,
            width: Length::Auto,
            height: Length::Auto,
            min_width: Length::Zero,
            min_height: Length::Zero,
            max_width: Length::Auto,
            max_height: Length::Auto,
            margin: Sides::all(Length::Zero),
            padding: Sides::all(Length::Zero),
            border: Sides::default(),
            border_radius: Sides::all(Length::Zero),
            box_sizing: BoxSizing::default(),
            top: Length::Auto,
            right: Length::Auto,
            bottom: Length::Auto,
            left: Length::Auto,
            flex_direction: FlexDirection::default(),
            flex_wrap: FlexWrap::NoWrap,
            justify_content: JustifyContent::default(),
            align_items: AlignItems::default(),
            align_self: AlignSelf::default(),
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Length::Auto,
            gap: Length::Zero,
            row_gap: Length::Zero,
            column_gap: Length::Zero,
            background_color: Color::TRANSPARENT,
            background_image: None,
            color: Color::BLACK,
            font_size: Length::Px(16.0),
            font_family: String::from("sans-serif"),
            font_weight: FontWeight::default(),
            line_height: None,
            letter_spacing: Length::Zero,
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

    /// Parse inline style string and apply to this style
    pub fn apply_inline_style(&mut self, style: &str) {
        for decl in style.split(';') {
            let decl = decl.trim();
            if decl.is_empty() {
                continue;
            }

            if let Some((prop, value)) = decl.split_once(':') {
                let prop = prop.trim();
                let value = value.trim();
                self.set_property(prop, value);
            }
        }
    }

    /// Set a CSS property
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
                    // Could store as Length, keeping it simple for now
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
                    self.border.top.width = sides.top;
                    self.border.right.width = sides.right;
                    self.border.bottom.width = sides.bottom;
                    self.border.left.width = sides.left;
                }
            }
            "border-style" => {
                if let Some(sides) = parse_sides_shorthand(value, BorderStyle::parse) {
                    self.border.top.style = sides.top;
                    self.border.right.style = sides.right;
                    self.border.bottom.style = sides.bottom;
                    self.border.left.style = sides.left;
                }
            }
            "border-color" => {
                if let Some(sides) = parse_sides_shorthand(value, Color::parse) {
                    self.border.top.color = sides.top;
                    self.border.right.color = sides.right;
                    self.border.bottom.color = sides.bottom;
                    self.border.left.color = sides.left;
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
        Self::new()
    }
}

/// Style sheet for applying styles to VNodes
#[derive(Debug, Clone, Default)]
pub struct StyleSheet {
    pub rules: Vec<StyleRule>,
}

/// A CSS rule with selector and declarations
#[derive(Debug, Clone)]
pub struct StyleRule {
    pub selector: Selector,
    pub declarations: HashMap<String, String>,
}

/// CSS selector (simplified)
#[derive(Debug, Clone, PartialEq)]
pub enum Selector {
    Universal,                                // *
    Element(String),                          // div
    Class(String),                            // .class
    Id(String),                               // #id
    Descendant(Box<Selector>, Box<Selector>), // parent child
    Child(Box<Selector>, Box<Selector>),      // parent > child
    Pseudo(Box<Selector>, String),            // :hover, :focus, etc.
}

impl StyleSheet {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Add a rule to the stylesheet
    pub fn add_rule(&mut self, selector: Selector, declarations: HashMap<String, String>) {
        self.rules.push(StyleRule {
            selector,
            declarations,
        });
    }

    /// Parse a simple CSS string and add rules
    pub fn parse(css: &str) -> Self {
        let mut sheet = Self::new();

        // Simple CSS parser - looks for selector { declarations }
        let mut in_decl = false;
        let mut current_selector = String::new();
        let mut current_decls = HashMap::new();

        for line in css.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if line.starts_with('{') {
                in_decl = true;
                continue;
            }

            if line.ends_with('}') {
                // End of rule
                if !current_selector.is_empty() {
                    let selector = Self::parse_selector(&current_selector);
                    sheet.add_rule(selector, current_decls.clone());
                }
                in_decl = false;
                current_selector.clear();
                current_decls.clear();
                continue;
            }

            if !in_decl {
                // This is a selector
                current_selector = line.trim_end_matches('{').trim().to_string();
            } else {
                // This is a declaration
                if let Some((prop, val)) = line.split_once(':') {
                    let prop = prop.trim().to_string();
                    let val = val.trim_end_matches(';').trim().to_string();
                    current_decls.insert(prop, val);
                }
            }
        }

        sheet
    }

    fn parse_selector(s: &str) -> Selector {
        let s = s.trim();

        // Universal
        if s == "*" {
            return Selector::Universal;
        }

        // ID
        if let Some(id) = s.strip_prefix('#') {
            return Selector::Id(id.to_string());
        }

        // Class
        if let Some(cls) = s.strip_prefix('.') {
            return Selector::Class(cls.to_string());
        }

        // Pseudo selector
        if let Some((base, pseudo)) = s.split_once(':') {
            let base_sel = Self::parse_selector(base.trim());
            return Selector::Pseudo(Box::new(base_sel), pseudo.trim().to_string());
        }

        // Child selector
        if s.contains('>') {
            let parts: Vec<&str> = s.split('>').collect();
            if parts.len() == 2 {
                let parent = Self::parse_selector(parts[0].trim());
                let child = Self::parse_selector(parts[1].trim());
                return Selector::Child(Box::new(parent), Box::new(child));
            }
        }

        // Descendant selector
        if s.contains(' ') {
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() == 2 {
                let ancestor = Self::parse_selector(parts[0]);
                let descendant = Self::parse_selector(parts[1]);
                return Selector::Descendant(Box::new(ancestor), Box::new(descendant));
            }
        }

        // Element
        Selector::Element(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_computed_style() {
        let mut style = ComputedStyle::new();

        style.set_property("width", "100px");
        assert_eq!(style.width, Length::Px(100.0));

        style.set_property("margin", "10px");
        assert_eq!(style.margin.top, Length::Px(10.0));
        assert_eq!(style.margin.right, Length::Px(10.0));

        style.set_property("background-color", "#ff0000");
        assert_eq!(style.background_color, Color::RED);
    }

    #[test]
    fn test_stylesheet_parse() {
        let css = r#"
            .container {
                width: 100px;
                height: 50px;
            }
            #main {
                color: red;
            }
        "#;

        let sheet = StyleSheet::parse(css);
        assert_eq!(sheet.rules.len(), 2);
    }
}
