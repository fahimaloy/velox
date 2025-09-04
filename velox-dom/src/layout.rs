use crate::{VNode};

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

/// Very simple block layout: each element is stacked vertically, full width
/// unless width/height are provided via inline `style` (width/height in px).
pub fn compute_layout(node: &VNode, viewport_w: i32) -> LayoutNode {
    match node {
        VNode::Text(t) => {
            let w = (t.len() as i32) * 8; // fake width estimate
            LayoutNode { rect: Rect { x: 0, y: 0, w, h: 16 }, children: vec![] }
        }
        VNode::Element { props, children, .. } => {
            let style = props.attrs.get("style").map(|s| s.as_str());
            let mut y = 0;
            let mut laid_children = Vec::new();
            for c in children {
                let mut ln = compute_layout(c, viewport_w);
                ln.rect.x = 0;
                ln.rect.y = y;
                y += ln.rect.h;
                laid_children.push(ln);
            }
            let mut w = viewport_w;
            if let Some(sw) = style_lookup(style, "width") { w = sw; }
            let mut h: i32 = if let Some(sh) = style_lookup(style, "height") { sh } else { y.max(16) };
            LayoutNode { rect: Rect { x: 0, y: 0, w, h }, children: laid_children }
        }
    }
}

