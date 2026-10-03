//! Per-frame cost breakdown for the pipeline a keystroke actually runs.
//!
//! A keystroke in the renderer runs: `make_view` (which re-parses the
//! stylesheet) -> `style_vnode_with_hover` -> `compute_layout` ->
//! `render_vnode_to_rgba` -> `present`. Everything here is unconditional, so
//! the honest way to prioritise a fix is to measure each stage rather than
//! guess from a profile of the wrong binary.
//!
//! The tree mirrors the shipped `myapp` todo app, because its NESTED FLEX
//! CONTAINERS are what make layout expensive: `.app` (column) contains
//! `.todos` (column) which contains `.todo-item` (row). `.todo-text` and
//! `.btn-danger` are deliberately left with no declared width, which is what
//! makes them content-basis and sends layout through the two-pass flex item
//! path (velox-dom/src/layout.rs:3798 probe, :3820 target) — two full subtree
//! layouts per such item, i.e. 2^depth across the nested levels.
//!
//! BEFORE commit ab152c8 ("perf(dom): lay out a content-basis flex item once
//! per level instead of twice") that was THREE descents per level, not two: an
//! unconditional pre-layout at the line's main size, then the wide probe, then
//! the target. The pre-layout's result was read only on the `!basis_is_content`
//! arm, so on the content-basis arm it was a full subtree layout computed and
//! thrown away. That path is now the `else` arm at velox-dom/src/layout.rs:3844.
//! So 3^depth -> 2^depth. The line numbers quoted above are POST-reorder; the
//! pre-reorder probe/target were at :3804 and :3826.
//!
//! IGNORED by default: this is a measurement tool, not a correctness gate, and
//! it must not slow CI. Run it with:
//!   cargo test -p velox-renderer --features skia-native \
//!     --test frame_cost_bench -- --ignored --nocapture
//!
//! Three caveats on reading the numbers:
//!   * Stylesheet selectors here are class-only. The compiled `.vx` output
//!     scopes with `.foo[data-v-<hash>]`, which adds a second simple-selector
//!     term per rule. Selector matching is O(rules) per node either way, so
//!     this slightly UNDERSTATES the real cascade cost.
//!   * `render_vnode_to_rgba` builds a fresh raster surface per call, which the
//!     real loop does not; it therefore OVERSTATES total frame cost relative to
//!     the steady-state loop.
//!   * Stronger still: `render_vnode_to_rgba` has ZERO `src/` callers. The live
//!     loop never calls it, so the "full frame" and "fps ceiling" rows below
//!     describe a headless test harness, not the product's frame rate. It also
//!     allocates a 1.92 MB zeroed buffer at 800x600 and copies it back out via
//!     `read_pixels` on every call. The `cascade` and `layout` rows call the
//!     real functions directly and are unaffected; they are the only two rows
//!     here that say anything about the live loop.

#![cfg(all(feature = "skia-native", unix))]

use std::time::Instant;

use velox_dom::{Props, VNode, h, text};
use velox_renderer::style_vnode_with_hover;
use velox_style::Stylesheet;

/// Representative of the app's generated stylesheet in structure: the nested
/// flex column/row rules that drive the content-basis probe, plus the
/// declarations each element actually carries.
fn sheet() -> Stylesheet {
    Stylesheet::parse(
        r#"
        .app { display: flex; flex-direction: column; width: 100%; min-height: 100vh; padding: 16px; }
        .title { font-size: 24px; margin: 0 0 12px 0; }
        .todos { display: flex; flex-direction: column; flex: 1; gap: 8px; }
        .todo-input { display: flex; flex-direction: row; gap: 8px; }
        .todo-list { display: flex; flex-direction: column; gap: 4px; }
        .todo-item { display: flex; flex-direction: row; align-items: center; gap: 8px; }
        .todo-text { flex: 1; }
        .btn-add { padding: 6px 12px; }
        .btn-danger { padding: 4px 8px; }
        "#,
    )
}

fn todo_item(text_body: &str) -> VNode {
    h(
        "div",
        Props::from_class("todo-item"),
        vec![
            // A declared width keeps the checkbox off the flex item probe
            // path (two descents since ab152c8, three before it).
            h(
                "input",
                Props::new()
                    .set("class", "checkbox")
                    .set("type", "checkbox"),
                vec![],
            ),
            // No declared width => content-basis => laid out 2x (probe, then
            // target). It was 3x before ab152c8; see the file header.
            h(
                "span",
                Props::from_class("todo-text"),
                vec![text(text_body)],
            ),
            h(
                "button",
                Props::from_class("btn-danger btn-small"),
                vec![text("×")],
            ),
        ],
    )
}

