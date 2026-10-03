//! Lightweight indexing of the `<script setup>` block.
//!
//! The SFC compiler does not parse the script into a Rust AST (the script is
//! emitted verbatim). These line/character-based helpers extract just enough
//! structure for codegen:
//!
//! - which `State` methods exist (so template keys can be resolved to the real
//!   method name, e.g. `title` → `get_title`), and
//! - which `State` fields exist (so an imported component tag can be matched to
//!   a persistent child-state field, e.g. `<Todos>` ↔ `state.todos`).

/// A method declared on the component's `State` struct.
#[derive(Debug, Clone)]
pub struct StateMethod {
    /// The method name as declared, e.g. `get_title`.
    pub name: String,
    /// Whether the method accepts an event payload as a second parameter,
    /// e.g. `fn on_input(&self, payload: &str)`.
    pub takes_payload: bool,
    /// The declared return type as written after `->`, e.g. `String` or
    /// `Vec<String>`. `None` when the method declares no return type (it returns
    /// `()`) or the signature is incomplete, so callers must not assume the
    /// value can be rendered.
    pub return_type: Option<String>,
}

/// Extract `pub fn <name>(&self[, ...])` method declarations from a script block.
///
/// Handles single-line and multi-line signatures; non-receiver methods such as
/// `pub fn new() -> Self` are ignored because they can never be event handlers
/// or getters.
pub fn extract_state_methods(script: &str) -> Vec<StateMethod> {
    let mut out: Vec<StateMethod> = Vec::new();
    let lines: Vec<&str> = script.lines().collect();
    let mut i = 0usize;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if let Some(rest) = strip_prefix_ignore(trimmed, "pub fn ") {
            let name = parse_ident(rest);
            if let Some(name) = name {
                // Consume the signature up to the closing ')' so multi-line
                // signatures are inspected too. Stop early when the first
                // line already contains a ')' — the common case.
                let mut sig = rest.to_string();
                let mut j = i;
                while !sig_has_closing_paren(&sig) && j + 1 < lines.len() {
                    j += 1;
                    sig.push('\n');
                    sig.push_str(lines[j]);
                }
                // Only receiver methods (`&self`) can be getters/handlers;
                // constructors like `pub fn new()` are not.
                if sig.contains("&self") {
                    let takes_payload = signature_takes_payload(&sig);
                    let return_type = signature_return_type(&sig);
                    out.push(StateMethod {
                        name,
                        takes_payload,
                        return_type,
                    });
                }
                i = j; // skip the consumed continuation lines
            }
        }
        i += 1;
    }
    out
}

/// One declared field of a component's `Props` struct.
///
/// The type is carried as the text the script wrote, e.g. `Vec<Row>`. Codegen
/// reads it to build the child's props with the type the child asked for; it is
/// not parsed into a type tree, because a `.vx` script is emitted verbatim and
/// may name types from anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropField {
    /// The field name as declared, e.g. `items`.
    pub name: String,
    /// The declared type as written, e.g. `String` or `Vec<Row>`.
    pub ty: String,
}

