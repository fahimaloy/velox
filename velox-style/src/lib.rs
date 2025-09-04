use std::collections::HashMap;

use velox_dom::{VNode, Props};

#[derive(Debug, Clone, PartialEq)]
pub enum SimpleSelector {
    Tag(String),
    Class(String),
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
            let selector = if let Some(rest) = sel.strip_prefix('.') {
                SimpleSelector::Class(rest.trim().to_string())
            } else {
                SimpleSelector::Tag(sel.to_string())
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

fn matches_selector(sel: &SimpleSelector, tag: &str, class_attr: Option<&str>) -> bool {
    match sel {
        SimpleSelector::Tag(t) => t == tag,
        SimpleSelector::Class(c) => {
            if let Some(classes) = class_attr {
                classes.split_whitespace().any(|x| x == c)
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
    match node {
        VNode::Text(_) => node.clone(),
        VNode::Element { tag, props, children } => {
            // Collect matching rules and merge
            let class_attr = props.attrs.get("class").map(|s| s.as_str());
            let mut acc: HashMap<String,String> = HashMap::new();
            // Class rules override tag rules when same prop appears later here.
            // We apply in two passes: tag then class, preserving source order within each.
            for pass in ["tag", "class"] {
                for rule in &sheet.rules {
                    let is_tag = matches!(rule.selector, SimpleSelector::Tag(_));
                    let pass_tag = (pass == "tag" && is_tag) || (pass == "class" && !is_tag);
                    if !pass_tag { continue; }
                    if matches_selector(&rule.selector, tag, class_attr) {
                        for (k,v) in &rule.decls {
                            acc.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
            let mut new_props = props.clone();
            if !acc.is_empty() {
                let merged = merge_styles(new_props.attrs.get("style").map(|s| s.as_str()), &acc);
                new_props = new_props.set("style", merged);
            }
            let new_children = children.iter().map(|c| apply_styles(c, sheet)).collect();
            VNode::Element { tag: tag.clone(), props: new_props, children: new_children }
        }
    }
}
