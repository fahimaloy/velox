use crate::template_ast::{AttrKind, Node, TemplateAttr};

/// Minimal hand-rolled HTML-ish parser with support for:
/// - nested elements and self-closing tags (`<input/>`)
/// - attributes: static (`class="x"`), bind (`:value="expr"`), event (`@click="foo"`)
/// - text and `{{ interpolation }}` splits
///
/// Warnings (unclosed/unmatched tags) are printed to stderr, preserving the
/// historical behavior of this entry point.
pub fn parse_template_to_ast(input: &str) -> Result<Vec<Node>, String> {
    let mut warnings = Vec::new();
    let nodes = parse_template_inner(input, &mut warnings)?;
    emit_warnings(&warnings);
    Ok(nodes)
}

fn emit_warnings(warnings: &[String]) {
    for warning in warnings {
        eprintln!("velox: warning: {warning}");
    }
}

/// Warnings collected while parsing a template, alongside the parsed AST.
#[derive(Debug, Clone)]
pub struct TemplateDiag {
    /// The parsed template AST.
    pub nodes: Vec<Node>,
    /// Non-fatal warnings: structural leniency notes (unclosed/unmatched
    /// tags) plus `unknown component` diagnostics for PascalCase tags that
    /// are not registered component imports.
    pub warnings: Vec<String>,
}

/// Parse a template into an AST plus a list of non-fatal warnings.
///
/// `known_components` lists the component tag names registered for this SFC
/// (its resolved `<script setup>` imports). A tag starting with an uppercase
/// ASCII letter that is not in that list produces an `unknown component`
/// warning: PascalCase tags are treated as component references, and an
/// unregistered one renders as an inert unknown element instead of the
/// intended component.
///
/// Unlike [`parse_template_to_ast`], warnings are returned to the caller
/// instead of being printed to stderr.
pub fn parse_template(input: &str, known_components: &[&str]) -> Result<TemplateDiag, String> {
    let mut warnings = Vec::new();
    let nodes = parse_template_inner(input, &mut warnings)?;
    warnings.extend(unknown_component_warnings(&nodes, known_components, input));
    Ok(TemplateDiag { nodes, warnings })
}

/// Warnings for PascalCase tags that are not registered components.
///
/// `known_components` is the set of imported component names for the SFC
/// being compiled; `source` is the raw template text (used for best-effort
/// line/column positions in the messages).
pub fn unknown_component_warnings(
    nodes: &[Node],
    known_components: &[&str],
    source: &str,
) -> Vec<String> {
    let mut warnings = Vec::new();
    collect_unknown_components(nodes, known_components, source, &mut warnings);
    warnings
}

fn collect_unknown_components(
    nodes: &[Node],
    known_components: &[&str],
    source: &str,
    warnings: &mut Vec<String>,
) {
    for node in nodes {
        if let Node::Element { tag, .. } = node
            && tag.starts_with(|c: char| c.is_ascii_uppercase())
            && !known_components.contains(&tag.as_str())
        {
            let suggestion = format!(
                "PascalCase tags are component references; \
                 import it in <script setup> (import {tag} from './{tag}.vx') or fix the tag name"
            );
            match tag_position(source, tag) {
                Some((line, col)) => warnings.push(format!(
                    "unknown component <{tag}> at {line},{col} — {suggestion}"
                )),
                None => warnings.push(format!("unknown component <{tag}> — {suggestion}")),
            }
        }
        if let Node::Element { children, .. } = node {
            collect_unknown_components(children, known_components, source, warnings);
        }
    }
}

/// Best-effort (line, col) of the first occurrence of `<tag` in `source`.
/// The template AST does not record source offsets, so the position is
/// located by scanning for the opening tag with a word boundary after it.
fn tag_position(source: &str, tag: &str) -> Option<(usize, usize)> {
    let needle = format!("<{tag}");
    let bytes = source.as_bytes();
    let mut search_from = 0usize;
    while let Some(rel) = source[search_from..].find(&needle) {
        let at = search_from + rel;
        let after = at + needle.len();
        let boundary = match bytes.get(after) {
            Some(&b) => !(b as char).is_ascii_alphanumeric() && b != b'_' && b != b'-',
            None => true,
        };
        if boundary {
            return Some(crate::diagnostic::line_col_at(source, at));
        }
        search_from = after;
    }
    None
}

