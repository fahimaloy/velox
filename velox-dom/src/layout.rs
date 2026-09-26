use crate::{Length, VNode};

/// Default font size for root element (used for rem calculations)
const DEFAULT_ROOT_FONT_SIZE: f32 = 16.0;

/// Sentinel value for unconstrained cross-size in flex layout (fit-content)
/// CSS Flexbox spec: when cross-size is indefinite, children lay out at natural size
const UNCONSTRAINED_CROSS_SIZE: f32 = i32::MAX as f32;

/// Font metrics for text measurement
pub struct FontMetrics {
    pub char_width: f32,
    pub line_height: f32,
}

impl FontMetrics {
    pub fn from_font_size(font_size_px: f32) -> Self {
        // Approximate character width ratio for typical fonts
        // '0' (zero) is roughly 0.6 * font_size for most fonts
        let char_width = font_size_px * 0.6;
        // Line height is typically 1.2 * font_size
        let line_height = font_size_px * 1.2;
        Self {
            char_width,
            line_height,
        }
    }
}

/// Returns true when the given source index corresponds to the root VNode.
/// Per spec, the first VNode (index 0 or None for the outermost) always fills the viewport.
/// Only the outermost `compute_layout` call (None) is considered the viewport root;
/// children with `Some(0)` are *not* viewport roots — otherwise every first child
/// would incorrectly fill the viewport and break fit-content sizing.
pub fn root_is_viewport_filling(source_index: Option<usize>) -> bool {
    source_index.is_none()
}

/// Compatibility helper for flat lists where caller tracks numeric index.
/// Index 0 => viewport root per spec phrasing.
pub fn root_is_viewport_filling_index(index: usize) -> bool {
    index == 0
}

/// Expanded viewport-filling predicate for layout's inline-style path.
/// Handles 100% | 100vw | 100dvw | 100vh | 100dvh and min-height variants.
/// `is_root_index` should be `root_is_viewport_filling(source_index)` for the element being laid out.
/// Per spec the first VNode always fills the viewport; explicit fixed sizes are
/// still respected via rect priority (declared wins) so this may return true
/// even when width is fixed — the layout rect will prefer the declared value.
pub fn is_viewport_filling(style: Option<&str>, is_root_index: bool) -> bool {
    if is_root_index {
        return true;
    }
    let parse = |key: &str| -> Option<Length> {
        let s = style?;
        for decl in s.split(';') {
            let d = decl.trim();
            if d.is_empty() {
                continue;
            }
            if let Some((k, v)) = d.split_once(':')
                && k.trim() == key
            {
                return Length::parse(v.trim());
            }
        }
        None
    };
    let width = parse("width");
    let height = parse("height");
    let min_h = parse("min-height");
    // shallow helper for ~100 checks
    let is_100 = |l: Length| match l {
        Length::Percent(v) => (v - 100.0).abs() < 0.01,
        Length::Vw(v) => (v - 100.0).abs() < 0.01,
        Length::Dvw(v) => (v - 100.0).abs() < 0.01,
        Length::Vh(v) => (v - 100.0).abs() < 0.01,
        Length::Dvh(v) => (v - 100.0).abs() < 0.01,
        _ => false,
    };
    let has_100pct_w =
        width.map(is_100).unwrap_or(false) && matches!(width, Some(Length::Percent(_)));
    let has_vw = matches!(
        width,
        Some(Length::Vw(v)) | Some(Length::Dvw(v)) if (v - 100.0).abs() < 0.01
    );
    let has_viewport_h = match height {
        Some(Length::Vh(v)) | Some(Length::Dvh(v)) if (v - 100.0).abs() < 0.01 => true,
        Some(Length::Percent(v)) if (v - 100.0).abs() < 0.01 => true,
        _ => false,
    } || match min_h {
        Some(Length::Vh(v)) | Some(Length::Dvh(v)) if (v - 100.0).abs() < 0.01 => true,
        Some(Length::Percent(v)) if (v - 100.0).abs() < 0.01 => true,
        _ => false,
    };
    // keep old strict combo as sufficient (width 100%|vw) && viewport_h, but root already true covers implicit fill.
    (has_100pct_w || has_vw) && has_viewport_h
}

/// Spec-compliant margin collapse per CSS 2.1 §8.3.1 (positive/negative partition).
/// Consumes two adjacent vertical margins (f32 px) and produces collapsed value:
/// - both >=0 => max (positives)
/// - both <=0 => min (most negative)
/// - opposite signs => sum
pub fn collapse_margins(a: f32, b: f32) -> f32 {
    if a >= 0.0 && b >= 0.0 {
        a.max(b)
    } else if a <= 0.0 && b <= 0.0 {
        a.min(b)
    } else {
        a + b
    }
}

#[allow(dead_code)]
fn collapse(a: f32, b: f32) -> f32 {
    collapse_margins(a, b)
}

/// Definite cross-size helper per CX-03/05.
/// For column flex (cross=width) definite if parent cross definite OR resolved width exists.
/// For row flex (cross=height) definite only if resolved height exists.
fn has_definite_cross(is_column: bool, parent_definite: bool, resolved_cross: Option<i32>) -> bool {
    if is_column {
        parent_definite || resolved_cross.is_some()
    } else {
        resolved_cross.is_some()
    }
}

