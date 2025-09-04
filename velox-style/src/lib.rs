use std::collections::HashMap;

use velox_dom::{VNode, Props};

#[derive(Debug, Clone, PartialEq)]
pub enum SimpleSelectorKind { Tag, Class }

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleSelector {
    pub kind: SimpleSelectorKind,
    pub name: String,
    pub hover: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub selector: SimpleSelector,
    pub decls: HashMap<String, String>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

impl Stylesheet {
    pub fn parse(css: &str) -> Self {
        // Extremely small parser: supports
        //  - `tag { key: value; }`
        //  - `.class { key: value; }`
        // Ignores unknown syntax.
        let mut rules = Vec::new();
        for block in css.split('}').map(str::trim) {
            if block.is_empty() { continue; }
            let (sel, body) = match block.split_once('{') { Some((a,b)) => (a.trim(), b.trim()), None => continue };
            let (name_raw, hover) = if let Some((base, pseudo)) = sel.split_once(':') {
                (base.trim(), pseudo.trim() == "hover")
            } else { (sel, false) };
            let selector = if let Some(rest) = name_raw.strip_prefix('.') {
                SimpleSelector { kind: SimpleSelectorKind::Class, name: rest.trim().to_string(), hover }
            } else {
                SimpleSelector { kind: SimpleSelectorKind::Tag, name: name_raw.to_string(), hover }
            };
            let mut decls = HashMap::new();
            for decl in body.split(';') {
                let decl = decl.trim();
                if decl.is_empty() { continue; }
                if let Some((k,v)) = decl.split_once(':') {
                    decls.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
            if !decls.is_empty() {
                rules.push(Rule { selector, decls });
            }
        }
        Stylesheet { rules }
    }
}

fn matches_selector(sel: &SimpleSelector, tag: &str, class_attr: Option<&str>, hovered: bool) -> bool {
    if sel.hover && !hovered { return false; }
    match sel.kind {
        SimpleSelectorKind::Tag => sel.name == tag,
        SimpleSelectorKind::Class => {
            if let Some(classes) = class_attr {
                classes.split_whitespace().any(|x| x == sel.name)
            } else { false }
        }
    }
}

fn merge_styles(existing: Option<&str>, new_map: &HashMap<String, String>) -> String {
    // Convert existing inline style to map
    let mut map: HashMap<String,String> = HashMap::new();
    if let Some(s) = existing {
        for decl in s.split(';') {
            let decl = decl.trim();
            if decl.is_empty() { continue; }
            if let Some((k,v)) = decl.split_once(':') {
                map.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    // Override/add new
    for (k,v) in new_map {
        map.insert(k.clone(), v.clone());
    }
    // Serialize deterministically by key
    let mut keys: Vec<_> = map.keys().cloned().collect();
    keys.sort();
    let mut out = String::new();
    for (i,k) in keys.iter().enumerate() {
        if i>0 { out.push_str(" "); }
        out.push_str(k);
        out.push_str(": ");
        out.push_str(map.get(k).unwrap());
        out.push_str(";");
    }
    out
}

/// Apply stylesheet to a VNode recursively, returning a new VNode
/// with inline `style` attributes populated.
pub fn apply_styles(node: &VNode, sheet: &Stylesheet) -> VNode {
    apply_styles_with_hover(node, sheet, &|_, _| false)
}

/// Apply stylesheet with a custom hover predicate that decides if a node is hovered.
/// The predicate receives (tag, props) and returns true if the node is hovered.
pub fn apply_styles_with_hover<F>(node: &VNode, sheet: &Stylesheet, is_hovered: &F) -> VNode
where
    F: Fn(&str, &Props) -> bool,
{
    // Cascade and inheritance for a subset of text properties
    fn filter_inheritable(style: Option<&str>) -> HashMap<String, String> {
        let mut map = HashMap::new();
        if let Some(s) = style {
            for decl in s.split(';') {
                let d = decl.trim();
                if d.is_empty() { continue; }
                if let Some((k, v)) = d.split_once(':') {
                    let k = k.trim();
                    let v = v.trim();
                    match k {
                        "color" | "font-size" | "font-weight" | "text-decoration" | "line-height" => {
                            map.insert(k.to_string(), v.to_string());
                        }
                        _ => {}
                    }
                }
            }
        }
        map
    }

    fn apply_rec<FN>(node: &VNode, sheet: &Stylesheet, is_hovered: &FN, inherited: &HashMap<String, String>) -> VNode
    where FN: Fn(&str, &Props) -> bool {
        match node {
            VNode::Text(_) => node.clone(),
            VNode::Element { tag, props, children } => {
                let class_attr = props.attrs.get("class").map(|s| s.as_str());
                let hovered = is_hovered(tag, props);
                let mut acc: HashMap<String,String> = inherited.clone();
                // Apply rules in two passes: tag then class (class overrides tag)
                for pass in ["tag", "class"] {
                    for rule in &sheet.rules {
                        let is_tag = matches!(rule.selector.kind, SimpleSelectorKind::Tag);
                        let pass_tag = (pass == "tag" && is_tag) || (pass == "class" && !is_tag);
                        if !pass_tag { continue; }
                        if matches_selector(&rule.selector, tag, class_attr, hovered) {
                            for (k, v) in &rule.decls {
                                acc.insert(k.clone(), v.clone());
                            }
                        }
                    }
                }
                // Inline style has highest precedence
                let mut new_props = props.clone();
                let merged = merge_styles(new_props.attrs.get("style").map(|s| s.as_str()), &acc);
                if !merged.is_empty() { new_props = new_props.set("style", merged.clone()); }
                // Inherit only inheritable props to children
                let inherit_next = filter_inheritable(Some(&merged));
                let new_children = children.iter().map(|c| apply_rec(c, sheet, is_hovered, &inherit_next)).collect();
                VNode::Element { tag: tag.clone(), props: new_props, children: new_children }
            }
        }
    }

    let inherited_root: HashMap<String,String> = HashMap::new();
    apply_rec(node, sheet, is_hovered, &inherited_root)
}