/// Extract field names from the `pub struct State { ... }` block.
pub fn extract_state_fields(script: &str) -> Vec<String> {
    extract_struct_fields(script, "State")
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

/// Extract the declared fields of the `pub struct Props { ... }` block, each with
/// the type the script wrote for it.
///
/// Empty when the script declares no `Props` struct. That emptiness is a signal
/// in its own right: a component with no `Props` struct has no typed channel a
/// `v-for` collection could be read out of, so nothing may claim one does.
pub fn extract_props_fields(script: &str) -> Vec<PropField> {
    extract_struct_fields(script, "Props")
        .into_iter()
        .map(|(name, ty)| PropField { name, ty })
        .collect()
}

/// Extract the field names declared by `pub struct {name} { ... }`, paired with
/// the type text that follows the `:`.
///
/// The struct name must end on a word boundary, so `pub struct PropsList` is not
/// read as a `Props` declaration. The scan itself is line-based and otherwise
/// unchanged from the `State` version it generalises: it tracks brace depth so
/// nested blocks inside the struct do not end it early, and it takes the text
/// before the first `:` on each line as the field name.
fn extract_struct_fields(script: &str, name: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut in_struct = false;
    let mut depth = 0usize;
    for line in script.lines() {
        let trimmed = line.trim();
        if !in_struct {
            if let Some(rest) = strip_prefix_ignore(trimmed, "pub struct ")
                .or_else(|| strip_prefix_ignore(trimmed, "struct "))
                && rest.starts_with(name)
                && rest[name.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| !is_ident_char(c))
            {
                in_struct = true;
                depth = count_char(trimmed, '{');
                // `struct Name;` unit struct — no fields.
                if trimmed.contains(";") && !trimmed.contains('{') {
                    in_struct = false;
                }
            }
            continue;
        }
        depth += count_char(trimmed, '{');
        if depth == 0 {
            in_struct = false;
            continue;
        }
        depth = depth.saturating_sub(count_char(trimmed, '}'));
        let stripped = trimmed
            .strip_prefix("pub")
            .map(str::trim_start)
            .unwrap_or(trimmed);
        if let Some(colon) = stripped.find(':') {
            let field = stripped[..colon].trim();
            let ty = stripped[colon + 1..].trim();
            if is_ident(field) {
                let ty = strip_type_comment(ty);
                // `pub todo: String,` — the separator is the field's, not the
                // type's, so the type it declared is `String`.
                let ty = ty.strip_suffix(',').unwrap_or(ty).trim_end();
                out.push((field.to_string(), ty.to_string()));
            }
        }
        if depth == 0 {
            in_struct = false;
        }
    }
    out
}

/// Drop a trailing line comment, and the separator after the type, from a type
/// written on a field line.
///
/// Only a `//` comment is handled: a `//` inside a string literal type is not a
/// thing a `.vx` script writes, and stopping short of full comment parsing is
/// what keeps this a line-based scan.
fn strip_type_comment(ty: &str) -> &str {
    match ty.find("//") {
        Some(at) => ty[..at].trim_end(),
        None => ty,
    }
}

/// Resolve a template key to the actual method name declared on `State`.
///
/// Convention: template keys map to bare getter names (`{{ title }}` → `title`).
/// For compatibility with the older `get_`/`is_`/`has_` convention we fall back
/// to the prefixed variant when the exact name does not exist.
pub fn resolve_method_name(methods: &[String], key: &str) -> String {
    if methods.iter().any(|m| m == key) {
        return key.to_string();
    }
    for prefix in ["get_", "is_", "has_"] {
        let candidate = format!("{prefix}{key}");
        if methods.iter().any(|m| m == &candidate) {
            return candidate;
        }
    }
    key.to_string()
}

/// Convenience: `methods` as a `Vec<String>` of just the names.
pub fn method_names(methods: &[StateMethod]) -> Vec<String> {
    methods.iter().map(|m| m.name.clone()).collect()
}

/// A handler collected from anywhere in the component tree, used to build the
/// root event dispatcher.
#[derive(Debug, Clone)]
pub struct TreeHandler {
    /// The handler name as it appears in a template (`@click="on_remove"`).
    pub name: String,
    /// The persistent State field that owns this handler (`Some("todos")` when
    /// the handler lives inside the `<Todos>` subtree), `None` for the root.
    pub owner: Option<String>,
    /// Whether the owning State method accepts an event payload parameter.
    pub takes_payload: bool,
}

fn strip_prefix_ignore<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    // Ignore leading whitespace when matching (line is already trim_start'ed).
    let p = s.find(prefix)?;
    Some(&s[p + prefix.len()..])
}

fn parse_ident(s: &str) -> Option<String> {
    let mut name = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            name.push(ch);
        } else {
            break;
        }
    }
    if name.is_empty() { None } else { Some(name) }
}

fn sig_has_closing_paren(sig: &str) -> bool {
    // A signature is complete once we've seen `( ... )`.
    if let Some(open) = sig.find('(') {
        return sig[open + 1..].contains(')');
    }
    false
}

fn signature_takes_payload(sig: &str) -> bool {
    let Some(open) = sig.find('(') else {
        return false;
    };
    let Some(close) = sig[open + 1..].find(')') else {
        return false;
    };
    let params = &sig[open + 1..open + 1 + close];
    let params = params.trim();
    if params.is_empty() {
        return false;
    }
    // A receiver-only method is `&self` or `&mut self` with nothing after the
    // comma. Any content past the first comma means an extra payload param.
    params
        .split_once(',')
        .map(|(_, rest)| !rest.trim().is_empty())
        .unwrap_or(false)
}

