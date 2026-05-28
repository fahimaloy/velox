//! Visual effects and decorations for CSS
//!
//! Handles border-radius, opacity, shadows, and other visual effects

use velox_dom::style::Length;

/// Border radius for corners
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderRadius {
    pub top_left: Length,
    pub top_right: Length,
    pub bottom_right: Length,
    pub bottom_left: Length,
}

impl BorderRadius {
    pub fn parse(value: &str) -> Option<Self> {
        let parts: Vec<&str> = value.split_whitespace().collect();

        match parts.len() {
            1 => {
                let v = Length::parse(parts[0])?;
                Some(Self {
                    top_left: v,
                    top_right: v,
                    bottom_right: v,
                    bottom_left: v,
                })
            }
            2 => {
                let h = Length::parse(parts[0])?;
                let v = Length::parse(parts[1])?;
                Some(Self {
                    top_left: h,
                    top_right: v,
                    bottom_right: h,
                    bottom_left: v,
                })
            }
            3 => {
                let tl = Length::parse(parts[0])?;
                let tr = Length::parse(parts[1])?;
                let br = Length::parse(parts[2])?;
                Some(Self {
                    top_left: tl,
                    top_right: tr,
                    bottom_right: br,
                    bottom_left: tr,
                })
            }
            4 => {
                let tl = Length::parse(parts[0])?;
                let tr = Length::parse(parts[1])?;
                let br = Length::parse(parts[2])?;
                let bl = Length::parse(parts[3])?;
                Some(Self {
                    top_left: tl,
                    top_right: tr,
                    bottom_right: br,
                    bottom_left: bl,
                })
            }
            _ => None,
        }
    }

    pub fn all(radius: Length) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }
}

impl Default for BorderRadius {
    fn default() -> Self {
        Self::all(Length::Zero)
    }
}

/// Box shadow effect
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    /// Horizontal offset
    pub offset_x: f32,
    /// Vertical offset
    pub offset_y: f32,
    /// Blur radius
    pub blur: f32,
    /// Spread radius (optional)
    pub spread: f32,
    /// Shadow color (rgba)
    pub color: (u8, u8, u8, u8),
    /// Inset shadow?
    pub inset: bool,
}

impl BoxShadow {
    pub fn new(offset_x: f32, offset_y: f32, blur: f32, color: (u8, u8, u8, u8)) -> Self {
        Self {
            offset_x,
            offset_y,
            blur,
            spread: 0.0,
            color,
            inset: false,
        }
    }

    pub fn with_spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }

    pub fn inset(mut self) -> Self {
        self.inset = true;
        self
    }

    /// Parse box-shadow: offset-x offset-y blur spread color
    /// Example: "2px 2px 5px 1px rgba(0,0,0,0.3)"
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        let inset = value.starts_with("inset");
        let value = if inset {
            value.strip_prefix("inset").unwrap().trim()
        } else {
            value
        };

        let mut offset_x = 0.0;
        let mut offset_y = 0.0;
        let mut blur = 0.0;
        let mut spread = 0.0;
        let mut color = (0u8, 0u8, 0u8, 128u8);

        // Collect all parts as owned Strings, handling multi-token color values like "rgba(0, 0, 0, 0.3)"
        let mut parts: Vec<String> = Vec::new();
        let mut current_part = String::new();
        let mut paren_depth = 0;

        for token in value.split_whitespace() {
            let open_parens = token.matches('(').count();
            let close_parens = token.matches(')').count();

            if paren_depth == 0 && open_parens == 0 && close_parens == 0 {
                // Simple token - add directly
                parts.push(token.to_string());
            } else {
                // Part of a function-like value (e.g., rgba, rgb)
                if paren_depth == 0 {
                    current_part.clear();
                }
                if !current_part.is_empty() {
                    current_part.push(' ');
                }
                current_part.push_str(token);
                paren_depth += open_parens as i32 - close_parens as i32;

                if paren_depth == 0 && !current_part.is_empty() {
                    parts.push(current_part.clone());
                }
            }
        }

        // If no multi-token colors were found, fall back to simple split
        if parts.is_empty() {
            parts = value.split_whitespace().map(|s| s.to_string()).collect();
        }

        let mut part_idx = 0;

        // Parse offsets and blur
        if part_idx < parts.len() {
            let p = &parts[part_idx];
            offset_x = p
                .strip_suffix("px")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            part_idx += 1;
        }
        if part_idx < parts.len() {
            let p = &parts[part_idx];
            offset_y = p
                .strip_suffix("px")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            part_idx += 1;
        }
        if part_idx < parts.len() {
            let p = &parts[part_idx];
            blur = p
                .strip_suffix("px")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            part_idx += 1;
        }
        // Fourth token could be spread (px) or color
        if part_idx < parts.len() {
            let p = &parts[part_idx];
            if p.ends_with("px") {
                spread = p
                    .strip_suffix("px")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0.0);
                part_idx += 1;
            }
        }
        // Remaining token(s) should be color
        if part_idx < parts.len() {
            let color_str = parts[part_idx..].join(" ");
            if let Some(c) = velox_dom::style::Color::parse(&color_str) {
                color = (c.r, c.g, c.b, c.a);
            }
        }

        Some(BoxShadow {
            offset_x,
            offset_y,
            blur,
            spread,
            color,
            inset,
        })
    }
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 0.0,
            color: (0, 0, 0, 128),
            inset: false,
        }
    }
}

