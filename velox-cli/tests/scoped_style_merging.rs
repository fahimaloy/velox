//! Regression tests for `<style scoped>` actually scoping in a scaffolded app.
//!
//! Defect: a child component's `<style scoped>` block was collected RAW and
//! spliced into the root's `STYLE` constant, so its selectors stayed bare and
//! leaked across component boundaries. A `.btn-add` rule, declared in one
//! component, styled the `<button class="btn btn-add">` that a different
//! component owns.
//!
//! Both halves of the contract are therefore stated as a MECHANISM — "every
//! component's rules carry THAT component's id, and no bare selector survives"
//! — and derived from the template's own `.vx` sources. Pinning a rule to one
//! file name instead would make this test go red the moment a rule is moved to
//! the component that owns its element, while the scoping stayed correct.
//!
//! Both layers below drive the REAL `build_vx` assembly and read the emitted
//! `STYLE` constant. Neither calls `scope_css` itself to produce the sheet it
//! inspects — a test that builds the expected artifact with the same helper the
//! production code uses would pass even with the defect present, and would
//! therefore be evidence of nothing.
//!
//! Layer A is the shipped-template regression. Layer B is the invariant that
//! actually matters — element A must not pick up component B's rule — proven on
//! the styled vnode and then in real pixels via `render_vnode_to_rgba`, which
//! runs `compute_layout`. (`render_vnode_to_raster_png` skips the layout pass
//! and false-greens, so it is deliberately not used.)

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use velox_dom::layout::compute_layout;
use velox_dom::{Props, VNode, h};
use velox_style::{Stylesheet, apply_with_cascade};

/// Extract the body of a generated `pub const STYLE: &str = r#" ... "#;`.
fn style_const_body(stub: &str) -> String {
    let start = stub.find("pub const STYLE").expect("stub has STYLE const");
    let open = stub[start..].find("r#\"").expect("STYLE is a raw string") + start + 3;
    let rest = &stub[open..];
    let end = rest.find("\"#;").expect("STYLE raw string is terminated");
    rest[..end].to_string()
}

fn scratch(tag: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../target/velox-cli-tests")
        .join(format!("scoped-{}-{tag}", std::process::id()))
}

/// Write a `.vx` file into `dir`.
fn write_vx(dir: &Path, file: &str, body: &str) -> PathBuf {
    fs::create_dir_all(dir).expect("create fixture dir");
    let p = dir.join(file);
    fs::write(&p, body).expect("write fixture");
    p
}

/// The `<style>` block of one `.vx` file, together with the id its selectors
/// must carry. Both halves are derived from the real file — nothing here is a
/// hand-typed literal, so this pins the mechanism rather than a snapshot.
struct ScopedComponent {
    /// File stem, e.g. `TodoInput` — the string `generate_scope_id` hashes.
    name: String,
    /// `data-v-<hash>` this component's selectors must carry.
    id: String,
    /// Every class declared by a standalone `.foo` rule in its `<style>`.
    declared: BTreeSet<String>,
}

/// Every `.vx` file under `dir`, recursively, sorted for stable failure output.
fn vx_files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(vx_files_under(&path));
        } else if path.extension().is_some_and(|e| e == "vx") {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// Collect the selector prelude of every style rule in `css`, descending into
/// at-rule containers (`@media`) and never descending into declaration bodies,
/// so a brace inside a value cannot desynchronise the scan.
fn rule_preludes(css: &str) -> Vec<String> {
    fn walk(css: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut prelude = String::new();
        let mut chars = css.chars();
        while let Some(c) = chars.next() {
            match c {
                '{' => {
                    let head = prelude.trim().to_string();
                    prelude.clear();
                    // Consume the balanced body verbatim.
                    let mut depth = 1usize;
                    let mut body = String::new();
                    for inner in chars.by_ref() {
                        match inner {
                            '{' => {
                                depth += 1;
                                body.push(inner);
                            }
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                                body.push(inner);
                            }
                            _ => body.push(inner),
                        }
                    }
                    if head.starts_with('@') {
                        // `@import` / `@charset` hold no rules; every other
                        // at-rule is a container whose rules do get scoped.
                        let is_statement = head
                            .split_whitespace()
                            .next()
                            .is_some_and(|kw| kw == "@import" || kw == "@charset");
                        if !is_statement {
                            out.extend(walk(&body));
                        }
                    } else {
                        out.push(head);
                    }
                }
                '}' => prelude.clear(),
                _ => prelude.push(c),
            }
        }
        out
    }
    walk(css)
}

