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
// Re-export the font types. (`FontMetrics` used to be re-exported here and collided
// with `velox_dom::style`; it had no users, so it is deleted rather than aliased.)
pub use fonts::{FontDescriptor, FontFamily, FontStyle, GenericFamily, LineHeight};
// Avoid collision with velox_dom::style::BoxShadow by aliasing visual effects type.
pub use visual_effects::{BorderRadius, BoxShadow as VisualBoxShadow, TextShadow};

use cssparser::ToCss;
use std::collections::HashMap;
use velox_dom::{Props, VNode};

// --- CSS Parser types (module-level for rust-analyzer compatibility) ---

/// How a selector part is connected to the part on its left in the source
/// selector (e.g. the `>` between `div` and `.card` in `div > .card`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Combinator {
    /// No combinator precedes this part — it is the leftmost part of the chain.
    #[default]
    None,
    /// Descendant combinator (whitespace): this part may match any ancestor of
    /// the part on its right.
    Descendant,
    /// Child combinator (`>`): this part must match the element exactly one
    /// level above the part on its right.
    Child,
}

/// Attribute the cascade writes `::placeholder` declarations onto.
///
/// A pseudo-element needs its own attribute because the element's own `style`
/// is the value-text style. `velox-renderer` reads this one only when the
/// field is empty, so the two can never be confused.
pub const PLACEHOLDER_STYLE_ATTR: &str = "style:placeholder";

/// Whether a `::placeholder` selector's target is a form control that is
/// currently SHOWING its placeholder.
///
/// Three conditions, all of them load-bearing:
/// * it is an `<input>` — `textarea::placeholder` is not supported, because the
///   painter has no placeholder pass for the multi-line control and claiming it
///   would be a false parity claim;
/// * it carries a non-empty `placeholder`;
/// * its `value` is empty or absent — a filled field shows the value, not the
///   placeholder, so a `::placeholder` rule must not win there.
fn shows_placeholder(tag: &str, props: &Props) -> bool {
    tag == "input"
        && props
            .attrs
            .get("placeholder")
            .is_some_and(|p| !p.trim().is_empty())
        && props.attrs.get("value").is_none_or(|v| v.is_empty())
}

/// A single part of a CSS selector (e.g., `h1`, `.class`, `h1.class`, `[attr]`).
#[derive(Debug, Clone, PartialEq)]
pub struct SelectorPart {
    pub tag: String,
    pub class: String,
    pub hover: bool,
    /// This part targets the `::placeholder` pseudo-element of a form control,
    /// i.e. the selector ended in `::placeholder`.
    ///
    /// Why a flag and not a general pseudo-element enum: `::placeholder` is
    /// the ONLY pseudo-element Velox has any notion of, because an `<input>`
    /// has no child node for a general pseudo-element to style — the
    /// placeholder is not a child, it is a string the painter draws when the
    /// value is empty. A general enum would advertise support that does not
    /// exist.
    pub placeholder: bool,
    pub attr_name: String,
    pub attr_value: Option<String>,
    /// Combinator that connected this part to the part on its left in the
    /// source selector. `Combinator::None` for the leftmost part.
    pub combinator: Combinator,
}

