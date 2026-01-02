use crate::VNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutNode { pub rect: Rect, pub children: Vec<LayoutNode> }

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

/// Very simple block layout: each element is stacked vertically, full width
/// unless width/height are provided via inline `style` (width/height in px).
pub fn compute_layout(node: &VNode, viewport_w: i32, viewport_h: i32) -> LayoutNode {
    fn at(node: &VNode, x: i32, y: i32, avail_w: i32, avail_h: i32) -> LayoutNode {
        match node {
            VNode::Text(t) => {
                let len = t.chars().count() as i32;
                let w = if len > 0 { len * 8 } else { 0 }; // simple estimate
                LayoutNode { rect: Rect { x, y, w, h: 16 }, children: vec![] }
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

                // Layout strategy: block (default) or flex
                let display = props.attrs.get("style").and_then(|s| {
                    for decl in s.split(';') { let d=decl.trim(); if d.is_empty(){continue;} if let Some((k,v))=d.split_once(':'){ if k.trim()=="display" { return Some(v.trim()); } } }
                    None
                }).unwrap_or("block");

                let mut laid_children = Vec::new();
                if display == "flex" {
                    // Minimal flexbox: direction (row|column), gap, align-items (start|center|end), justify-content (flex-start|center|space-between)
                    let flex_dir = props.attrs.get("style").and_then(|s| {
                        for decl in s.split(';') { let d=decl.trim(); if d.is_empty(){continue;} if let Some((k,v))=d.split_once(':'){ if k.trim()=="flex-direction" { return Some(v.trim()); } } }
                        None
                    }).unwrap_or("row");
                    let gap = style_lookup_len(style, "gap", 0).unwrap_or(0);
                    let mut cursor_x = content_x;
                    let mut cursor_y = content_y_start;
                    let mut line_max_h = 0;
                    if flex_dir == "column" {
                        for c in children {
                            let child_ln = at(c, content_x, cursor_y, content_w, (avail_h - pt - pb).max(0));
                            cursor_y = child_ln.rect.y + child_ln.rect.h + gap;
                            laid_children.push(child_ln);
                        }
                    } else { // row
                        for c in children {
                            let child_ln = at(c, cursor_x, content_y_start, content_w, (avail_h - pt - pb).max(0));
                            cursor_x = child_ln.rect.x + child_ln.rect.w + gap;
                            if child_ln.rect.h > line_max_h { line_max_h = child_ln.rect.h; }
                            laid_children.push(child_ln);
                        }
                        // set all y to top for now (no align-items support beyond start)
                        for ln in &mut laid_children { ln.rect.y = content_y_start; }
                    }
                } else { // block with inline text flow
                    let mut cur_x = content_x;
                    let mut cur_y = content_y_start;
                    let mut line_h = 0;
                    let mut max_y_end = content_y_start;
                    for c in children {
                        let is_text = matches!(c, VNode::Text(_));
                        if !is_text && cur_x != content_x {
                            cur_y += line_h;
                            cur_x = content_x;
                            line_h = 0;
                        }

                        let child_ln = at(
                            c,
                            cur_x,
                            cur_y,
                            (content_w - (cur_x - content_x)).max(0),
                            (avail_h - pt - pb).max(0),
                        );

                        if is_text {
                            let line_limit = content_x + content_w;
                            if cur_x != content_x && (cur_x + child_ln.rect.w) > line_limit {
                                cur_y += line_h.max(child_ln.rect.h);
                                cur_x = content_x;
                                line_h = 0;
                            }
                        }

                        let child_ln = if is_text {
                            at(
                                c,
                                cur_x,
                                cur_y,
                                (content_w - (cur_x - content_x)).max(0),
                                (avail_h - pt - pb).max(0),
                            )
                        } else {
                            child_ln
                        };

                        if is_text {
                            cur_x += child_ln.rect.w;
                            line_h = line_h.max(child_ln.rect.h);
                        } else {
                            let child_style = match c { VNode::Element { props, .. } => props.attrs.get("style").map(|s| s.as_str()), _ => None };
                            let (_cml, _cmr, _cmt, cmb) = style_box_sides(child_style, "margin");
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
                    .map(|c| c.rect.y + c.rect.h)
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

                LayoutNode { rect: Rect { x: elem_x, y: elem_y, w: rect_w, h: rect_h }, children: laid_children }
            }
        }
    }
    at(node, 0, 0, viewport_w, viewport_h)
}
