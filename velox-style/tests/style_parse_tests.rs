use velox_style::{Stylesheet};

#[test]
fn parses_color_and_fontsize_from_style_string() {
    let css = ".app { background: #101216; color: #e6edf3; font-size: 18px; }";
    let ss = Stylesheet::parse(css);
    assert_eq!(ss.rules.len(), 1);
}

