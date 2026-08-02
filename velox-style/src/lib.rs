//! Velox Style - CSS Styling System
//!
//! This crate provides comprehensive CSS support for Velox:
//! - CSS property parsing and computed styles
//! - Unit handling (px, %, rem, em, vw, vh)
//! - Color parsing (hex, rgb, rgba, named colors)
//! - Flexbox layout properties
//! - Style inheritance and cascading

pub mod fonts;
pub mod properties;
pub mod units;
pub mod visual_effects;

// Re-export types from velox-dom
pub use velox_dom::style::*;
// Re-export non-conflicting font types.
pub use fonts::{FontDescriptor, FontFamily, FontMetrics, FontStyle, GenericFamily, LineHeight};
// Avoid collision with properties::BoxShadow by aliasing visual effects type.
pub use visual_effects::{BorderRadius, BoxShadow as VisualBoxShadow, TextShadow};

use cssparser::ToCss;
use std::collections::HashMap;
use velox_dom::{Props, VNode};

// --- CSS Parser types (module-level for rust-analyzer compatibility) ---

/// A single part of a CSS selector (e.g., `h1`, `.class`, `h1.class`).
#[derive(Debug, Clone, PartialEq)]
pub struct SelectorPart {
    pub tag: String,
    pub class: String,
    pub hover: bool,
}

impl SelectorPart {
    fn matches_element(&self, tag: &str, class_attr: Option<&str>, hovered: bool) -> bool {
        if self.hover && !hovered {
            return false;
        }
        let tag_ok = self.tag.is_empty() || self.tag == tag;
        let class_ok = if self.class.is_empty() {
            true
        } else if let Some(classes) = class_attr {
            classes.split_whitespace().any(|x| x == self.class)
        } else {
            false
        };
        tag_ok && class_ok
    }

    #[allow(dead_code)]
    fn is_simple_tag(&self) -> bool {
        !self.tag.is_empty() && self.class.is_empty()
    }

    #[allow(dead_code)]
    fn is_class_only(&self) -> bool {
        self.tag.is_empty() && !self.class.is_empty()
    }
}

/// A compound CSS selector — a chain of `SelectorPart`s connected by
/// descendant combinators (spaces). The rightmost part matches the
/// target element; preceding parts must match ancestors.
///
/// Example: `.header h1` → `[SelectorPart(class="header"), SelectorPart(tag="h1")]`
#[derive(Debug, Clone, PartialEq)]
pub struct CompoundSelector {
    pub parts: Vec<SelectorPart>,
}

impl CompoundSelector {
    fn new(parts: Vec<SelectorPart>) -> Self {
        Self { parts }
    }

    /// True when the selector is a simple tag-only selector (e.g., `button`).
    #[allow(dead_code)]
    fn is_simple_tag(&self) -> bool {
        self.parts.len() == 1 && self.parts[0].is_simple_tag()
    }

    /// True when the selector is a simple class-only selector (e.g., `.app`).
    #[allow(dead_code)]
    fn is_class_only(&self) -> bool {
        self.parts.len() == 1 && self.parts[0].is_class_only()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub selector: CompoundSelector,
    pub decls: HashMap<String, String>,
}

struct SheetParser {
    rules: Vec<Rule>,
}

impl<'i> cssparser::QualifiedRuleParser<'i> for &mut SheetParser {
    type Prelude = String;
    type QualifiedRule = ();
    type Error = ();

    fn parse_prelude<'t>(
        &mut self,
        input: &mut cssparser::Parser<'i, 't>,
    ) -> Result<String, cssparser::ParseError<'i, ()>> {
        let mut selector = String::new();
        while let Ok(token) = input.next_including_whitespace() {
            let _ = token.to_css(&mut selector);
        }
        Ok(selector.trim().to_string())
    }

    fn parse_block<'t>(
        &mut self,
        prelude: String,
        _start: &cssparser::ParserState,
        input: &mut cssparser::Parser<'i, 't>,
    ) -> Result<(), cssparser::ParseError<'i, ()>> {
        let mut decls = HashMap::new();
        for (name, value) in
            cssparser::DeclarationListParser::new(input, DeclarationParser).flatten()
        {
            if !name.is_empty() {
                decls.insert(name, value);
            }
        }
        if decls.is_empty() {
            return Ok(());
        }
        for selector in parse_selector_list(&prelude) {
            self.rules.push(Rule {
                selector,
                decls: decls.clone(),
            });
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
        input: &mut cssparser::Parser<'i, 't>,
    ) -> Result<(String, String), cssparser::ParseError<'i, ()>> {
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

