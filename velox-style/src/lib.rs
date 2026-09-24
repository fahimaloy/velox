//! Velox Style - CSS Styling System
//!
//! This crate provides comprehensive CSS support for Velox:
//! - CSS property parsing and computed styles
//! - Unit handling (px, %, rem, em, vw, vh)
//! - Color parsing (hex, rgb, rgba, named colors)
//! - Flexbox layout properties
//! - Style inheritance and cascading

pub mod fonts;
pub mod ua;
pub mod visual_effects;

// Re-export types from velox-dom
pub use velox_dom::style::*;
// Re-export non-conflicting font types.
pub use fonts::{FontDescriptor, FontFamily, FontMetrics, FontStyle, GenericFamily, LineHeight};
// Avoid collision with velox_dom::style::BoxShadow by aliasing visual effects type.
pub use visual_effects::{BorderRadius, BoxShadow as VisualBoxShadow, TextShadow};

use cssparser::ToCss;
use std::collections::HashMap;
use velox_dom::{Props, VNode};

// --- CSS Parser types (module-level for rust-analyzer compatibility) ---

/// A single part of a CSS selector (e.g., `h1`, `.class`, `h1.class`, `[attr]`).
#[derive(Debug, Clone, PartialEq)]
pub struct SelectorPart {
    pub tag: String,
    pub class: String,
    pub hover: bool,
    pub attr_name: String,
    pub attr_value: Option<String>,
}

impl SelectorPart {
    fn matches_element(&self, tag: &str, props: &Props, hovered: bool) -> bool {
        if self.hover && !hovered {
            return false;
        }
        let tag_ok = self.tag.is_empty() || self.tag == "*" || self.tag == tag;
        let class_ok = if self.class.is_empty() {
            true
        } else if let Some(classes) = props.attrs.get("class") {
            classes.split_whitespace().any(|x| x == self.class)
        } else {
            false
        };
        let attr_ok = if self.attr_name.is_empty() {
            true
        } else if let Some(val) = props.attrs.get(&self.attr_name) {
            if let Some(expected) = &self.attr_value {
                val == expected
            } else {
                true
            }
        } else {
            false
        };
        tag_ok && class_ok && attr_ok
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
        // Use slice_from to preserve attribute selectors and complex selectors exactly.
        // `to_css` for SquareBracketBlock loses inner content (emits ".btn[ ").
        let start = input.position();
        while let Ok(token) = input.next_including_whitespace() {
            let _ = token;
        }
        Ok(input.slice_from(start).trim().to_string())
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
    type Prelude = String;
    type AtRule = ();
    type Error = ();

    fn parse_prelude<'t>(
        &mut self,
        name: cssparser::CowRcStr<'i>,
        input: &mut cssparser::Parser<'i, 't>,
    ) -> Result<String, cssparser::ParseError<'i, ()>> {
        let mut prelude = String::new();
        while let Ok(token) = input.next_including_whitespace() {
            let _ = token.to_css(&mut prelude);
        }
        Ok(format!("@{} {}", name, prelude.trim()))
    }

    fn parse_block<'t>(
        &mut self,
        prelude: String,
        _start: &cssparser::ParserState,
        input: &mut cssparser::Parser<'i, 't>,
    ) -> Result<(), cssparser::ParseError<'i, ()>> {
        // Handle @media / @supports by parsing nested rules recursively.
        // @keyframes and other at-rules are ignored (inner selectors are not style rules).
        let prelude_trim = prelude.trim().to_lowercase();
        if prelude_trim.starts_with("@keyframes")
            || prelude_trim.starts_with("@-webkit-keyframes")
            || prelude_trim.starts_with("@-moz-keyframes")
            || prelude_trim.starts_with("@font-face")
        {
            return Ok(());
        }
        // For @media / @supports / other, parse inner qualified rules as flattened rules.
        {
            use cssparser::RuleListParser;
            let mut tmp = SheetParser { rules: Vec::new() };
            {
                let mut rule_list = RuleListParser::new_for_nested_rule(input, &mut tmp);
                for rule in &mut rule_list {
                    let _ = rule;
                }
            }
            self.rules.extend(tmp.rules);
        }
        Ok(())
    }

    fn rule_without_block(
        &mut self,
        prelude: String,
        _start: &cssparser::ParserState,
    ) -> Result<(), ()> {
        let _ = prelude;
        Err(())
    }
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