fn parse_template_inner(input: &str, warnings: &mut Vec<String>) -> Result<Vec<Node>, String> {
    let mut i = 0usize;
    let bytes = input.as_bytes();
    let mut stack: Vec<Node> = Vec::new();
    // Parallel stack tracking (tag, byte_offset) of *opening* elements, so we
    // can report precise line/col positions for unclosed-tag warnings.
    let mut open_info: Vec<(String, usize)> = Vec::new();
    let mut roots: Vec<Node> = Vec::new();

    #[allow(clippy::ptr_arg)]
    fn push_child(stack: &mut Vec<Node>, roots: &mut Vec<Node>, node: Node) {
        if let Some(Node::Element { children, .. }) = stack.last_mut() {
            children.push(node);
        } else {
            roots.push(node);
        }
    }

    while i < bytes.len() {
        if bytes[i] == b'<' {
            // closing tag?
            if i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                let close_pos = i;
                i += 2;
                let tag = read_ident(bytes, &mut i);
                skip_ws(bytes, &mut i);
                // expect '>'
                if i < bytes.len() && bytes[i] == b'>' {
                    i += 1;
                }
                // pop until matching tag
                let mut popped: Option<Node> = None;
                while let Some(n) = stack.pop() {
                    let tag_matches = match &n {
                        Node::Element { tag: t, .. } => t == &tag,
                        _ => false,
                    };
                    if tag_matches {
                        popped = Some(n);
                        break;
                    }
                }
                if let Some(n) = popped {
                    // Drop the innermost recorded opening tag of the same name
                    // so it is no longer reported as unclosed.
                    if let Some(idx) = open_info.iter().rposition(|(t, _)| *t == tag) {
                        open_info.remove(idx);
                    }
                    push_child(&mut stack, &mut roots, n);
                } else {
                    let (line, col) = crate::diagnostic::line_col_at(input, close_pos);
                    warnings.push(format!(
                        "unmatched closing tag </{tag}> at {line},{col} — no matching opening tag; ignoring"
                    ));
                }
                continue;
            }

            // opening or self-closing tag
            let open_pos = i;
            i += 1;
            let tag = read_ident(bytes, &mut i);
            let mut attrs: Vec<TemplateAttr> = Vec::new();
            let mut self_closing = false;

            loop {
                skip_ws(bytes, &mut i);
                if i >= bytes.len() {
                    break;
                }
                match bytes[i] {
                    b'/' => {
                        // possible "/>"
                        self_closing = true;
                        i += 1;
                        skip_ws(bytes, &mut i);
                        if i < bytes.len() && bytes[i] == b'>' {
                            i += 1;
                        }
                        break;
                    }
                    b'>' => {
                        i += 1;
                        break;
                    }
                    _ => {
                        // attribute
                        if let Some(attr) = read_attribute(bytes, &mut i) {
                            attrs.push(attr);
                        } else {
                            // skip unknown token
                            i += 1;
                        }
                    }
                }
            }

            if self_closing {
                push_child(
                    &mut stack,
                    &mut roots,
                    Node::Element {
                        tag,
                        attrs,
                        children: Vec::new(),
                        self_closing: true,
                    },
                );
            } else {
                open_info.push((tag.clone(), open_pos));
                stack.push(Node::Element {
                    tag,
                    attrs,
                    children: Vec::new(),
                    self_closing: false,
                });
            }
        } else if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            // interpolation
            let open_pos = i;
            i += 2;
            let start = i;
            while i + 1 < bytes.len() && !(bytes[i] == b'}' && bytes[i + 1] == b'}') {
                i += 1;
            }
            if i + 1 >= bytes.len() {
                // Unclosed interpolation: report a rich, user-facing error instead
                // of silently treating the remainder as text.
                let (line, col) = crate::diagnostic::line_col_at(input, open_pos);
                let message = "unclosed '{{' — expected a matching '}}'".to_string();
                let suggestion =
                    "close the interpolation with '}}', e.g. '{{ count }}'".to_string();
                return Err(crate::diagnostic::render_parse_error(
                    input,
                    line,
                    col,
                    2,
                    &message,
                    Some(&suggestion),
                ));
            }
            let expr = input[start..i].trim().to_string();
            i += 2; // skip "}}"
            push_child(&mut stack, &mut roots, Node::Interpolation(expr));
        } else {
            // text until next '<' or '{{'
            let start = i;
            while i < bytes.len()
                && bytes[i] != b'<'
                && !(i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{')
            {
                i += 1;
            }
            let mut text = input[start..i].to_string();
            if !text.is_empty() {
                // normalize simple newlines around indentation
                if is_all_ws(&text) {
                    // keep a single space if inside element text
                    text = " ".to_string();
                }
                push_child(&mut stack, &mut roots, Node::Text(text));
            }
        }
    }

    // Unclosed tags: drain stack to roots (best-effort), warning about each so
    // users know their markup is malformed even though we parse leniently.
    while let Some(n) = stack.pop() {
        if let Node::Element { tag, .. } = &n
            && let Some((line, col)) = open_info
                .iter()
                .find(|(t, _)| t == tag)
                .map(|(_, pos)| crate::diagnostic::line_col_at(input, *pos))
        {
            warnings.push(format!(
                "unclosed tag <{tag}> at {line},{col} — add a matching </{tag}>"
            ));
        }
        push_child(&mut stack, &mut roots, n);
    }

    // Trim root whitespace-only text nodes
    roots.retain(|n| match n {
        Node::Text(t) => !is_all_ws(t),
        _ => true,
    });

    Ok(roots)
}