/// Parse a single selector part (tag, .class, or tag.class) with optional :hover.
fn parse_selector_part(raw: &str) -> Option<SelectorPart> {
    let (name_raw, hover) = if let Some((base, pseudo)) = raw.split_once(':') {
        (base.trim(), pseudo.trim() == "hover")
    } else {
        (raw, false)
    };
    if name_raw.is_empty() {
        return None;
    }
    if let Some(rest) = name_raw.strip_prefix('.') {
        let class = rest.trim();
        if class.is_empty() {
            return None;
        }
        Some(SelectorPart {
            tag: String::new(),
            class: class.to_string(),
            hover,
        })
    } else if let Some((tag, class)) = name_raw.split_once('.') {
        let tag = tag.trim();
        let class = class.trim();
        if tag.is_empty() || class.is_empty() {
            return None;
        }
        Some(SelectorPart {
            tag: tag.to_string(),
            class: class.to_string(),
            hover,
        })
    } else {
        Some(SelectorPart {
            tag: name_raw.to_string(),
            class: String::new(),
            hover,
        })
    }
}

fn parse_selector_list(selector: &str) -> Vec<CompoundSelector> {
    let mut out = Vec::new();
    for part in selector.split(',') {
        let raw = part.trim();
        if raw.is_empty() {
            continue;
        }
        // Split by whitespace to get compound selector parts (descendant combinators).
        // Each whitespace-separated token is one part of the chain.
        let mut parts = Vec::new();
        let mut valid = true;
        for token in raw.split_whitespace() {
            if token == ">" {
                // Child combinator — for now treat as descendant (full > support later).
                continue;
            }
            match parse_selector_part(token) {
                Some(sp) => parts.push(sp),
                None => {
                    valid = false;
                    break;
                }
            }
        }
        if valid && !parts.is_empty() {
            out.push(CompoundSelector::new(parts));
        }
    }
    out
}

// --- Stylesheet ---

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

impl Stylesheet {
    pub fn parse(css: &str) -> Self {
        use cssparser::{Parser, ParserInput, RuleListParser};

        let mut input = ParserInput::new(css);
        let mut parser = Parser::new(&mut input);
        let mut sheet_parser = SheetParser { rules: Vec::new() };
        let mut rule_list = RuleListParser::new_for_stylesheet(&mut parser, &mut sheet_parser);
        for rule in &mut rule_list {
            let _ = rule;
        }

        Stylesheet {
            rules: sheet_parser.rules,
        }
    }
}

/// Match a compound selector against a VNode element, walking ancestors as needed.
///
/// For a single-part selector (e.g., `.app`, `button`), only the target element is checked.
/// For a multi-part selector (e.g., `.header h1`), the rightmost part matches the target
/// and preceding parts must match ancestor VNodes (walking up the provided `ancestors` slice).
fn matches_selector(
    sel: &CompoundSelector,
    tag: &str,
    class_attr: Option<&str>,
    hovered: bool,
    ancestors: &[&VNode],
) -> bool {
    if sel.parts.is_empty() {
        return false;
    }
    let last = &sel.parts[sel.parts.len() - 1];
    if !last.matches_element(tag, class_attr, hovered) {
        return false;
    }
    // Single-part selector: no ancestor check needed.
    if sel.parts.len() == 1 {
        return true;
    }
    // Multi-part (compound/descendant) selector:
    // Walk ancestors right-to-left matching earlier parts of the chain.
    let mut ancestor_idx = ancestors.len(); // start from nearest ancestor
    for part_idx in (0..sel.parts.len() - 1).rev() {
        let part = &sel.parts[part_idx];
        let mut found = false;
        while ancestor_idx > 0 {
            ancestor_idx -= 1;
            if let VNode::Element {
                tag: a_tag,
                props: a_props,
                ..
            } = ancestors[ancestor_idx]
            {
                let a_class = a_props.attrs.get("class").map(|s| s.as_str());
                if part.matches_element(a_tag, a_class, false) {
                    found = true;
                    break;
                }
            }
        }
        if !found {
            return false;
        }
    }
    true
}

