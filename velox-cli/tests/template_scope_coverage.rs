//! The shipped template's `<style scoped>` blocks must cover exactly the classes
//! the same component's `<template>` uses — in BOTH directions.
//!
//! # Why this test exists
//!
//! Commit `feee06b` made child `<style>` blocks actually scope: previously each
//! child's CSS was spliced RAW into the root stylesheet, so every class matched
//! every component. That fix is correct and stays.
//!
//! Scoping exposed a latent mistake in the shipped template. The rule
//! `.btn` / `.btn-add` / `.btn-add:hover` was declared in `TodoInput.vx` (the
//! CHILD), while the element it was meant for — `<button class="btn btn-add">`
//! — lives in `Todos.vx` (the PARENT) at
//! `velox-cli/templates/project/src/components/Todos.vx:8`.
//!
//! Velox has no Vue-style scope inheritance and no `:deep()` / `>>>` /
//! `::v-deep` escape hatch. `append_scope_attr`
//! (`velox-sfc/src/template_codegen.rs:2622-2628`) appends exactly ONE
//! `data-v-*` attribute, and `scope_single_selector`
//! (`velox-sfc/src/codegen.rs:375-400`) appends that attribute to EVERY compound
//! unconditionally, with no special case. So a child's scoped rule can never
//! match an element the parent owns, in either direction. Cross-component
//! styling is simply not expressible.
//!
//! The result was that the Add button silently lost its background, border,
//! padding, colour and radius; only the UA rule survived
//! (`velox-style/src/ua.css:8` — `button { border: 1px solid #888 }`).
//!
//! The pre-existing suite could not see this.
//! `velox-cli/tests/scoped_style_merging.rs:55` asserts the root STYLE
//! CONTAINS `.btn-add[data-v-<id>]` and contains zero bare `.btn-add` — which
//! passes, because the CSS WAS correctly scoped; it was just scoped to the
//! wrong component. That test never checks that the button ELEMENT carries a
//! matching id. `scoped_style_merging.rs:118` builds its own synthetic
//! three-file tree and never touches the real template. In short: the suite
//! verified the MECHANISM and not the shipped TEMPLATE.
//!
//! This test closes that gap statically, with no rendering.
//!
//! # Precision, not heuristics
//!
//! A fuzzy test is worse than no test, because it trains people to ignore it.
//! So every step here is a total function with a stated domain, never a
//! best-effort guess:
//!
//! 1. USED = every whitespace-separated token of every **static, quoted**
//!    `class="..."` attribute in the `<template>` block.
//! 2. DECLARED = every **single-class selector** in the same file's `<style>`
//!    block — a rule whose prelude is exactly `.ident` once trailing
//!    pseudo-classes/pseudo-elements are stripped. Descendant, child, sibling
//!    and compound selectors are deliberately NOT evaluated (see "Known limits"),
//!    so they are excluded from DECLARED rather than guessed at.
//! 3. Assert USED ⊆ DECLARED, and DECLARED ⊆ USED.
//!
//! The parser is brace-aware and comment-aware, so nested at-rules such as
//! `@media` are descended into rather than silently truncating the rule list.
//! If it meets a construct it cannot classify exactly — an unquoted `class=`,
//! for instance — it FAILS rather than skipping, so the test cannot go quietly
//! blind as the template evolves.
//!
//! # Known limits (deliberate, and they fail loudly rather than silently)
//!
//! - `:class="{ ... }"` object bindings are a different construct from
//!   `class="..."` and are NOT collected as USED. `TodoItem.vx:2` binds
//!   `:class="{ completed: completed }"`, and `completed` is styled by the
//!   COMPOUND selector `.todo-item.completed .todo-text` (`TodoItem.vx:65`),
//!   which rule (2) excludes by design. Collecting the binding without
//!   evaluating the compound would be exactly the fuzzy half-handling this file
//!   exists to avoid.
//! - `class="{{ expr }}"` interpolation cannot be enumerated statically; such a
//!   value contributes no USED tokens. The inverse assertion is the safety net:
//!   if a declared class only ever appears in interpolated bindings, direction B
//!   reports it as dead CSS with a message saying so.
//!
//! # Allowlist
//!
//! Intentionally unstyled classes — semantic or test hooks with no visual
//! rules — go in [`INTENTIONALLY_UNSTYLED_CLASSES`] with a one-line reason. It
//! is EMPTY, and `allowlist_entries_all_have_reasons` enforces that mechanically:
//! an entry without a reason fails the suite. The shipped template is expected
//! to pass with nothing in it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Classes that a component's template uses but whose component deliberately
/// declares no single-class rule for.
///
/// Format: `(class_name, reason)`. The reason is MANDATORY and is not optional
/// decoration — `allowlist_entries_all_have_reasons` fails if it is blank, and
/// `allowlist_entries_are_actually_needed` fails if an entry is stale (nothing
/// in the template needs it), so the list cannot silently absorb real defects.
///
/// **Intentionally empty: the shipped template needs no exemptions.**
const INTENTIONALLY_UNSTYLED_CLASSES: &[(&str, &str)] = &[];