/// Split a selector list on top-level commas, ignoring commas nested inside
/// `()` / `[]` (e.g. `:not(a, b)`).
fn split_selector_list(list: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut depth, mut current) = (0i32, String::new());
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
            ',' if depth == 0 => out.push(std::mem::take(&mut current)),
            _ => current.push(ch),
        }
    }
    out.push(current);
    out.into_iter().map(|s| s.trim().to_string()).collect()
}

fn is_css_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'\\') || b >= 0x80
}

/// If `sel` is a standalone class selector return the class name.
///
/// `.btn` -> `btn`; pseudo-classes and pseudo-elements are stripped, so
/// `.btn-add:hover` and `.btn-add::before` also classify to `btn-add`.
///
/// Returns `None` for anything that would need element nesting or element
/// identity to judge — descendant, child, sibling and compound selectors, type
/// and attribute selectors, and functional pseudo-classes like `:not(.a)`.
/// Declining to guess is the point: a fuzzy classifier would make this whole
/// test a guess.
fn standalone_class(sel: &str) -> Option<String> {
    let s = sel.trim();
    let b = s.as_bytes();
    if b.first() != Some(&b'.') {
        return None;
    }
    let mut i = 1usize;
    while i < b.len() && is_css_ident_byte(b[i]) {
        i += 1;
    }
    if i == 1 {
        return None; // a bare `.` with no class name
    }
    let name = s[1..i].to_string();
    // Only a run of pseudo-classes / pseudo-elements may follow. Anything else
    // — a combinator, a second `.`, a tag, `[attr]` — is out of scope.
    while i < b.len() {
        if b[i] != b':' {
            return None;
        }
        i += 1;
        if i < b.len() && b[i] == b':' {
            i += 1; // pseudo-element, e.g. `::before`
        }
        let name_start = i;
        while i < b.len() && is_css_ident_byte(b[i]) {
            i += 1;
        }
        if i == name_start || (i < b.len() && b[i] == b'(') {
            return None; // no pseudo name, or a functional pseudo we decline
        }
    }
    Some(name)
}

/// Read one `.vx` file and extract its standalone `<style scoped>` class rules.
fn read_scoped_component(path: &Path) -> ScopedComponent {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("vx file has a UTF-8 stem")
        .to_string();
    let src = fs::read_to_string(path).expect("read .vx source");
    let sfc = velox_sfc::parse_sfc(&src).expect("parse .vx source");

    let mut declared = BTreeSet::new();
    if let Some(style) = &sfc.style
        && velox_sfc::is_scoped(style)
    {
        for prelude in rule_preludes(&style.content) {
            for sel in split_selector_list(&prelude) {
                if let Some(class) = standalone_class(&sel) {
                    declared.insert(class);
                }
            }
        }
    }

    ScopedComponent {
        id: velox_sfc::generate_scope_id(&name),
        name,
        declared,
    }
}

