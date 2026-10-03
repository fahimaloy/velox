use anyhow::{Context, Result};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

/// Walk up from the current directory looking for a Velox project root, defined
/// as the first ancestor directory containing a `src/App.vx` entry point (the
/// conventional project layout produced by `velox init`).
fn find_project_root() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let mut current = cwd.as_path();
    loop {
        if current.join("src").join("App.vx").exists() {
            return Some(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

/// Split a name into lowercase word tokens, handling separators (`_`, `-`,
/// spaces), digits, and camelCase boundaries. e.g. `"My Counter2"` ->
/// `["my", "counter", "2"]`.
fn split_words(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut prev_lower = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            // Start a new word on uppercase following lowercase (camelCase).
            if ch.is_uppercase() && prev_lower && !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            current.push(ch.to_ascii_lowercase());
            prev_lower = ch.is_lowercase();
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
            prev_lower = false;
        } else {
            prev_lower = false;
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Convert a user-supplied component name into a Rust-safe PascalCase struct
/// name (e.g. `"my-counter"` -> `"MyCounter"`). Falls back to `"Component"` if
/// the input contains no word characters.
fn to_pascal_case(name: &str) -> String {
    let mut out = String::new();
    for word in split_words(name) {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    if out.is_empty() {
        out = "Component".to_string();
    }
    out
}

/// Convert a PascalCase name into a kebab-case CSS class identifier
/// (e.g. `"MyCounter"` -> `"my-counter"`).
fn to_kebab_case(name: &str) -> String {
    let words = split_words(name);
    if words.is_empty() {
        return "component".to_string();
    }
    words.join("-")
}

/// The named slots a scaffolded component declares when the caller does not ask
/// for a specific set. The `default` slot is implicit — a `<slot>` with no
/// `name` attribute — so it is deliberately not listed here; it is generated
/// separately and always exists.
const DEFAULT_SLOTS: [&str; 1] = ["header"];

/// Normalise a user-supplied slot name to the form the SFC parser actually
/// registers.
///
/// `v-slot:footerBar` and `#footerBar` both fold onto `slot:footer-bar` (see
/// `normalize_directive_name` in `velox-sfc/src/template_parse.rs`), so a name
/// is kebab-cased *before* it is written into `<slot name="...">` and before it
/// is printed in the fill hint. Skipping that step would produce a scaffold
/// whose own hint tells the caller to author a name that resolves to a
/// different slot.
fn normalize_slot_name(name: &str) -> Result<String> {
    let mut dashed = String::with_capacity(name.len());
    for ch in name.chars() {
        // `_` and a camelCase hump both become a dash, matching the parser.
        if ch == '_' {
            dashed.push('-');
        } else if ch.is_ascii_uppercase() {
            dashed.push('-');
            dashed.push(ch.to_ascii_lowercase());
        } else {
            dashed.push(ch);
        }
    }

    // The parser collapses runs of dashes but keeps a leading one; a name that
    // starts with `-` is legal per the grammar (`slot_name` accepts `-`) and
    // unreadable in a template, so the surrounding dashes go too.
    let trimmed = dashed.trim_matches('-');
    let mut out = String::with_capacity(trimmed.len());
    let mut prev_dash = false;
    for ch in trimmed.chars() {
        if ch == '-' {
            if !prev_dash {
                out.push(ch);
                prev_dash = true;
            }
        } else {
            out.push(ch);
            prev_dash = false;
        }
    }

    if out.is_empty() {
        anyhow::bail!("invalid slot name '{name}': needs at least one letter or digit");
    }
    if !out.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        anyhow::bail!("invalid slot name '{name}': use letters, digits and '-' only");
    }
    Ok(out)
}

/// Parse a `--slots` value — a comma-separated list of slot names — into
/// normalised, de-duplicated names in the order they were written.
fn parse_slot_names(raw: &str) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let name = normalize_slot_name(part)?;
        if name == "default" {
            anyhow::bail!(
                "'default' is the implicit slot and is always generated; pick another name"
            );
        }
        if !out.contains(&name) {
            out.push(name);
        }
    }
    if out.is_empty() {
        anyhow::bail!("--slots needs at least one slot name, e.g. --slots header,footer");
    }
    Ok(out)
}

/// The named slots to scaffold: the caller's `--slots` list, or
/// [`DEFAULT_SLOTS`] when they named none.
fn resolve_slots(requested: Option<&str>) -> Result<Vec<String>> {
    match requested {
        Some(raw) => parse_slot_names(raw),
        None => Ok(DEFAULT_SLOTS.iter().map(|s| (*s).to_string()).collect()),
    }
}

/// Scaffold a new `.vx` component for the current project.
///
/// `slots` is the raw `--slots` value (`None` for the default set). Returns the
/// path of the created file on success.
pub fn add_component(name: &str, slots: Option<&str>) -> Result<PathBuf> {
    let root = find_project_root().ok_or_else(|| {
        anyhow::anyhow!(
            "not inside a Velox project (no src/App.vx found). Run `velox init <name>` first."
        )
    })?;

    let struct_name = to_pascal_case(name);
    let kebab = to_kebab_case(&struct_name);
    let slots = resolve_slots(slots)?;

    // Reject names whose PascalCase form is not a valid Rust identifier start
    // (e.g. `add component 1foo`).
    //
    // There is exactly one way to get here, and both halves of that statement
    // used to be written as if there were more:
    //   * `to_pascal_case` substitutes `"Component"` for an empty result, so it
    //     never returns an empty string — an `unwrap_or('_')` fallback could
    //     never fire, and it invented a character that was not in the name.
    //   * `split_words` emits lowercased ASCII alphanumerics and drops
    //     everything else, so the first character is always an ASCII letter or
    //     digit — an `|| first != '_'` allowance for a leading underscore could
    //     never fire either, and read as though `_foo` were a supported name.
    // What is left is the leading-digit case, which is what the message says.
    if !struct_name.starts_with(|c: char| c.is_ascii_alphabetic()) {
        anyhow::bail!("invalid component name '{name}': must start with a letter");
    }

    let components_dir = root.join("src").join("components");
    // `create_dir_all` happily succeeds when the directory it is asked to
    // create already exists as a symlink to one, which would scaffold the
    // component through the link and write into whatever it points at. Refuse
    // that outright: `src/components` is part of the project's own tree, and a
    // link there is far more likely to be an accident than an intent.
    if let Ok(meta) = fs::symlink_metadata(&components_dir)
        && meta.file_type().is_symlink()
    {
        anyhow::bail!(
            "{} is a symlink; refusing to scaffold through it",
            components_dir.display()
        );
    }
    fs::create_dir_all(&components_dir)
        .with_context(|| format!("create {}", components_dir.display()))?;

    // Match the project convention of PascalCase `.vx` filenames
    // (e.g. `TodoItem.vx`).
    let file_name = format!("{}.vx", struct_name);
    let path = components_dir.join(&file_name);

    let content = component_template(&struct_name, &kebab, &slots);
    // `create_new` IS the no-overwrite guard, in one atomic syscall. The pair
    // it replaces was wrong in three ways:
    //   * `Path::exists()` resolves symlinks, so a DANGLING symlink at `path`
    //     reported "does not exist", and `fs::write` -> `File::create`
    //     followed that link and created the file it pointed at. `create_new`
    //     fails on the link itself.
    //   * `exists()` then `write` is a check-then-act race: a file created in
    //     between was silently TRUNCATED, because `write` opens with
    //     `.truncate(true)`.
    //   * a live symlink was refused only because its target existed, so the
    //     refusal was a side effect of the target, not a property of the path.
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(f) => f,
        // Same message as before, so the refusal a user sees is unchanged —
        // only the mechanism that detects it is now correct.
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            anyhow::bail!("component already exists: {}", path.display());
        }
        Err(e) => return Err(e).with_context(|| format!("create {}", path.display())),
    };
    file.write_all(content.as_bytes())
        .with_context(|| format!("write {}", path.display()))?;

    println!("✅ Created component: {}", path.display());
    // The implicit slot is generated too, so it is reported first — it is the
    // one a caller reaches for without naming anything.
    let reported: Vec<&str> = std::iter::once("default")
        .chain(slots.iter().map(String::as_str))
        .collect();
    println!("   Slots: {}", reported.join(", "));
    if let Some(first) = slots.first() {
        println!(
            "   Fill one from a parent: <{struct_name}><template v-slot:{first}>…</template></{struct_name}>"
        );
        println!("   (shorthand: <{struct_name}><template #{first}>…</template></{struct_name}>)");
    }
    println!("   Import it from a parent component, e.g.:");
    println!("   import {struct_name} from './components/{file_name}'");

    Ok(path)
}

