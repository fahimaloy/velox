use crate::{Length, VNode};

/// Default font size for root element (used for rem calculations)
const DEFAULT_ROOT_FONT_SIZE: f32 = 16.0;

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
    pub children: Vec<LayoutNode>,
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
        return Some((pct / 100.0) * parent_size);
    }

    // Pixels
    if let Some(px) = val.strip_suffix("px")
        && let Ok(v) = px.trim().parse::<f32>()
    {
        return Some(v);
    }

    // rem (root em)
    if let Some(rem) = val.strip_suffix("rem")
        && let Ok(v) = rem.trim().parse::<f32>()
    {
        return Some(v * root_font_size);
    }

    // em (parent-relative)
    if let Some(em) = val.strip_suffix("em")
        && let Ok(v) = em.trim().parse::<f32>()
    {
        return Some(v * parent_font_size);
    }

    // viewport width
    if let Some(vw) = val.strip_suffix("vw")
        && let Ok(v) = vw.trim().parse::<f32>()
    {
        return Some((v / 100.0) * viewport.0);
    }

    // viewport height
    if let Some(vh) = val.strip_suffix("vh")
        && let Ok(v) = vh.trim().parse::<f32>()
    {
        return Some((v / 100.0) * viewport.1);
    }

    // Plain number -> pixels
    if let Ok(v) = val.parse::<f32>() {
        return Some(v);
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
        Length::Auto => 0.0,
        Length::Zero => 0.0,
    }
}