/// Parse a single selector part (tag, .class, tag.class, [attr], [attr="val"], etc.) with optional :hover.
/// Attribute selectors: `[attr]`, `[attr="value"]`, `[attr='value']` appended to tag/class (e.g. `.btn[data-v-abc]`).
fn parse_selector_part(raw: &str) -> Option<SelectorPart> {
    // Robustly extract attribute selector: find '[' and matching ']' (first ']' after '[')
    // Allows trailing pseudo like `.btn[data-v-x]:hover`.
    let (base_raw, attr_raw): (String, Option<String>) = if let Some(lb) = raw.find('[') {
        if let Some(rb_rel) = raw[lb..].find(']') {
            let rb = lb + rb_rel;
            let attr_inner = raw[lb + 1..rb].to_string();
            let before = &raw[..lb];
            let after = &raw[rb + 1..];
            // base without attr is before + after (after may contain :hover)
            let base = format!("{}{}", before, after);
            (base, Some(attr_inner))
        } else {
            return None;
        }
    } else {
        (raw.to_string(), None)
    };

    let (attr_name, attr_value) = if let Some(inner) = attr_raw {
        let inner = inner.trim().to_string();
        if inner.is_empty() {
            return None;
        }
        if let Some(eq) = inner.find('=') {
            let name = inner[..eq].trim().to_string();
            if name.is_empty() {
                return None;
            }
            let mut val = inner[eq + 1..].trim().to_string();
            // Strip surrounding quotes
            if (val.starts_with('"') && val.ends_with('"') && val.len() >= 2)
                || (val.starts_with('\'') && val.ends_with('\'') && val.len() >= 2)
            {
                val = val[1..val.len() - 1].to_string();
            }
            (name, Some(val))
        } else {
            (inner, None)
        }
    } else {
        (String::new(), None)
    };

    // Handle base being empty (pure attribute selector like [data-v-x])
    let base_trimmed = base_raw.trim();
    if base_trimmed.is_empty() {
        if attr_name.is_empty() {
            return None;
        }
        // Pure attribute selector may have hover pseudo in after part already removed? but if base empty, check hover not applicable
        // However `[data-v-x]:hover` would have base ":hover" not empty - handled below
        if base_trimmed.is_empty() {
            return Some(SelectorPart {
                tag: String::new(),
                class: String::new(),
                hover: false,
                attr_name,
                attr_value,
            });
        }
    }

    let (name_raw, hover) = if let Some((base, pseudo)) = base_trimmed.split_once(':') {
        (base.trim().to_string(), pseudo.trim() == "hover")
    } else {
        (base_trimmed.to_string(), false)
    };
    // Allow `*` universal selector: treat like empty tag (matches any tag)
    if name_raw == "*" {
        return Some(SelectorPart {
            tag: "*".to_string(),
            class: String::new(),
            hover,
            attr_name,
            attr_value,
        });
    }
    if name_raw.is_empty() {
        if attr_name.is_empty() {
            return None;
        }
        // e.g. `[data-v-x]:hover` => name_raw is "" but attr present, hover true
        return Some(SelectorPart {
            tag: String::new(),
            class: String::new(),
            hover,
            attr_name,
            attr_value,
        });
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
            attr_name,
            attr_value,
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
            attr_name,
            attr_value,
        })
    } else {
        Some(SelectorPart {
            tag: name_raw,
            class: String::new(),
            hover,
            attr_name,
            attr_value,
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
    props: &Props,
    hovered: bool,
    ancestors: &[&VNode],
) -> bool {
    if sel.parts.is_empty() {
        return false;
    }
    let last = &sel.parts[sel.parts.len() - 1];
    if !last.matches_element(tag, props, hovered) {
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
                && part.matches_element(a_tag, a_props, false)
            {
                found = true;
                break;
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

/// Apply the 3-layer cascade UA < author < inline.
///
/// Composes the user-agent sheet (`ua::ua_sheet()`) under `author`, then
/// delegates to `apply_styles_with_hover`. Inline styles win via `merge_styles`.
pub fn apply_with_cascade(node: &VNode, author: &Stylesheet) -> VNode {
    apply_with_cascade_with_hover(node, author, &|_, _| false)
}

/// Cascade with a custom hover predicate.
pub fn apply_with_cascade_with_hover<F>(node: &VNode, author: &Stylesheet, is_hovered: &F) -> VNode
where
    F: Fn(&str, &Props) -> bool,
{
    let ua = crate::ua::ua_sheet();
    let mut merged = ua.rules.clone();
    merged.extend(author.rules.clone());
    let cascade = Stylesheet { rules: merged };
    apply_styles_with_hover(node, &cascade, is_hovered)
}

/// Apply stylesheet with a custom hover predicate
pub fn apply_styles_with_hover<F>(node: &VNode, sheet: &Stylesheet, is_hovered: &F) -> VNode
where
    F: Fn(&str, &Props) -> bool,
{
    #[allow(dead_code)]
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
                let hovered = is_hovered(tag, props);
                let mut acc: HashMap<String, String> = inherited.clone();
                // Match all selectors against this element, passing the ancestor chain
                // so compound selectors (e.g., `.header h1`) can walk up the tree.
                for rule in &sheet.rules {
                    if matches_selector(&rule.selector, tag, props, hovered, ancestors) {
                        for (k, v) in &rule.decls {
                            acc.insert(k.clone(), v.clone());
                        }
                    }
                }
                let mut new_props = props.clone();
                let merged = merge_styles(new_props.attrs.get("style").map(|s| s.as_str()), &acc);
                let final_style = merged.clone();
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
        // Apply matching rules from stylesheet
        for rule in &sheet.rules {
            if matches_selector(&rule.selector, tag, props, is_hovered, ancestors) {
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