/// LAYER A — assembly regression over the shipped project template.
///
/// The root's merged `STYLE` must carry every component's rules scoped to THAT
/// component's own id, and ZERO bare selectors.
///
/// Both halves of that contract are derived from the template's own `.vx`
/// sources rather than hardcoded. A previous version of this test asserted
/// `.btn-add[<id of TodoInput>]`, pinning the rule to one specific file. That
/// was correct when `.btn-add` was declared in `TodoInput.vx`, and stopped being
/// true the moment the rule moved to `Todos.vx` — the component that actually
/// owns `<button class="btn btn-add">`. The scoping was correct throughout; only
/// the test's expectation had gone stale. Deriving the owner means this test
/// keeps testing the MECHANISM after the template is reorganised, and stays
/// green when a rule is moved to the component whose element it styles.
#[test]
fn layer_a_child_rules_are_scoped_in_root_stylesheet() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("templates/project/src");
    let input = src_dir.join("App.vx");
    let out_dir = scratch("a");
    let _ = fs::remove_dir_all(&out_dir);

    velox_cli::commands::build::build_vx(&input, Some(out_dir.as_path()))
        .expect("build_vx over the shipped template");

    let stub = fs::read_to_string(out_dir.join("app.rs")).expect("read generated app.rs");
    let body = style_const_body(&stub);

    // The real template, root and children alike, read straight from disk.
    let components: Vec<ScopedComponent> = vx_files_under(&src_dir)
        .iter()
        .map(|p| read_scoped_component(p))
        .collect();

    // Guard against the analysis below passing vacuously: if the walk found
    // nothing, or found no scoped rules, every assertion below would be a
    // tautology over an empty set.
    assert!(
        components.len() >= 2,
        "expected the root and at least one child .vx under {}, found {}",
        src_dir.display(),
        components.len()
    );
    assert!(
        components.iter().any(|c| !c.declared.is_empty()),
        "no scoped <style> rules found in any template .vx — the walk or the \
         parser went blind, so every assertion below would pass vacuously"
    );

    // Distinct ids per component, or scoping could not work at all.
    let mut ids: Vec<&str> = components.iter().map(|c| c.id.as_str()).collect();
    ids.sort_unstable();
    let distinct = ids.len();
    ids.dedup();
    assert_eq!(
        ids.len(),
        distinct,
        "scope ids must differ per component or scoping cannot work: {ids:?}"
    );

    for c in &components {
        for class in &c.declared {
            let scoped = format!(".{class}[{}]", c.id);
            assert!(
                body.contains(&scoped),
                "{}.vx declares `{}` as a standalone rule, so the merged sheet must \
                 carry it as `{scoped}` — scoped with the DECLARING component's own \
                 id.\n--- sheet ---\n{body}",
                c.name,
                class
            );
        }
    }

    // ZERO bare selectors across every class the template declares: a bare
    // `.foo` matches every element on the page, which is the leak this whole
    // file exists to prevent. Note this checks for the ABSENCE of any scope
    // attribute, not for one specific id — a class may legitimately be declared
    // by two components, each scoped to itself.
    for c in &components {
        for class in &c.declared {
            let bare: Vec<&str> = body
                .lines()
                .filter(|l| l.contains(&format!(".{class}")) && !l.contains("[data-v-"))
                .collect();
            assert!(
                bare.is_empty(),
                "found {} bare `.{class}` selector(s) in the merged sheet, declared \
                 by {}.vx — a bare selector matches every element on the page:\n{}\n\
                 --- sheet ---\n{body}",
                bare.len(),
                c.name,
                bare.join("\n")
            );
        }
    }
}