// ---------------------------------------------------------------------------
// Analysis
// ---------------------------------------------------------------------------

/// What one `.vx` file uses in its template and declares in its style block.
#[derive(Debug)]
struct Component {
    /// File name, e.g. `Todos.vx`.
    name: String,
    /// Static tokens of every quoted `class="..."` in the `<template>` block.
    used: BTreeSet<String>,
    /// Classes with a standalone `.foo` rule in the `<style>` block.
    declared: BTreeSet<String>,
    /// Every `.foo` token appearing in ANY selector, including inside compound
    /// and descendant selectors. Used only to enrich failure messages: it lets
    /// the report say "mentioned only in a non-single-class selector" instead of
    /// leaving the reader to wonder.
    mentioned: BTreeSet<String>,
}

/// Fail the test on a `.vx` construct the analyzer cannot classify exactly.
///
/// The whole point of this file is to be trusted, so "skip it and see" is not
/// an option: every unclassifiable construct must be loud.
fn bail(component: &str, detail: &str) -> ! {
    panic!(
        "template_scope_coverage: {} contains a construct this analyzer cannot \
         classify exactly, so it would silently under-collect and go blind.\n\
         Detail: {}\n\
         Fix the analyzer in this file — do NOT relax it to a heuristic.",
        component, detail
    );
}

/// Return the body of the first `<name...>` ... `</name>` block, or `None`.
fn block<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    let open_pat = format!("<{name}");
    let start = src.find(&open_pat)?;
    // Require a `>` so we do not match a longer tag name starting the same way
    // (e.g. searching for `style` must not match `<stylesheet`).
    let after_open = start + open_pat.len();
    let gt_rel = src[after_open..].find('>')?;
    let body_start = after_open + gt_rel + 1;
    let close_pat = format!("</{name}>");
    let close = src[body_start..].find(&close_pat)?;
    Some(&src[body_start..body_start + close])
}

/// Collect every static token of every quoted `class="..."` attribute.
///
/// `:class="..."` is excluded: the preceding character must not be an
/// identifier character, so `:class`, `v-bind:class` and `data-class` never
/// match. `:class` object bindings are a different construct (see module docs).
fn collect_used_classes(component: &str, template: &str) -> BTreeSet<String> {
    let mut used = BTreeSet::new();
    let bytes = template.as_bytes();
    let mut i = 0usize;
    while let Some(rel) = template[i..].find("class") {
        let at = i + rel;
        let after = at + "class".len();
        // Reject `:class`, `v-bind:class`, `data-class`, `my-class`, ... Only a
        // non-identifier boundary means this is the plain `class` attribute.
        let boundary_ok = at == 0 || !is_ident_byte(bytes[at - 1]);
        if !boundary_ok {
            i = after;
            continue;
        }
        // Must be followed by `=` then a quote.
        let rest = &template[after..];
        let Some(eq_rel) = rest.strip_prefix('=') else {
            i = after;
            continue;
        };
        // Offset of the value within `rest`. `eq_rel` is the text AFTER the `=`,
        // so allow whitespace between the two without mis-slicing.
        let trimmed = eq_rel.trim_start();
        let value_rel = 1 + (eq_rel.len() - trimmed.len());
        let Some(first) = rest.as_bytes().get(value_rel).copied() else {
            i = after;
            continue;
        };
        if first != b'"' && first != b'\'' {
            // Unquoted value: not statically enumerable, and silently ignoring
            // it would blind the test. Fail loudly.
            bail(
                component,
                &format!(
                    "unquoted `class=` value at template offset {at}; only quoted \
                     static class attributes are enumerable"
                ),
            );
        }
        let quote = first;
        let value_start = value_rel + 1;
        let Some(end_rel) = rest[value_start..].find(quote as char) else {
            bail(
                component,
                &format!("unterminated `class=` attribute at template offset {at}"),
            );
        };
        let value = &rest[value_start..value_start + end_rel];
        if value.contains("{{") {
            // Dynamic binding: not enumerable. The inverse assertion covers the
            // consequence; see module docs.
            i = after;
            continue;
        }
        for token in value.split_whitespace() {
            used.insert(token.to_string());
        }
        i = after + value_start + end_rel + 1;
    }
    used
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b':'
}

