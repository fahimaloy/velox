//! Human-friendly parse diagnostics for the SFC compiler.
//!
//! Produces rustc-style error output: a one-line summary prefixed with the
//! offending `line:col`, a short excerpt of the offending source line with a
//! `^` caret marking the position, and (where known) an actionable suggestion.
//!
//! Example rendered output:
//!
//! ```text
//! SFC parse error at 3:17
//!   |
//! 3 | <template> <div>{{ count </div>
//!   |                ^^^^^^^^^
//!   |
//!   = help: close the interpolation with '}}'
//! ```

/// Convert a byte offset into a 1-based (line, column) pair. Columns are
/// counted in characters from the start of the line. Newlines (`\n`) end a
/// line; `\r` is treated as part of the preceding line and does not advance it.
pub fn line_col_at(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (idx, ch) in source.char_indices() {
        if idx >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Return the full text of the 1-based `line_number` (without the trailing
/// newline), or `None` when the line does not exist.
fn line_text(source: &str, line_number: usize) -> Option<&str> {
    source.lines().nth(line_number.saturating_sub(1))
}

/// Render a caret-style error message against `source`.
///
/// * `line` – 1-based line number of the error.
/// * `column` – 1-based byte/char column of the error (start of the caret).
/// * `width` – number of columns the caret should underline (defaults to 1).
/// * `message` – the human-readable problem description.
/// * `suggestion` – optional `help:` line with an actionable fix.
pub fn render_parse_error(
    source: &str,
    line: usize,
    column: usize,
    width: usize,
    message: &str,
    suggestion: Option<&str>,
) -> String {
    let caret_width = width.max(1);
    let gutter_width = line.to_string().len();
    let gutter = " ".repeat(gutter_width);

    let mut out = String::new();
    out.push_str(&format!("SFC parse error at {line}:{column}: {message}\n"));

    match line_text(source, line) {
        Some(text) => {
            // Only render the source excerpt when the line actually exists.
            out.push_str(&format!("{gutter} |\n"));
            out.push_str(&format!("{line} | {text}\n"));
            // Pad in *characters* so the caret lines up with the source line even
            // when multi-byte UTF-8 (e.g. CJK) appears before the error.
            let pad_chars = column.saturating_sub(1);
            let text_chars = text.chars().count();
            let underline_len = if pad_chars > text_chars {
                // Error column is past the end of the line; just cap the caret.
                (text_chars.saturating_sub(pad_chars)).max(1)
            } else {
                // Clamp the underline so it does not spill past the line end.
                usize::min(caret_width, text_chars.saturating_sub(pad_chars).max(1))
            };
            out.push_str(&format!("{gutter} | {}{}", " ".repeat(pad_chars), "^"));
            for _ in 1..underline_len {
                out.push('^');
            }
            out.push('\n');
        }
        None => {
            // No source line available (e.g. error at EOF with a blank tail).
            out.push_str(&format!("{gutter} |\n"));
            out.push_str(&format!("{line}| <end of input>\n"));
        }
    }

    if let Some(help) = suggestion {
        out.push_str(&format!("{gutter} |\n"));
        out.push_str(&format!("{gutter} = help: {help}\n"));
    }

    out
}