/// LAYER B — two components whose selectors collide on the same class.
///
/// Builds a real three-file component tree, runs the real assembly, and asserts
/// element A does not pick up component B's rule — on the styled vnode and in
/// real rendered pixels.
#[test]
fn layer_b_colliding_class_does_not_leak_across_components() {
    let proj = scratch("b-src");
    let out_dir = scratch("b-out");
    let _ = fs::remove_dir_all(&proj);
    let _ = fs::remove_dir_all(&out_dir);

    // Both components declare `.btn` with a different color. Unfixed, the merged
    // sheet holds two BARE `.btn` rules and last-wins silently picks one, so
    // both elements paint the same color.
    write_vx(
        &proj,
        "WidgetA.vx",
        r#"<template>
  <div class="btn">A</div>
</template>

<style scoped>
.btn {
    background: #ff0000;
    width: 20px;
    height: 20px;
}
</style>
"#,
    );
    write_vx(
        &proj,
        "WidgetB.vx",
        r#"<template>
  <div class="btn">B</div>
</template>

<style scoped>
.btn {
    background: #0000ff;
    width: 20px;
    height: 20px;
}
</style>
"#,
    );
    let root = write_vx(
        &proj,
        "Root.vx",
        r#"<template>
  <div class="root">
    <WidgetA />
    <WidgetB />
  </div>
</template>

<script setup>
import WidgetA from './WidgetA.vx'
import WidgetB from './WidgetB.vx'
</script>

<style scoped>
.root {
    width: 100px;
    height: 100px;
}
</style>
"#,
    );

    velox_cli::commands::build::build_vx(&root, Some(out_dir.as_path()))
        .expect("build_vx over the collision fixture");

    let sheet_text =
        style_const_body(&fs::read_to_string(out_dir.join("root.rs")).expect("root.rs"));
    let sheet = Stylesheet::parse(&sheet_text);

    // ids derived from the shared function, matching what the assembly emitted.
    let id_a = velox_sfc::generate_scope_id("WidgetA");
    let id_b = velox_sfc::generate_scope_id("WidgetB");
    assert_ne!(id_a, id_b, "ids must differ or the fixture proves nothing");

    // Build one element per component, tagged exactly as its own template tags
    // it. Only the owning component's scope id is present.
    let element = |id: &str| h("div", Props::new().set("class", "btn").set(id, ""), vec![]);
    let styled_a = apply_with_cascade(&element(&id_a), &sheet);
    let styled_b = apply_with_cascade(&element(&id_b), &sheet);

    let style_of = |v: &VNode| -> String {
        match v {
            VNode::Element { props, .. } => props.attrs.get("style").cloned().unwrap_or_default(),
            _ => panic!("expected element"),
        }
    };
    let (sa, sb) = (style_of(&styled_a), style_of(&styled_b));

    assert!(
        sa.contains("#ff0000"),
        "element A must pick up its OWN component's rule, got: {sa}"
    );
    assert!(
        !sa.contains("#0000ff"),
        "element A LEAKED component B's rule: {sa}"
    );
    assert!(
        sb.contains("#0000ff"),
        "element B must pick up its OWN component's rule, got: {sb}"
    );
    assert!(
        !sb.contains("#ff0000"),
        "element B LEAKED component A's rule: {sb}"
    );

    // Pixel confirmation through the real layout pass.
    let px_a = render_center_pixel(&styled_a, 20, 20, 10, 10);
    let px_b = render_center_pixel(&styled_b, 20, 20, 10, 10);
    assert_eq!(
        px_a,
        [255, 0, 0, 255],
        "element A must paint red in real pixels, got {px_a:?}"
    );
    assert_eq!(
        px_b,
        [0, 0, 255, 255],
        "element B must paint blue in real pixels, got {px_b:?}"
    );
}

/// Render through `compute_layout` + `render_vnode_to_rgba`, return one pixel.
fn render_center_pixel(vnode: &VNode, w: i32, h: i32, x: i32, y: i32) -> [u8; 4] {
    // Explicit layout pass: the invariant under test needs a real layout, and
    // the inline `style` attrs asserted above are what the raster pass paints.
    let _layout = compute_layout(vnode, w, h);

    // The vnode already carries resolved inline styles from the cascade, so the
    // renderer gets an empty sheet rather than a second, differently-derived one.
    let empty = Stylesheet::parse("");
    let rgba = velox_renderer::render_vnode_to_rgba(vnode, &empty, w, h).expect("render rgba");

    let i = ((y * w + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}
