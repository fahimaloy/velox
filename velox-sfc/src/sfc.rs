use pest::Parser;
use pest::iterators::Pair;
use pest::error::ErrorVariant;

#[derive(pest_derive::Parser)]
#[grammar = "grammar.pest"]
struct SfcParser;

#[derive(Debug, Clone, PartialEq)]
pub struct Attr {
    pub name: String,
    pub value: Option<String>, // boolean attrs allowed, e.g., `scoped` or `setup`
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TemplateBlock {
    pub attrs: Vec<Attr>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScriptBlock {
    pub attrs: Vec<Attr>,
    pub content: String,
    pub setup: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StyleBlock {
    pub attrs: Vec<Attr>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sfc {
    pub template: Option<TemplateBlock>,
    pub script_setup: Option<ScriptBlock>,
    pub script: Option<ScriptBlock>,
    pub style: Option<StyleBlock>,
}

/// Format a Pest parse error into a human-readable message with line number
/// and context about which SFC block was being parsed.
fn format_pest_error(err: pest::error::Error<Rule>, source: &str) -> String {
    // Extract the line number from the error's location.
    // Pest provides either a single position or a span (start, end).
    let line = match err.line_col {
        pest::error::LineColLocation::Pos((l, _)) => l,
        pest::error::LineColLocation::Span((l, _), _) => l,
    };

    // Determine what kind of parsing failure occurred and produce a
    // human-readable summary.
    let description = match &err.variant {
        ErrorVariant::ParsingError {
            positives,
            negatives,
        } => {
            let expected = positives
                .iter()
                .map(|r| rule_to_block_name(r))
                .collect::<Vec<_>>();
            let unexpected = negatives
                .iter()
                .map(|r| rule_to_block_name(r))
                .collect::<Vec<_>>();

            if expected.is_empty() {
                "unexpected token found while parsing the SFC".to_string()
            } else {
                let unexpected_str = if unexpected.is_empty() {
                    "an unexpected token".to_string()
                } else {
                    unexpected.join(", ")
                };
                format!(
                    "expected {} but found {}",
                    expected.join(", "),
                    unexpected_str
                )
            }
        }
        ErrorVariant::CustomError { message } => message.clone(),
    };

    // Try to infer which block the error falls in by examining the source
    // up to the error position.
    let block_context = infer_block_context(source, line);

    format!(
        "SFC parse error at line {}: {}\nContext: parsing {} block",
        line, description, block_context
    )
}

/// Map a Pest `Rule` to a human-readable name (block-level where possible).
fn rule_to_block_name(rule: &Rule) -> String {
    match rule {
        Rule::template | Rule::template_open | Rule::template_body => "<template>".to_string(),
        Rule::script | Rule::script_open | Rule::script_body => "<script>".to_string(),
        Rule::style | Rule::style_open | Rule::style_body => "<style>".to_string(),
        Rule::attribute => "attribute".to_string(),
        Rule::ident => "identifier".to_string(),
        Rule::quoted | Rule::dq | Rule::sq => "quoted value".to_string(),
        Rule::block => "SFC block (<template>, <script>, or <style>)".to_string(),
        Rule::file => "SFC file".to_string(),
        Rule::WS => "whitespace".to_string(),
        Rule::EOI => "end of input".to_string(),
    }
}

/// Given the full source text and a line number, try to infer which SFC block
/// the error occurred in by scanning for the most recent block-opening tag
/// before that line.
fn infer_block_context(source: &str, error_line: usize) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut last_block = "top-level";

    for (idx, line) in lines.iter().enumerate() {
        let ln = idx + 1;
        if ln > error_line {
            break;
        }
        let trimmed = line.trim();
        if trimmed.starts_with("<template") {
            last_block = "template";
        } else if trimmed.starts_with("<script") {
            last_block = "script";
        } else if trimmed.starts_with("<style") {
            last_block = "style";
        } else if trimmed.starts_with("</template") {
            last_block = "top-level";
        } else if trimmed.starts_with("</script") {
            last_block = "top-level";
        } else if trimmed.starts_with("</style") {
            last_block = "top-level";
        }
    }

    last_block.to_string()
}

pub fn parse_sfc(source: &str) -> Result<Sfc, String> {
    let mut sfc = Sfc::default();

    // Parse the root and immediately descend into the `file` node.
    let mut pairs = SfcParser::parse(Rule::file, source)
        .map_err(|e| format_pest_error(e, source))?;
    let file = pairs.next().ok_or_else(|| "SFC parse error: input is empty".to_string())?;
    debug_assert!(file.as_rule() == Rule::file);

    // Walk children of `file`: they will be `block` nodes (and nothing else,
    // since WS is a silent rule in the grammar).
    for node in file.into_inner() {
        match node.as_rule() {
            Rule::block => {
                for inner in node.into_inner() {
                    consume_top_level(inner, &mut sfc);
                }
            }
            // (Defensive: in case grammar changes and blocks appear directly)
            Rule::template | Rule::script | Rule::style => {
                consume_top_level(node, &mut sfc);
            }
            _ => {}
        }
    }

    Ok(sfc)
}

fn consume_top_level(node: Pair<Rule>, sfc: &mut Sfc) {
    match node.as_rule() {
        Rule::template => {
            let (attrs, content) = parse_template(node);
            sfc.template = Some(TemplateBlock { attrs, content });
        }
        Rule::script => {
            let (attrs, content) = parse_script(node);
            let setup = has_bool_attr(&attrs, "setup");
            let sb = ScriptBlock {
                attrs,
                content,
                setup,
            };
            if setup {
                sfc.script_setup = Some(sb);
            } else {
                sfc.script = Some(sb);
            }
        }
        Rule::style => {
            let (attrs, content) = parse_style(node);
            sfc.style = Some(StyleBlock { attrs, content });
        }
        _ => {}
    }
}

fn parse_template(tpl: Pair<Rule>) -> (Vec<Attr>, String) {
    let mut attrs = Vec::new();
    let mut content = String::new();

    for p in tpl.into_inner() {
        match p.as_rule() {
            Rule::template_open => {
                // attributes are direct children of *_open
                for a in p.into_inner() {
                    if a.as_rule() == Rule::attribute {
                        attrs.push(parse_attr(a));
                    }
                }
            }
            Rule::template_body => content = p.as_str().to_string(),
            _ => {}
        }
    }
    (attrs, content)
}

fn parse_script(scr: Pair<Rule>) -> (Vec<Attr>, String) {
    let mut attrs = Vec::new();
    let mut content = String::new();

    for p in scr.into_inner() {
        match p.as_rule() {
            Rule::script_open => {
                for a in p.into_inner() {
                    if a.as_rule() == Rule::attribute {
                        attrs.push(parse_attr(a));
                    }
                }
            }
            Rule::script_body => content = p.as_str().to_string(),
            _ => {}
        }
    }
    (attrs, content)
}

fn parse_style(sty: Pair<Rule>) -> (Vec<Attr>, String) {
    let mut attrs = Vec::new();
    let mut content = String::new();

    for p in sty.into_inner() {
        match p.as_rule() {
            Rule::style_open => {
                for a in p.into_inner() {
                    if a.as_rule() == Rule::attribute {
                        attrs.push(parse_attr(a));
                    }
                }
            }
            Rule::style_body => content = p.as_str().to_string(),
            _ => {}
        }
    }
    (attrs, content)
}

fn parse_attr(attr: Pair<Rule>) -> Attr {
    // attribute = ident ( "=" quoted )?
    let mut name = String::new();
    let mut value: Option<String> = None;

    for part in attr.into_inner() {
        match part.as_rule() {
            Rule::ident => name = part.as_str().to_string(),
            Rule::quoted => value = Some(strip_quotes(part.as_str())),
            _ => {}
        }
    }
    Attr { name, value }
}

fn strip_quotes(s: &str) -> String {
    let b = s.as_bytes();
    if b.len() >= 2
        && ((b[0] == b'"' && b[b.len() - 1] == b'"') || (b[0] == b'\'' && b[b.len() - 1] == b'\''))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn has_bool_attr(attrs: &[Attr], key: &str) -> bool {
    attrs.iter().any(|a| a.name == key)
}

/// Validate a parsed SFC for structural issues that go beyond grammar-level
/// parsing errors. Returns a list of error messages (empty if valid).
///
/// Checks:
/// - A `<template>` block without any `<script>` or `<script setup>` block
///   is likely incomplete and cannot produce a functional component.
pub fn validate_sfc(sfc: &Sfc) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();

    // Template without any script block: the component will have no state
    // or logic, which means render_with_state and event handlers cannot work.
    if sfc.template.is_some() && sfc.script_setup.is_none() && sfc.script.is_none() {
        errors.push(
            "SFC has a <template> block but no <script> or <script setup> block — \
             the component will lack state and event handling"
                .to_string(),
        );
    }

    errors
}