/// Content vs border-box outer size resolver (F-04).
/// When border-box and no declared size, fallback to avail minus margins (outer fills).
fn content_size_for(
    declared: Option<i32>,
    avail: i32,
    is_border_box: bool,
    pl: i32,
    pr: i32,
    bl: i32,
    br: i32,
    ml: i32,
    mr: i32,
    is_viewport_filling: bool,
    legacy_pair: bool,
) -> i32 {
    if is_border_box {
        if let Some(dw) = declared {
            dw
        } else {
            (avail - ml - mr).max(1)
        }
    } else if let Some(dw) = declared {
        dw + pl + pr + bl + br
    } else if is_viewport_filling || legacy_pair {
        (avail - ml - mr).max(1)
    } else {
        avail
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutNode {
    pub rect: Rect,
    pub z_index: i32,
    pub display_none: bool,
    pub source_index: Option<usize>,
    pub scroll_x: i32,
    pub scroll_y: i32,
    pub clip: Option<Rect>,
    pub stacking_context: bool,
    /// Total scrollable content height (content_h + padding + border).
    pub scroll_height: i32,
    /// Maximum vertical scroll offset (scroll_height - rect.h) clamped to 0.
    pub max_scroll_y: i32,
    /// True when overflow is auto/scroll and content exceeds rect.
    pub scrollable: bool,
    pub children: Vec<LayoutNode>,
}

/// ScrollState tracks the current vertical scroll offset and its clamp.
/// Mutated by `on_wheel(delta)` / `scroll_by(delta)`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollState {
    pub offset_y: f32,
    pub max_y: f32,
}

impl ScrollState {
    pub fn new(max_y: f32) -> Self {
        Self {
            offset_y: 0.0,
            max_y,
        }
    }
    /// Advance by `delta` and clamp to `[0, max_y]`.
    pub fn scroll_by(&mut self, delta: f32) {
        self.offset_y = (self.offset_y + delta).clamp(0.0, self.max_y.max(0.0));
    }
    /// Wheel entry point: advance by a wheel `delta` (positive increases the
    /// offset, i.e. content moves up) and clamp to `[0, max_y]`.
    pub fn on_wheel(&mut self, delta: f32) {
        self.scroll_by(delta);
    }
}

/// Returns true when `overflow` is `auto` or `scroll` and content exceeds viewport.
pub fn is_scrollable(overflow: &str, content_h: f32, rect_h: f32) -> bool {
    matches!(overflow, "auto" | "scroll") && content_h > rect_h
}

fn translate_layout_subtree(node: &mut LayoutNode, dx: i32, dy: i32) {
    node.rect.x += dx;
    node.rect.y += dy;
    if let Some(clip) = &mut node.clip {
        clip.x += dx;
        clip.y += dy;
    }
    for child in &mut node.children {
        translate_layout_subtree(child, dx, dy);
    }
}

fn translate_layout_descendants(node: &mut LayoutNode, dx: i32, dy: i32) {
    for child in &mut node.children {
        translate_layout_subtree(child, dx, dy);
    }
}

#[allow(dead_code)]
fn parse_px(s: &str) -> Option<i32> {
    let t = s.trim();
    if let Some(px) = t.strip_suffix("px") {
        px.trim().parse().ok()
    } else {
        t.parse().ok()
    }
}

#[allow(dead_code)]
fn style_lookup(style: Option<&str>, key: &str) -> Option<i32> {
    let s = style?;
    for decl in s.split(';') {
        let d = decl.trim();
        if d.is_empty() {
            continue;
        }
        if let Some((k, v)) = d.split_once(':')
            && k.trim() == key
        {
            return parse_px(v);
        }
    }
    None
}

#[allow(dead_code)]
fn style_lookup_len(style: Option<&str>, key: &str, base: i32) -> Option<i32> {
    let s = style?;
    for decl in s.split(';') {
        let d = decl.trim();
        if d.is_empty() {
            continue;
        }
        if let Some((k, v)) = d.split_once(':')
            && k.trim() == key
        {
            let val = v.trim();
            if let Some(p) = val.strip_suffix('%')
                && let Ok(pct) = p.trim().parse::<f32>()
            {
                return Some(((pct / 100.0) * base as f32).round() as i32);
            }
            return parse_px(val);
        }
    }
    None
}

/// Extract font-size from style string, resolving all units to pixels.
/// Returns None if font-size is not declared, allowing callers to use inherited/default.
fn style_lookup_font_size(
    style: Option<&str>,
    parent_font_size: f32,
    root_font_size: f32,
    viewport: (f32, f32),
) -> Option<f32> {
    let s = style?;
    for decl in s.split(';') {
        let d = decl.trim();
        if d.is_empty() {
            continue;
        }
        if let Some((k, v)) = d.split_once(':')
            && k.trim() == "font-size"
        {
            return parse_length_value(
                v.trim(),
                parent_font_size,
                parent_font_size,
                root_font_size,
                viewport,
            );
        }
    }
    None
}

/// Resolve a CSS length value with ALL units to pixels.
///
/// Supported units:
/// - `px`: direct pixel value
/// - `%`: percentage of `parent_size`
/// - `rem`: relative to `root_font_size`
/// - `em`: relative to `parent_font_size`
/// - `vw`: percentage of viewport width
/// - `vh`: percentage of viewport height
/// - `auto`: returns None (caller decides)
/// - `0`: returns Some(0)
/// - plain number: treated as pixels
fn style_lookup_len_full(
    style: Option<&str>,
    key: &str,
    parent_size: f32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> Option<i32> {
    let s = style?;
    for decl in s.split(';') {
        let d = decl.trim();
        if d.is_empty() {
            continue;
        }
        if let Some((k, v)) = d.split_once(':')
            && k.trim() == key
        {
            let val = v.trim();
            return parse_length_value(
                val,
                parent_size,
                parent_font_size,
                root_font_size,
                (viewport_w, viewport_h),
            )
            .map(|f| f.round() as i32);
        }
    }
    None
}

/// Parse a single CSS length value string and convert to pixels.
fn parse_length_value(
    val: &str,
    parent_size: f32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport: (f32, f32),
) -> Option<f32> {
    let val = val.trim();

    if val == "auto" {
        return None;
    }

    if val == "0" {
        return Some(0.0);
    }

    // Percentage
    if let Some(p) = val.strip_suffix('%')
        && let Ok(pct) = p.trim().parse::<f32>()
    {
        let pct = if pct.is_finite() { pct } else { 0.0 };
        return Some((pct / 100.0) * parent_size);
    }

    // Pixels
    if let Some(px) = val.strip_suffix("px")
        && let Ok(v) = px.trim().parse::<f32>()
    {
        return Some(if v.is_finite() { v } else { 0.0 });
    }

    // rem (root em)
    if let Some(rem) = val.strip_suffix("rem")
        && let Ok(v) = rem.trim().parse::<f32>()
    {
        let v = if v.is_finite() { v } else { 0.0 };
        return Some(v * root_font_size);
    }

    // em (parent-relative)
    if let Some(em) = val.strip_suffix("em")
        && let Ok(v) = em.trim().parse::<f32>()
    {
        let v = if v.is_finite() { v } else { 0.0 };
        return Some(v * parent_font_size);
    }

    // dynamic viewport width
    if let Some(dvw) = val.strip_suffix("dvw")
        && let Ok(v) = dvw.trim().parse::<f32>()
    {
        let v = if v.is_finite() { v } else { 0.0 };
        return Some((v / 100.0) * viewport.0);
    }

    // dynamic viewport height
    if let Some(dvh) = val.strip_suffix("dvh")
        && let Ok(v) = dvh.trim().parse::<f32>()
    {
        let v = if v.is_finite() { v } else { 0.0 };
        return Some((v / 100.0) * viewport.1);
    }

    // viewport width
    if let Some(vw) = val.strip_suffix("vw")
        && let Ok(v) = vw.trim().parse::<f32>()
    {
        let v = if v.is_finite() { v } else { 0.0 };
        return Some((v / 100.0) * viewport.0);
    }

    // viewport height
    if let Some(vh) = val.strip_suffix("vh")
        && let Ok(v) = vh.trim().parse::<f32>()
    {
        let v = if v.is_finite() { v } else { 0.0 };
        return Some((v / 100.0) * viewport.1);
    }

    // Plain number -> pixels
    if let Ok(v) = val.parse::<f32>() {
        return Some(if v.is_finite() { v } else { 0.0 });
    }

    None
}

fn style_lookup_str(style: Option<&str>, key: &str) -> Option<String> {
    let s = style?;
    for decl in s.split(';') {
        let d = decl.trim();
        if d.is_empty() {
            continue;
        }
        if let Some((k, v)) = d.split_once(':')
            && k.trim() == key
        {
            return Some(v.trim().to_string());
        }
    }
    None
}

/// Elements whose Velox default `display` is `inline`, i.e. the elements that
/// get a `display: inline` rule in the `velox-style/src/ua.css` UA sheet.
///
/// Public so `velox-style`'s cascade test can assert that the UA sheet and
/// this table carry the same set: `ua.css` is the cascade-side source of these
/// defaults, but velox-dom cannot depend on velox-style, so the same list is
/// kept here for the layout engine's own notion of an element's default
/// display. `ua_inline_tag_list_matches_the_layout_fallback_table` in
/// `velox-style/tests/cascade.rs` fails if the two drift apart.
///
/// In browsers almost all of these take CSS's initial value, `inline`,
/// because the HTML rendering section declares no `display` for them. The
/// documented deviations from a literal browser reading:
///
/// - `progress` and `meter` are `inline-block` in browsers, not `inline`.
///   Velox has no `inline-block` layout, and `display: inline` is a far closer
///   approximation than `block`, so they are claimed as `inline`. Do not
///   "correct" them back to block; an `inline-block` UA rule is not available
///   until inline-block layout exists.
/// - The obsolete phrasing elements `big`, `tt`, `font`, `nobr` and `strike`
///   are also `inline` in browsers, but are deliberately out of scope: nothing
///   in the framework or the examples needs them, and every tag in this list
///   is meant to be individually checkable against the spec.
///
/// `wbr` belongs here because its initial `display` is `inline`;
/// `display-outside: break-opportunity` is a separate property Velox does not
/// implement (same for `br`'s `display-outside: newline`).
pub const INLINE_BY_DEFAULT_TAGS: &[&str] = &[
    "a", "abbr", "b", "bdi", "bdo", "cite", "code", "data", "del", "dfn", "em", "i", "img", "ins",
    "kbd", "label", "mark", "meter", "output", "progress", "q", "s", "samp", "small", "span",
    "strong", "sub", "sup", "time", "u", "var", "wbr",
];

/// Elements that are `inline-block` in browsers (`input, button { display:
/// inline-block; }` in the HTML rendering section; `select` and `textarea` in
/// the browser UA sheets) and so are *inline-level* for the whitespace rules
/// below, but which Velox deliberately leaves defaulting to `block`.
///
/// This table answers exactly one question — "would this neighbour share a line
/// with a sibling in a browser?" — and nothing else. It is disjoint from
/// [`INLINE_BY_DEFAULT_TAGS`] (a tag is never in both), and these elements get
/// no UA `display` rule: claiming `display: inline` for them would be a false
/// parity claim, and `inline-block` has no layout implementation yet.
///
/// The known cost, recorded so it is not mistaken for a bug in the table:
/// until inline layout exists these elements lay out as block boxes in Velox,
/// so a preserved space beside one produces a vertical gap a browser would not
/// have. Dropping the space instead would be worse — it is the same trade-off
/// the `inline` elements already make, and it is what the task's negative
/// control forbids.
const INLINE_BLOCK_BY_DEFAULT_TAGS: &[&str] = &["button", "input", "select", "textarea"];

/// The `display` an element gets when neither the cascade nor an author rule
/// specifies one.
fn default_display_for_tag(tag: &str) -> &'static str {
    if INLINE_BY_DEFAULT_TAGS.contains(&tag.to_ascii_lowercase().as_str()) {
        "inline"
    } else {
        "block"
    }
}

/// Whether an element with no explicit `display` is inline-level, i.e. it
/// would sit in an inline formatting context next to its siblings.
fn is_inline_level_by_default(tag: &str) -> bool {
    let tag = tag.to_ascii_lowercase();
    INLINE_BY_DEFAULT_TAGS.contains(&tag.as_str())
        || INLINE_BLOCK_BY_DEFAULT_TAGS.contains(&tag.as_str())
}

fn is_inline_formatting_participant(node: &VNode) -> bool {
    match node {
        VNode::Text(text) => !text.chars().all(|c| c.is_whitespace()),
        VNode::Element { tag, props, .. } => {
            let style = props.attrs.get("style").map(|s| s.as_str());
            let display =
                style_lookup_str(style, "display").map(|value| value.trim().to_ascii_lowercase());
            let position = style_lookup_str(style, "position")
                .map(|value| value.trim().to_ascii_lowercase())
                .unwrap_or_else(|| "static".to_string());
            if position == "absolute" || position == "fixed" || display.as_deref() == Some("none") {
                return false;
            }
            match display.as_deref() {
                Some("inline") | Some("inline-block") | Some("inline-flex")
                | Some("inline-grid") => true,
                Some(_) => false,
                None => is_inline_level_by_default(tag),
            }
        }
    }
}

fn is_formatting_participant(node: &VNode) -> bool {
    match node {
        VNode::Text(text) => !text.chars().all(|c| c.is_whitespace()),
        VNode::Element { props, .. } => {
            let style = props.attrs.get("style").map(|s| s.as_str());
            let display = style_lookup_str(style, "display")
                .map(|value| value.trim().to_ascii_lowercase())
                .unwrap_or_else(|| "static".to_string());
            let position = style_lookup_str(style, "position")
                .map(|value| value.trim().to_ascii_lowercase())
                .unwrap_or_else(|| "static".to_string());
            display != "none" && position != "absolute" && position != "fixed"
        }
    }
}

fn should_drop_collapsible_whitespace(
    children: &[VNode],
    idx: usize,
    parent_style: Option<&str>,
) -> bool {
    let Some(VNode::Text(text)) = children.get(idx) else {
        return false;
    };
    if !text.chars().all(|c| c.is_whitespace()) {
        return false;
    }
    let white_space = style_lookup_str(parent_style, "white-space")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "normal".to_string());
    if !matches!(white_space.as_str(), "normal" | "nowrap") {
        return false;
    }
    let has_inline_before = children[..idx]
        .iter()
        .rev()
        .find(|candidate| is_formatting_participant(candidate))
        .is_some_and(is_inline_formatting_participant);
    let has_inline_after = children[idx + 1..]
        .iter()
        .find(|candidate| is_formatting_participant(candidate))
        .is_some_and(is_inline_formatting_participant);
    !has_inline_before || !has_inline_after
}

fn style_lookup_i32(style: Option<&str>, key: &str) -> Option<i32> {
    let s = style?;
    for decl in s.split(';') {
        let d = decl.trim();
        if d.is_empty() {
            continue;
        }
        if let Some((k, v)) = d.split_once(':')
            && k.trim() == key
        {
            return v.trim().parse::<i32>().ok();
        }
    }
    None
}

#[allow(dead_code)]
fn style_box_sides(style: Option<&str>, base: &str) -> (i32, i32, i32, i32) {
    // returns (left, right, top, bottom)
    let s = style.unwrap_or("");
    let get = |k: &str| -> Option<i32> {
        for decl in s.split(';') {
            let d = decl.trim();
            if d.is_empty() {
                continue;
            }
            if let Some((kk, vv)) = d.split_once(':')
                && kk.trim() == k
            {
                return parse_px(vv);
            }
        }
        None
    };
    let all = get(base).unwrap_or(0);
    let l = get(&format!("{}-left", base)).unwrap_or(all);
    let r = get(&format!("{}-right", base)).unwrap_or(all);
    let t = get(&format!("{}-top", base)).unwrap_or(all);
    let b = get(&format!("{}-bottom", base)).unwrap_or(all);
    (l, r, t, b)
}