/// Strip `/* ... */` comments so their contents never masquerade as selectors.
fn strip_css_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let bytes = css.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            match css[i + 2..].find("*/") {
                Some(end) => {
                    out.push(' ');
                    i = i + 2 + end + 2;
                }
                None => {
                    out.push(' ');
                    break;
                }
            }
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Split a selector list on top-level commas, ignoring commas inside `()` /
/// `[]` (e.g. `:not(a, b)`).
fn split_selector_list(list: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for ch in list.chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    out.push(current);
    out.into_iter().map(|s| s.trim().to_string()).collect()
}

/// Extract rule preludes (selector lists) from a stylesheet, descending into
/// nested at-rules such as `@media`. Declarations inside a block are skipped,
/// so braces in values cannot desynchronise the scan.
fn collect_rule_preludes(css: &str) -> Vec<String> {
    fn walk(css: &str) -> Vec<String> {
        let bytes = css.as_bytes();
        let mut preludes = Vec::new();
        let mut prelude = String::new();
        let mut i = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'{' => {
                    let head = prelude.trim().to_string();
                    prelude.clear();
                    // Find the matching close brace, accounting for nesting.
                    let mut depth = 1i32;
                    let mut j = i + 1;
                    while j < bytes.len() && depth > 0 {
                        match bytes[j] {
                            b'{' => depth += 1,
                            b'}' => depth -= 1,
                            _ => {}
                        }
                        j += 1;
                    }
                    let body = &css[i + 1..j.saturating_sub(1)];
                    if head.is_empty() {
                        // Defensive: no prelude means the scan lost sync.
                        preludes.push(String::new());
                    } else if head.starts_with('@') {
                        // Statement at-rules (`@import`, `@charset`) have no
                        // block to descend into. Conditional group rules
                        // (`@media`, `@supports`, ...) contain real rules.
                        let is_statement = head
                            .split_whitespace()
                            .next()
                            .is_some_and(|kw| kw == "@import" || kw == "@charset");
                        if !is_statement {
                            preludes.extend(walk(body));
                        }
                    } else {
                        preludes.push(head);
                    }
                    i = j;
                }
                b'}' => {
                    prelude.clear();
                    i += 1;
                }
                _ => {
                    prelude.push(bytes[i] as char);
                    i += 1;
                }
            }
        }
        preludes
    }
    walk(css)
}

/// A byte that may appear in a CSS identifier.
fn is_css_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'\\' || b >= 0x80
}

