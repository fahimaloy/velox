use crate::VNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }

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

fn parse_px(s: &str) -> Option<i32> {
    let t = s.trim();
    if let Some(px) = t.strip_suffix("px") { px.trim().parse().ok() } else { t.parse().ok() }
}

fn style_lookup(style: Option<&str>, key: &str) -> Option<i32> {
    let s = style?;
    for decl in s.split(';') {
        let d = decl.trim();
        if d.is_empty() { continue; }
        if let Some((k,v)) = d.split_once(':') {
            if k.trim() == key { return parse_px(v); }
        }
    }
    None
}

fn style_lookup_len(style: Option<&str>, key: &str, base: i32) -> Option<i32> {
    let s = style?;
    for decl in s.split(';') {
        let d = decl.trim(); if d.is_empty() { continue; }
        if let Some((k,v)) = d.split_once(':') {
            if k.trim() == key {
                let val = v.trim();
                if let Some(p) = val.strip_suffix('%') { if let Ok(pct) = p.trim().parse::<f32>() { return Some(((pct/100.0) * base as f32).round() as i32); } }
                return parse_px(val);
            }
        }
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
        if let Some((k, v)) = d.split_once(':') {
            if k.trim() == key {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

fn style_lookup_i32(style: Option<&str>, key: &str) -> Option<i32> {
    let s = style?;
    for decl in s.split(';') {
        let d = decl.trim();
        if d.is_empty() { continue; }
        if let Some((k, v)) = d.split_once(':') {
            if k.trim() == key {
                return v.trim().parse::<i32>().ok();
            }
        }
    }
    None
}

fn style_box_sides(style: Option<&str>, base: &str) -> (i32, i32, i32, i32) {
    // returns (left, right, top, bottom)
    let s = style.unwrap_or("");
    let mut get = |k: &str| -> Option<i32> {
        for decl in s.split(';') {
            let d = decl.trim();
            if d.is_empty() { continue; }
            if let Some((kk, vv)) = d.split_once(':') {
                if kk.trim() == k { return parse_px(vv); }
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

fn apply_relative_position(style: Option<&str>, node: &mut LayoutNode, base_w: i32, base_h: i32) {
    let pos = style_lookup_str(style, "position").unwrap_or_else(|| "static".to_string());
    if pos != "relative" && pos != "sticky" { return; }
    if let Some(left) = style_lookup_len(style, "left", base_w) {
        node.rect.x += left;
    } else if let Some(right) = style_lookup_len(style, "right", base_w) {
        node.rect.x -= right;
    }
    if let Some(top) = style_lookup_len(style, "top", base_h) {
        node.rect.y += top;
    } else if let Some(bottom) = style_lookup_len(style, "bottom", base_h) {
        node.rect.y -= bottom;
    }
}

fn apply_sticky_position(style: Option<&str>, node: &mut LayoutNode, container_x: i32, container_y: i32, container_w: i32, container_h: i32) {
    let pos = style_lookup_str(style, "position").unwrap_or_else(|| "static".to_string());
    if pos != "sticky" { return; }
    if let Some(left) = style_lookup_len(style, "left", container_w) {
        node.rect.x = node.rect.x.max(container_x + left);
    } else if let Some(right) = style_lookup_len(style, "right", container_w) {
        node.rect.x = node.rect.x.min(container_x + container_w - right - node.rect.w);
    }
    if let Some(top) = style_lookup_len(style, "top", container_h) {
        node.rect.y = node.rect.y.max(container_y + top);
    } else if let Some(bottom) = style_lookup_len(style, "bottom", container_h) {
        node.rect.y = node.rect.y.min(container_y + container_h - bottom - node.rect.h);
    }
}

fn apply_absolute_position(style: Option<&str>, node: &mut LayoutNode, cb_x: i32, cb_y: i32, cb_w: i32, cb_h: i32) {
    let left = style_lookup_len(style, "left", cb_w);
    let right = style_lookup_len(style, "right", cb_w);
    let top = style_lookup_len(style, "top", cb_h);
    let bottom = style_lookup_len(style, "bottom", cb_h);
    let declared_w = style_lookup_len(style, "width", cb_w);
    let declared_h = style_lookup_len(style, "height", cb_h);

    if declared_w.is_none() {
        if let (Some(l), Some(r)) = (left, right) {
            node.rect.w = (cb_w - l - r).max(0);
        }
    }
    if declared_h.is_none() {
        if let (Some(t), Some(b)) = (top, bottom) {
            node.rect.h = (cb_h - t - b).max(0);
        }
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

/// Very simple block layout: each element is stacked vertically, full width
/// unless width/height are provided via inline `style` (width/height in px).
pub fn compute_layout(node: &VNode, viewport_w: i32, viewport_h: i32) -> LayoutNode {
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
    ) -> LayoutNode {
        match node {
            VNode::Text(t) => {
                let len = t.chars().count() as i32;
                let w = if len > 0 { len * 8 } else { 0 }; // simple estimate
                LayoutNode {
                    rect: Rect { x, y, w, h: 16 },
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
            VNode::Element { tag, props, children } => {
                let style = props.attrs.get("style").map(|s| s.as_str());
                let (ml, mr, mt, mb) = style_box_sides(style, "margin");
                let (pl, pr, pt, pb) = style_box_sides(style, "padding");
                let is_root = matches!(tag.as_str(), "body" | "html");

                // Element outer position with margins
                let elem_x = x + ml;
                let elem_y = y + mt;

                // Determine width: if set, use as content+padding width; else take available width
                let declared_w = style_lookup_len(style, "width", avail_w);
                let rect_w = if is_root {
                    (avail_w - ml - mr).max(1)
                } else {
                    declared_w.unwrap_or(avail_w)
                };

                // Content box
                let content_x = elem_x + pl;
                let content_y_start = elem_y + pt;
                let content_w = (rect_w - pl - pr).max(0);

                let overflow = style_lookup_str(style, "overflow").unwrap_or_else(|| "visible".to_string());
                let scroll_x = style_lookup_len(style, "scroll-left", content_w).unwrap_or(0);
                let scroll_y = style_lookup_len(style, "scroll-top", (avail_h - pt - pb).max(0)).unwrap_or(0);
                let content_x_scrolled = content_x - scroll_x;
                let content_y_scrolled = content_y_start - scroll_y;

                // Layout strategy: block (default) or flex
                let display = style_lookup_str(style, "display").unwrap_or_else(|| "block".to_string());
                let position = style_lookup_str(style, "position").unwrap_or_else(|| "static".to_string());
                let z_index = if position != "static" {
                    style_lookup_i32(style, "z-index").unwrap_or(0)
                } else {
                    0
                };
                let opacity = style_lookup_str(style, "opacity")
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(1.0);
                let transform = style_lookup_str(style, "transform").unwrap_or_else(|| "none".to_string());
                let stacking_context = opacity < 1.0 || (transform != "none" && !transform.is_empty()) || position != "static";
                if display == "none" {
                    return LayoutNode {
                        rect: Rect { x: elem_x, y: elem_y, w: 0, h: 0 },
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
                    // Minimal flexbox: direction (row|column), gap, align-items (start|center|end), justify-content (flex-start|center|space-between)
                    let flex_dir = props.attrs.get("style").and_then(|s| {
                        for decl in s.split(';') { let d=decl.trim(); if d.is_empty(){continue;} if let Some((k,v))=d.split_once(':'){ if k.trim()=="flex-direction" { return Some(v.trim()); } } }
                        None
                    }).unwrap_or("row");
                    let gap = style_lookup_len(style, "gap", 0).unwrap_or(0);
                    let mut cursor_x = content_x_scrolled;
                    let mut cursor_y = content_y_scrolled;
                    let mut line_max_h = 0;
                    if flex_dir == "column" {
                        for (idx, c) in children.iter().enumerate() {
                            let child_style = match c { VNode::Element { props, .. } => props.attrs.get("style").map(|s| s.as_str()), _ => None };
                            let child_display = style_lookup_str(child_style, "display").unwrap_or_else(|| "block".to_string());
                            if child_display == "none" { continue; }
                            let position = style_lookup_str(child_style, "position").unwrap_or_else(|| "static".to_string());
                            if position == "absolute" || position == "fixed" {
                                let cb_x = if position == "fixed" { 0 } else { content_x_scrolled };
                                let cb_y = if position == "fixed" { 0 } else { content_y_scrolled };
                                let cb_w = if position == "fixed" { viewport_w } else { content_w };
                                let cb_h = if position == "fixed" { viewport_h } else { (avail_h - pt - pb).max(0) };
                                let mut child_ln = at(c, cb_x, cb_y, cb_w, cb_h, viewport_w, viewport_h, cb_x, cb_y, cb_w, cb_h, Some(idx));
                                apply_absolute_position(child_style, &mut child_ln, cb_x, cb_y, cb_w, cb_h);
                                abs_children.push(child_ln);
                                continue;
                            }
                            let mut child_ln = at(c, content_x_scrolled, cursor_y, content_w, (avail_h - pt - pb).max(0), viewport_w, viewport_h, content_x, content_y_start, content_w, (avail_h - pt - pb).max(0), Some(idx));
                            apply_relative_position(child_style, &mut child_ln, content_w, (avail_h - pt - pb).max(0));
                            apply_sticky_position(child_style, &mut child_ln, content_x, content_y_start, content_w, (avail_h - pt - pb).max(0));
                            cursor_y = child_ln.rect.y + child_ln.rect.h + gap;
                            laid_children.push(child_ln);
                        }
                    } else { // row
                        for (idx, c) in children.iter().enumerate() {
                            let child_style = match c { VNode::Element { props, .. } => props.attrs.get("style").map(|s| s.as_str()), _ => None };
                            let child_display = style_lookup_str(child_style, "display").unwrap_or_else(|| "block".to_string());
                            if child_display == "none" { continue; }
                            let position = style_lookup_str(child_style, "position").unwrap_or_else(|| "static".to_string());
                            if position == "absolute" || position == "fixed" {
                                let cb_x = if position == "fixed" { 0 } else { content_x_scrolled };
                                let cb_y = if position == "fixed" { 0 } else { content_y_scrolled };
                                let cb_w = if position == "fixed" { viewport_w } else { content_w };
                                let cb_h = if position == "fixed" { viewport_h } else { (avail_h - pt - pb).max(0) };
                                let mut child_ln = at(c, cb_x, cb_y, cb_w, cb_h, viewport_w, viewport_h, cb_x, cb_y, cb_w, cb_h, Some(idx));
                                apply_absolute_position(child_style, &mut child_ln, cb_x, cb_y, cb_w, cb_h);
                                abs_children.push(child_ln);
                                continue;
                            }
                            let mut child_ln = at(c, cursor_x, content_y_scrolled, content_w, (avail_h - pt - pb).max(0), viewport_w, viewport_h, content_x, content_y_start, content_w, (avail_h - pt - pb).max(0), Some(idx));
                            // set all y to top for now (no align-items support beyond start)
                            child_ln.rect.y = content_y_scrolled;
                            apply_relative_position(child_style, &mut child_ln, content_w, (avail_h - pt - pb).max(0));
                            apply_sticky_position(child_style, &mut child_ln, content_x, content_y_start, content_w, (avail_h - pt - pb).max(0));
                            cursor_x = child_ln.rect.x + child_ln.rect.w + gap;
                            if child_ln.rect.h > line_max_h { line_max_h = child_ln.rect.h; }
                            laid_children.push(child_ln);
                        }
                    }
                } else { // block with inline text flow
                    let mut cur_x = content_x_scrolled;
                    let mut cur_y = content_y_scrolled;
let mut last_bottom_margin = 0;
                    let mut line_h = 0;
                    let mut max_y_end = content_y_start;
                    for (idx, c) in children.iter().enumerate() {
                        let is_text = matches!(c, VNode::Text(_));
                        let child_style = match c { VNode::Element { props, .. } => props.attrs.get("style").map(|s| s.as_str()), _ => None };
                        let child_display = style_lookup_str(child_style, "display").unwrap_or_else(|| "block".to_string());
                        if child_display == "none" { continue; }
                        let position = style_lookup_str(child_style, "position").unwrap_or_else(|| "static".to_string());
                        if position == "absolute" || position == "fixed" {
                            let cb_x = if position == "fixed" { 0 } else { content_x_scrolled };
                            let cb_y = if position == "fixed" { 0 } else { content_y_scrolled };
                            let cb_w = if position == "fixed" { viewport_w } else { content_w };
                            let cb_h = if position == "fixed" { viewport_h } else { (avail_h - pt - pb).max(0) };
                            let mut child_ln = at(c, cb_x, cb_y, cb_w, cb_h, viewport_w, viewport_h, cb_x, cb_y, cb_w, cb_h, Some(idx));
                            apply_absolute_position(child_style, &mut child_ln, cb_x, cb_y, cb_w, cb_h);
                            abs_children.push(child_ln);
                            continue;
                        }

                        if !is_text && cur_x != content_x_scrolled {
                            cur_y += last_bottom_margin.max(line_h); // Consider bottom margin of last child
                            cur_x = content_x_scrolled;
                            line_h = 0;
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
                        );

                        if is_text {
                            let line_limit = content_x_scrolled + content_w;
                            if cur_x != content_x_scrolled && (cur_x + child_ln.rect.w) > line_limit {
                                cur_y += line_h.max(child_ln.rect.h);
                                cur_x = content_x_scrolled;
                                line_h = 0;
                            }
                        }

                        if is_text {
                            child_ln = at(
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
                            );
                        }

                        apply_relative_position(child_style, &mut child_ln, content_w, (avail_h - pt - pb).max(0));
                        apply_sticky_position(child_style, &mut child_ln, content_x, content_y_start, content_w, (avail_h - pt - pb).max(0));

                        if is_text {  
                            cur_x += child_ln.rect.w;
                            line_h = line_h.max(child_ln.rect.h);
                        } else {
                            let (_cml, _cmr, _cmt, cmb) = style_box_sides(child_style, "margin");
                            last_bottom_margin = cmb;
cur_y = child_ln.rect.y + child_ln.rect.h + cmb;
                            cur_x = content_x;
                            line_h = 0;
                        }

                        max_y_end = max_y_end.max(child_ln.rect.y + child_ln.rect.h);
                        laid_children.push(child_ln);
                    }
                    if line_h > 0 {
                        max_y_end = max_y_end.max(cur_y + line_h);
                    }
                    cur_y = max_y_end;
                }

                // Height: declared or content height + paddings
                let declared_h = style_lookup_len(style, "height", avail_h);
                let content_h = laid_children
                    .iter()
                    .map(|c| c.rect.y + c.rect.h + scroll_y)
                    .max()
                    .map(|max_y| (max_y - content_y_start).max(0))
                    .unwrap_or(0);
                let rect_h = if is_root {
                    (avail_h - mt - mb).max(1)
                } else {
                    declared_h.unwrap_or(content_h + pt + pb)
                };

                if tag == "button" && children.len() == 1 {
                    if let Some(child) = laid_children.get_mut(0) {
                        let content_h = (rect_h - pt - pb).max(0);
                        let child_h = child.rect.h;
                        let offset_y = ((content_h - child_h).max(0)) / 2;
                        child.rect.y = elem_y + pt + offset_y;

                        let align = style_lookup_str(style, "text-align").unwrap_or_else(|| "left".to_string());
                        let child_w = child.rect.w;
                        let offset_x = match align.as_str() {
                            "center" => ((content_w - child_w).max(0)) / 2,
                            "right" => (content_w - child_w).max(0),
                            _ => 0,
                        };
                        child.rect.x = content_x + offset_x;
                    }
                }

                let clip = if matches!(overflow.as_str(), "hidden" | "scroll" | "auto") {
                    Some(Rect { x: elem_x, y: elem_y, w: rect_w, h: rect_h })
                } else {
                    None
                };
                laid_children.extend(abs_children);
                LayoutNode {
                    rect: Rect { x: elem_x, y: elem_y, w: rect_w, h: rect_h },
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
    at(node, 0, 0, viewport_w, viewport_h, viewport_w, viewport_h, 0, 0, viewport_w, viewport_h, None)
}