/// Box sides (margin/padding) with full CSS unit support
fn style_box_sides_full(
    style: Option<&str>,
    base: &str,
    parent_size: f32,
    parent_font_size: f32,
    root_font_size: f32,
    viewport_w: f32,
    viewport_h: f32,
) -> (i32, i32, i32, i32) {
    let get = |k: &str| -> Option<i32> {
        style_lookup_len_full(
            style,
            k,
            parent_size,
            parent_font_size,
            root_font_size,
            viewport_w,
            viewport_h,
        )
    };
    let all = get(base).unwrap_or(0);
    let l = get(&format!("{}-left", base)).unwrap_or(all);
    let r = get(&format!("{}-right", base)).unwrap_or(all);
    let t = get(&format!("{}-top", base)).unwrap_or(all);
    let b = get(&format!("{}-bottom", base)).unwrap_or(all);
    (l, r, t, b)
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
        _containing_w: i32,
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
                    avail_w as f32,
                    parent_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );
                let (pl, pr, pt, pb) = style_box_sides_full(
                    style,
                    "padding",
                    avail_w as f32,
                    parent_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );
                let is_root = matches!(tag.as_str(), "body" | "html");

                // Element outer position with margins
                let elem_x = x + ml;
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
                let rect_w = if is_root {
                    (avail_w - ml - mr).max(1)
                } else {
                    declared_w.unwrap_or(avail_w)
                };

                // Content box
                let content_x = elem_x + pl;
                let content_y_start = elem_y + pt;
                let content_w = (rect_w - pl - pr).max(0);

                let overflow =
                    style_lookup_str(style, "overflow").unwrap_or_else(|| "visible".to_string());
                let scroll_x = style_lookup_len_full(
                    style,
                    "scroll-left",
                    content_w as f32,
                    my_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                )
                .unwrap_or(0);
                let scroll_y = style_lookup_len_full(
                    style,
                    "scroll-top",
                    (avail_h - pt - pb).max(0) as f32,
                    my_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                )
                .unwrap_or(0);
                let content_x_scrolled = content_x - scroll_x;
                let content_y_scrolled = content_y_start - scroll_y;

                // Layout strategy: block (default) or flex
                let display =
                    style_lookup_str(style, "display").unwrap_or_else(|| "block".to_string());
                let position =
                    style_lookup_str(style, "position").unwrap_or_else(|| "static".to_string());
                let z_index = if position != "static" {
                    style_lookup_i32(style, "z-index").unwrap_or(0)
                } else {
                    0
                };
                let opacity = style_lookup_str(style, "opacity")
                    .and_then(|v| v.parse::<f32>().ok())
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
                        children: vec![],
                    };
                }

                let mut laid_children: Vec<LayoutNode> = Vec::new();
                let mut abs_children: Vec<LayoutNode> = Vec::new();
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
                                (avail_h - pt - pb).max(0)
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

                    // Determine main/cross axis dimensions
                    let is_column = flex_dir == "column" || flex_dir == "column-reverse";
                    let is_reverse = flex_dir == "row-reverse" || flex_dir == "column-reverse";
                    let is_wrap = flex_wrap == "wrap" || flex_wrap == "wrap-reverse";
                    let _wrap_reverse = flex_wrap == "wrap-reverse";

                    let main_size = if is_column {
                        (avail_h - pt - pb).max(0)
                    } else {
                        content_w
                    };
                    // Cross size: use explicit container dimension if set, otherwise use available
                    let explicit_h = style_lookup_len_full(
                        style,
                        "height",
                        avail_h as f32,
                        my_font_size,
                        root_font_size,
                        vw_f,
                        vh_f,
                    );
                    let _explicit_w = style_lookup_len_full(
                        style,
                        "width",
                        avail_w as f32,
                        my_font_size,
                        root_font_size,
                        vw_f,
                        vh_f,
                    );
                    let cross_size = if is_column {
                        content_w
                    } else {
                        explicit_h.unwrap_or(avail_h - pt - pb).max(0)
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

                    let mut items: Vec<FlexItem> = Vec::new();
                    for fc in &flex_children {
                        // Pre-layout child to get its natural size
                        let child_avail_main = if is_column {
                            (avail_h - pt - pb).max(0)
                        } else {
                            main_size
                        };
                        let child_avail_cross = if is_column {
                            cross_size
                        } else {
                            (avail_h - pt - pb).max(0)
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
                            (avail_h - pt - pb).max(0),
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
                        let flex_basis = if let Some(fb) = flex_basis_val {
                            fb as f32
                        } else if let Some(exp) = explicit_main {
                            exp as f32
                        } else {
                            // For items without explicit size or flex-basis,
                            // use 0 as intrinsic main size (CSS spec: content-based sizing)
                            // flex-grow will distribute remaining space
                            0.0
                        };

                        let flex_grow: f32 = style_lookup_str(fc.style, "flex-grow")
                            .and_then(|v| v.parse::<f32>().ok())
                            .unwrap_or(0.0);
                        let flex_shrink: f32 = style_lookup_str(fc.style, "flex-shrink")
                            .and_then(|v| v.parse::<f32>().ok())
                            .unwrap_or(1.0);

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
                            Some(v) if v > 0 => v as f32,
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
                    #[allow(dead_code)]
                    struct FlexLine {
                        items: Vec<usize>, // indices into items
                        main_size: f32,
                        cross_size: f32,
                    }
                    let mut lines: Vec<FlexLine> = Vec::new();

                    if !is_wrap {
                        // Single line - all items
                        lines.push(FlexLine {
                            items: (0..items.len()).collect(),
                            main_size: 0.0,
                            cross_size: 0.0,
                        });
                    } else {
                        let mut current_line = FlexLine {
                            items: Vec::new(),
                            main_size: 0.0,
                            cross_size: 0.0,
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
                        });
                    }

                    // Step 3: For each line, distribute flex
                    let mut main_offset = if is_column { pt as f32 } else { pl as f32 };
                    for line in &mut lines {
                        // Calculate total flex basis and grow/shrink factors
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

                        // Resolve flex grow
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
                            // Resolve flex shrink
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

                        // Recalculate line main_size after flex resolution
                        line.main_size =
                            line.items.iter().map(|&i| items[i].flex_basis).sum::<f32>()
                                + total_gap;

                        // Step 4: Justify content (main axis alignment)
                        let mut main_positions: Vec<(usize, f32)> = Vec::new(); // (item_idx, position)
                        let effective_main = line.main_size;
                        let extra_space = (main_size as f32 - effective_main).max(0.0);
                        let gap_val = if is_column { row_gap } else { column_gap };

                        let start_offset = match justify_content.as_str() {
                            "flex-start" | "start" => 0.0,
                            "flex-end" | "end" => extra_space,
                            "center" => extra_space / 2.0,
                            "space-between" => {
                                if line.items.len() <= 1 {
                                    0.0
                                } else {
                                    extra_space / (line.items.len() as f32 - 1.0)
                                }
                            }
                            "space-around" => extra_space / line.items.len() as f32,
                            "space-evenly" => extra_space / (line.items.len() as f32 + 1.0),
                            _ => 0.0,
                        };

                        let mut cursor = main_offset + start_offset;
                        // For space-around, add half gap at start
                        if justify_content == "space-around" && line.items.len() > 1 {
                            cursor += start_offset / 2.0;
                        }
                        // For space-evenly, start after one slot
                        if justify_content == "space-evenly" && !line.items.is_empty() {
                            cursor = main_offset + start_offset;
                        }

                        for (line_idx, &item_idx) in line.items.iter().enumerate() {
                            let item_main_size = items[item_idx].flex_basis;
                            let pos = if justify_content == "space-between" && line.items.len() > 1
                            {
                                main_offset
                                    + line_idx as f32
                                        * (extra_space / (line.items.len() as f32 - 1.0)
                                            + items[item_idx].flex_basis
                                            + gap_val as f32)
                                    - gap_val as f32
                            } else if justify_content == "space-evenly" {
                                main_offset
                                    + start_offset
                                    + line_idx as f32 * (items[item_idx].flex_basis + start_offset)
                                    + start_offset
                            } else if justify_content == "space-around" {
                                let _half_gap = start_offset / 2.0;
                                if line_idx == 0 {
                                    main_offset
                                } else {
                                    let prev_size = items[line.items[line_idx - 1]].flex_basis;
                                    let prev_pos = if line_idx == 1 {
                                        main_offset
                                    } else {
                                        main_positions[line_idx - 1].1
                                    };
                                    prev_pos + prev_size + start_offset
                                }
                            } else {
                                cursor
                            };

                            main_positions.push((item_idx, pos));
                            if justify_content == "flex-start"
                                || justify_content == "start"
                                || justify_content == "center"
                                || justify_content == "flex-end"
                                || justify_content == "end"
                            {
                                cursor = pos + item_main_size + gap_val as f32;
                            }
                        }

                        // Step 5: Cross axis alignment (align-items / align-self)
                        // First, find max cross size for the line
                        let mut max_cross_size: f32 = 0.0;
                        for &(item_idx, _) in &main_positions {
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

                        // For stretch, expand items without explicit cross size
                        let effective_align_items = align_items.clone();
                        if effective_align_items == "stretch" {
                            for &(item_idx, _) in &main_positions {
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
                                if explicit_cross.is_none() && items[item_idx].align_self == "auto"
                                {
                                    // Stretch the item
                                    if let Some(ref mut ln) = items[item_idx].layout_node {
                                        if is_column {
                                            ln.rect.w = cross_size as i32;
                                        } else {
                                            ln.rect.h = cross_size as i32;
                                        }
                                    }
                                    max_cross_size = max_cross_size.max(cross_size as f32);
                                }
                            }
                        }

                        // Position items
                        for &(item_idx, main_pos) in &main_positions {
                            if let Some(mut ln) = items[item_idx].layout_node.take() {
                                // Update main dimension based on flex distribution
                                let fb = items[item_idx].flex_basis.round() as i32;
                                if is_column {
                                    ln.rect.h = fb;
                                } else {
                                    ln.rect.w = fb;
                                }

                                let item_align = if items[item_idx].align_self == "auto" {
                                    effective_align_items.clone()
                                } else {
                                    items[item_idx].align_self.clone()
                                };

                                // Calculate cross position based on alignment
                                let item_cross_size = if is_column {
                                    ln.rect.w as f32
                                } else {
                                    ln.rect.h as f32
                                };
                                // Use cross_size (container's content dimension) not avail_h
                                let container_cross = cross_size as f32;
                                let cross_pos = match item_align.as_str() {
                                    "flex-start" | "start" => {
                                        if is_column {
                                            pl as f32
                                        } else {
                                            pt as f32
                                        }
                                    }
                                    "flex-end" | "end" => {
                                        let padding_cross = if is_column {
                                            pl as f32 + pr as f32
                                        } else {
                                            pt as f32 + pb as f32
                                        };
                                        (if is_column { pr as f32 } else { pb as f32 })
                                            + (container_cross - item_cross_size - padding_cross)
                                                .max(0.0)
                                    }
                                    "center" => {
                                        let padding_cross = if is_column {
                                            pl as f32 + pr as f32
                                        } else {
                                            pt as f32 + pb as f32
                                        };
                                        let available = (container_cross - padding_cross).max(0.0);
                                        (if is_column { pl as f32 } else { pt as f32 })
                                            + (available - item_cross_size) / 2.0
                                    }
                                    _ => {
                                        if is_column {
                                            pl as f32
                                        } else {
                                            pt as f32
                                        }
                                    } // stretch/flex-start
                                };

                                // Set final position
                                if is_column {
                                    ln.rect.x = content_x_scrolled;
                                    ln.rect.y = main_pos as i32;
                                } else {
                                    ln.rect.x = main_pos as i32;
                                    ln.rect.y = cross_pos as i32;
                                }

                                // Handle reverse directions
                                if is_reverse {
                                    if is_column {
                                        let container_bottom =
                                            content_y_start + (avail_h - pt - pb).max(0);
                                        ln.rect.y = container_bottom - ln.rect.y - ln.rect.h;
                                    } else {
                                        let container_right = content_x_scrolled + content_w;
                                        ln.rect.x = container_right - ln.rect.x - ln.rect.w;
                                    }
                                }

                                let child_style = flex_children
                                    .iter()
                                    .find(|fc| fc.index == items[item_idx].child_index)
                                    .map(|fc| fc.style)
                                    .unwrap_or(None);

                                apply_relative_position(
                                    child_style,
                                    &mut ln,
                                    content_w,
                                    (avail_h - pt - pb).max(0),
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
                                    (avail_h - pt - pb).max(0),
                                    scroll_x,
                                    scroll_y,
                                    my_font_size,
                                    root_font_size,
                                    vw_f,
                                    vh_f,
                                );

                                laid_children.push(ln);
                            }
                        }

                        // Advance main offset for next line
                        main_offset +=
                            line.main_size + (if is_column { row_gap } else { column_gap }) as f32;
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
                    let mut max_y_end = content_y_start;
                    for (idx, c) in children.iter().enumerate() {
                        let is_text = matches!(c, VNode::Text(_));
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
                                (avail_h - pt - pb).max(0)
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
                            let wrapped = crate::text_wrap::wrap_text(t, line_limit, font_sz);

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
                                    children: vec![],
                                };
                                cur_y += line.height;
                                line_h = line_h.max(line.height);
                                max_y_end = max_y_end.max(cur_y);
                                laid_children.push(child_ln);
                            }

                            continue;
                        }

                        let mut child_ln = at(
                            c,
                            cur_x,
                            cur_y,
                            (content_w - (cur_x - content_x_scrolled)).max(0),
                            (avail_h - pt - pb).max(0),
                            viewport_w,
                            viewport_h,
                            content_x,
                            content_y_start,
                            content_w,
                            (avail_h - pt - pb).max(0),
                            Some(idx),
                            root_font_size,
                            my_font_size,
                        );

                        apply_relative_position(
                            child_style,
                            &mut child_ln,
                            content_w,
                            (avail_h - pt - pb).max(0),
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
                            (avail_h - pt - pb).max(0),
                            scroll_x,
                            scroll_y,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        );

                        let (_cml, _cmr, _cmt, cmb) = style_box_sides_full(
                            child_style,
                            "margin",
                            content_w as f32,
                            my_font_size,
                            root_font_size,
                            vw_f,
                            vh_f,
                        );
                        last_bottom_margin = cmb;
                        cur_y = child_ln.rect.y + child_ln.rect.h + cmb;
                        cur_x = content_x;
                        line_h = 0;

                        max_y_end = max_y_end.max(child_ln.rect.y + child_ln.rect.h);
                        laid_children.push(child_ln);
                    }
                    if line_h > 0 {
                        max_y_end = max_y_end.max(cur_y + line_h);
                    }
                    let _cur_y_end = max_y_end;
                }

                // Height: declared or content height + paddings, clamped by min/max-height
                let declared_h = style_lookup_len_full(
                    style,
                    "height",
                    avail_h as f32,
                    my_font_size,
                    root_font_size,
                    vw_f,
                    vh_f,
                );
                let content_h = laid_children
                    .iter()
                    .map(|c| c.rect.y + c.rect.h + scroll_y)
                    .max()
                    .map(|max_y| (max_y - content_y_start).max(0))
                    .unwrap_or(0);
                let mut rect_h = if is_root {
                    (avail_h - mt - mb).max(1)
                } else {
                    declared_h.unwrap_or(content_h + pt + pb)
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
                    Some(v) if v > 0 => v,
                    _ => i32::MAX,
                };
                rect_h = rect_h.max(min_h).min(max_h);

                if tag == "button"
                    && children.len() == 1
                    && let Some(child) = laid_children.get_mut(0)
                {
                    let content_h = (rect_h - pt - pb).max(0);
                    let child_h = child.rect.h;
                    let offset_y = ((content_h - child_h).max(0)) / 2;
                    child.rect.y = elem_y + pt + offset_y;

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

                let clip = if matches!(overflow.as_str(), "hidden" | "scroll" | "auto") {
                    Some(Rect {
                        x: elem_x,
                        y: elem_y,
                        w: rect_w,
                        h: rect_h,
                    })
                } else {
                    None
                };
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
