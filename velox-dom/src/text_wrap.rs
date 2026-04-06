// Text wrapping and layout utilities
//
// Provides word-by-word text wrapping with proper line breaking
// for the layout engine.

use crate::layout::LayoutNode;
use crate::layout::{FontMetrics, Rect};

/// Result of wrapping text into lines
pub struct TextLine {
    pub text: String,
    pub width: i32,
    pub height: i32,
}

/// Wrap text into lines based on available width
pub fn wrap_text(text: &str, max_width: i32, font_size_px: f32) -> Vec<TextLine> {
    let metrics = FontMetrics::from_font_size(font_size_px);
    let char_width = metrics.char_width;
    let line_height = metrics.line_height.round() as i32;
    let space_w = (char_width * 0.5).round() as i32;

    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return vec![TextLine {
            text: String::new(),
            width: 0,
            height: line_height,
        }];
    }

    let mut lines: Vec<TextLine> = Vec::new();
    let mut line_words: Vec<&str> = Vec::new();
    let mut line_width: i32 = 0;

    for word in &words {
        let word_w = (word.chars().count() as f32 * char_width).round() as i32;
        let needed = if line_words.is_empty() {
            word_w
        } else {
            line_width + space_w + word_w
        };

        if !line_words.is_empty() && needed > max_width {
            // Emit current line and start new one
            let line_text = line_words.join(" ");
            let lw = (line_text.chars().count() as f32 * char_width).round() as i32;
            lines.push(TextLine {
                text: line_text,
                width: lw,
                height: line_height,
            });
            line_words.clear();
            line_width = 0;
        }

        let add_space = !line_words.is_empty();
        line_words.push(word);
        line_width = if add_space { line_width + space_w } else { 0 } + word_w;
    }

    // Emit remaining line
    if !line_words.is_empty() {
        let line_text = line_words.join(" ");
        let lw = (line_text.chars().count() as f32 * char_width).round() as i32;
        lines.push(TextLine {
            text: line_text,
            width: lw,
            height: line_height,
        });
    }

    lines
}

/// Create layout nodes for wrapped text lines
pub fn create_text_nodes(
    text: &str,
    x: i32,
    y: i32,
    max_width: i32,
    font_size_px: f32,
    source_index: Option<usize>,
) -> Vec<LayoutNode> {
    let lines = wrap_text(text, max_width, font_size_px);
    let mut nodes = Vec::new();
    let mut cur_y = y;

    for line in lines {
        nodes.push(LayoutNode {
            rect: Rect {
                x,
                y: cur_y,
                w: line.width,
                h: line.height,
            },
            z_index: 0,
            display_none: false,
            source_index,
            scroll_x: 0,
            scroll_y: 0,
            clip: None,
            stacking_context: false,
            children: vec![],
        });
        cur_y += line.height;
    }

    nodes
}
