use std::collections::HashMap;

use cssparser::{Parser, ParserInput, RuleListParser, ToCss};
use velox_dom::{VNode, Props};

#[derive(Debug, Clone, PartialEq)]
pub enum SimpleSelectorKind { Tag, Class, TagClass }

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleSelector {
    pub kind: SimpleSelectorKind,
    pub tag: String,
    pub class: String,
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
        struct SheetParser {
            rules: Vec<Rule>,
        }

        impl<'i> cssparser::QualifiedRuleParser<'i> for &mut SheetParser {
            type Prelude = String;
            type QualifiedRule = ();
            type Error = ();

            fn parse_prelude<'t>(
                &mut self,
                input: &mut Parser<'i, 't>,
            ) -> Result<Self::Prelude, cssparser::ParseError<'i, Self::Error>> {
                let mut selector = String::new();
                while let Ok(token) = input.next_including_whitespace() {
                    let _ = token.to_css(&mut selector);
                }
                Ok(selector.trim().to_string())
            }

            fn parse_block<'t>(
                &mut self,
                prelude: Self::Prelude,
                _start: &cssparser::ParserState,
                input: &mut Parser<'i, 't>,
            ) -> Result<Self::QualifiedRule, cssparser::ParseError<'i, Self::Error>> {
                let mut decls = HashMap::new();
                for decl in cssparser::DeclarationListParser::new(input, DeclarationParser) {
                    if let Ok((name, value)) = decl {
                        if !name.is_empty() {
                            decls.insert(name, value);
                        }
                    }
                }
                if decls.is_empty() {
                    return Ok(());
                }
                for selector in parse_selector_list(&prelude) {
                    self.rules.push(Rule { selector, decls: decls.clone() });
                }
                Ok(())
            }
        }

        impl<'i> cssparser::AtRuleParser<'i> for &mut SheetParser {
            type Prelude = ();
            type AtRule = ();
            type Error = ();
        }

        struct DeclarationParser;
        impl<'i> cssparser::DeclarationParser<'i> for DeclarationParser {
            type Declaration = (String, String);
            type Error = ();

            fn parse_value<'t>(
                &mut self,
                name: cssparser::CowRcStr<'i>,
                input: &mut Parser<'i, 't>,
            ) -> Result<Self::Declaration, cssparser::ParseError<'i, Self::Error>> {
                let mut value = String::new();
                while let Ok(token) = input.next_including_whitespace() {
                    let _ = token.to_css(&mut value);
                }
                Ok((name.to_string(), value.trim().to_string()))
            }
        }

        impl<'i> cssparser::AtRuleParser<'i> for DeclarationParser {
            type Prelude = ();
            type AtRule = (String, String);
            type Error = ();
        }

        fn parse_selector_list(selector: &str) -> Vec<SimpleSelector> {
            let mut out = Vec::new();
            for part in selector.split(',') {
                let raw = part.trim();
                if raw.is_empty() {
                    continue;
                }
                let (name_raw, hover) = if let Some((base, pseudo)) = raw.split_once(':') {
                    (base.trim(), pseudo.trim() == "hover")
                } else {
                    (raw, false)
                };
                if let Some(rest) = name_raw.strip_prefix('.') {
                    let name = rest.trim();
                    if !name.is_empty() {
                        out.push(SimpleSelector {
                            kind: SimpleSelectorKind::Class,
                            tag: String::new(),
                            class: name.to_string(),
                            hover,
                        });
                    }
                } else if let Some((tag, class)) = name_raw.split_once('.') {
                    let tag = tag.trim();
                    let class = class.trim();
                    if !tag.is_empty() && !class.is_empty() {
                        out.push(SimpleSelector {
                            kind: SimpleSelectorKind::TagClass,
                            tag: tag.to_string(),
                            class: class.to_string(),
                            hover,
                        });
                    }
                } else if !name_raw.is_empty() {
                    out.push(SimpleSelector {
                        kind: SimpleSelectorKind::Tag,
                        tag: name_raw.to_string(),
                        class: String::new(),
                        hover,
                    });
                }
            }
            out
        }

        let mut input = ParserInput::new(css);
        let mut parser = Parser::new(&mut input);
        let mut sheet_parser = SheetParser { rules: Vec::new() };
        let mut rule_list = RuleListParser::new_for_stylesheet(&mut parser, &mut sheet_parser);
        for rule in &mut rule_list {
            let _ = rule;
        }

        Stylesheet { rules: sheet_parser.rules }
    }
}

fn matches_selector(sel: &SimpleSelector, tag: &str, class_attr: Option<&str>, hovered: bool) -> bool {
    if sel.hover && !hovered { return false; }
    match sel.kind {
        SimpleSelectorKind::Tag => sel.tag == tag,
        SimpleSelectorKind::Class => {
            if let Some(classes) = class_attr {
                classes.split_whitespace().any(|x| x == sel.class)
            } else { false }
        }
        SimpleSelectorKind::TagClass => {
            if sel.tag != tag {
                return false;
            }
            if let Some(classes) = class_attr {
                classes.split_whitespace().any(|x| x == sel.class)
            } else {
                false
            }
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
    fn has_style_key(style: &str, key: &str) -> bool {
        for decl in style.split(';') {
            let d = decl.trim();
            if d.is_empty() {
                continue;
            }
            if let Some((k, _)) = d.split_once(':') {
                if k.trim() == key {
                    return true;
                }
            }
        }
        false
    }

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
                let mut final_style = merged.clone();
                if tag == "button" {
                    let has_padding = has_style_key(&final_style, "padding")
                        || has_style_key(&final_style, "padding-left")
                        || has_style_key(&final_style, "padding-right")
                        || has_style_key(&final_style, "padding-top")
                        || has_style_key(&final_style, "padding-bottom");
                    if !has_padding {
                        final_style.push_str(" padding: 6px 12px;");
                    }
                    if !has_style_key(&final_style, "text-align") {
                        final_style.push_str(" text-align: center;");
                    }
                }
                if !final_style.is_empty() { new_props = new_props.set("style", final_style.clone()); }
                // Inherit only inheritable props to children
                let inherit_next = filter_inheritable(Some(&final_style));
                let new_children = children.iter().map(|c| apply_rec(c, sheet, is_hovered, &inherit_next)).collect();
                VNode::Element { tag: tag.clone(), props: new_props, children: new_children }
            }
        }
    }

    let inherited_root: HashMap<String,String> = HashMap::new();
    apply_rec(node, sheet, is_hovered, &inherited_root)
}