/// Text shadow effect
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextShadow {
    /// Horizontal offset
    pub offset_x: f32,
    /// Vertical offset
    pub offset_y: f32,
    /// Blur radius
    pub blur: f32,
    /// Shadow color (rgba)
    pub color: (u8, u8, u8, u8),
}

impl TextShadow {
    pub fn new(offset_x: f32, offset_y: f32, blur: f32, color: (u8, u8, u8, u8)) -> Self {
        Self {
            offset_x,
            offset_y,
            blur,
            color,
        }
    }

    /// Parse text-shadow: offset-x offset-y blur color
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        let mut offset_x = 0.0;
        let mut offset_y = 0.0;
        let mut blur = 0.0;
        let mut color = (0u8, 0u8, 0u8, 128u8);

        // Collect all parts as owned Strings, handling multi-token color values like "rgba(0, 0, 0, 0.3)"
        let mut parts: Vec<String> = Vec::new();
        let mut current_part = String::new();
        let mut paren_depth = 0;

        for token in value.split_whitespace() {
            let open_parens = token.matches('(').count();
            let close_parens = token.matches(')').count();

            if paren_depth == 0 && open_parens == 0 && close_parens == 0 {
                parts.push(token.to_string());
            } else {
                if paren_depth == 0 {
                    current_part.clear();
                }
                if !current_part.is_empty() {
                    current_part.push(' ');
                }
                current_part.push_str(token);
                paren_depth += open_parens as i32 - close_parens as i32;

                if paren_depth == 0 && !current_part.is_empty() {
                    parts.push(current_part.clone());
                }
            }
        }

        if parts.is_empty() {
            parts = value.split_whitespace().map(|s| s.to_string()).collect();
        }

        let mut part_idx = 0;

        if part_idx < parts.len() {
            let p = &parts[part_idx];
            offset_x = p
                .strip_suffix("px")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            part_idx += 1;
        }
        if part_idx < parts.len() {
            let p = &parts[part_idx];
            offset_y = p
                .strip_suffix("px")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            part_idx += 1;
        }
        // Third token could be blur (px) or color
        if part_idx < parts.len() {
            let p = &parts[part_idx];
            if p.ends_with("px") {
                blur = p
                    .strip_suffix("px")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0.0);
                part_idx += 1;
            }
        }
        // Remaining token(s) should be color
        if part_idx < parts.len() {
            let color_str = parts[part_idx..].join(" ");
            if let Some(c) = velox_dom::style::Color::parse(&color_str) {
                color = (c.r, c.g, c.b, c.a);
            }
        }

        Some(TextShadow {
            offset_x,
            offset_y,
            blur,
            color,
        })
    }
}

impl Default for TextShadow {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            color: (0, 0, 0, 128),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_border_radius_single() {
        let br = BorderRadius::parse("8px").unwrap();
        assert_eq!(br.top_left, Length::Px(8.0));
        assert_eq!(br.bottom_right, Length::Px(8.0));
    }

    #[test]
    fn test_border_radius_four() {
        let br = BorderRadius::parse("4px 8px 12px 16px").unwrap();
        assert_eq!(br.top_left, Length::Px(4.0));
        assert_eq!(br.top_right, Length::Px(8.0));
        assert_eq!(br.bottom_right, Length::Px(12.0));
        assert_eq!(br.bottom_left, Length::Px(16.0));
    }

    #[test]
    fn test_box_shadow() {
        let shadow = BoxShadow::new(2.0, 2.0, 5.0, (0, 0, 0, 200));
        assert_eq!(shadow.offset_x, 2.0);
        assert_eq!(shadow.blur, 5.0);
        assert!(!shadow.inset);
    }
}