pub fn is_all_ws(s: &str) -> bool {
    s.chars().all(|c| c.is_whitespace())
}

fn skip_ws(bytes: &[u8], i: &mut usize) {
    while *i < bytes.len() && (bytes[*i] as char).is_whitespace() {
        *i += 1;
    }
}

fn read_ident(bytes: &[u8], i: &mut usize) -> String {
    let start = *i;
    while *i < bytes.len() {
        let c = bytes[*i] as char;
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            *i += 1;
        } else {
            break;
        }
    }
    String::from_utf8(bytes[start..*i].to_vec()).unwrap_or_default()
}

fn read_attribute(bytes: &[u8], i: &mut usize) -> Option<TemplateAttr> {
    let name_start = *i;
    while *i < bytes.len() {
        let c = bytes[*i] as char;
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ':' || c == '@' {
            *i += 1;
        } else {
            break;
        }
    }
    if *i == name_start {
        return None;
    }
    let raw_name = String::from_utf8(bytes[name_start..*i].to_vec()).ok()?;

    skip_ws(bytes, i);
    let mut value: Option<String> = None;
    if *i < bytes.len() && bytes[*i] == b'=' {
        *i += 1;
        skip_ws(bytes, i);
        value = read_quoted(bytes, i);
    }

    let (kind, name) = if let Some(rest) = raw_name.strip_prefix(':') {
        (AttrKind::Bind, rest.to_string())
    } else if let Some(rest) = raw_name.strip_prefix('@') {
        (AttrKind::On, rest.to_string())
    } else if let Some(rest) = raw_name.strip_prefix("v-") {
        // normalize directive name: strip `v-` and convert camelCase or underscores to kebab-case
        let raw_dir = rest.to_string();
        let name = normalize_directive_name(&raw_dir);
        (AttrKind::Directive, name)
    } else {
        (AttrKind::Static, raw_name)
    };

    Some(TemplateAttr { name, value, kind })
}

fn read_quoted(bytes: &[u8], i: &mut usize) -> Option<String> {
    if *i >= bytes.len() {
        return None;
    }
    let quote = bytes[*i];
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    *i += 1;
    let start = *i;
    while *i < bytes.len() && bytes[*i] != quote {
        *i += 1;
    }
    let s = String::from_utf8(bytes[start..*i].to_vec()).ok()?;
    if *i < bytes.len() {
        *i += 1;
    } // consume closing quote
    Some(s)
}

fn normalize_directive_name(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch == '_' {
            out.push('-');
        } else if ch.is_ascii_uppercase() {
            out.push('-');
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
        } else {
            out.push(ch.to_ascii_lowercase());
        }
    }
    // collapse any duplicated dashes
    let mut prev_dash = false;
    let mut compact = String::with_capacity(out.len());
    for c in out.chars() {
        if c == '-' {
            if !prev_dash {
                compact.push(c);
                prev_dash = true;
            }
        } else {
            compact.push(c);
            prev_dash = false;
        }
    }
    compact.trim_matches('-').to_string()
}
