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
                let w = (t.trim().len() as i32).max(1) * 8; // simple estimate
                LayoutNode { rect: Rect { x, y, w, h: 16 }, children: vec![] }
            }
            VNode::Element { props, children, .. } => {
                let style = props.attrs.get("style").map(|s| s.as_str());
                let (ml, mr, mt, mb) = style_box_sides(style, "margin");
                let (pl, pr, pt, pb) = style_box_sides(style, "padding");

                // Element outer position with margins
                let elem_x = x + ml;
                let elem_y = y + mt;

                // Determine width: if set, use as content+padding width; else take available width
                let declared_w = style_lookup_len(style, "width", avail_w);
                let rect_w = declared_w.unwrap_or(avail_w);

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
                } else { // block
                    let mut cur_y = content_y_start;
                    for c in children {
                        let child_ln = at(c, content_x, cur_y, content_w, (avail_h - pt - pb).max(0));
                        // increment cur_y by child's own outer height (we approximate bottom margin via its style)
                        let child_style = match c { VNode::Element { props, .. } => props.attrs.get("style").map(|s| s.as_str()), _ => None };
                        let (_cml, _cmr, _cmt, cmb) = style_box_sides(child_style, "margin");
                        cur_y = child_ln.rect.y + child_ln.rect.h + cmb;
                        laid_children.push(child_ln);
                    }
                }

                // Height: declared or content height + paddings
                let declared_h = style_lookup_len(style, "height", avail_h);
                let content_h = if let Some(last) = laid_children.last() { (last.rect.y + last.rect.h - content_y_start).max(0) } else { 0 };
                let rect_h = declared_h.unwrap_or(content_h + pb);

                LayoutNode { rect: Rect { x: elem_x, y: elem_y, w: rect_w, h: rect_h }, children: laid_children }
            }
        }
    }
    at(node, 0, 0, viewport_w, viewport_h)
}