impl SelectorPart {
    fn matches_element(&self, tag: &str, props: &Props, hovered: bool) -> bool {
        if self.hover && !hovered {
            return false;
        }
        // `::placeholder` matches only where a placeholder is actually
        // VISIBLE. Before this flag existed the selector was silently
        // truncated to `input` (see `parse_selector_part`), so
        // `input::placeholder { color: red }` matched every input and painted
        // the VALUE red — the opposite of what the author wrote, and the more
        // damaging of the two failure modes because it looks intentional.
        if self.placeholder && !shows_placeholder(tag, props) {
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
}

/// A compound CSS selector — a chain of `SelectorPart`s connected by
/// combinators (whitespace for descendant, `>` for child). The rightmost part
/// matches the target element; preceding parts must match ancestors according
/// to the combinator stored on each part.
///
/// Examples: `.header h1` → `[SelectorPart(class="header", None), SelectorPart(tag="h1", Descendant)]`,
/// `div > .card` → `[SelectorPart(tag="div", None), SelectorPart(class="card", Child)]`
#[derive(Debug, Clone, PartialEq)]
pub struct CompoundSelector {
    pub parts: Vec<SelectorPart>,
}

impl CompoundSelector {
    fn new(parts: Vec<SelectorPart>) -> Self {
        Self { parts }
    }

    /// Whether any part of this selector names the `::placeholder`
    /// pseudo-element (e.g. `input::placeholder`, `.field::placeholder`).
    fn has_placeholder(&self) -> bool {
        self.parts.iter().any(|p| p.placeholder)
    }

    /// Whether this selector's `::placeholder` part is the selector's SUBJECT:
    /// its only `::placeholder` part, and the rightmost one.
    ///
    /// `::placeholder` is a pseudo-ELEMENT, and a pseudo-element is not a node.
    /// It is the string the painter draws inside an empty control, so it has no
    /// box of its own and therefore cannot be an ANCESTOR of anything. CSS lets
    /// it appear as the subject only, so `.wrap input::placeholder` is real CSS
    /// (the placeholder of an input inside `.wrap`) and stays accepted, while
    /// `.field::placeholder .row` is not: its subject is `.row` and the
    /// placeholder is a link in the chain, which no node can satisfy.
    ///
    /// The distinction matters because the cascade writes a placeholder rule's
    /// declarations to a DIFFERENT attribute from the element's own (see
    /// `PLACEHOLDER_STYLE_ATTR`). A rule whose placeholder part is not the
    /// subject matched an ordinary element and would land in that element's own
    /// `style`, repainting it — the opposite of what the author wrote, and
    /// silently so, because the rule looks like it was honoured.
    fn placeholder_is_subject(&self) -> bool {
        self.parts.last().is_some_and(|p| p.placeholder)
            && self.parts.iter().filter(|p| p.placeholder).count() == 1
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
        // Same nested-block handling as a declaration value: a media query's
        // condition list is parenthesised (`@media (min-width: 700px)`), so the
        // old bare-token loop truncated every at-rule prelude at its first `(`.
        // The only consumer of this string is the `starts_with` dispatch in
        // `parse_block`, whose `@keyframes`/`@font-face` prefixes are unaffected
        // by a more complete prelude.
        write_component_values(input, &mut prelude)?;
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

/// Append every component value remaining in `input` to `dest`.
///
/// This is the CSS "consume a component value list" loop, and the one thing a
/// declaration parser cannot do without: the CONTENTS of a function or block
/// live in a nested block that the outer token stream does not reach. Per
/// cssparser's own docs on `Parser::next` (cssparser-0.28.1/src/parser.rs:546)
/// the call after a `Function`/`ParenthesisBlock`/`SquareBracketBlock`/
/// `CurlyBracketBlock` token "will skip until after the matching
/// `CloseParenthesis`…" — so a plain `while let Ok(token) = input.next()`
/// loses everything between the brackets.
///
/// That is exactly the defect this function exists to remove. The old loop
/// ended on the `Function` token having written only its opening `(` (see
/// `ToCss for Token`, cssparser-0.28.1/src/serializer.rs:135-141, which writes
/// `name(` and NOT the closing delimiter), so every function value was
/// truncated: `background: rgba(20, 24, 27, 0.34)` became `background:
/// rgba(`, `width: calc(100% - 8px)` became `width: calc(`, and
/// `color: var(--brand)` became `color: var(`.
///
/// So for every block-opening token we emit the arguments ourselves via
/// `parse_nested_block`, then write the matching closing delimiter — once.
/// Whitespace tokens are passed through as-is, so author spacing survives
/// verbatim (`rgba(calc(1 + 2), 0, 0, 1)` round-trips unchanged).
///
/// A `parse_nested_block` error cannot discard what was already written: the
/// value stays best-effort rather than failing the whole declaration, which
/// is what this parser has always done for a value it cannot model.
fn write_component_values<'i, 't>(
    input: &mut cssparser::Parser<'i, 't>,
    dest: &mut String,
) -> Result<(), cssparser::ParseError<'i, ()>> {
    while let Ok(token) = input.next_including_whitespace() {
        let closing = match *token {
            cssparser::Token::Function(_) => Some(')'),
            cssparser::Token::ParenthesisBlock => Some(')'),
            cssparser::Token::SquareBracketBlock => Some(']'),
            cssparser::Token::CurlyBracketBlock => Some('}'),
            _ => None,
        };
        // The borrow of `token` must end here: `parse_nested_block` needs
        // `&mut input`, and the token is borrowed from it.
        let _ = token.to_css(dest);
        let Some(closing) = closing else {
            continue;
        };
        // `parse_nested_block` takes `parser.at_start_of` and `.expect()`s that
        // it is `Some` (cssparser-0.28.1/src/parser.rs:1023). It is, because
        // `next_including_whitespace` set it when it returned this opening
        // token (cssparser-0.28.1/src/parser.rs:619-621) and we have touched
        // nothing in between.
        let _ = input.parse_nested_block(|input| write_component_values(input, dest));
        dest.push(closing);
    }
    Ok(())
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
        write_component_values(input, &mut value)?;
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
/// Split a selector part's trailing pseudos off its name.
///
/// Returns `(name, hover, placeholder)`. Handles the single-colon form
/// (`:hover`) and the DOUBLE-colon pseudo-element form (`::placeholder`)
/// separately, which the previous `split_once(':')` could not: for
/// `input::placeholder` it produced `pseudo == ":placeholder"`, which matched
/// neither `"hover"` nor anything else, so the pseudo was dropped and the rule
/// silently became a plain `input` rule.
///
/// An unknown pseudo is NOT an error and does not reject the selector — the
/// previous behaviour for `:focus`, `::before` and friends was to ignore them,
/// and silently dropping an unknown pseudo is strictly better than discarding
/// the rule an author wrote around it. What changed is only that the one
/// pseudo-element Velox actually implements is now seen.
///
/// Returns the base name borrowed when there is no pseudo to strip, so the
/// common no-pseudo selector parses without an allocation here (the owner
/// materialises it with `into_owned` only if it keeps the part).
fn split_pseudos(base_trimmed: &str) -> (std::borrow::Cow<'_, str>, bool, bool) {
    use std::borrow::Cow;
    let Some(colon) = base_trimmed.find(':') else {
        return (Cow::Borrowed(base_trimmed), false, false);
    };
    let name = base_trimmed[..colon].trim().to_string();
    // Everything after the first colon, then peel one leading colon per
    // pseudo: `::placeholder` -> `:placeholder` -> `placeholder`.
    let mut rest = base_trimmed[colon + 1..].trim_start_matches(':');
    let mut hover = false;
    let mut placeholder = false;
    // Only two pseudos exist, so one pass over the `:`-separated tail is
    // enough; the loop only exists so `::placeholder:hover` (harmless) does
    // not lose either flag.
    while !rest.is_empty() {
        let (pseudo, tail) = match rest.split_once(':') {
            Some((p, t)) => (p, Some(t)),
            None => (rest, None),
        };
        match pseudo.trim().to_ascii_lowercase().as_str() {
            "hover" => hover = true,
            "placeholder" => placeholder = true,
            _ => {}
        }
        match tail {
            Some(t) => rest = t.trim_start_matches(':'),
            None => break,
        }
    }
    (Cow::Owned(name), hover, placeholder)
}

fn parse_selector_part(raw: &str) -> Option<SelectorPart> {
    // Robustly extract attribute selector: find '[' and matching ']' (first ']' after '[')
    // Allows trailing pseudo like `.btn[data-v-x]:hover`.
    let (base_raw, attr_raw): (String, Option<String>) = if let Some(lb) = raw.find('[') {
        {
            let rb_rel = raw[lb..].find(']')?;
            let rb = lb + rb_rel;
            let attr_inner = raw[lb + 1..rb].to_string();
            let before = &raw[..lb];
            let after = &raw[rb + 1..];
            // base without attr is before + after (after may contain :hover)
            let base = format!("{}{}", before, after);
            (base, Some(attr_inner))
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
                placeholder: false,
                attr_name,
                attr_value,
                combinator: Combinator::None,
            });
        }
    }

    let (name_raw, hover, placeholder) = split_pseudos(base_trimmed);
    // Allow `*` universal selector: treat like empty tag (matches any tag)
    if name_raw.as_ref() == "*" {
        return Some(SelectorPart {
            tag: "*".to_string(),
            class: String::new(),
            hover,
            placeholder,
            attr_name,
            attr_value,
            combinator: Combinator::None,
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
            placeholder,
            attr_name,
            attr_value,
            combinator: Combinator::None,
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
            placeholder,
            attr_name,
            attr_value,
            combinator: Combinator::None,
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
            placeholder,
            attr_name,
            attr_value,
            combinator: Combinator::None,
        })
    } else {
        Some(SelectorPart {
            tag: name_raw.into_owned(),
            class: String::new(),
            hover,
            placeholder,
            attr_name,
            attr_value,
            combinator: Combinator::None,
        })
    }
}

/// Re-space `>` so the whitespace tokenizer sees it as a standalone token,
/// which makes `div>.card` behave like `div > .card`. A `>` inside an
/// attribute selector (e.g. `[data-x="a>b"]`) is left untouched.
fn pad_child_combinators(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 4);
    let mut bracket_depth = 0usize;
    let mut quote: Option<char> = None;
    for ch in raw.chars() {
        if let Some(q) = quote {
            out.push(ch);
            if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '[' => {
                bracket_depth += 1;
                out.push(ch);
            }
            ']' => {
                bracket_depth = bracket_depth.saturating_sub(1);
                out.push(ch);
            }
            '"' | '\'' if bracket_depth > 0 => {
                quote = Some(ch);
                out.push(ch);
            }
            '>' if bracket_depth == 0 => out.push_str(" > "),
            _ => out.push(ch),
        }
    }
    out
}

fn parse_selector_list(selector: &str) -> Vec<CompoundSelector> {
    let mut out = Vec::new();
    for part in selector.split(',') {
        let raw = part.trim();
        if raw.is_empty() {
            continue;
        }
        // Tokenize on whitespace. A `>` token (padded so `div>.card` also
        // works) sets the combinator of the compound that follows it instead
        // of being silently dropped; plain whitespace between compounds is a
        // descendant combinator.
        let mut parts: Vec<SelectorPart> = Vec::new();
        let mut pending: Option<Combinator> = None;
        let mut valid = true;
        for token in pad_child_combinators(raw).split_whitespace() {
            if token == ">" {
                if parts.is_empty() {
                    // Leading `>` (e.g. `> .card`): there is no left compound
                    // to be the child of — invalid selector, drop the rule.
                    valid = false;
                    break;
                }
                pending = Some(Combinator::Child);
                continue;
            }
            match parse_selector_part(token) {
                Some(mut sp) => {
                    sp.combinator = pending.take().unwrap_or(if parts.is_empty() {
                        Combinator::None
                    } else {
                        Combinator::Descendant
                    });
                    parts.push(sp);
                }
                None => {
                    valid = false;
                    break;
                }
            }
        }
        // A trailing combinator (`div >`) is ignored: the compounds parsed so
        // far still form a usable selector (previous behavior).
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
/// For a single-part selector (e.g., `.app`, `button`), only the target element
/// is checked. For a multi-part selector (e.g., `.header h1`, `div > .card`),
/// the rightmost part matches the target and preceding parts must match
/// ancestor VNodes: a `Descendant` combinator may match any ancestor, while a
/// `Child` (`>`) combinator must match exactly one level above the part on
/// its right. `ancestors[0]` is the target's parent, `ancestors[1]` the
/// grandparent, and so on.
fn matches_selector(
    sel: &CompoundSelector,
    tag: &str,
    props: &Props,
    hovered: bool,
    ancestors: &[&VNode],
) -> bool {
    let Some((last, prefix)) = sel.parts.split_last() else {
        return false;
    };
    if !last.matches_element(tag, props, hovered) {
        return false;
    }
    // The last part matched the target element itself — "position -1" in the
    // ancestor chain. Match the remaining chain, constrained by the
    // combinator that connected the last part to the part on its left.
    match_prefix(prefix, ancestors, -1, last.combinator)
}

/// Whether every part in `prefix` (the chain left of an already-matched part)
/// can match the ancestor chain.
///
/// `below` is the ancestor position of the element matched by the part just
/// right of `prefix`'s last part (the target element itself sits at position
/// -1). `comb` is the combinator that connected that rightward part to
/// `prefix`'s last part: `Child` pins it exactly one level above, while
/// `Descendant` (and a leftmost `None`) allow any strictly-higher ancestor.
fn match_prefix(
    prefix: &[SelectorPart],
    ancestors: &[&VNode],
    below: isize,
    comb: Combinator,
) -> bool {
    let Some((last, rest)) = prefix.split_last() else {
        return true;
    };
    let candidates: Box<dyn Iterator<Item = usize> + '_> = match comb {
        Combinator::Child => {
            let pos = below + 1;
            if pos < 0 {
                return false;
            }
            Box::new(pos as usize..pos as usize + 1)
        }
        _ => Box::new((below + 1).max(0) as usize..ancestors.len()),
    };
    for pos in candidates {
        if let VNode::Element {
            tag: a_tag,
            props: a_props,
            ..
        } = ancestors[pos]
            && last.matches_element(a_tag, a_props, false)
            && match_prefix(rest, ancestors, pos as isize, last.combinator)
        {
            return true;
        }
    }
    false
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
    // Borrow the keys for ordering: the merged string is built from `&str`
    // lookups, so no key or value string is cloned just to be sorted.
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    let mut out = String::new();
    for (i, k) in keys.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(k);
        out.push_str(": ");
        out.push_str(&map[*k]);
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
/// Borrows the user-agent sheet (`ua::ua_sheet()`, a `OnceLock` singleton) and
/// `author` as two ordered layers instead of cloning both rule vecs into one
/// merged sheet per call. The cascade runs on every frame, so the old merge
/// allocated two full rule vecs plus every declaration string on each pass.
pub fn apply_with_cascade(node: &VNode, author: &Stylesheet) -> VNode {
    apply_with_cascade_with_hover(node, author, &|_, _| false)
}

/// Cascade with a custom hover predicate.
pub fn apply_with_cascade_with_hover<F>(node: &VNode, author: &Stylesheet, is_hovered: &F) -> VNode
where
    F: Fn(&str, &Props) -> bool,
{
    let ua = crate::ua::ua_sheet();
    apply_with_sheets(node, &[ua, author], is_hovered)
}

/// Properties that inherit to descendants, per browser CSS behavior.
/// The original filter carried `color`, `font-size`, `font-weight`,
/// `text-decoration`, `line-height`; this set only ADDS properties
/// (font-family, font-style, letter-spacing, text-align, vertical-align,
/// visibility, cursor) — `text-decoration` is retained to avoid regressing
/// behavior.
///
/// Pinned deliberately by this list's ABSENCE: `clip-path` (Masking 1 §3.1)
/// and the renderer's `img-filter` (an image filter, not the CSS `filter`
/// property, which is Filter Effects 1 §2.1) are both non-inherited in real
/// CSS, so each element reads its own declaration and a descendant of a
/// clipped box is not clipped unless it says so.
const INHERITABLE: &[&str] = &[
    "color",
    "font-size",
    "font-family",
    "font-weight",
    "font-style",
    "line-height",
    "letter-spacing",
    "text-align",
    // `vertical-align` is an inherited property (CSS 2.1 §10.8.1): it aligns an
    // inline-level box within its line box, and the line box belongs to the
    // parent block, so a value set on an ancestor is the one that applies.
    "vertical-align",
    // `white-space` is an inherited property (CSS 2.1 §16.6): it describes how
    // a box's inline content is wrapped, and a nested element's own content is
    // wrapped by the same rules as the text that flows into it. Without this,
    // a `white-space: pre` on an ancestor is dropped at the first element
    // boundary between the ancestor and its nested text.
    "white-space",
    "visibility",
    "cursor",
    "text-decoration",
];

/// Apply stylesheet with a custom hover predicate
pub fn apply_styles_with_hover<F>(node: &VNode, sheet: &Stylesheet, is_hovered: &F) -> VNode
where
    F: Fn(&str, &Props) -> bool,
{
    apply_with_sheets(node, &[sheet], is_hovered)
}

/// Cascade core: style `node` against `sheets` in layer order (earlier sheets
/// lose to later ones, inline `style` wins over everything).
///
/// Sheets are only borrowed: matching reads `&rule.decls` straight out of each
/// layer, so no merged rule vec is ever built.
fn apply_with_sheets<F>(node: &VNode, sheets: &[&Stylesheet], is_hovered: &F) -> VNode
where
    F: Fn(&str, &Props) -> bool,
{
    /// Overlay the inheritable declarations of a `style` attribute onto `map`.
    ///
    /// This is the threaded-accumulator half of the cascade: instead of
    /// serialising the element's resolved style to a string and re-parsing it
    /// for the inheritable subset (a serialize→parse round-trip per element),
    /// the children inherit straight from the accumulator this element already
    /// built, plus the inline declarations that override it — the same sources
    /// `merge_styles` combined, so the result is identical.
    fn overlay_inheritable(map: &mut HashMap<String, String>, style: &str) {
        for decl in style.split(';') {
            let d = decl.trim();
            if d.is_empty() {
                continue;
            }
            if let Some((k, v)) = d.split_once(':') {
                let k = k.trim();
                let v = v.trim();
                if INHERITABLE.contains(&k) {
                    map.insert(k.to_string(), v.to_string());
                }
            }
        }
    }

    fn apply_rec<FN>(
        node: &VNode,
        sheets: &[&Stylesheet],
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
                // Declarations from `::placeholder` rules, kept APART from the
                // element's own. They cannot go into `style`: an `<input>`'s
                // `style` is what the painter reads the VALUE's colour and
                // geometry from, so `input::placeholder { color: red }` landing
                // there would paint the value red. They are not inherited
                // either — a placeholder never inherits from its own element's
                // `color` through this path, because the element's colour is
                // exactly what a placeholder is dimmed FROM.
                let mut pseudo_acc: HashMap<String, String> = HashMap::new();
                // Match all selectors against this element, passing the ancestor chain
                // so compound selectors (e.g., `.header h1`) can walk up the tree.
                // Layers apply in order so a later sheet wins over an earlier one.
                for sheet in sheets {
                    for rule in &sheet.rules {
                        if matches_selector(&rule.selector, tag, props, hovered, ancestors) {
                            let target = if rule.selector.has_placeholder() {
                                // A selector that names `::placeholder` anywhere but
                                // on its own subject matches no element in real CSS,
                                // so its declarations must reach NOBODY. Keying off
                                // the last part alone is what made `.a::placeholder
                                // .b` repaint `.b`: the placeholder part sat in the
                                // chain, `.b` was the subject, the last part had no
                                // placeholder flag, and the declarations fell into
                                // `.b`'s own `style`.
                                if !rule.selector.placeholder_is_subject() {
                                    continue;
                                }
                                &mut pseudo_acc
                            } else {
                                &mut acc
                            };
                            for (k, v) in &rule.decls {
                                target.insert(k.clone(), v.clone());
                            }
                        }
                    }
                }
                let inline_style: Option<&str> = props.attrs.get("style").map(String::as_str);
                let mut new_props = props.clone();
                let merged = merge_styles(inline_style, &acc);
                if !merged.is_empty() {
                    new_props = new_props.set("style", merged);
                }
                if !pseudo_acc.is_empty() {
                    let merged_pseudo = merge_styles(
                        new_props
                            .attrs
                            .get(PLACEHOLDER_STYLE_ATTR)
                            .map(String::as_str),
                        &pseudo_acc,
                    );
                    new_props = new_props.set(PLACEHOLDER_STYLE_ATTR, merged_pseudo);
                }
                // Thread the accumulator into the children's inherited map
                // without re-parsing the serialised `style` string: the
                // inheritable subset of the merged style is exactly the
                // inheritable subset of `acc` overlaid with the inheritable
                // subset of the inline declarations (which win).
                let mut inherit_next: HashMap<String, String> = HashMap::with_capacity(acc.len());
                for (k, v) in &acc {
                    if INHERITABLE.contains(&k.as_str()) {
                        inherit_next.insert(k.clone(), v.clone());
                    }
                }
                if let Some(inline) = inline_style {
                    overlay_inheritable(&mut inherit_next, inline);
                }
                // Build child ancestors: this element + current ancestors
                let mut child_ancestors: Vec<&VNode> = Vec::with_capacity(ancestors.len() + 1);
                child_ancestors.push(node);
                child_ancestors.extend_from_slice(ancestors);
                let new_children = children
                    .iter()
                    .map(|c| apply_rec(c, sheets, is_hovered, &inherit_next, &child_ancestors))
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
    apply_rec(node, sheets, is_hovered, &inherited_root, &[])
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
                // `ComputedStyle` is the element's OWN resolved style, so it can
                // never hold `::placeholder` declarations — the painter reads the
                // placeholder's colour and geometry from
                // `PLACEHOLDER_STYLE_ATTR` instead, and a placeholder's colour
                // here would paint the VALUE. A rule whose placeholder part is
                // not the selector's subject matches nothing at all, so it is
                // skipped too. `apply_styles_with_hover` makes both distinctions
                // by routing into a second accumulator; this function has one
                // accumulator, so both cases collapse into "drop the rule".
                if rule.selector.has_placeholder() {
                    continue;
                }
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

#[cfg(test)]
mod placeholder_tests {
    use super::*;
    use velox_dom::h;

    // ── split_pseudos ────────────────────────────────────────────────────
    //
    // The regression that mattered: the old parser used `split_once(':')`,
    // which turns `input::placeholder` into `pseudo == ":placeholder"` — one
    // colon too many, matching neither `"hover"` nor anything else. The pseudo
    // was silently dropped and `input::placeholder { color: red }` degraded
    // into a plain `input` rule, which paints the VALUE text red. Every one of
    // these is the shape that bug took.

    #[test]
    fn a_double_colon_pseudo_element_is_not_mistaken_for_a_single_colon_one() {
        let (name, hover, placeholder) = split_pseudos("input::placeholder");
        assert_eq!(name.into_owned(), "input");
        assert!(placeholder, "`::placeholder` was not recognised");
        assert!(!hover, "`::placeholder` must not also set :hover");
    }

    #[test]
    fn the_single_colon_pseudo_classes_still_parse() {
        let (name, hover, placeholder) = split_pseudos("div:hover");
        assert_eq!(
            name.into_owned(),
            "div",
            ":hover regressed while ::placeholder was being added"
        );
        assert!(hover);
        assert!(!placeholder);
    }

    #[test]
    fn a_selector_with_no_pseudo_is_untouched() {
        // No pseudo means the name is borrowed, not allocated: the Cow stays
        // `Borrowed` so a plain `input` parses without a heap allocation here.
        let (field, hover, placeholder) = split_pseudos(".field");
        assert!(matches!(field, std::borrow::Cow::Borrowed(".field")));
        assert!(!hover);
        assert!(!placeholder);
        let (tag, hover, placeholder) = split_pseudos("input");
        assert!(matches!(tag, std::borrow::Cow::Borrowed("input")));
        assert!(!hover);
        assert!(!placeholder);
    }

    #[test]
    fn a_class_with_a_placeholder_pseudo_keeps_its_class() {
        let (name, hover, placeholder) = split_pseudos(".field::placeholder");
        assert_eq!(name.into_owned(), ".field");
        assert!(placeholder);
        assert!(!hover);
    }

    #[test]
    fn two_pseudos_in_one_selector_keep_both_flags() {
        let (name, hover, placeholder) = split_pseudos("input::placeholder:hover");
        assert_eq!(name.into_owned(), "input");
        assert!(placeholder);
        assert!(hover);
    }

    #[test]
    fn an_unknown_pseudo_is_ignored_rather_than_rejecting_the_selector() {
        // The pre-existing behaviour for `:focus` / `::before` was to ignore
        // them, and dropping the whole rule would be a regression.
        let (name, hover, placeholder) = split_pseudos("input::before");
        assert_eq!(name.into_owned(), "input");
        assert!(!placeholder);
        assert!(!hover);
    }

    #[test]
    fn the_placeholder_pseudo_is_case_insensitive() {
        assert!(
            split_pseudos("input::PLACEHOLDER").2,
            "CSS pseudo-element names are ASCII case-insensitive"
        );
    }

    // ── shows_placeholder ────────────────────────────────────────────────

    fn props(pairs: &[(&str, &str)]) -> Props {
        let mut p = Props::new();
        for (k, v) in pairs {
            p = p.set(*k, *v);
        }
        p
    }

    #[test]
    fn a_placeholder_only_matches_a_control_that_is_showing_it() {
        assert!(shows_placeholder(
            "input",
            &props(&[("placeholder", "hint")])
        ));
        assert!(shows_placeholder(
            "input",
            &props(&[("placeholder", "hint"), ("value", "")])
        ));
    }

    #[test]
    fn a_filled_input_is_not_showing_its_placeholder() {
        assert!(
            !shows_placeholder("input", &props(&[("placeholder", "hint"), ("value", "x")])),
            "a ::placeholder rule that matched a filled field would repaint the \
             VALUE text, which is how the double-colon bug presented"
        );
    }

    #[test]
    fn an_input_with_no_placeholder_attribute_does_not_match() {
        assert!(!shows_placeholder("input", &props(&[])));
        assert!(!shows_placeholder(
            "input",
            &props(&[("placeholder", "   ")])
        ));
    }

    #[test]
    fn a_non_input_never_shows_a_placeholder() {
        assert!(!shows_placeholder(
            "div",
            &props(&[("placeholder", "hint")])
        ));
        assert!(!shows_placeholder(
            "textarea",
            &props(&[("placeholder", "hint")])
        ));
    }

    // ── end to end through the cascade ───────────────────────────────────

    fn styled_input(pairs: &[(&str, &str)], sheet: &Stylesheet) -> Props {
        let tree = h("div", Props::new(), vec![h("input", props(pairs), vec![])]);
        let mut out = None;
        fn walk(node: &VNode, out: &mut Option<Props>) {
            if let VNode::Element {
                tag,
                props,
                children,
            } = node
            {
                if tag == "input" {
                    *out = Some(props.clone());
                }
                for c in children {
                    walk(c, out);
                }
            }
        }
        walk(&apply_with_cascade(&tree, sheet), &mut out);
        out.expect("the input must survive the cascade")
    }

    #[test]
    fn a_placeholder_rule_lands_on_its_own_attribute_and_not_on_style() {
        let sheet = Stylesheet::parse("input::placeholder { color: #ff0000; }");
        let p = styled_input(&[("placeholder", "hint")], &sheet);
        assert_eq!(
            p.attrs.get(PLACEHOLDER_STYLE_ATTR).map(String::as_str),
            Some("color: #ff0000;"),
            "the ::placeholder declarations did not reach {PLACEHOLDER_STYLE_ATTR}"
        );
        assert!(
            p.attrs.get("style").is_none_or(|s| !s.contains("ff0000")),
            "the ::placeholder colour leaked into the element's own `style`, \
             which is the VALUE text's style: the two would fight"
        );
    }

    #[test]
    fn a_placeholder_rule_does_not_land_on_a_filled_field() {
        let sheet = Stylesheet::parse("input::placeholder { color: #ff0000; }");
        let p = styled_input(&[("placeholder", "hint"), ("value", "typed")], &sheet);
        assert!(
            !p.attrs.contains_key(PLACEHOLDER_STYLE_ATTR),
            "the ::placeholder rule applied to a field showing a value"
        );
    }

    #[test]
    fn a_placeholder_rule_is_not_inherited_by_descendants() {
        // The declarations live on a separate attribute precisely so they
        // cannot flow down the tree the way `style` does.
        let sheet = Stylesheet::parse("input::placeholder { color: #ff0000; }");
        let tree = h(
            "div",
            Props::new(),
            vec![h(
                "input",
                props(&[("placeholder", "hint")]),
                vec![h("span", Props::new(), vec![])],
            )],
        );
        let styled = apply_with_cascade(&tree, &sheet);
        let child = match &styled {
            VNode::Element { children, .. } => match &children[0] {
                VNode::Element { children, .. } => match &children[0] {
                    VNode::Element { props, .. } => props.clone(),
                    _ => panic!("expected an element"),
                },
                _ => panic!("expected an element"),
            },
            _ => panic!("expected an element"),
        };
        assert!(
            !child.attrs.contains_key(PLACEHOLDER_STYLE_ATTR),
            "the placeholder declarations were inherited by a child element"
        );
    }

    // ── has_placeholder / placeholder_is_subject ──────────────────────────
    //
    // The routing table these two decide. `apply_styles_with_hover` asks them
    // once per matching rule, so a `false` here is the difference between a
    // declaration painting a placeholder and painting an ordinary element.

    fn sel(css: &str) -> CompoundSelector {
        Stylesheet::parse(&format!("{css} {{ color: red; }}"))
            .rules
            .into_iter()
            .next()
            .expect("the fixture must parse to exactly one rule")
            .selector
    }

    #[test]
    fn a_selector_with_no_placeholder_part_is_not_a_placeholder_selector() {
        let s = sel(".wrap input.b .c");
        assert!(!s.has_placeholder(), "no part names ::placeholder");
        assert!(
            !s.placeholder_is_subject(),
            "a selector with no ::placeholder has no placeholder subject"
        );
    }

    #[test]
    fn a_placeholder_as_the_sole_subject_part_is_accepted() {
        for css in [
            "input::placeholder",
            ".field::placeholder",
            "*::placeholder",
        ] {
            let s = sel(css);
            assert!(s.has_placeholder(), "{css}: no part named ::placeholder");
            assert!(
                s.placeholder_is_subject(),
                "{css}: a sole ::placeholder part on the subject is real CSS"
            );
        }
    }

    #[test]
    fn a_placeholder_after_a_descendant_chain_is_still_the_subject() {
        // `.wrap` is a chain link, `input::placeholder` is the subject. This is
        // valid CSS and must keep routing to the placeholder attribute; the
        // rejection below is about the placeholder being the LINK, not about the
        // selector having more than one part.
        for css in [".wrap input::placeholder", ".wrap > input::placeholder"] {
            let s = sel(css);
            assert!(
                s.placeholder_is_subject(),
                "{css}: the rightmost part is the ::placeholder subject, so this \
                 is valid CSS and must be accepted"
            );
        }
    }

    #[test]
    fn a_placeholder_that_is_not_the_subject_is_rejected() {
        for css in [".a::placeholder .b", ".a::placeholder > .b"] {
            let s = sel(css);
            assert!(s.has_placeholder(), "{css}: a part does name ::placeholder");
            assert!(
                !s.placeholder_is_subject(),
                "{css}: the ::placeholder part is a chain LINK, not the subject. \
                 A pseudo-element has no box, so it cannot be an ancestor; real \
                 CSS matches nothing here and Velox must not paint the subject."
            );
        }
    }

    #[test]
    fn a_placeholder_in_the_chain_order_does_not_change_the_subject() {
        // The mirror of the case above, and the one that shows which end of the
        // chain decides. `.a .b::placeholder` still ends in the placeholder, so
        // it styles `.b`'s placeholder and is valid CSS — it is the ORDER that
        // matters, not the presence of another part.
        let s = sel(".a .b::placeholder");
        assert!(
            s.placeholder_is_subject(),
            ".a .b::placeholder ends in the subject and is real CSS"
        );
    }

    #[test]
    fn a_second_placeholder_part_disqualifies_the_selector() {
        // Two ::placeholder parts: the subject carries one, but a LINK carries
        // one too, and the link alone is already illegal. Guarding only on the
        // last part would accept this and reintroduce the original defect.
        let s = sel(".a::placeholder .b::placeholder");
        assert!(
            !s.placeholder_is_subject(),
            "a ::placeholder in the chain disqualifies the selector even when \
             the subject also has one"
        );
    }
}
