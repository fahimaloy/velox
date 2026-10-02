use pest::Parser;
use pest::error::ErrorVariant;
use pest::iterators::Pair;

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
    /// Non-fatal template diagnostics collected during parsing: structural
    /// leniency notes (unclosed/unmatched tags) and `unknown component`
    /// warnings for PascalCase tags that are not registered component
    /// imports.
    ///
    /// Computed by the shared template-diagnostics channel
    /// ([`crate::template_parse::parse_template`]) — the same computation the
    /// compile path prints to stderr — so the two surfaces cannot diverge.
    /// `parse_sfc` stores warnings for programmatic consumers but never
    /// prints them, so users never see a warning twice.
    pub warnings: Vec<String>,
}

/// Format a Pest parse error into a human-readable message with line number,
/// column, a source excerpt with a `^` caret, an explanatory message, and an
/// actionable suggestion where one can be inferred.
fn format_pest_error(err: pest::error::Error<Rule>, source: &str) -> String {
    // Pest reports either a single position or a span; use the span start so the
    // caret points at the first offending character.
    let (line, column) = match err.line_col {
        pest::error::LineColLocation::Pos((l, c)) => (l, c),
        pest::error::LineColLocation::Span((l, c), _) => (l, c),
    };

    // Determine what kind of parsing failure occurred and produce a
    // human-readable summary.
    let mut expected: Vec<String> = Vec::new();
    let mut unexpected: Vec<String> = Vec::new();
    let description = match &err.variant {
        ErrorVariant::ParsingError {
            positives,
            negatives,
        } => {
            expected = positives.iter().map(rule_to_block_name).collect::<Vec<_>>();
            unexpected = negatives.iter().map(rule_to_block_name).collect::<Vec<_>>();

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
    let message = format!("{description} while parsing the {block_context} block");

    // Build an actionable suggestion from the failure.
    let suggestion = suggest_pest_error(&expected, &unexpected, &block_context);

    crate::diagnostic::render_parse_error(source, line, column, 1, &message, suggestion.as_deref())
}

/// Heuristically produce a helpful `help:` line for common SFC mistakes, based
/// on what the grammar expected, what was found, and which block broke.
fn suggest_pest_error(expected: &[String], unexpected: &[String], block: &str) -> Option<String> {
    let end_of_input = expected
        .iter()
        .any(|e| e.contains("end of input") || e.contains("SFC file"));
    let unexp_join = unexpected.join(" ");

    if end_of_input {
        return Some(format!(
            "the {block} block may be missing its closing tag (e.g. </{block}>), or a block was never opened"
        ));
    }

    if block != "top-level" {
        // We are inside a block; the likely fixes are structural.
        return Some(format!(
            "check the {block} tags and attribute quotes near this line; \
             every value after '=' should be wrapped in \"double\" or 'single' quotes"
        ));
    }

    if unexp_join.contains("end of input") {
        return Some(
            "the file ended before a block was closed — add the missing </template>, </script>, or </style>".to_string(),
        );
    }

    if expected.iter().any(|e| e.contains("<template")) {
        return Some(
            "after a <template> block you need a <script setup> block to define component state and logic"
                .to_string(),
        );
    }

    None
}

/// Map a Pest `Rule` to a human-readable name (block-level where possible).
fn rule_to_block_name(rule: &Rule) -> String {
    match rule {
        Rule::template | Rule::template_open | Rule::template_body | Rule::nested_template => {
            "<template>".to_string()
        }
        // A nested `<template v-slot:foo>` is still a template block as far as a
        // syntax error inside it is concerned, and it is the only place a slot
        // binding can appear, so a name here has to say which.
        Rule::slot_attr => "slot binding (v-slot:name or #name)".to_string(),
        Rule::slot_name => "slot name".to_string(),
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
        } else if trimmed.starts_with("</template")
            || trimmed.starts_with("</script")
            || trimmed.starts_with("</style")
        {
            last_block = "top-level";
        }
    }

    last_block.to_string()
}

/// How many literal `<template` openers one SFC may contain before `parse_sfc`
/// refuses the file, bounding the pest grammar's own recursion.
///
/// This is deliberately NOT [`crate::template_parse::MAX_TEMPLATE_DEPTH`]. That
/// constant bounds the hand-written element builder's open-element `Vec`, which
/// is a different stack in a different parser — reached only after pest has
/// already returned. Nothing bounded this one: `grammar.pest:51-52` spells the
/// nesting as
///
/// ```text
/// nested_template = { template_open ~ template_body ~ "</template>" }
/// template_body  = @{ (nested_template | !"</template>" ~ ANY)* }
/// ```
///
/// — mutual recursion with no ceiling of its own. A file made of nothing but
/// `<template` openers drives one recursive `template_body` frame per opener
/// with nothing to stop it, so the ceiling has to live OUTSIDE the grammar: pest
/// offers no way to parameterise a rule's depth.
///
/// 256 matches `MAX_TEMPLATE_DEPTH` and borrows its rationale (see that
/// constant's doc): several times deeper than any hand-written template needs,
/// so reaching this means the input is machine-generated or malformed rather
/// than merely elaborate.
///
/// # Why counting occurrences is enough
///
/// Every level of `nested_template` consumes a distinct literal `<template` —
/// that is what `template_open` starts with. So the number of those substrings in
/// the source is an UPPER BOUND on the grammar's real recursion depth: this
/// guard can never wave a recursion bomb through. It can, however, reject a file
/// whose count is inflated by `<template` text the grammar would not open on
/// (inside an attribute value, a comment, or a `<script>` body holding a code
/// sample). This scan makes no attempt to exclude those, because proving a
/// position is inert requires modelling the grammar's own state, and getting
/// that wrong in a guard whose only job is to be an upper bound turns the guard
/// into the hazard. The trade is explicit: a file carrying more than 256
/// `<template` strings anywhere is rejected with an actionable message. Any such
/// file is already pathological. One exclusion IS safe and IS made, in
/// `nested_template_openers`: a `<template` not followed by whitespace or `>`
/// cannot be what `template_open` matches, so a `<templates>` tag is not counted.
pub const MAX_NESTED_TEMPLATE_DEPTH: usize = 256;

/// Count the literal `<template` openers the grammar could recurse on, and the
/// byte offset of the first one past [`MAX_NESTED_TEMPLATE_DEPTH`]. See that
/// constant for why the raw count is the right thing to compare, and what it
/// deliberately does not try to exclude.
///
/// Returns `(count, offending_offset)`, with the offset `0` when nothing
/// exceeds the limit — no caller reports a position it did not fail on.
fn nested_template_openers(source: &str) -> (usize, usize) {
    const OPEN: &str = "<template";
    // `template_open` is `"<template" ~ (WS+ ~ (slot_attr | attribute))* ~ WS*
    // ~ ">"`, so the character after the literal is either one of the grammar's
    // four whitespace characters or the `>` itself — never anything else. That
    // is exactly what tells a nested block from `<templates>`, and matching it
    // here means the count is the number of things the grammar can ACTUALLY
    // open on rather than the number of strings that start the same way. It
    // still only ever REDUCES the count, so the upper-bound property that makes
    // this a sound guard is untouched.
    const WS: [char; 4] = [' ', '\t', '\r', '\n'];
    let mut count = 0usize;
    let mut offending_offset = 0usize;
    // `char_indices`, not a byte counter: `source[i..]` panics if `i` lands in
    // the middle of a multi-byte character, and a `.vx` file may well contain
    // one. Every position yielded here is a character boundary. Once
    // `starts_with(OPEN)` holds, `i + OPEN.len()` is a boundary too — ASCII bytes
    // never occur inside a multi-byte sequence — so the lookahead below is safe.
    for (i, _) in source.char_indices() {
        if !source[i..].starts_with(OPEN) {
            continue;
        }
        let opens_a_block = source[i + OPEN.len()..]
            .chars()
            .next()
            .is_some_and(|after| after == '>' || WS.contains(&after));
        if !opens_a_block {
            continue;
        }
        // The (MAX + 1)th opener is the first one past the ceiling. It is not
        // necessarily the last, and byte 0 is never it: every SFC opens with
        // a top-level `<template>`, so pointing a reader there would send
        // them to a tag they cannot change.
        if count == MAX_NESTED_TEMPLATE_DEPTH {
            offending_offset = i;
        }
        count += 1;
    }
    (count, offending_offset)
}

/// Refuse a file whose `<template` count exceeds
/// [`MAX_NESTED_TEMPLATE_DEPTH`], before pest is handed it.
fn check_nested_template_depth(source: &str) -> Result<(), String> {
    let (openers, offending_offset) = nested_template_openers(source);
    if openers <= MAX_NESTED_TEMPLATE_DEPTH {
        return Ok(());
    }
    let (line, col) = crate::diagnostic::line_col_at(source, offending_offset);
    Err(crate::diagnostic::render_parse_error(
        source,
        line,
        col,
        "<template".len(),
        &format!(
            "{openers} nested `<template` blocks exceed the limit of \
             {MAX_NESTED_TEMPLATE_DEPTH}"
        ),
        Some(
            "nest fewer than 256 `<template` blocks deep, or flatten the markup — \
             a `<template v-slot:name>` block is the only thing that nests, so this \
             means a slot-fragment tree far deeper than any real template",
        ),
    ))
}

pub fn parse_sfc(source: &str) -> Result<Sfc, String> {
    let mut sfc = Sfc::default();

    // BEFORE handing the source to pest: the grammar's `template_body` /
    // `nested_template` pair recurses once per `<template` with no ceiling of
    // its own, so the bound has to be enforced out here. See
    // `MAX_NESTED_TEMPLATE_DEPTH`.
    check_nested_template_depth(source)?;

    // Parse the root and immediately descend into the `file` node.
    let mut pairs =
        SfcParser::parse(Rule::file, source).map_err(|e| format_pest_error(e, source))?;
    let file = pairs
        .next()
        .ok_or_else(|| "SFC parse error: input is empty".to_string())?;
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

    sfc.warnings = template_warnings(&sfc);

    Ok(sfc)
}

/// Collect non-fatal template diagnostics for a parsed SFC.
///
/// Warnings come from the shared template-diagnostics channel
/// ([`crate::template_parse::parse_template`]), so the stored messages are
/// identical to the ones the compile path
/// (`template_codegen::compile_template_to_rs_full`) prints to stderr.
///
/// Known components are the imports declared in `<script setup>`, mirroring
/// how `ComponentResolver` is populated during a build; imports in a plain
/// `<script>` block do not register components (the compile path treats such
/// tags as unknown too).
///
/// Hard template errors (e.g. an unclosed `{{` interpolation) are not
/// warnings — they surface as `Err` from the compile path — so this function
/// returns no warnings for them, leaving `parse_sfc`'s block-splitting
/// semantics unchanged.
fn template_warnings(sfc: &Sfc) -> Vec<String> {
    let Some(template) = &sfc.template else {
        return Vec::new();
    };
    let mut known: Vec<String> = Vec::new();
    if let Some(script_setup) = &sfc.script_setup {
        let mut resolver =
            crate::component_resolver::ComponentResolver::new(std::path::PathBuf::from("."));
        resolver.parse_imports(&script_setup.content);
        known.extend(resolver.component_names());
    }
    let known_refs: Vec<&str> = known.iter().map(String::as_str).collect();
    match crate::template_parse::parse_template(&template.content, &known_refs) {
        Ok(diag) => diag.warnings,
        Err(_) => Vec::new(),
    }
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
