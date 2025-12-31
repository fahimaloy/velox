use crate::template_ast::{AttrKind, Node, TemplateAttr};

/// Minimal hand-rolled HTML-ish parser with support for:
/// - nested elements and self-closing tags (`<input/>`)
/// - attributes: static (`class="x"`), bind (`:value="expr"`), event (`@click="foo"`)
/// - text and `{{ interpolation }}` splits
pub fn parse_template_to_ast(input: &str) -> Result<Vec<Node>, String> {
    let mut i = 0usize;
    let bytes = input.as_bytes();
    let mut stack: Vec<Node> = Vec::new();
    let mut roots: Vec<Node> = Vec::new();

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
                i += 2;
                let tag = read_ident(&bytes, &mut i);
                skip_ws(&bytes, &mut i);
                // expect '>'
                if i < bytes.len() && bytes[i] == b'>' {
                    i += 1;
                }
                // pop until matching tag
                let mut popped: Option<Node> = None;
                while let Some(n) = stack.pop() {
                    if let Node::Element { tag: t, .. } = &n {
                        if t == &tag {
                            popped = Some(n);
                            break;
                        }
                    }
                }
                if let Some(n) = popped {
                    push_child(&mut stack, &mut roots, n);
                }
                continue;
            }

            // opening or self-closing tag
            i += 1;
            let tag = read_ident(&bytes, &mut i);
            let mut attrs: Vec<TemplateAttr> = Vec::new();
            let mut self_closing = false;

            loop {
                skip_ws(&bytes, &mut i);
                if i >= bytes.len() {
                    break;
                }
                match bytes[i] {
                    b'/' => {
                        // possible "/>"
                        self_closing = true;
                        i += 1;
                        skip_ws(&bytes, &mut i);
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
                        if let Some(attr) = read_attribute(&bytes, &mut i) {
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
                stack.push(Node::Element {
                    tag,
                    attrs,
                    children: Vec::new(),
                    self_closing: false,
                });
            }
        } else if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            // interpolation
            i += 2;
            let start = i;
            while i + 1 < bytes.len() && !(bytes[i] == b'}' && bytes[i + 1] == b'}') {
                i += 1;
            }
            let expr = input[start..i].trim().to_string();
            if i + 1 < bytes.len() {
                i += 2;
            } // skip "}}"
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

    // Unclosed tags: drain stack to roots (best-effort)
    while let Some(n) = stack.pop() {
        push_child(&mut stack, &mut roots, n);
    }

    // Trim root whitespace-only text nodes
    roots.retain(|n| match n {
        Node::Text(t) => !is_all_ws(t),
        _ => true,
    });

    Ok(roots)
}

fn is_all_ws(s: &str) -> bool {
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

    let (kind, name) = if raw_name.starts_with(':') {
        (AttrKind::Bind, raw_name[1..].to_string())
    } else if raw_name.starts_with('@') {
        (AttrKind::On, raw_name[1..].to_string())
    } else if raw_name.starts_with("v-") {
        // normalize directive name: strip `v-` and convert camelCase or underscores to kebab-case
        let raw_dir = raw_name[2..].to_string();
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
            for lc in ch.to_lowercase() { out.push(lc); }
        } else {
            out.push(ch.to_ascii_lowercase());
        }
    }
    // collapse any duplicated dashes
    let mut prev_dash = false;
    let mut compact = String::with_capacity(out.len());
    for c in out.chars() {
        if c == '-' {
            if !prev_dash { compact.push(c); prev_dash = true; }
        } else { compact.push(c); prev_dash = false; }
    }
    compact.trim_matches('-').to_string()
}