/// One `<slot name="...">` region per named slot, indented as children of the
/// component root.
///
/// The first carries the `{{ title }}` heading the scaffold has always emitted,
/// so a component with nothing slotted still shows its own name. The rest fall
/// back to a label naming the slot they are waiting for.
fn named_slot_blocks(kebab: &str, slots: &[String]) -> String {
    let mut out = String::new();
    for (i, slot) in slots.iter().enumerate() {
        let fallback = if i == 0 {
            "        <h3>{{ title }}</h3>\n".to_string()
        } else {
            format!("        <span class=\"{kebab}-empty\">{slot}</span>\n")
        };
        out.push_str(&format!(
            "    <div class=\"{kebab}-{slot}\">\n      <slot name=\"{slot}\">\n{fallback}      </slot>\n    </div>\n\n"
        ));
    }
    out
}

/// Render the default scaffold for a newly created component. The generated
/// component is self-contained (no imports) and is known to compile with the
/// same `main.rs` / codegen contract as the project templates.
///
/// It declares one `<slot name="...">` per entry in `slots` plus the implicit
/// `default` one. The child side is the `<slot>` element; the parent fills a
/// named slot with `<template v-slot:name>` or its `#name` shorthand, and
/// both spellings resolve to the same slot.
fn component_template(struct_name: &str, kebab: &str, slots: &[String]) -> String {
    let body = r#"<template>
  <div class="__KEBAB__">
__SLOTS__    <div class="__KEBAB__-body">
      <slot>
        <p>Slot content goes here.</p>
      </slot>
    </div>
  </div>
</template>

<script setup>
pub struct State {
    pub title: String,
}

impl State {
    pub fn new() -> Self {
        Self {
            title: String::from("__NAME__"),
        }
    }

    pub fn title(&self) -> String {
        self.title.clone()
    }
}
</script>

<style scoped>
.__KEBAB__ {
    padding: 16px;
    border-radius: 10px;
    background: #16213e;
    color: #e6edf3;
}

.__KEBAB__-body {
    margin-top: 8px;
}
</style>
"#;
    body.replace("__KEBAB__", kebab)
        .replace("__NAME__", struct_name)
        .replace("__SLOTS__", &named_slot_blocks(kebab, slots))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(raw: &str) -> Vec<String> {
        parse_slot_names(raw).expect("parse slots")
    }

    #[test]
    fn pascal_case_keeps_working() {
        assert_eq!(to_pascal_case("my-counter"), "MyCounter");
        assert_eq!(to_pascal_case("Todo Item"), "TodoItem");
        assert_eq!(to_pascal_case("My Counter2"), "MyCounter2");
        assert_eq!(to_pascal_case("!!!"), "Component");
    }

    #[test]
    fn kebab_case_keeps_working() {
        assert_eq!(to_kebab_case("MyCounter"), "my-counter");
        assert_eq!(to_kebab_case("Card"), "card");
    }

    #[test]
    fn slot_names_fold_to_the_parsers_kebab_case() {
        // These mirror `normalize_directive_name`: an authored `v-slot:fooBar`
        // or `#fooBar` registers as `foo-bar`, so the scaffold has to write and
        // report that form.
        assert_eq!(normalize_slot_name("header").unwrap(), "header");
        assert_eq!(normalize_slot_name("footerBar").unwrap(), "footer-bar");
        assert_eq!(normalize_slot_name("main_content").unwrap(), "main-content");
        assert_eq!(normalize_slot_name("Foo").unwrap(), "foo");
        assert_eq!(normalize_slot_name("__a--b__").unwrap(), "a-b");
    }

    #[test]
    fn invalid_slot_names_are_rejected() {
        assert!(normalize_slot_name("").is_err());
        assert!(normalize_slot_name("---").is_err());
        // `.` is not in the grammar's `slot_name` character set, so it would
        // silently fail to parse rather than name a slot.
        assert!(normalize_slot_name("a.b").is_err());
    }

    #[test]
    fn slot_lists_dedupe_and_keep_order() {
        assert_eq!(slots("header,footer"), ["header", "footer"]);
        assert_eq!(
            slots(" header , footerBar , header "),
            ["header", "footer-bar"]
        );
        assert_eq!(slots("a,,b"), ["a", "b"]);
    }

    #[test]
    fn default_slot_cannot_be_requested_explicitly() {
        let err = parse_slot_names("default").unwrap_err().to_string();
        assert!(err.contains("implicit slot"), "unexpected error: {err}");
        assert!(parse_slot_names(" , ").is_err());
    }

    #[test]
    fn no_requested_slots_falls_back_to_the_default_set() {
        assert_eq!(resolve_slots(None).unwrap(), DEFAULT_SLOTS);
    }

    #[test]
    fn template_declares_a_named_slot_and_the_implicit_one() {
        let t = component_template(
            "TodoCard",
            "todo-card",
            &["header".to_string(), "footer".to_string()],
        );
        // Long-form and shorthand authoring both land on `name`.
        assert!(t.contains("<slot name=\"header\">"), "{t}");
        assert!(t.contains("<slot name=\"footer\">"), "{t}");
        // The implicit slot is a bare `<slot>`, and must not be given a name.
        assert!(t.contains("<slot>\n"), "{t}");
        assert!(!t.contains("name=\"default\""), "{t}");
        // First named slot keeps the title heading as its fallback.
        assert!(t.contains("<h3>{{ title }}</h3>"), "{t}");
        // No placeholder survives the substitution.
        assert!(!t.contains("__KEBAB__") && !t.contains("__NAME__") && !t.contains("__SLOTS__"));
    }

    #[test]
    fn template_uses_the_kebab_class_and_pascal_title() {
        let t = component_template("TodoCard", "todo-card", &slots("header"));
        assert!(t.contains("<div class=\"todo-card\">"), "{t}");
        assert!(t.contains("String::from(\"TodoCard\")"), "{t}");
    }
}