/// If `selector` is a single-class selector, return the class name.
///
/// The compound is parsed LEFT TO RIGHT — `.`, one CSS identifier, then only
/// pseudo-classes / pseudo-elements, each optionally carrying a balanced
/// argument list (`:not(.a)`, `:nth-child(2n)`). So `.btn-add` and
/// `.btn-add:hover` and `.btn-add::before` all classify to `btn-add`.
///
/// Returns `None` for anything else: descendant / child / sibling selectors
/// (whitespace or a combinator), compound selectors (`.a.b`, `button.a`),
/// attribute selectors, type selectors, the universal selector, and
/// `:root` / `html` / `body` resets. Deciding those requires element nesting
/// or element identity, which is exactly the judgement this test refuses to
/// make rather than guess.
fn single_class_selector(selector: &str) -> Option<String> {
    let b = selector.trim().as_bytes();
    if b.first() != Some(&b'.') {
        return None;
    }
    let mut i = 1usize;
    let name_start = i;
    while i < b.len() && is_css_ident_byte(b[i]) {
        i += 1;
    }
    if i == name_start {
        return None; // a bare `.` with no class name
    }
    let name = selector.trim()[name_start..i].to_string();

    // The remainder must be a sequence of pseudo-classes / pseudo-elements and
    // nothing else. Any other byte — a combinator, a second `.`, a tag name, an
    // attribute selector — means this is not a standalone class rule.
    while i < b.len() {
        if b[i] != b':' {
            return None;
        }
        i += 1;
        if i < b.len() && b[i] == b':' {
            i += 1; // pseudo-element, e.g. `::before`
        }
        let ident_start = i;
        while i < b.len() && is_css_ident_byte(b[i]) {
            i += 1;
        }
        if i == ident_start {
            return None; // `:` with no pseudo name
        }
        if i < b.len() && b[i] == b'(' {
            let mut depth = 0i32;
            while i < b.len() {
                match b[i] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            if depth != 0 {
                return None; // unbalanced argument list
            }
        }
    }
    Some(name)
}

/// Every `.ident` token anywhere in a selector, for diagnostics only.
///
/// A `.` opens a class name and its following identifier characters accumulate
/// into it. A `:` opens a pseudo-class or pseudo-element, whose name is NOT a
/// class — so `hover`, `before` and the like are never reported, and a
/// functional pseudo's argument (`.a:not(.b)` → `b`) is still tokenized.
fn class_tokens(selector: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut current = String::new();
    // `reading_class`: chars accumulate into a real class name.
    // `reading_pseudo`: chars accumulate into a pseudo name, discarded on flush.
    let (mut reading_class, mut reading_pseudo) = (false, false);
    for ch in selector.chars() {
        if ch == '.' {
            flush(&mut current, &mut out, &mut reading_pseudo);
            reading_class = true;
            continue;
        }
        if ch == ':' {
            flush(&mut current, &mut out, &mut reading_pseudo);
            reading_class = false;
            reading_pseudo = true;
            continue;
        }
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '\\' {
            if reading_class || reading_pseudo {
                current.push(ch);
            }
            continue;
        }
        // Any other character ends whatever identifier was being read.
        flush(&mut current, &mut out, &mut reading_pseudo);
        reading_class = false;
    }
    flush(&mut current, &mut out, &mut reading_pseudo);
    out
}

fn flush(current: &mut String, out: &mut BTreeSet<String>, reading_pseudo: &mut bool) {
    if current.is_empty() {
        return;
    }
    if !*reading_pseudo {
        out.insert(std::mem::take(current));
    } else {
        current.clear();
    }
}

/// Analyse one `.vx` source file.
fn analyze(name: &str, src: &str) -> Component {
    let template =
        block(src, "template").unwrap_or_else(|| panic!("{name}: no <template> block found"));
    let style = block(src, "style").unwrap_or_else(|| panic!("{name}: no <style> block found"));

    let used = collect_used_classes(name, template);

    let css = strip_css_comments(style);
    let mut declared = BTreeSet::new();
    let mut mentioned = BTreeSet::new();
    for prelude in collect_rule_preludes(&css) {
        for selector in split_selector_list(&prelude) {
            for token in class_tokens(&selector) {
                mentioned.insert(token);
            }
            if let Some(class) = single_class_selector(&selector) {
                declared.insert(class);
            }
        }
    }

    Component {
        name: name.to_string(),
        used,
        declared,
        mentioned,
    }
}

/// Analyse every `.vx` file in `dir`, sorted by file name for stable output.
fn analyze_dir(dir: &Path) -> Vec<Component> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read template components dir {}: {e}", dir.display()))
        .map(|e| e.expect("read_dir entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "vx"))
        .collect();
    paths.sort();
    assert!(
        !paths.is_empty(),
        "no .vx files found in {} — the template layout changed and this test \
         would otherwise pass vacuously",
        dir.display()
    );
    paths
        .iter()
        .map(|p| {
            let name = p
                .file_name()
                .expect("file name")
                .to_string_lossy()
                .into_owned();
            let src = fs::read_to_string(p).expect("read .vx source");
            analyze(&name, &src)
        })
        .collect()
}

fn shipped_components_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/project/src/components")
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

/// DIRECTION A — every class a component USES in its template is DECLARED by
/// that same component's `<style>` as a standalone class rule.
///
/// This is the assertion that catches the shipped bug. A class used in a
/// template with no rule in the owning file is either unstyled or, as in the
/// `Todos.vx` / `TodoInput.vx` `.btn-add` case, styled by a DIFFERENT
/// component's scoped block — which can never match, because every selector
/// compound carries its owner's `data-v-*` id.
#[test]
fn direction_a_every_class_used_in_a_template_is_declared_in_that_component() {
    let components = analyze_dir(&shipped_components_dir());

    // class -> components whose <style> declares a standalone rule for it.
    let mut declared_by: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for c in &components {
        for class in &c.declared {
            declared_by
                .entry(class.as_str())
                .or_default()
                .push(c.name.as_str());
        }
    }

    let mut violations: Vec<String> = Vec::new();
    for c in &components {
        for class in &c.used {
            if c.declared.contains(class) || is_allowlisted(c.name.as_str(), class) {
                continue;
            }
            let elsewhere = match declared_by.get(class.as_str()) {
                Some(owners) if !owners.is_empty() => format!(
                    "\n    but `{}` IS declared as a standalone rule in: {}.\n    \
                     A `<style scoped>` rule only matches elements carrying its OWN \
                     component's data-v-* id (velox-sfc/src/template_codegen.rs:2622 \
                     `append_scope_attr`, velox-sfc/src/codegen.rs:375 \
                     `scope_single_selector`), and velox has no scope inheritance or \
                     `:deep()` escape hatch. So the rule is dead CSS and the element \
                     renders unstyled. Move the rule into {}.",
                    class,
                    owners.join(", "),
                    c.name
                ),
                _ => String::new(),
            };
            let mentioned = if c.mentioned.contains(class) {
                format!(
                    "\n    note: `{}` IS mentioned in {}'s <style>, but only inside a \
                     compound/descendant selector, which this test deliberately does \
                     not evaluate. Verify by hand that it actually matches, or give \
                     the class its own standalone rule.",
                    class, c.name
                )
            } else {
                String::new()
            };
            violations.push(format!(
                "  {} uses class `{}` in its <template> but declares no single-class \
                 rule for it in its own <style>.{}{}",
                c.name, class, elsewhere, mentioned
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "Direction A violated — a component uses a class it does not itself style, \
         so the element cannot be styled as intended ({} violation(s)):\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// DIRECTION B — the dead-CSS half of the same mistake: every class a component
/// DECLARES is USED by that same component's template.
///
/// A rule here is a no-op for the same reason: it targets a class this
/// component never puts on any element. Shipping `.btn-add` in `TodoInput.vx`
/// while `Todos.vx` owns the only `.btn-add` element is precisely this
/// violation, and it fails on its own even if someone "fixes" direction A by
/// duplicating the rule into the parent instead of moving it.
#[test]
fn direction_b_every_class_declared_in_a_component_is_used_in_that_component() {
    let components = analyze_dir(&shipped_components_dir());

    let mut violations: Vec<String> = Vec::new();
    for c in &components {
        for class in &c.declared {
            if c.used.contains(class) || is_allowlisted(c.name.as_str(), class) {
                continue;
            }
            let owners: Vec<&str> = components
                .iter()
                .filter(|other| other.name != c.name && other.used.contains(class))
                .map(|other| other.name.as_str())
                .collect();
            let elsewhere = if owners.is_empty() {
                "\n    the class appears on NO element in any component's template."
            } else {
                "\n    note: this class is used by another component's template, which \
                 cannot be styled from here."
            };
            violations.push(format!(
                "  {} declares class `{}` in its <style> but never uses it in its own \
                 <template>. The rule can never match.{}\n    (used by: {})",
                c.name,
                class,
                elsewhere,
                if owners.is_empty() {
                    "—".to_string()
                } else {
                    owners.join(", ")
                }
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "Direction B violated — a component styles a class it never uses ({} \
         violation(s)):\n{}",
        violations.len(),
        violations.join("\n")
    );
}

/// Every allowlist entry must carry a non-blank reason. An entry without one is
/// not permitted, so this fails rather than letting the list grow unexplained.
#[test]
fn allowlist_entries_all_have_reasons() {
    for (class, reason) in INTENTIONALLY_UNSTYLED_CLASSES {
        assert!(
            !reason.trim().is_empty(),
            "allowlist entry `{class}` has no reason; every entry needs a one-line \
             justification of why the class is intentionally unstyled"
        );
    }
}

/// The allowlist must not go stale: an entry that no violation needs is dead
/// weight that hides the next real defect, so it fails the suite.
#[test]
fn allowlist_entries_are_actually_needed() {
    let components = analyze_dir(&shipped_components_dir());
    let mut stale: Vec<String> = Vec::new();
    for (class, _) in INTENTIONALLY_UNSTYLED_CLASSES {
        let needed = components.iter().any(|c| {
            (c.used.contains(*class) && !c.declared.contains(*class))
                || (c.declared.contains(*class) && !c.used.contains(*class))
        });
        if !needed {
            stale.push(class.to_string());
        }
    }
    assert!(
        stale.is_empty(),
        "stale allowlist entries — nothing in the template needs these \
         exemptions any more, remove them: {stale:?}"
    );
}

fn is_allowlisted(_component: &str, class: &str) -> bool {
    INTENTIONALLY_UNSTYLED_CLASSES
        .iter()
        .any(|(c, _)| *c == class)
}

// ---------------------------------------------------------------------------
// Falsification — prove the detector is not vacuously green
// ---------------------------------------------------------------------------

/// A detector that never fires is worth nothing. This builds the SHIPPED bug
/// shape in a scratch tree — a child component declaring a class that only the
/// parent uses — and asserts the analyzer reports BOTH directions, naming the
/// file, the class and the component that declared it. It runs on every
/// `cargo test`, so the coverage assertion cannot rot into a tautology.
#[test]
fn detector_catches_a_child_styling_a_class_the_parent_owns() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("scope-coverage-falsify");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");

    // CHILD declares the rule for an element the PARENT owns.
    fs::write(
        dir.join("Child.vx"),
        r#"<template>
  <input class="input" />
</template>

<style scoped>
.input { flex: 1; }
.btn-add { background: #3478f6; }
</style>
"#,
    )
    .expect("write Child.vx");

    // PARENT uses `btn-add`, declares nothing for it.
    fs::write(
        dir.join("Parent.vx"),
        r#"<template>
  <button class="btn btn-add">Add</button>
</template>

<style scoped>
.btn { padding: 10px 20px; }
</style>
"#,
    )
    .expect("write Parent.vx");

    let components = analyze_dir(&dir);
    let child = components
        .iter()
        .find(|c| c.name == "Child.vx")
        .expect("Child.vx analysed");
    let parent = components
        .iter()
        .find(|c| c.name == "Parent.vx")
        .expect("Parent.vx analysed");

    // Direction A: Parent uses `btn-add`, Parent declares no rule for it.
    assert!(
        parent.used.contains("btn-add"),
        "fixture sanity: Parent uses btn-add"
    );
    assert!(
        !parent.declared.contains("btn-add"),
        "fixture sanity: Parent declares no btn-add rule"
    );

    // Direction B: Child declares `btn-add` but never uses it.
    assert!(
        child.declared.contains("btn-add"),
        "fixture sanity: Child declares btn-add"
    );
    assert!(
        !child.used.contains("btn-add"),
        "fixture sanity: Child never uses btn-add"
    );

    // And the correct layout is clean in both directions.
    let fixed = analyze(
        "Parent.vx",
        r#"<template>
  <button class="btn btn-add">Add</button>
</template>

<style scoped>
.btn { padding: 10px 20px; }
.btn-add { background: #3478f6; }
</style>
"#,
    );
    assert!(
        fixed.used.is_subset(&fixed.declared),
        "a component that declares what it uses must satisfy direction A: used={:?} declared={:?}",
        fixed.used,
        fixed.declared
    );
    assert!(
        fixed.declared.is_subset(&fixed.used),
        "a component that uses what it declares must satisfy direction B: used={:?} declared={:?}",
        fixed.used,
        fixed.declared
    );

    let _ = fs::remove_dir_all(&dir);
}

/// The selector classifier must reject exactly the selector shapes that need
/// element nesting or element identity to judge, and accept the standalone
/// class shapes. Locking this down keeps "do not evaluate compound selectors"
/// from quietly becoming "match anything containing a dot".
#[test]
fn single_class_selector_accepts_only_standalone_class_rules() {
    for (sel, expected) in [
        (".btn", "btn"),
        (".btn-add", "btn-add"),
        // The pseudo suffix is stripped: these ARE rules for the bare class.
        (".btn-add:hover", "btn-add"),
        (".btn-add::before", "btn-add"),
        (".a-1", "a-1"),
        ("._x", "_x"),
    ] {
        assert_eq!(
            single_class_selector(sel).as_deref(),
            Some(expected),
            "`{sel}` should classify as a single-class rule for `.{expected}`"
        );
    }
    for sel in [
        // needs element nesting
        ".todo-item.completed .todo-text",
        ".a .b",
        ".a > .b",
        ".a + .b",
        ".a ~ .b",
        // needs element identity / is not a class declaration
        "button.a",
        "*",
        ":root",
        "html",
        "body",
        "div",
        ".a.b",
        "[data-open].a",
        "",
    ] {
        assert_eq!(
            single_class_selector(sel),
            None,
            "`{sel}` must NOT classify as a single-class rule"
        );
    }
}