fn apply_relative_position(
    style: Option<&str>,
    node: &mut LayoutNode,
    base_w: i32,
    base_h: i32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) {
    let pos = style_lookup_str(style, "position").unwrap_or_else(|| "static".to_string());
    if pos != "relative" && pos != "sticky" {
        return;
    }
    if let Some(left) = style_lookup_len_full(
        style,
        "left",
        base_w as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    ) {
        node.rect.x += left;
    } else if let Some(right) = style_lookup_len_full(
        style,
        "right",
        base_w as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    ) {
        node.rect.x -= right;
    }
    if let Some(top) = style_lookup_len_full(
        style,
        "top",
        base_h as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    ) {
        node.rect.y += top;
    } else if let Some(bottom) = style_lookup_len_full(
        style,
        "bottom",
        base_h as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    ) {
        node.rect.y -= bottom;
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_sticky_position(
    style: Option<&str>,
    node: &mut LayoutNode,
    container_x: i32,
    container_y: i32,
    container_w: i32,
    container_h: i32,
    scroll_offset_x: i32,
    scroll_offset_y: i32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) {
    let pos = style_lookup_str(style, "position").unwrap_or_else(|| "static".to_string());
    if pos != "sticky" {
        return;
    }

    // The sticky element's natural position (where it would be without sticky)
    let natural_x = node.rect.x;
    let natural_y = node.rect.y;

    // The viewport/scroll container edges
    let viewport_left = scroll_offset_x;
    let viewport_top = scroll_offset_y;
    let viewport_right = scroll_offset_x + container_w;
    let viewport_bottom = scroll_offset_y + container_h;

    // Sticky: element is constrained by both its natural position AND sticky offsets
    // For top/left: take the MAX of natural and sticky constraint
    // For bottom/right: take the MIN of natural and sticky constraint
    // Then clamp within parent container

    // Horizontal sticky
    let sticky_left = style_lookup_len_full(
        style,
        "left",
        container_w as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );
    let sticky_right = style_lookup_len_full(
        style,
        "right",
        container_w as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );

    let mut final_x = natural_x;
    if let Some(left) = sticky_left {
        // Element should not go above viewport_left + left
        let sticky_x = viewport_left + left;
        final_x = final_x.max(sticky_x);
    }
    if let Some(right) = sticky_right {
        // Element should not go beyond viewport_right - right
        let sticky_x = viewport_right - right - node.rect.w;
        final_x = final_x.min(sticky_x);
    }

    // Vertical sticky
    let sticky_top = style_lookup_len_full(
        style,
        "top",
        container_h as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );
    let sticky_bottom = style_lookup_len_full(
        style,
        "bottom",
        container_h as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );

    let mut final_y = natural_y;
    if let Some(top) = sticky_top {
        // Element should not go above viewport_top + top
        let sticky_y = viewport_top + top;
        final_y = final_y.max(sticky_y);
    }
    if let Some(bottom) = sticky_bottom {
        // Element should not go beyond viewport_bottom - bottom
        let sticky_y = viewport_bottom - bottom - node.rect.h;
        final_y = final_y.min(sticky_y);
    }

    // Clamp within parent container bounds
    final_x = final_x.max(container_x);
    final_x = final_x.min(container_x + container_w - node.rect.w);
    final_y = final_y.max(container_y);
    final_y = final_y.min(container_y + container_h - node.rect.h);

    node.rect.x = final_x;
    node.rect.y = final_y;
}

#[allow(clippy::too_many_arguments)]
fn apply_absolute_position(
    style: Option<&str>,
    node: &mut LayoutNode,
    cb_x: i32,
    cb_y: i32,
    cb_w: i32,
    cb_h: i32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) {
    let left = style_lookup_len_full(
        style,
        "left",
        cb_w as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );
    let right = style_lookup_len_full(
        style,
        "right",
        cb_w as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );
    let top = style_lookup_len_full(
        style,
        "top",
        cb_h as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );
    let bottom = style_lookup_len_full(
        style,
        "bottom",
        cb_h as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );
    let declared_w = style_lookup_len_full(
        style,
        "width",
        cb_w as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );
    let declared_h = style_lookup_len_full(
        style,
        "height",
        cb_h as f32,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    );

    if declared_w.is_none()
        && let (Some(l), Some(r)) = (left, right)
    {
        node.rect.w = (cb_w - l - r).max(0);
    }
    if declared_h.is_none()
        && let (Some(t), Some(b)) = (top, bottom)
    {
        node.rect.h = (cb_h - t - b).max(0);
    }

    if let Some(l) = left {
        node.rect.x = cb_x + l;
    } else if let Some(r) = right {
        node.rect.x = cb_x + (cb_w - r - node.rect.w);
    } else {
        node.rect.x = cb_x;
    }

    if let Some(t) = top {
        node.rect.y = cb_y + t;
    } else if let Some(b) = bottom {
        node.rect.y = cb_y + (cb_h - b - node.rect.h);
    } else {
        node.rect.y = cb_y;
    }
}

/// Calculate text dimensions using font-based metrics
fn text_dimensions(t: &str, font_size_px: f32) -> (i32, i32) {
    let metrics = FontMetrics::from_font_size(font_size_px);
    let len = t.chars().count() as f32;
    let w = if len > 0.0 {
        (len * metrics.char_width).round() as i32
    } else {
        0
    };
    let h = metrics.line_height.round() as i32;
    (w, h)
}

/// Convert a Length value to pixels given context
#[allow(dead_code)]
fn length_to_px(len: Length, parent_size: f32, root_size: f32, viewport: (f32, f32)) -> f32 {
    match len {
        Length::Px(v) => v,
        Length::Percent(v) => parent_size * v / 100.0,
        Length::Rem(v) => v * root_size,
        Length::Em(v) => v * parent_size,
        Length::Vw(v) => v * viewport.0 / 100.0,
        Length::Vh(v) => v * viewport.1 / 100.0,
        Length::Dvw(v) => v * viewport.0 / 100.0,
        Length::Dvh(v) => v * viewport.1 / 100.0,
        Length::Auto => 0.0,
        Length::Zero => 0.0,
    }
}

/// Box sides (margin/padding) with full CSS unit support.
///
/// Resolves CSS shorthand values (e.g. `padding: 10px 20px`) by splitting on
/// whitespace and expanding into individual sides following the CSS shorthand
/// rules:
///   1 value  → all sides
///   2 values → top/bottom, left/right
///   3 values → top, left/right, bottom
///   4 values → top, right, bottom, left
///
/// Individual longhand properties (e.g. `padding-top`) always take precedence
/// over the shorthand.
fn style_box_sides_full(
    style: Option<&str>,
    base: &str,
    parent_size: f32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> (i32, i32, i32, i32) {
    let resolve = |val: &str| -> Option<i32> {
        parse_length_value(
            val,
            parent_size,
            parent_font_size,
            root_font_size,
            (viewport_w, viewport_h),
        )
        .map(|f| f.round() as i32)
    };

    // `margin` accepts `auto`, which has no numeric value until the block's
    // free space is resolved (CSS 2.1 §10.3.3). Keep the whole shorthand
    // intact by treating those sides as 0 here; the auto sides are tracked
    // separately via `style_margin_auto_sides` and centered downstream.
    let resolve_side = |val: &str| -> Option<i32> {
        if base == "margin" && val.trim().eq_ignore_ascii_case("auto") {
            Some(0)
        } else {
            resolve(val)
        }
    };

    // Try expanding the shorthand value into individual sides.
    // CSS shorthand rules: 1 val = all, 2 = v h, 3 = t h b, 4 = t r b l
    let shorthand_sides: Option<(i32, i32, i32, i32)> = style.and_then(|s| {
        // Find the shorthand declaration (e.g. "padding: 10px 20px")
        let raw = s.split(';').find_map(|decl| {
            let d = decl.trim();
            if d.is_empty() {
                return None;
            }
            let (k, v) = d.split_once(':')?;
            if k.trim() == base {
                Some(v.trim())
            } else {
                None
            }
        })?;
        let parts: Vec<&str> = raw.split_whitespace().collect();
        match parts.len() {
            0 => None,
            1 => resolve_side(parts[0]).map(|v| (v, v, v, v)),
            2 => {
                let v = resolve_side(parts[0])?;
                let h = resolve_side(parts[1])?;
                Some((h, h, v, v)) // left, right, top, bottom
            }
            3 => {
                let t = resolve_side(parts[0])?;
                let h = resolve_side(parts[1])?;
                let b = resolve_side(parts[2])?;
                Some((h, h, t, b))
            }
            4 => {
                let t = resolve_side(parts[0])?;
                let r = resolve_side(parts[1])?;
                let b = resolve_side(parts[2])?;
                let l = resolve_side(parts[3])?;
                Some((l, r, t, b))
            }
            _ => None,
        }
    });

    // Destructure shorthand: (left, right, top, bottom)
    let (sh_l, sh_r, sh_t, sh_b) = shorthand_sides.unwrap_or((0, 0, 0, 0));

    // Individual longhand properties override the shorthand
    let l = style_lookup_len_full(
        style,
        &format!("{}-left", base),
        parent_size,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    )
    .unwrap_or(sh_l);
    let r = style_lookup_len_full(
        style,
        &format!("{}-right", base),
        parent_size,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    )
    .unwrap_or(sh_r);
    let t = style_lookup_len_full(
        style,
        &format!("{}-top", base),
        parent_size,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    )
    .unwrap_or(sh_t);
    let b = style_lookup_len_full(
        style,
        &format!("{}-bottom", base),
        parent_size,
        parent_font_size,
        root_font_size,
        viewport_w,
        viewport_h,
    )
    .unwrap_or(sh_b);
    (l, r, t, b)
}

/// Whether `margin-left` / `margin-right` resolve to `auto` for the given
/// inline style. Honors the CSS shorthand expansion (1-4 values) and the
/// longhand-over-shorthand precedence used by `style_box_sides_full`.
fn style_margin_auto_sides(style: Option<&str>) -> (bool, bool) {
    let Some(s) = style else {
        return (false, false);
    };
    let is_auto = |tok: &str| tok.trim().eq_ignore_ascii_case("auto");
    let mut shorthand: Option<(bool, bool)> = None; // (left, right)
    let mut long_l: Option<bool> = None;
    let mut long_r: Option<bool> = None;
    for decl in s.split(';') {
        let d = decl.trim();
        if d.is_empty() {
            continue;
        }
        let Some((k, v)) = d.split_once(':') else {
            continue;
        };
        match k.trim() {
            "margin" => {
                let parts: Vec<&str> = v.split_whitespace().collect();
                shorthand = match parts.as_slice() {
                    [all] => Some((is_auto(all), is_auto(all))),
                    [_, h] => Some((is_auto(h), is_auto(h))),
                    [_, h, _] => Some((is_auto(h), is_auto(h))),
                    [_, right, _, left] => Some((is_auto(left), is_auto(right))),
                    _ => None,
                };
            }
            "margin-left" => long_l = Some(is_auto(v)),
            "margin-right" => long_r = Some(is_auto(v)),
            _ => {}
        }
    }
    let (sh_l, sh_r) = shorthand.unwrap_or((false, false));
    (long_l.unwrap_or(sh_l), long_r.unwrap_or(sh_r))
}

fn style_border_widths(
    style: Option<&str>,
    parent_size: f32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> (i32, i32, i32, i32) {
    let resolve = |val: &str| -> Option<i32> {
        parse_length_value(
            val,
            parent_size,
            parent_font_size,
            root_font_size,
            (viewport_w, viewport_h),
        )
        .map(|f| f.round() as i32)
    };

    // default from `border` shorthand first token if it parses as length
    let mut default_border: Option<i32> = None;
    if let Some(s) = style
        && let Some(raw) = s.split(';').find_map(|decl| {
            let d = decl.trim();
            if d.is_empty() {
                return None;
            }
            let (k, v) = d.split_once(':')?;
            if k.trim() == "border" {
                Some(v.trim())
            } else {
                None
            }
        })
    {
        let first = raw.split_whitespace().next().unwrap_or("");
        if let Some(v) = resolve(first) {
            default_border = Some(v);
        } else if first == "0" {
            default_border = Some(0);
        }
    }
    let (mut bl, mut br, mut bt, mut bb) =
        default_border.map(|v| (v, v, v, v)).unwrap_or((0, 0, 0, 0));

    // `border-width` shorthand (1-4 values) overrides `border` default if present
    let has_border_width = style.is_some_and(|s| {
        s.split(';').any(|decl| {
            let d = decl.trim();
            if d.is_empty() {
                return false;
            }
            if let Some((k, _)) = d.split_once(':') {
                k.trim() == "border-width"
            } else {
                false
            }
        })
    });
    if has_border_width {
        let (l, r, t, b) = style_box_sides_full(
            style,
            "border-width",
            parent_size,
            parent_font_size,
            root_font_size,
            viewport_w,
            viewport_h,
        );
        bl = l;
        br = r;
        bt = t;
        bb = b;
    }

    // individual `border-*-width` overrides
    for (key, target) in [
        ("border-left-width", &mut bl),
        ("border-right-width", &mut br),
        ("border-top-width", &mut bt),
        ("border-bottom-width", &mut bb),
    ] {
        if let Some(v) = style_lookup_len_full(
            style,
            key,
            parent_size,
            parent_font_size,
            root_font_size,
            viewport_w,
            viewport_h,
        ) {
            *target = v;
        }
    }
    // also support legacy `border-left` etc shorthand width extraction
    for (key, target) in [
        ("border-left", &mut bl),
        ("border-right", &mut br),
        ("border-top", &mut bt),
        ("border-bottom", &mut bb),
    ] {
        if let Some(s) = style
            && let Some(raw) = s.split(';').find_map(|decl| {
                let d = decl.trim();
                if d.is_empty() {
                    return None;
                }
                let (k, v) = d.split_once(':')?;
                if k.trim() == key {
                    Some(v.trim())
                } else {
                    None
                }
            })
        {
            let first = raw.split_whitespace().next().unwrap_or("");
            if let Some(v) = resolve(first) {
                *target = v;
            }
        }
    }

    (bl, br, bt, bb)
}

pub fn compute_layout(node: &VNode, viewport_w: i32, viewport_h: i32) -> LayoutNode {
    #[allow(clippy::too_many_arguments)]
    fn at(
        node: &VNode,
        x: i32,
        y: i32,
        avail_w: i32,
        avail_h: i32,
        viewport_w: i32,
        viewport_h: i32,
        _containing_x: i32,
        _containing_y: i32,
        containing_w: i32,
        _containing_h: i32,
        source_index: Option<usize>,
        root_font_size: f32,
        parent_font_size: f32,
    ) -> LayoutNode {
        match node {
            VNode::Text(t) => {
                // Text nodes use inherited font size
                let (w, h) = text_dimensions(t, parent_font_size);
                LayoutNode {
                    rect: Rect { x, y, w, h },
                    z_index: 0,
                    display_none: false,
                    source_index,
                    scroll_x: 0,
                    scroll_y: 0,
                    clip: None,
                    stacking_context: false,
                    scroll_height: h,
                    max_scroll_y: 0,
                    scrollable: false,
                    children: vec![],
                }
            }
            VNode::Element {
                tag,
                props,
                children,
            } => {
                let style = props.attrs.get("style").map(|s| s.as_str());

                // Resolve this element's font-size for children to inherit
                let my_font_size = style_lookup_font_size(
                    style,
                    parent_font_size,
                    root_font_size,
                    (viewport_w as f32, viewport_h as f32),
                )
                .unwrap_or(parent_font_size);

                let vw_f = viewport_w as f32;
                let vh_f = viewport_h as f32;

                let (ml, mr, mt, mb) = style_box_sides_full(
                    style,
                    "margin",
                    containing_w as f32,
                    parent_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );
                let (pl, pr, pt, pb) = style_box_sides_full(
                    style,
                    "padding",
                    containing_w as f32,
                    parent_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );
                let (bl, br, bt, bb) = style_border_widths(
                    style,
                    containing_w as f32,
                    parent_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );
                let box_sizing = style_lookup_str(style, "box-sizing")
                    .map(|s| s.trim().to_ascii_lowercase())
                    .unwrap_or_else(|| "content-box".to_string());
                let is_border_box = box_sizing == "border-box";
                let is_root_tag = matches!(tag.as_str(), "body" | "html");
                let is_root_index = root_is_viewport_filling(source_index);
                // Viewport root normalization: first VNode (and html/body) always fills.
                // Expanded predicate handles 100% | 100vw | 100dvw | 100vh | 100dvh | min-height:100%/vh/dvh
                let is_viewport_filling = is_viewport_filling(style, is_root_tag || is_root_index);

                // Check if element has height: 100vh / 100dvh (viewport-relative) — pixel equality covers both vh/dvh
                let has_viewport_height = style_lookup_len_full(
                    style,
                    "height",
                    vh_f,
                    parent_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                )
                .map(|h| (h as f32 - vh_f).abs() < 0.5)
                .unwrap_or(false);
                let min_height_vh = style_lookup_len_full(
                    style,
                    "min-height",
                    vh_f,
                    parent_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                )
                .map(|h| (h as f32 - vh_f).abs() < 0.5)
                .unwrap_or(false);

                // Also treat min-height:100% as viewport-filling when it resolves to available/viewport height
                let min_height_is_100pct = {
                    let raw = style.and_then(|s| {
                        for decl in s.split(';') {
                            let d = decl.trim();
                            if d.is_empty() {
                                continue;
                            }
                            if let Some((k, v)) = d.split_once(':')
                                && k.trim() == "min-height"
                            {
                                return Some(v.trim().to_string());
                            }
                        }
                        None
                    });
                    raw.as_deref()
                        .and_then(|v| crate::Length::parse(v))
                        .map(|l| matches!(l, crate::Length::Percent(p) if (p - 100.0).abs() < 0.01))
                        .unwrap_or(false)
                };

                // Element outer position with margins; `margin: auto` on a
                // block-level box with a declared width centers it by
                // splitting the free space equally (CSS 2.1 §10.3.3).
                let elem_y = y + mt;

                // Determine width: if set, use as content+padding width; else take available width
                let declared_w = style_lookup_len_full(
                    style,
                    "width",
                    avail_w as f32,
                    parent_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );

                let (ml_auto, mr_auto) = style_margin_auto_sides(style);
                let (ml, mr) = if let Some(dw) = declared_w.filter(|_| ml_auto || mr_auto) {
                    // Border-box: the declared width already includes
                    // padding+border (content_size_for convention, F-04) —
                    // no double subtraction.
                    let outer_w = if is_border_box {
                        dw as f32
                    } else {
                        dw as f32 + pl as f32 + pr as f32 + bl as f32 + br as f32
                    };
                    let (l, r) = crate::style::resolve_auto_margins_core(
                        avail_w as f32,
                        outer_w,
                        ml as f32,
                        mr as f32,
                        ml_auto,
                        mr_auto,
                    );
                    (l.round() as i32, r.round() as i32)
                } else {
                    (ml, mr)
                };
                let elem_x = x + ml;

                // Determine height: handle viewport-relative heights (100vh, min-height: 100vh)
                let declared_h = style_lookup_len_full(
                    style,
                    "height",
                    avail_h as f32,
                    parent_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );

                // Legacy 100% viewport-filling pair still respected, but root already true covers implicit fill
                let has_100p_width = declared_w
                    .map(|w| (w as f32 - avail_w as f32).abs() < 0.5)
                    .unwrap_or(false);
                let has_100p_height = declared_h
                    .map(|h| (h as f32 - avail_h as f32).abs() < 0.5)
                    .unwrap_or(false);
                let legacy_pair = has_100p_width && has_100p_height;

                // Update is_viewport_height to include viewport-filling elements (must be before rect calcs)
                // Covers root, expanded predicate, legacy 100% pair, explicit vh/dvh, and min-height variants
                let is_viewport_height = is_viewport_filling
                    || legacy_pair
                    || has_viewport_height
                    || min_height_vh
                    || min_height_is_100pct;

                let rect_w = content_size_for(
                    declared_w,
                    avail_w,
                    is_border_box,
                    pl,
                    pr,
                    bl,
                    br,
                    ml,
                    mr,
                    is_viewport_filling,
                    legacy_pair,
                );

                // For viewport-height elements, use viewport height as the base, otherwise box-sizing adjusted
                // Declared height takes precedence over viewport filling so explicit fixed heights are respected
                let mut _rect_h = if let Some(dh) = declared_h {
                    if is_border_box {
                        dh
                    } else {
                        dh + pt + pb + bt + bb
                    }
                } else if is_viewport_height {
                    (avail_h - mt - mb).max(1)
                } else {
                    declared_h.unwrap_or(avail_h)
                };

                // Content box (border inside padding offset)
                let content_x = elem_x + bl + pl;
                let content_y_start = elem_y + bt + pt;
                let content_w = (rect_w - pl - pr - bl - br).max(0);
                let content_h_available = (_rect_h - pt - pb - bt - bb).max(0);

                let overflow =
                    style_lookup_str(style, "overflow").unwrap_or_else(|| "visible".to_string());
                // Deprecated synthetic scroll-left/top — keep parsing for compat with warning, map to ScrollState
                let raw_scroll_x = style_lookup_len_full(
                    style,
                    "scroll-left",
                    content_w as f32,
                    my_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );
                let raw_scroll_y = style_lookup_len_full(
                    style,
                    "scroll-top",
                    content_h_available as f32,
                    my_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );
                if raw_scroll_x.is_some() || raw_scroll_y.is_some() {
                    // Deprecated synthetic scroll-left/scroll-top: still parsed and
                    // applied for compat, but warn once per process.
                    static SCROLL_DEPRECATION_WARNED: std::sync::atomic::AtomicBool =
                        std::sync::atomic::AtomicBool::new(false);
                    if !SCROLL_DEPRECATION_WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                        eprintln!(
                            "[velox] warning: scroll-left/scroll-top styles are deprecated; \
                             use overflow:auto/scroll with wheel scrolling instead"
                        );
                    }
                }
                let scroll_x = raw_scroll_x.unwrap_or(0);
                let scroll_y = raw_scroll_y.unwrap_or(0);
                let content_x_scrolled = content_x - scroll_x;
                let content_y_scrolled = content_y_start - scroll_y;

                // Layout strategy: block (default) or flex. An element's
                // default `display` comes from `default_display_for_tag`, which
                // agrees with the UA sheet in velox-style; both routes below
                // only test for "none" and "flex", so an `inline` default keeps
                // using the block flow (there is no inline layout yet).
                let display = style_lookup_str(style, "display")
                    .unwrap_or_else(|| default_display_for_tag(tag).to_string());
                let position =
                    style_lookup_str(style, "position").unwrap_or_else(|| "static".to_string());
                let z_index = if position != "static" {
                    style_lookup_i32(style, "z-index").unwrap_or(0)
                } else {
                    0
                };
                let opacity = style_lookup_str(style, "opacity")
                    .and_then(|v| {
                        v.parse::<f32>()
                            .ok()
                            .map(|f| if f.is_finite() { f } else { 1.0 })
                    })
                    .unwrap_or(1.0);
                let transform =
                    style_lookup_str(style, "transform").unwrap_or_else(|| "none".to_string());
                let stacking_context = opacity < 1.0
                    || (transform != "none" && !transform.is_empty())
                    || position != "static";
                if display == "none" {
                    return LayoutNode {
                        rect: Rect {
                            x: elem_x,
                            y: elem_y,
                            w: 0,
                            h: 0,
                        },
                        z_index,
                        display_none: true,
                        source_index,
                        scroll_x: 0,
                        scroll_y: 0,
                        clip: None,
                        stacking_context: false,
                        scroll_height: 0,
                        max_scroll_y: 0,
                        scrollable: false,
                        children: vec![],
                    };
                }

                let mut laid_children: Vec<LayoutNode> = Vec::new();
                let mut abs_children: Vec<LayoutNode> = Vec::new();
                let mut max_y_end = content_y_start;
                if display == "flex" {
                    // Full CSS Flexbox implementation
                    let flex_dir = style_lookup_str(style, "flex-direction")
                        .unwrap_or_else(|| "row".to_string());
                    let flex_wrap = style_lookup_str(style, "flex-wrap")
                        .unwrap_or_else(|| "nowrap".to_string());
                    let justify_content = style_lookup_str(style, "justify-content")
                        .unwrap_or_else(|| "flex-start".to_string());
                    let align_items = style_lookup_str(style, "align-items")
                        .unwrap_or_else(|| "stretch".to_string());
                    let row_gap = style_lookup_len_full(
                        style,
                        "row-gap",
                        0.0,
                        my_font_size,
                        root_font_size,
                        vw_f,
                        vh_f,
                    )
                    .or_else(|| {
                        style_lookup_len_full(
                            style,
                            "gap",
                            0.0,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        )
                    })
                    .unwrap_or(0);
                    let column_gap = style_lookup_len_full(
                        style,
                        "column-gap",
                        0.0,
                        my_font_size,
                        root_font_size,
                        vw_f,
                        vh_f,
                    )
                    .or_else(|| {
                        style_lookup_len_full(
                            style,
                            "gap",
                            0.0,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        )
                    })
                    .unwrap_or(0);

                    // Collect flex children (exclude absolute/fixed, display:none)
                    struct FlexChild<'a> {
                        index: usize,
                        node: &'a VNode,
                        style: Option<&'a str>,
                    }
                    let mut flex_children: Vec<FlexChild> = Vec::new();
                    for (idx, c) in children.iter().enumerate() {
                        // Skip whitespace-only text nodes (generated by template indentation)
                        if let VNode::Text(t) = c
                            && t.chars().all(|c| c.is_whitespace())
                        {
                            continue;
                        }
                        let child_style = match c {
                            VNode::Element { props, .. } => {
                                props.attrs.get("style").map(|s| s.as_str())
                            }
                            _ => None,
                        };
                        let child_display = style_lookup_str(child_style, "display")
                            .unwrap_or_else(|| "block".to_string());
                        if child_display == "none" {
                            continue;
                        }
                        let position = style_lookup_str(child_style, "position")
                            .unwrap_or_else(|| "static".to_string());
                        if position == "absolute" || position == "fixed" {
                            // Handle absolute children separately
                            let cb_x = if position == "fixed" {
                                0
                            } else {
                                content_x_scrolled
                            };
                            let cb_y = if position == "fixed" {
                                0
                            } else {
                                content_y_scrolled
                            };
                            let cb_w = if position == "fixed" {
                                viewport_w
                            } else {
                                content_w
                            };
                            let cb_h = if position == "fixed" {
                                viewport_h
                            } else {
                                content_h_available
                            };
                            let mut child_ln = at(
                                c,
                                cb_x,
                                cb_y,
                                cb_w,
                                cb_h,
                                viewport_w,
                                viewport_h,
                                cb_x,
                                cb_y,
                                cb_w,
                                cb_h,
                                Some(idx),
                                root_font_size,
                                my_font_size,
                            );
                            apply_absolute_position(
                                child_style,
                                &mut child_ln,
                                cb_x,
                                cb_y,
                                cb_w,
                                cb_h,
                                my_font_size,
                                root_font_size,
                                vw_f,
                                vh_f,
                            );
                            abs_children.push(child_ln);
                            continue;
                        }
                        flex_children.push(FlexChild {
                            index: idx,
                            node: c,
                            style: child_style,
                        });
                    }

                    // Determine main/cross axis dimensions — check flex-flow shorthand first
                    let mut flex_dir_val = flex_dir.clone();
                    let mut flex_wrap_val = flex_wrap.clone();
                    if let Some(flow_raw) = style_lookup_str(style, "flex-flow") {
                        let tokens: Vec<String> = flow_raw
                            .split_whitespace()
                            .map(|s| s.to_ascii_lowercase())
                            .collect();
                        for tok in &tokens {
                            match tok.as_str() {
                                "row" | "row-reverse" | "column" | "column-reverse" => {
                                    flex_dir_val = tok.clone();
                                }
                                "nowrap" | "wrap" | "wrap-reverse" => {
                                    flex_wrap_val = tok.clone();
                                }
                                _ => {}
                            }
                        }
                    }
                    // also handle flex-direction / flex-wrap longhands overriding flow if they appear later —
                    // style_lookup_str already returns last occurrence, so respect that if different from defaults
                    // (we already parsed those above, but flex-flow may have set them; if individual exists, use it)
                    let has_dir = style.is_some_and(|s| {
                        s.split(';').any(|decl| {
                            if let Some((k, _)) = decl.split_once(':') {
                                k.trim() == "flex-direction"
                            } else {
                                false
                            }
                        })
                    });
                    let has_wrap = style.is_some_and(|s| {
                        s.split(';').any(|decl| {
                            if let Some((k, _)) = decl.split_once(':') {
                                k.trim() == "flex-wrap"
                            } else {
                                false
                            }
                        })
                    });
                    if has_dir {
                        flex_dir_val = flex_dir.clone();
                    }
                    if has_wrap {
                        flex_wrap_val = flex_wrap.clone();
                    }
                    let is_column = flex_dir_val == "column" || flex_dir_val == "column-reverse";
                    let is_reverse =
                        flex_dir_val == "row-reverse" || flex_dir_val == "column-reverse";
                    let is_wrap = flex_wrap_val == "wrap" || flex_wrap_val == "wrap-reverse";
                    let wrap_reverse = flex_wrap_val == "wrap-reverse";
                    let align_content = style_lookup_str(style, "align-content")
                        .unwrap_or_else(|| "stretch".to_string());

                    let main_size = if is_column {
                        content_h_available
                    } else {
                        content_w
                    };
                    // Cross size: use explicit container dimension if set, otherwise use available
                    let explicit_h_raw = style_lookup_len_full(
                        style,
                        "height",
                        avail_h as f32,
                        my_font_size,
                        root_font_size,
                        vw_f,
                        vh_f,
                    );
                    let explicit_h_content = explicit_h_raw.map(|v| {
                        if is_border_box {
                            (v - pt - pb - bt - bb).max(0)
                        } else {
                            v
                        }
                    });
                    let _explicit_w = style_lookup_len_full(
                        style,
                        "width",
                        avail_w as f32,
                        my_font_size,
                        root_font_size,
                        vw_f,
                        vh_f,
                    );

                    // Determine if flex container has a definite cross-size per CX-03/05
                    // For row flex: cross = height, definite only if resolved height present
                    // For column flex: cross = width, definite if parent cross definite OR resolved width present
                    let explicit_w_content = _explicit_w.map(|v| {
                        if is_border_box {
                            (v - pl - pr - bl - br).max(0)
                        } else {
                            v
                        }
                    });
                    let parent_definite = containing_w > 0;
                    let has_definite_cross_size = has_definite_cross(
                        is_column,
                        parent_definite,
                        if is_column {
                            explicit_w_content
                        } else {
                            explicit_h_content
                        },
                    );

                    // Initial cross_size: if definite, use it; if indefinite, start with 0 (will compute from children)
                    let cross_size = if is_column {
                        content_w
                    } else if has_definite_cross_size {
                        explicit_h_content.unwrap_or(content_h_available).max(0)
                    } else {
                        0 // indefinite - will be computed from children's natural sizes
                    };

                    // Step 1: Compute flex-basis for each child
                    struct FlexItem {
                        child_index: usize,
                        layout_node: Option<LayoutNode>,
                        flex_basis: f32,
                        flex_grow: f32,
                        flex_shrink: f32,
                        align_self: String,
                        min_main_size: f32,
                        max_main_size: f32,
                    }

                    // L-H4: order support — stable sort flex children by `order`
                    flex_children
                        .sort_by_key(|fc| style_lookup_i32(fc.style, "order").unwrap_or(0));

                    // Helper to parse `flex` shorthand per CSS spec
                    let parse_flex_shorthand =
                        |style: Option<&str>| -> Option<(f32, f32, Option<String>)> {
                            let raw = style_lookup_str(style, "flex")?;
                            let raw = raw.trim();
                            if raw.is_empty() {
                                return None;
                            }
                            if raw.eq_ignore_ascii_case("auto") {
                                Some((1.0, 1.0, None))
                            } else if raw.eq_ignore_ascii_case("none") {
                                Some((0.0, 0.0, None))
                            } else if raw.eq_ignore_ascii_case("initial") {
                                Some((0.0, 1.0, None))
                            } else {
                                let toks: Vec<&str> = raw.split_whitespace().collect();
                                match toks.len() {
                                    1 => {
                                        if let Ok(g) = toks[0].parse::<f32>() {
                                            if g.is_finite() {
                                                Some((g, 1.0, Some("0".to_string())))
                                            } else {
                                                Some((1.0, 1.0, Some(toks[0].to_string())))
                                            }
                                        } else {
                                            // single length basis e.g. "100px"
                                            Some((1.0, 1.0, Some(toks[0].to_string())))
                                        }
                                    }
                                    2 => {
                                        // either grow shrink or grow basis
                                        if let Ok(g) = toks[0].parse::<f32>() {
                                            if g.is_finite() {
                                                // second token: check if it's a number (shrink) or length
                                                if let Ok(s) = toks[1].parse::<f32>() {
                                                    if s.is_finite()
                                                        && !toks[1].contains('%')
                                                        && !toks[1].contains("px")
                                                        && !toks[1].contains("rem")
                                                        && !toks[1].contains("em")
                                                        && !toks[1].contains("vw")
                                                        && !toks[1].contains("vh")
                                                        && toks[1] != "auto"
                                                    {
                                                        Some((g, s, Some("0".to_string())))
                                                    } else {
                                                        Some((g, 1.0, Some(toks[1].to_string())))
                                                    }
                                                } else {
                                                    // otherwise basis
                                                    Some((g, 1.0, Some(toks[1].to_string())))
                                                }
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        }
                                    }
                                    3 => {
                                        let g = toks[0]
                                            .parse::<f32>()
                                            .ok()
                                            .filter(|v| v.is_finite())?;
                                        let s = toks[1]
                                            .parse::<f32>()
                                            .ok()
                                            .filter(|v| v.is_finite())?;
                                        Some((g, s, Some(toks[2].to_string())))
                                    }
                                    _ => None,
                                }
                            }
                        };

                    let mut items: Vec<FlexItem> = Vec::new();
                    for fc in &flex_children {
                        // Pre-layout child to get its natural size
                        let child_avail_main = if is_column {
                            content_h_available
                        } else {
                            main_size
                        };
                        // For indefinite cross-size, give children unconstrained cross-size so they lay out at natural size
                        let child_avail_cross = if is_column {
                            cross_size as f32 // column flex cross-size (width) is always definite
                        } else if has_definite_cross_size {
                            cross_size as f32 // definite cross-size: use it for children (align-items: stretch will apply)
                        } else {
                            UNCONSTRAINED_CROSS_SIZE // indefinite: unconstrained, children use natural size
                        };
                        let ln = at(
                            fc.node,
                            0,
                            0,
                            child_avail_cross as i32,
                            child_avail_main as i32,
                            viewport_w,
                            viewport_h,
                            content_x,
                            content_y_start,
                            content_w,
                            content_h_available,
                            Some(fc.index),
                            root_font_size,
                            my_font_size,
                        );

                        let flex_basis_val = style_lookup_len_full(
                            fc.style,
                            "flex-basis",
                            main_size as f32,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        );
                        // Check if child has explicit main size
                        let explicit_main = style_lookup_len_full(
                            fc.style,
                            if is_column { "height" } else { "width" },
                            main_size as f32,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        );
                        // Parse `flex` shorthand if present; it overrides flex-grow/shrink/basis specifics
                        let flex_shorthand = parse_flex_shorthand(fc.style);
                        let (flex_grow, flex_shrink, shorthand_basis): (f32, f32, Option<String>) =
                            if let Some((g, s, b)) = flex_shorthand.as_ref() {
                                (*g, *s, b.clone())
                            } else {
                                (
                                    style_lookup_str(fc.style, "flex-grow")
                                        .and_then(|v| {
                                            v.parse::<f32>()
                                                .ok()
                                                .map(|f| if f.is_finite() { f } else { 0.0 })
                                        })
                                        .unwrap_or(0.0),
                                    style_lookup_str(fc.style, "flex-shrink")
                                        .and_then(|v| {
                                            v.parse::<f32>()
                                                .ok()
                                                .map(|f| if f.is_finite() { f } else { 1.0 })
                                        })
                                        .unwrap_or(1.0),
                                    None,
                                )
                            };

                        // If flex shorthand present, also use it for basis unless longhand overrides
                        // For `flex: 1` => basis 0% (not auto); for `flex: auto` => auto; for shorthand_basis "auto" => auto.
                        let shorthand_basis_px: Option<f32> =
                            shorthand_basis.as_deref().and_then(|raw| {
                                if raw.eq_ignore_ascii_case("auto") {
                                    return None; // handled as auto below
                                }
                                if raw == "0" || raw == "0%" || raw == "0px" {
                                    // 0 can be unitless -> 0px, already 0
                                    return Some(0.0);
                                }
                                parse_length_value(
                                    raw,
                                    main_size as f32,
                                    my_font_size,
                                    root_font_size,
                                    (vw_f, vh_f),
                                )
                            });
                        let is_flex_one_zero = flex_shorthand.is_some()
                            && flex_grow == 1.0
                            && flex_shrink == 1.0
                            && shorthand_basis.is_none(); // flex: auto case handled else

                        // Determine effective basis:
                        // - shorthand_basis_px if explicit
                        // - else flex_basis_val longhand
                        // - else explicit_main
                        // - else auto vs 0% depending on shorthand semantics
                        let flex_basis = if let Some(v) = shorthand_basis_px {
                            v
                        } else if shorthand_basis
                            .as_deref()
                            .map(|s| s.eq_ignore_ascii_case("auto"))
                            .unwrap_or(false)
                            && is_flex_one_zero
                        {
                            // auto => content size
                            if is_column {
                                ln.rect.h as f32
                            } else {
                                ln.rect.w as f32
                            }
                        } else if flex_shorthand.is_some()
                            && shorthand_basis.as_deref() == Some("0")
                        {
                            0.0
                        } else if let Some(fb) = flex_basis_val {
                            fb as f32
                        } else if let Some(exp) = explicit_main {
                            // If flex shorthand existed and gave implicit 0 basis, ignore explicit width for basis 0 case
                            if flex_shorthand.is_some() && shorthand_basis == Some("0".to_string())
                            {
                                0.0
                            } else {
                                exp as f32
                            }
                        } else if let Some(raw) = flex_shorthand.as_ref().map(|_| {
                            style_lookup_str(fc.style, "flex")
                                .unwrap_or_default()
                                .trim()
                                .to_ascii_lowercase()
                        }) {
                            // flex shorthand present without basis token => 0%
                            // unless it's `auto` / `none` / `initial` handled above
                            if raw == "auto" || raw == "none" || raw == "initial" {
                                if is_column {
                                    ln.rect.h as f32
                                } else {
                                    ln.rect.w as f32
                                }
                            } else {
                                0.0
                            }
                        } else if flex_grow > 0.0 {
                            0.0
                        } else if is_column {
                            ln.rect.h as f32
                        } else {
                            ln.rect.w as f32
                        };

                        let min_main = style_lookup_len_full(
                            fc.style,
                            if is_column { "min-height" } else { "min-width" },
                            main_size as f32,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        )
                        .unwrap_or(0) as f32;
                        let max_main_val = style_lookup_len_full(
                            fc.style,
                            if is_column { "max-height" } else { "max-width" },
                            main_size as f32,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        );
                        let max_main = match max_main_val {
                            Some(v) if v >= 0 => v as f32,
                            _ => f32::MAX,
                        };

                        items.push(FlexItem {
                            child_index: fc.index,
                            layout_node: Some(ln),
                            flex_basis,
                            flex_grow,
                            flex_shrink,
                            align_self: style_lookup_str(fc.style, "align-self")
                                .unwrap_or_else(|| "auto".to_string()),
                            min_main_size: min_main,
                            max_main_size: max_main,
                        });
                    }

                    // Step 2: Line breaking (flex-wrap)
                    struct FlexLine {
                        items: Vec<usize>, // indices into items
                        main_size: f32,
                        cross_size: f32,
                        main_positions: Vec<(usize, f32)>, // (item_idx, main_pos)
                    }
                    let mut lines: Vec<FlexLine> = Vec::new();

                    if !is_wrap {
                        // Single line - all items
                        lines.push(FlexLine {
                            items: (0..items.len()).collect(),
                            main_size: 0.0,
                            cross_size: 0.0,
                            main_positions: Vec::new(),
                        });
                    } else {
                        let mut current_line = FlexLine {
                            items: Vec::new(),
                            main_size: 0.0,
                            cross_size: 0.0,
                            main_positions: Vec::new(),
                        };
                        #[allow(clippy::needless_range_loop)]
                        for i in 0..items.len() {
                            let item_size = items[i]
                                .flex_basis
                                .max(items[i].min_main_size)
                                .min(items[i].max_main_size);
                            let gap_to_add = if current_line.items.is_empty() {
                                0.0
                            } else {
                                (if is_column { row_gap } else { column_gap }) as f32
                            };
                            if !current_line.items.is_empty()
                                && current_line.main_size + gap_to_add + item_size
                                    > main_size as f32
                            {
                                // Start new line
                                lines.push(current_line);
                                current_line = FlexLine {
                                    items: vec![i],
                                    main_size: item_size,
                                    cross_size: 0.0,
                                    main_positions: Vec::new(),
                                };
                            } else {
                                current_line.items.push(i);
                                current_line.main_size += item_size + gap_to_add;
                            }
                        }
                        if !current_line.items.is_empty() {
                            lines.push(current_line);
                        }
                    }
                    if lines.is_empty() && !items.is_empty() {
                        lines.push(FlexLine {
                            items: (0..items.len()).collect(),
                            main_size: 0.0,
                            cross_size: 0.0,
                            main_positions: Vec::new(),
                        });
                    }

                    // Step 3: For each line, distribute flex & compute line cross sizes (pre-positioning)
                    // L-C1: wrap-reverse support; L-C2: track cross_offset not main_offset; L-M5: stretch & align-content
                    let cross_gap = (if is_column { column_gap } else { row_gap }) as f32;
                    let main_start = if is_column { pt as f32 } else { pl as f32 };
                    // Pre-compute reversal by reversing lines order; later we restore DOM order via sort
                    if wrap_reverse {
                        lines.reverse();
                    }
                    // Two-pass flex layout: pass 1 distributes flex + justify + computes per-line cross sizes
                    let pre_lines_len = lines.len();
                    for line in &mut lines {
                        let total_basis: f32 = line
                            .items
                            .iter()
                            .map(|&i| {
                                items[i]
                                    .flex_basis
                                    .max(items[i].min_main_size)
                                    .min(items[i].max_main_size)
                            })
                            .sum();
                        let total_gap = if line.items.len() > 1 {
                            (line.items.len() as i32 - 1) as f32
                                * if is_column {
                                    row_gap as f32
                                } else {
                                    column_gap as f32
                                }
                        } else {
                            0.0
                        };
                        let free_space = (main_size as f32) - total_basis - total_gap;
                        if free_space > 0.0 {
                            let total_grow: f32 =
                                line.items.iter().map(|&i| items[i].flex_grow).sum();
                            if total_grow > 0.0 {
                                for &idx in &line.items {
                                    let grow_amount =
                                        (items[idx].flex_grow / total_grow) * free_space;
                                    items[idx].flex_basis = (items[idx].flex_basis + grow_amount)
                                        .max(items[idx].min_main_size)
                                        .min(items[idx].max_main_size);
                                }
                            }
                        } else if free_space < 0.0 {
                            let total_shrink: f32 = line
                                .items
                                .iter()
                                .map(|&i| items[i].flex_shrink * items[i].flex_basis)
                                .sum();
                            if total_shrink > 0.0 {
                                for &idx in &line.items {
                                    let shrink_factor = items[idx].flex_shrink
                                        * items[idx].flex_basis
                                        / total_shrink;
                                    let shrink_amount = shrink_factor * free_space.abs();
                                    items[idx].flex_basis = (items[idx].flex_basis - shrink_amount)
                                        .max(items[idx].min_main_size)
                                        .min(items[idx].max_main_size);
                                }
                            }
                        }
                        line.main_size =
                            line.items.iter().map(|&i| items[i].flex_basis).sum::<f32>()
                                + total_gap;
                        if line.items.is_empty() {
                            line.main_positions = Vec::new();
                            line.cross_size = 0.0;
                            continue;
                        }
                        let effective_main = line.main_size;
                        let extra_space = (main_size as f32 - effective_main).max(0.0);
                        let gap_val = if is_column { row_gap } else { column_gap };
                        let n = line.items.len();
                        let per_gap_extra: f32 = match justify_content.as_str() {
                            "space-between" => {
                                if n > 1 {
                                    extra_space / (n as f32 - 1.0)
                                } else {
                                    0.0
                                }
                            }
                            "space-around" => {
                                if n > 0 {
                                    extra_space / n as f32
                                } else {
                                    0.0
                                }
                            }
                            "space-evenly" => {
                                if n > 0 {
                                    extra_space / (n as f32 + 1.0)
                                } else {
                                    0.0
                                }
                            }
                            _ => 0.0,
                        };
                        #[allow(unused_assignments)]
                        let mut cursor = main_start;
                        cursor = match justify_content.as_str() {
                            "flex-start" | "start" => main_start,
                            "flex-end" | "end" => main_start + extra_space,
                            "center" => main_start + extra_space / 2.0,
                            "space-between" => main_start,
                            "space-around" => main_start + per_gap_extra / 2.0,
                            "space-evenly" => main_start + per_gap_extra,
                            _ => main_start,
                        };
                        let mut mpos: Vec<(usize, f32)> = Vec::new();
                        let extra_for_gap = match justify_content.as_str() {
                            "space-between" | "space-around" | "space-evenly" => per_gap_extra,
                            _ => 0.0,
                        };
                        for &item_idx in &line.items {
                            let pos = cursor;
                            mpos.push((item_idx, pos));
                            cursor =
                                pos + items[item_idx].flex_basis + gap_val as f32 + extra_for_gap;
                        }
                        line.main_positions = mpos;
                        // Compute line cross size
                        let mut max_cross_size: f32 = 0.0;
                        for &(item_idx, _) in &line.main_positions {
                            if let Some(ref ln) = items[item_idx].layout_node {
                                let cross_sz = if is_column {
                                    ln.rect.w as f32
                                } else {
                                    ln.rect.h as f32
                                };
                                if cross_sz > max_cross_size {
                                    max_cross_size = cross_sz;
                                }
                            }
                        }
                        // L-M5: stretch — only applies when cross-size is DEFINITE
                        // When cross-size is indefinite (fit-content), stretch behaves as flex-start
                        let effective_align_items = align_items.clone();
                        let is_single_line = pre_lines_len == 1;
                        let can_stretch =
                            has_definite_cross_size && effective_align_items == "stretch";
                        if can_stretch {
                            let stretch_target = if is_single_line {
                                cross_size as f32
                            } else {
                                max_cross_size
                            };
                            for &(item_idx, _) in &line.main_positions {
                                let child_style = flex_children
                                    .iter()
                                    .find(|fc| fc.index == items[item_idx].child_index)
                                    .map(|fc| fc.style)
                                    .unwrap_or(None);
                                let explicit_cross = style_lookup_len_full(
                                    child_style,
                                    if is_column { "width" } else { "height" },
                                    cross_size as f32,
                                    my_font_size,
                                    root_font_size,
                                    vw_f,
                                    vh_f,
                                );
                                // baseline fallback: treat as flex-start, so don't stretch baseline items
                                let is_baseline = items[item_idx].align_self == "baseline"
                                    || (items[item_idx].align_self == "auto"
                                        && effective_align_items == "baseline");
                                if explicit_cross.is_none()
                                    && items[item_idx].align_self == "auto"
                                    && !is_baseline
                                    && let Some(ref mut ln) = items[item_idx].layout_node
                                {
                                    if is_column {
                                        ln.rect.w = stretch_target as i32;
                                    } else {
                                        ln.rect.h = stretch_target as i32;
                                    }
                                }
                            }
                            if is_single_line {
                                max_cross_size = max_cross_size.max(cross_size as f32);
                            }
                        }
                        // For indefinite cross-size, line.cross_size = max_cross_size (natural size)
                        // For definite cross-size with stretch, line.cross_size may be stretched to cross_size
                        line.cross_size = max_cross_size;
                    }
                    // Compute align-content distribution for multi-line
                    let total_cross: f32 = if lines.is_empty() {
                        0.0
                    } else {
                        lines.iter().map(|l| l.cross_size).sum::<f32>()
                            + cross_gap * (lines.len() as f32 - 1.0)
                    };
                    // For indefinite cross-size (fit-content), there's no free space to distribute
                    let free_cross = if has_definite_cross_size {
                        (cross_size as f32 - total_cross).max(0.0)
                    } else {
                        0.0
                    };
                    let n_lines = lines.len();
                    let mut cross_start: f32 = 0.0;
                    let mut cross_extra_per_gap: f32 = 0.0;
                    #[allow(unused_assignments)]
                    let mut cross_line_extra: f32 = 0.0;
                    match align_content.as_str() {
                        "flex-start" | "start" => cross_start = 0.0,
                        "flex-end" | "end" => cross_start = free_cross,
                        "center" => cross_start = free_cross / 2.0,
                        "space-between" => {
                            if n_lines > 1 {
                                cross_extra_per_gap = free_cross / (n_lines as f32 - 1.0);
                            }
                        }
                        "space-around" => {
                            if n_lines > 0 {
                                cross_extra_per_gap = free_cross / n_lines as f32;
                                cross_start = cross_extra_per_gap / 2.0;
                            }
                        }
                        "space-evenly" => {
                            if n_lines > 0 {
                                cross_extra_per_gap = free_cross / (n_lines as f32 + 1.0);
                                cross_start = cross_extra_per_gap;
                            }
                        }
                        "stretch" => {
                            if has_definite_cross_size && n_lines > 1 && free_cross > 0.0 {
                                cross_line_extra = free_cross / n_lines as f32;
                                for line in &mut lines {
                                    line.cross_size += cross_line_extra;
                                }
                                // Re-stretch items that were stretch to new line size
                                if align_items == "stretch" {
                                    for line in &lines {
                                        for &(item_idx, _) in &line.main_positions {
                                            if items[item_idx].align_self == "auto"
                                                && let Some(ref mut ln) =
                                                    items[item_idx].layout_node
                                            {
                                                if !is_column {
                                                    ln.rect.h = line.cross_size as i32;
                                                } else {
                                                    ln.rect.w = line.cross_size as i32;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    // For indefinite cross-size, update cross_size to the computed total_cross
                    // so that the flex container's layout node gets the correct height
                    let final_cross_size = if has_definite_cross_size {
                        cross_size as f32
                    } else {
                        total_cross
                    };
                    // Second pass: position items with computed cross offsets
                    let mut cross_offset: f32 = cross_start;
                    let single_line_mode = n_lines == 1;
                    for line in &lines {
                        for &(item_idx, main_pos) in &line.main_positions {
                            if let Some(mut ln) = items[item_idx].layout_node.take() {
                                let pre_x = ln.rect.x;
                                let pre_y = ln.rect.y;
                                let has_flex = items[item_idx].flex_grow > 0.0
                                    || items[item_idx].flex_shrink > 0.0;
                                if has_flex {
                                    let fb = items[item_idx].flex_basis.round() as i32;
                                    if is_column {
                                        ln.rect.h = fb;
                                    } else {
                                        ln.rect.w = fb;
                                    }
                                }
                                let item_align = if items[item_idx].align_self == "auto" {
                                    align_items.clone()
                                } else {
                                    items[item_idx].align_self.clone()
                                };
                                let item_cross_size = if is_column {
                                    ln.rect.w as f32
                                } else {
                                    ln.rect.h as f32
                                };
                                // baseline fallback to flex-start
                                let resolved_align = if item_align == "baseline" {
                                    "flex-start"
                                } else {
                                    item_align.as_str()
                                };
                                let effective_cross = if single_line_mode {
                                    final_cross_size
                                } else {
                                    line.cross_size
                                };
                                let cross_pos = match resolved_align {
                                    "flex-end" | "end" => {
                                        (effective_cross - item_cross_size).max(0.0)
                                    }
                                    "center" => (effective_cross - item_cross_size) / 2.0,
                                    _ => 0.0,
                                };
                                let line_cross = cross_offset + cross_pos;
                                let (mut resolved_x, mut resolved_y) = if is_column {
                                    (
                                        (content_x_scrolled as f32 + line_cross).round() as i32,
                                        (content_y_scrolled as f32 + (main_pos - main_start))
                                            .round() as i32,
                                    )
                                } else {
                                    (
                                        (content_x_scrolled as f32 + (main_pos - main_start))
                                            .round() as i32,
                                        (content_y_scrolled as f32 + line_cross).round() as i32,
                                    )
                                };
                                if is_reverse {
                                    if is_column {
                                        let container_bottom =
                                            content_y_start + content_h_available;
                                        resolved_y = container_bottom - resolved_y - ln.rect.h;
                                    } else {
                                        let container_right = content_x_scrolled + content_w;
                                        resolved_x = container_right - resolved_x - ln.rect.w;
                                    }
                                }
                                translate_layout_subtree(
                                    &mut ln,
                                    resolved_x - pre_x,
                                    resolved_y - pre_y,
                                );
                                ln.rect.x = resolved_x;
                                ln.rect.y = resolved_y;
                                let child_style = flex_children
                                    .iter()
                                    .find(|fc| fc.index == items[item_idx].child_index)
                                    .map(|fc| fc.style)
                                    .unwrap_or(None);
                                apply_relative_position(
                                    child_style,
                                    &mut ln,
                                    content_w,
                                    content_h_available,
                                    my_font_size,
                                    root_font_size,
                                    vw_f,
                                    vh_f,
                                );
                                apply_sticky_position(
                                    child_style,
                                    &mut ln,
                                    content_x,
                                    content_y_start,
                                    content_w,
                                    content_h_available,
                                    scroll_x,
                                    scroll_y,
                                    my_font_size,
                                    root_font_size,
                                    vw_f,
                                    vh_f,
                                );
                                let descendant_dx = ln.rect.x - resolved_x;
                                let descendant_dy = ln.rect.y - resolved_y;
                                translate_layout_descendants(&mut ln, descendant_dx, descendant_dy);
                                max_y_end = max_y_end.max(ln.rect.y + ln.rect.h);
                                laid_children.push(ln);
                            }
                        }
                        cross_offset += line.cross_size + cross_gap + cross_extra_per_gap;
                    }
                    if wrap_reverse {
                        laid_children.sort_by_key(|n| n.source_index.unwrap_or(usize::MAX));
                    }

                    // Handle reverse main axis ordering for multi-line
                    if is_reverse && lines.len() > 1 {
                        // Already handled above per-item
                    }
                } else {
                    // block with inline text flow
                    let mut cur_x = content_x_scrolled;
                    let mut cur_y = content_y_scrolled;
                    let mut last_bottom_margin = 0;
                    let mut line_h = 0;
                    for (idx, c) in children.iter().enumerate() {
                        let is_text = matches!(c, VNode::Text(_));
                        if should_drop_collapsible_whitespace(children, idx, style) {
                            continue;
                        }
                        let child_style = match c {
                            VNode::Element { props, .. } => {
                                props.attrs.get("style").map(|s| s.as_str())
                            }
                            _ => None,
                        };
                        let child_display = style_lookup_str(child_style, "display")
                            .unwrap_or_else(|| "block".to_string());
                        if child_display == "none" {
                            continue;
                        }
                        let position = style_lookup_str(child_style, "position")
                            .unwrap_or_else(|| "static".to_string());
                        if position == "absolute" || position == "fixed" {
                            let cb_x = if position == "fixed" {
                                0
                            } else {
                                content_x_scrolled
                            };
                            let cb_y = if position == "fixed" {
                                0
                            } else {
                                content_y_scrolled
                            };
                            let cb_w = if position == "fixed" {
                                viewport_w
                            } else {
                                content_w
                            };
                            let cb_h = if position == "fixed" {
                                viewport_h
                            } else {
                                content_h_available
                            };
                            let mut child_ln = at(
                                c,
                                cb_x,
                                cb_y,
                                cb_w,
                                cb_h,
                                viewport_w,
                                viewport_h,
                                cb_x,
                                cb_y,
                                cb_w,
                                cb_h,
                                Some(idx),
                                root_font_size,
                                my_font_size,
                            );
                            apply_absolute_position(
                                child_style,
                                &mut child_ln,
                                cb_x,
                                cb_y,
                                cb_w,
                                cb_h,
                                my_font_size,
                                root_font_size,
                                vw_f,
                                vh_f,
                            );
                            abs_children.push(child_ln);
                            continue;
                        }

                        if !is_text && cur_x != content_x_scrolled {
                            cur_y += last_bottom_margin.max(line_h); // Consider bottom margin of last child
                            cur_x = content_x_scrolled;
                            line_h = 0;
                        }

                        if is_text && let VNode::Text(t) = c {
                            // Use inherited font-size, resolved with full unit support
                            let font_sz = style_lookup_font_size(
                                style,
                                my_font_size,
                                root_font_size,
                                (vw_f, vh_f),
                            )
                            .unwrap_or(my_font_size);

                            // Parse text-align from container style
                            let text_align = style
                                .and_then(|s| {
                                    for decl in s.split(';') {
                                        let d = decl.trim();
                                        if d.is_empty() {
                                            continue;
                                        }
                                        if let Some((k, v)) = d.split_once(':')
                                            && k.trim() == "text-align"
                                        {
                                            return Some(v.trim().to_lowercase());
                                        }
                                    }
                                    None
                                })
                                .unwrap_or_else(|| "left".to_string());

                            let line_limit = content_w;
                            // Thread white-space, text-overflow, font-family, scale into wrap (CX-06)
                            let ws_str = style_lookup_str(style, "white-space")
                                .unwrap_or_else(|| "normal".to_string());
                            let ws = match ws_str.trim().to_ascii_lowercase().as_str() {
                                "nowrap" => crate::style::WhiteSpace::Nowrap,
                                "pre" => crate::style::WhiteSpace::Pre,
                                "pre-wrap" => crate::style::WhiteSpace::PreWrap,
                                "pre-line" => crate::style::WhiteSpace::PreLine,
                                _ => crate::style::WhiteSpace::Normal,
                            };
                            let to_str = style_lookup_str(style, "text-overflow")
                                .unwrap_or_else(|| "clip".to_string());
                            let to = if to_str.trim().eq_ignore_ascii_case("ellipsis") {
                                crate::style::TextOverflow::Ellipsis
                            } else {
                                crate::style::TextOverflow::Clip
                            };
                            let fam = style_lookup_str(style, "font-family")
                                .unwrap_or_else(|| "system-ui".to_string());
                            let scale = crate::text_wrap::current_scale();
                            let wrapped = crate::text_wrap::wrap_text_with_options(
                                t, line_limit, font_sz, &fam, scale, ws, to,
                            );

                            for line in &wrapped {
                                // Calculate x offset based on text-align
                                let line_x = match text_align.as_str() {
                                    "center" => {
                                        content_x_scrolled + ((content_w - line.width).max(0) / 2)
                                    }
                                    "right" => content_x_scrolled + (content_w - line.width).max(0),
                                    "justify" => {
                                        // Justify: stretch to fill (handled during rendering)
                                        // For layout, use left alignment with full width
                                        content_x_scrolled
                                    }
                                    _ => content_x_scrolled, // left
                                };

                                let child_ln = LayoutNode {
                                    rect: Rect {
                                        x: line_x,
                                        y: cur_y,
                                        w: line.width,
                                        h: line.height,
                                    },
                                    z_index: 0,
                                    display_none: false,
                                    source_index: Some(idx),
                                    scroll_x: 0,
                                    scroll_y: 0,
                                    clip: None,
                                    stacking_context: false,
                                    scroll_height: line.height,
                                    max_scroll_y: 0,
                                    scrollable: false,
                                    children: vec![],
                                };
                                cur_y += line.height;
                                line_h = line_h.max(line.height);
                                max_y_end = max_y_end.max(cur_y);
                                laid_children.push(child_ln);
                            }

                            continue;
                        }

                        // Get child's margins for collapsing
                        let (_, _, cmt, cmb) = style_box_sides_full(
                            child_style,
                            "margin",
                            content_w as f32,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        );

                        // Margin collapsing: spec-compliant positive/negative partition
                        // First child may collapse through parent if parent has no top border/padding
                        let collapsed_margin_top = if idx == 0 {
                            if pt == 0 && bt == 0 {
                                collapse_margins(mt as f32, cmt as f32).round() as i32
                            } else {
                                cmt
                            }
                        } else {
                            collapse_margins(last_bottom_margin as f32, cmt as f32).round() as i32
                        };

                        // Adjust cur_y for collapsed margin.
                        // at() will add cmt, so we pass cur_y + collapsed_margin_top - cmt
                        // so that at() computes elem_y = (cur_y + collapsed_margin_top - cmt) + cmt = cur_y + collapsed_margin_top
                        // For parent-through, collapsed includes parent mt, but cur_y already includes parent mt via content_y_start;
                        // per brief we keep parent rect y not offset and adjust child via collapsed difference, so the above
                        // formula places child at cur_y + collapsed (where collapsed may be > cmt).
                        // If parent had pt/bt, collapsed == cmt so child at cur_y + cmt as before.
                        let adjusted_cur_y = cur_y + collapsed_margin_top - cmt;

                        let mut child_ln = at(
                            c,
                            cur_x,
                            adjusted_cur_y,
                            (content_w - (cur_x - content_x_scrolled)).max(0),
                            content_h_available,
                            viewport_w,
                            viewport_h,
                            content_x,
                            content_y_start,
                            content_w,
                            content_h_available,
                            Some(idx),
                            root_font_size,
                            my_font_size,
                        );

                        apply_relative_position(
                            child_style,
                            &mut child_ln,
                            content_w,
                            content_h_available,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        );
                        apply_sticky_position(
                            child_style,
                            &mut child_ln,
                            content_x,
                            content_y_start,
                            content_w,
                            content_h_available,
                            scroll_x,
                            scroll_y,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        );

                        // Determine if this block child is empty (no content height, no children)
                        // Empty blocks collapse their own margins together and do not advance cur_y
                        let is_empty_block = child_ln.rect.h == 0 && child_ln.children.is_empty();
                        if is_empty_block {
                            // Effective collapsed for empty = collapse(collapsed_top, cmb)
                            // collapsed_top already is collapse(prev, cmt) or collapse(parent,cmt)
                            // This merges previous bottom, empty top, and empty bottom into one partition value
                            last_bottom_margin =
                                collapse_margins(collapsed_margin_top as f32, cmb as f32).round()
                                    as i32;
                            // Empty does not advance cur_y beyond previous position;
                            // its collapsed margins are represented via last_bottom_margin for next sibling.
                            cur_x = content_x;
                            line_h = 0;
                            max_y_end = max_y_end.max(cur_y);
                            laid_children.push(child_ln);
                        } else {
                            cur_y = child_ln.rect.y + child_ln.rect.h;
                            last_bottom_margin = cmb;
                            cur_x = content_x;
                            line_h = 0;

                            max_y_end = max_y_end.max(child_ln.rect.y + child_ln.rect.h);
                            laid_children.push(child_ln);
                        }
                    }
                    if line_h > 0 {
                        max_y_end = max_y_end.max(cur_y + line_h);
                    }
                }

                // Height: declared or content height + paddings/borders, clamped by min/max-height
                let declared_h2 = style_lookup_len_full(
                    style,
                    "height",
                    avail_h as f32,
                    my_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );
                // Use max_y_end which correctly tracks the spatial extent of all children,
                // including those positioned above content_y_start via negative margins/offsets.
                let content_h = (max_y_end - content_y_start).max(0);
                let mut rect_h = if let Some(dh) = declared_h2 {
                    if is_border_box {
                        dh
                    } else {
                        dh + pt + pb + bt + bb
                    }
                } else if is_viewport_height {
                    // viewport/root heights are viewport-relative - already computed as _rect_h outer
                    _rect_h
                } else {
                    content_h + pt + pb + bt + bb
                };
                let min_h = style_lookup_len_full(
                    style,
                    "min-height",
                    avail_h as f32,
                    my_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                )
                .unwrap_or(0);
                let max_h_val = style_lookup_len_full(
                    style,
                    "max-height",
                    avail_h as f32,
                    my_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );
                let max_h = match max_h_val {
                    Some(v) if v >= 0 => v,
                    _ => i32::MAX,
                };
                rect_h = rect_h.max(min_h).min(max_h);

                if tag == "button"
                    && children.len() == 1
                    && let Some(child) = laid_children.get_mut(0)
                {
                    let content_h = (rect_h - pt - pb - bt - bb).max(0);
                    let child_h = child.rect.h;
                    let offset_y = ((content_h - child_h).max(0)) / 2;
                    child.rect.y = elem_y + bt + pt + offset_y;

                    let align =
                        style_lookup_str(style, "text-align").unwrap_or_else(|| "left".to_string());
                    let child_w = child.rect.w;
                    let offset_x = match align.as_str() {
                        "center" => ((content_w - child_w).max(0)) / 2,
                        "right" => (content_w - child_w).max(0),
                        _ => 0,
                    };
                    child.rect.x = content_x + offset_x;
                }

                // Scrollable overflow model: scrollHeight separation, is_scrollable, clip logic
                let scroll_height = content_h + pt + pb + bt + bb;
                let overflow_lower = overflow.to_ascii_lowercase();
                let scrollable =
                    is_scrollable(&overflow_lower, scroll_height as f32, rect_h as f32);
                let clip = if scrollable || overflow_lower == "hidden" {
                    Some(Rect {
                        x: elem_x,
                        y: elem_y,
                        w: rect_w,
                        h: rect_h,
                    })
                } else {
                    None
                };
                let max_scroll_y = (scroll_height - rect_h).max(0);
                // Synthetic scroll-left/scroll-top stay raw here for compat: children
                // are positioned from the raw values above, so the stored fields must
                // match or render/hit-test would disagree. Wheel-driven scrolling
                // clamps via ScrollState / apply_scroll_offsets instead.
                laid_children.extend(abs_children);
                LayoutNode {
                    rect: Rect {
                        x: elem_x,
                        y: elem_y,
                        w: rect_w,
                        h: rect_h,
                    },
                    z_index,
                    display_none: false,
                    source_index,
                    scroll_x,
                    scroll_y,
                    clip,
                    stacking_context,
                    scroll_height,
                    max_scroll_y,
                    scrollable,
                    children: laid_children,
                }
            }
        }
    }
    at(
        node,
        0,
        0,
        viewport_w,
        viewport_h,
        viewport_w,
        viewport_h,
        0,
        0,
        viewport_w,
        viewport_h,
        None,
        DEFAULT_ROOT_FONT_SIZE,
        DEFAULT_ROOT_FONT_SIZE,
    )
}
