//! Regression tests for `<style scoped>` actually scoping in a scaffolded app.
//!
//! Defect: a child component's `<style scoped>` block was collected RAW and
//! spliced into the root's `STYLE` constant, so its selectors stayed bare and
//! leaked across component boundaries. A `.btn-add` declared in `TodoInput.vx`
//! styled the `<button class="btn btn-add">` that lives in `Todos.vx`.
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

/// LAYER A — assembly regression over the shipped project template.
///
/// The root's merged `STYLE` must carry each child's rules scoped to that
/// child's own id, and ZERO bare selectors from a child block.
#[test]
fn layer_a_child_rules_are_scoped_in_root_stylesheet() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input = manifest_dir.join("templates/project/src/App.vx");
    let out_dir = scratch("a");
    let _ = fs::remove_dir_all(&out_dir);

    velox_cli::commands::build::build_vx(&input, Some(out_dir.as_path()))
        .expect("build_vx over the shipped template");

    let stub = fs::read_to_string(out_dir.join("app.rs")).expect("read generated app.rs");

    // Scope ids are derived with the same function production uses, on both
    // sides of every assertion. A hand-typed literal here would pin nothing.
    let todo_input_id = velox_sfc::generate_scope_id("TodoInput");
    let todos_id = velox_sfc::generate_scope_id("Todos");
    let todo_item_id = velox_sfc::generate_scope_id("TodoItem");
    let app_id = velox_sfc::generate_scope_id("App");
    assert_ne!(
        todo_input_id, todos_id,
        "scope ids must differ per component or scoping cannot work"
    );

    let body = style_const_body(&stub);

    // `.btn-add` is declared in TodoInput.vx, not in Todos.vx — this is the
    // selector that was leaking onto the `Add` button.
    assert!(
        body.contains(&format!(".btn-add[{todo_input_id}]")),
        "TodoInput's .btn-add must be scoped in the merged sheet.\n--- sheet ---\n{body}"
    );

    // ZERO bare `.btn-add` — a bare selector matches every element on the page.
    let bare = body
        .lines()
        .filter(|l| l.contains(".btn-add") && !l.contains(&todo_input_id))
        .count();
    assert_eq!(
        bare, 0,
        "found {bare} bare `.btn-add` selector(s) in the merged sheet:\n{body}"
    );

    for (component, id, class) in [
        ("Todos", todos_id, ".todos"),
        ("TodoItem", todo_item_id, ".btn-danger"),
    ] {
        assert!(
            body.contains(&format!("{class}[{id}]")),
            "{component}'s {class} must be scoped to its own id.\n--- sheet ---\n{body}"
        );
    }

    assert!(
        body.contains(&format!(".app[{app_id}]")),
        "root's own .app rule must remain scoped.\n--- sheet ---\n{body}"
    );
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