fn tree(n: usize) -> VNode {
    let mut items: Vec<VNode> = (0..n)
        .map(|i| todo_item(&format!("Task number {i}")))
        .collect();
    items.push(todo_item(
        "Task one two three four five six seven eight nine ten",
    ));

    h(
        "div",
        Props::from_class("app"),
        vec![
            h(
                "header",
                Props::new(),
                vec![h(
                    "h1",
                    Props::from_class("title"),
                    vec![text("Velox Todo")],
                )],
            ),
            h(
                "div",
                Props::from_class("todos"),
                vec![
                    h(
                        "div",
                        Props::from_class("todo-input"),
                        vec![
                            h(
                                "input",
                                Props::new()
                                    .set("class", "todo-field")
                                    .set("type", "text")
                                    .set("value", "a fairly long todo string being typed"),
                                vec![],
                            ),
                            h("button", Props::from_class("btn-add"), vec![text("Add")]),
                        ],
                    ),
                    h("div", Props::from_class("todo-list"), items),
                ],
            ),
        ],
    )
}

/// Median of `runs` timings, discarding the first (cold) run so allocator and
/// font-cache warmup does not dominate. Median rather than mean so a single
/// scheduling hiccup does not read as a regression.
fn median_ms<F: FnMut()>(mut runs: usize, mut f: F) -> f64 {
    let mut samples = Vec::with_capacity(runs);
    f(); // warm
    while runs > 0 {
        let t = Instant::now();
        f();
        samples.push(t.elapsed().as_secs_f64() * 1000.0);
        runs -= 1;
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    samples[samples.len() / 2]
}

fn report(n_items: usize) {
    let sheet = sheet();
    let tree = tree(n_items);
    const W: i32 = 800;
    const H: i32 = 600;

    // Stage 1: what `make_view` does beyond building the tree — the stylesheet
    // re-parse that main.rs performs INSIDE the closure, every redraw.
    let parse_us = median_ms(30, || {
        std::hint::black_box(Stylesheet::parse(
            "body { margin: 0; } .app { display: flex; padding: 16px; }",
        ));
    }) * 1000.0;

    // Stage 2: cascade. Full re-cascade of every rule against every node.
    let cascade_us = median_ms(30, || {
        let styled = style_vnode_with_hover(&tree, &sheet, &|_, _| false);
        std::hint::black_box(styled);
    }) * 1000.0;

    // Stage 3: layout, which is where the two-pass flex item probe lives
    // (three passes before ab152c8; see the file header).
    let styled = style_vnode_with_hover(&tree, &sheet, &|_, _| false);
    let layout_us = median_ms(30, || {
        let l = velox_dom::layout::compute_layout(&styled, W, H);
        std::hint::black_box(l);
    }) * 1000.0;

    // Stages 2-4 together, as one full headless frame.
    let frame_us = median_ms(30, || {
        let r = velox_renderer::render_vnode_to_rgba(&tree, &sheet, W, H);
        let _ = std::hint::black_box(r);
    }) * 1000.0;

    // The tail not covered by the three stages above is paint + readback.
    let paint_us = (frame_us - cascade_us - layout_us).max(0.0);

    println!("\n  todos={n_items}");
    println!("    stylesheet re-parse : {parse_us:9.1} us");
    println!("    cascade             : {cascade_us:9.1} us");
    println!("    layout              : {layout_us:9.1} us");
    println!("    paint + readback    : {paint_us:9.1} us");
    println!("    ------------------------------------");
    // frame_us is microseconds, so fps is 1e6 / frame_us. Printing
    // frame_us / 1000 here is milliseconds and reads as a nonsense ceiling.
    println!(
        "    full frame          : {frame_us:9.1} us  ({:.0} fps ceiling)",
        1_000_000.0 / frame_us
    );
}

#[test]
#[ignore = "measurement tool; run explicitly with --ignored --nocapture"]
fn per_frame_cost_breakdown() {
    println!("\n=== velox per-frame cost (headless, no window) ===");
    // The sweep is the measurement surface the plan's per-keystroke cost table
    // assumes: it has to run well past 10 todos, because the whole question is
    // whether layout cost is LINEAR in item count or constant-dominated per
    // item. Four points cannot tell those apart; a run that stops at 10 also
    // cannot reproduce the 50/100/200-todo curves the plan quotes.
    for n in [0usize, 1, 3, 10, 25, 50, 100, 200] {
        report(n);
    }
    println!("\nNote: idle-CPU numbers from a live window are NOT this measurement.");
    println!("Typing latency is the cost of ONE keystroke redraw, which is here.\n");
}