fn merge_styles(existing: Option<&str>, new_map: &HashMap<String, String>) -> String {
    let mut map: HashMap<String, String> = HashMap::new();
    // Apply stylesheet styles first (lower precedence)
    for (k, v) in new_map {
        map.insert(k.clone(), v.clone());
    }
    // Apply inline styles second (highest precedence - they override stylesheet)
    if let Some(s) = existing {
        for decl in s.split(';') {
            let decl = decl.trim();
            if decl.is_empty() {
                continue;
            }
            if let Some((k, v)) = decl.split_once(':') {
                map.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    let mut keys: Vec<_> = map.keys().cloned().collect();
    keys.sort();
    let mut out = String::new();
    for (i, k) in keys.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(k);
        out.push_str(": ");
        out.push_str(&map[k]);
        out.push(';');
    }
    out
}

/// Apply stylesheet to a VNode recursively, returning a new VNode
/// with inline `style` attributes populated.
pub fn apply_styles(node: &VNode, sheet: &Stylesheet) -> VNode {
    apply_styles_with_hover(node, sheet, &|_, _| false)
}

/// Apply stylesheet with a custom hover predicate
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
            if let Some((k, _)) = d.split_once(':')
                && k.trim() == key
            {
                return true;
            }
        }
        false
    }

    fn filter_inheritable(style: Option<&str>) -> HashMap<String, String> {
        let mut map = HashMap::new();
        if let Some(s) = style {
            for decl in s.split(';') {
                let d = decl.trim();
                if d.is_empty() {
                    continue;
                }
                if let Some((k, v)) = d.split_once(':') {
                    let k = k.trim();
                    let v = v.trim();
                    match k {
                        "color" | "font-size" | "font-weight" | "text-decoration"
                        | "line-height" => {
                            map.insert(k.to_string(), v.to_string());
                        }
                        _ => {}
                    }
                }
            }
        }
        map
    }

    fn apply_rec<FN>(
        node: &VNode,
        sheet: &Stylesheet,
        is_hovered: &FN,
        inherited: &HashMap<String, String>,
        ancestors: &[&VNode],
    ) -> VNode
    where
        FN: Fn(&str, &Props) -> bool,
    {
        match node {
            VNode::Text(_) => node.clone(),
            VNode::Element {
                tag,
                props,
                children,
            } => {
                let class_attr = props.attrs.get("class").map(|s| s.as_str());
                let hovered = is_hovered(tag, props);
                let mut acc: HashMap<String, String> = inherited.clone();
                // Match all selectors against this element, passing the ancestor chain
                // so compound selectors (e.g., `.header h1`) can walk up the tree.
                for rule in &sheet.rules {
                    if matches_selector(&rule.selector, tag, class_attr, hovered, ancestors) {
                        for (k, v) in &rule.decls {
                            acc.insert(k.clone(), v.clone());
                        }
                    }
                }
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
                if !final_style.is_empty() {
                    new_props = new_props.set("style", final_style.clone());
                }
                let inherit_next = filter_inheritable(Some(&final_style));
                // Build child ancestors: this element + current ancestors
                let mut child_ancestors: Vec<&VNode> = Vec::with_capacity(ancestors.len() + 1);
                child_ancestors.push(node);
                child_ancestors.extend_from_slice(ancestors);
                let new_children = children
                    .iter()
                    .map(|c| apply_rec(c, sheet, is_hovered, &inherit_next, &child_ancestors))
                    .collect();
                VNode::Element {
                    tag: tag.clone(),
                    props: new_props,
                    children: new_children,
                }
            }
        }
    }

    let inherited_root: HashMap<String, String> = HashMap::new();
    apply_rec(node, sheet, is_hovered, &inherited_root, &[])
}

/// Compute styles for a VNode given inline styles and optional stylesheet
/// Returns a ComputedStyle with all properties resolved.
/// `ancestors` provides the VNode ancestor chain for compound selector matching.
pub fn compute_styles_for_node(
    node: &VNode,
    inline_style: Option<&str>,
    sheet: Option<&Stylesheet>,
    is_hovered: bool,
    ancestors: &[&VNode],
) -> ComputedStyle {
    let mut computed = ComputedStyle::new();

    // Apply stylesheet styles first (lower precedence)
    if let Some(sheet) = sheet
        && let VNode::Element { tag, props, .. } = node
    {
        let class_attr = props.attrs.get("class").map(|s| s.as_str());

        // Apply matching rules from stylesheet
        for rule in &sheet.rules {
            if matches_selector(&rule.selector, tag, class_attr, is_hovered, ancestors) {
                for (prop, value) in &rule.decls {
                    computed.set_property(prop, value);
                }
            }
        }
    }

    // Apply inline styles last (highest precedence - they override stylesheet)
    if let Some(style) = inline_style {
        computed.apply_inline_style(style);
    }

    computed
}