/// The return type declared after `->` in a method signature, or `None` when the
/// method declares none (it returns `()`).
///
/// Only the part of the signature before the body is inspected, so an arrow
/// appearing inside the body is never mistaken for a return type.
fn signature_return_type(sig: &str) -> Option<String> {
    let open = sig.find('(')?;
    let close = open + 1 + sig[open + 1..].find(')')?;
    let after = &sig[close + 1..];
    let head_end = after.find('{').unwrap_or(after.len());
    let head = &after[..head_end];
    let arrow = head.find("->")?;
    let ty = head[arrow + 2..].trim();
    if ty.is_empty() {
        None
    } else {
        Some(ty.to_string())
    }
}

fn count_char(s: &str, c: char) -> usize {
    s.chars().filter(|&ch| ch == c).count()
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// One character of a Rust identifier.
///
/// Deliberately the same predicate `codegen.rs` uses, not a narrower one: a
/// narrower predicate would turn a struct name that used to match into one that
/// does not, and `extract_state_fields` matching `pub struct StateExtra` is a
/// measured behaviour, not an accident to tidy away.
fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_methods_with_payload_flag() {
        let script = r#"
pub struct State {
    pub count: Cell<i32>,
}

impl State {
    pub fn new() -> Self {
        Self { count: Cell::new(0) }
    }
    pub fn title(&self) -> String { String::from("Velox App") }
    pub fn count(&self) -> i32 { self.count.get() }
    pub fn positive(&self) -> bool { self.count.get() > 0 }
    pub fn increment(&self) { self.count.set(self.count.get() + 1); }
    pub fn on_input(&self, payload: &str) { self.new_todo.set(payload.to_string()); }
}
"#;
        let methods = extract_state_methods(script);
        let names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"title"));
        assert!(names.contains(&"count"));
        assert!(names.contains(&"positive"));
        assert!(names.contains(&"increment"));
        assert!(names.contains(&"on_input"));
        assert!(!names.contains(&"new"));

        let on_input = methods.iter().find(|m| m.name == "on_input").unwrap();
        assert!(on_input.takes_payload);
        let increment = methods.iter().find(|m| m.name == "increment").unwrap();
        assert!(!increment.takes_payload);
    }

    #[test]
    fn extracts_return_types_from_signatures() {
        let script = r#"
impl State {
    pub fn title(&self) -> String { String::from("Velox App") }
    pub fn count(&self) -> i32 { 0 }
    pub fn positive(&self) -> bool { true }
    pub fn label(&self) -> &'static str { "x" }
    pub fn reset(&self) { self.count.set(0); }
    pub fn items(&self) -> Vec<String> { Vec::new() }
    pub fn maybe(
        &self,
        fallback: String,
    ) -> String {
        fallback
    }
}
"#;
        let methods = extract_state_methods(script);
        let return_type = |name: &str| {
            methods
                .iter()
                .find(|m| m.name == name)
                .unwrap()
                .return_type
                .as_deref()
        };

        assert_eq!(return_type("title"), Some("String"));
        assert_eq!(return_type("count"), Some("i32"));
        assert_eq!(return_type("positive"), Some("bool"));
        assert_eq!(return_type("label"), Some("&'static str"));
        assert_eq!(return_type("items"), Some("Vec<String>"));
        assert_eq!(return_type("maybe"), Some("String"));
        // No `->` clause means the method returns `()`.
        assert_eq!(return_type("reset"), None);
    }

    #[test]
    fn extracts_fields_from_state_struct() {
        let script = r#"
pub struct State {
    pub count: Cell<i32>,
    pub todos: Rc<Signal<Vec<Todo>>>,
}

impl State { ... }
"#;
        let fields = extract_state_fields(script);
        assert_eq!(fields, vec!["count", "todos"]);
    }

    #[test]
    fn resolves_prefixed_getters() {
        let methods = vec![
            "get_title".to_string(),
            "get_counter".to_string(),
            "is_positive".to_string(),
            "increment".to_string(),
        ];
        assert_eq!(resolve_method_name(&methods, "title"), "get_title");
        assert_eq!(resolve_method_name(&methods, "counter"), "get_counter");
        assert_eq!(resolve_method_name(&methods, "positive"), "is_positive");
        assert_eq!(resolve_method_name(&methods, "increment"), "increment");
        assert_eq!(resolve_method_name(&methods, "missing"), "missing");
    }
}
