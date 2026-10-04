//! "Clear completed" and "About" looked dead while the theme toggle worked.
//!
//! ## What was actually wrong
//!
//! NOT the click path. `collect_click_targets` walks `layout.children` and
//! resolves each one's `source_index` against the VNode children, and for the
//! real scaffolded tree it collects `toggle_theme`, `open_clear` and `open_about`
//! with correct rects; a hit test at the centre of either ghost button returns
//! that button's handler. `a_dead_button_is_still_collected_and_hit` pins that.
//!
//! The handler DID fire. It set `clear_open` / `about_open`, the dialog
//! rendered — one viewport BELOW the fold. `App.vx` puts `<Confirm>` / `<Modal>`
//! after a `min-height: 100vh` shell, and their `.overlay` is `position: fixed`,
//! so the tail pass moved the overlay to the viewport while leaving its `.panel`
//! at the page's foot. The panel's y was `viewport_h + 299` at EVERY height.
//!
//! `a_fixed_dialog_panel_is_inside_the_viewport` is the regression gate.
//!
//! ## Real CSS, real tree
//!
//! The CSS is read out of the six `.vx` files and scoped with the same
//! `scope_css` + `generate_scope_id` the compiler uses, so a declaration edited
//! in a template reaches these tests. Only the nesting is transcribed.

use std::fs;
use std::path::{Path, PathBuf};

use velox_dom::layout::{LayoutNode, compute_layout};
use velox_dom::{Props, VNode, h, text};
use velox_sfc::codegen::{generate_scope_id, scope_css};
use velox_sfc::parse_sfc;
use velox_style::Stylesheet;

const SHEETS: &[(&str, &str)] = &[
    ("App", "App.vx"),
    ("Confirm", "components/Confirm.vx"),
    ("Modal", "components/Modal.vx"),
    ("TodoInput", "components/TodoInput.vx"),
    ("TodoItem", "components/TodoItem.vx"),
    ("Todos", "components/Todos.vx"),
];

fn template_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("veloxc/templates/project/src")
}

/// The six sheets of the scaffolded app, scoped the way `velox-sfc` scopes them.
fn real_sheet() -> Stylesheet {
    let dir = template_src();
    let mut css = String::new();
    for (stem, rel) in SHEETS {
        let src = fs::read_to_string(dir.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
        let sfc = parse_sfc(&src).unwrap_or_else(|e| panic!("parse {rel}: {e:?}"));
        let style = sfc
            .style
            .as_ref()
            .unwrap_or_else(|| panic!("{rel} has no <style>"));
        css.push_str(&scope_css(&style.content, &generate_scope_id(stem)));
        css.push('\n');
    }
    Stylesheet::parse(&css)
}

fn el(tag: &str, stem: &str, classes: &[&str], children: Vec<VNode>) -> VNode {
    h(
        tag,
        Props::new()
            .set(generate_scope_id(stem), "")
            .set("class", classes.join(" ")),
        children,
    )
}

fn on(tag: &str, stem: &str, classes: &[&str], handler: &str, children: Vec<VNode>) -> VNode {
    h(
        tag,
        Props::new()
            .set(generate_scope_id(stem), "")
            .set("class", classes.join(" "))
            .set("on:click", handler.to_string()),
        children,
    )
}

fn txt(s: &str) -> VNode {
    text(s.to_string())
}

fn masthead(dark: bool) -> VNode {
    el(
        "header",
        "App",
        &["masthead"],
        vec![
            el(
                "div",
                "App",
                &["brand"],
                vec![
                    el("p", "App", &["eyebrow"], vec![txt("VELOX")]),
                    el("h1", "App", &["title"], vec![txt("Velox Todo")]),
                    el(
                        "p",
                        "App",
                        &["tagline"],
                        vec![txt(
                            "One file per component. A template, a state block and a scoped style \
                             block, drawn natively with Skia.",
                        )],
                    ),
                ],
            ),
            on(
                "button",
                "App",
                &["toggle"],
                "toggle_theme",
                vec![el(
                    "span",
                    "App",
                    &["glyph"],
                    vec![txt(if dark { "\u{2600}" } else { "\u{263E}" })],
                )],
            ),
        ],
    )
}

/// App.vx:17-28 — the row with the two buttons that read as dead.
fn meta_row() -> VNode {
    el(
        "div",
        "App",
        &["meta"],
        vec![
            el("span", "App", &["meta-count"], vec![txt("2 left of 3")]),
            // The empty `flex: 1 1 auto` span. Named in the report as the
            // suspected cause of a duplicate ordinal; it is not — see
            // `no_layout_child_duplicates_a_source_index`.
            el("span", "App", &["meta-fill"], vec![]),
            on(
                "button",
                "App",
                &["ghost"],
                "open_clear",
                vec![
                    el("span", "App", &["btn-icon"], vec![txt("\u{232B}")]),
                    el("span", "App", &["btn-label"], vec![txt("Clear completed")]),
                ],
            ),
            on(
                "button",
                "App",
                &["ghost"],
                "open_about",
                vec![
                    el("span", "App", &["btn-icon"], vec![txt("\u{2139}")]),
                    el("span", "App", &["btn-label"], vec![txt("About")]),
                ],
            ),
        ],
    )
}

const SEED: &[(&str, bool)] = &[
    ("Read the Velox guide", false),
    ("Try the theme switch, top right", false),
    ("Ship something small", true),
];

fn todo_item(t: &str, done: bool, dark: bool) -> VNode {
    let theme = if dark { "dark" } else { "" };
    let item_class = if done {
        "todo-item completed"
    } else {
        "todo-item"
    };
    let mark = if done {
        vec![el(
            "span",
            "TodoItem",
            &["check-mark"],
            vec![txt("\u{2713}")],
        )]
    } else {
        vec![]
    };
    h(
        "div",
        Props::new()
            .set(generate_scope_id("TodoItem"), "")
            .set("class", theme),
        vec![h(
            "div",
            Props::new()
                .set(generate_scope_id("TodoItem"), "")
                .set("class", item_class),
            vec![
                on("button", "TodoItem", &["check"], "todo_toggle", mark),
                el("span", "TodoItem", &["todo-text"], vec![txt(t)]),
                on(
                    "button",
                    "TodoItem",
                    &["remove"],
                    "todo_remove",
                    vec![el(
                        "span",
                        "TodoItem",
                        &["remove-mark"],
                        vec![txt("\u{2715}")],
                    )],
                ),
            ],
        )],
    )
}

fn todos(dark: bool) -> VNode {
    let theme = if dark { "dark" } else { "" };
    let rows: Vec<VNode> = SEED
        .iter()
        .map(|(t, done)| todo_item(t, *done, dark))
        .collect();
    h(
        "div",
        Props::new()
            .set(generate_scope_id("Todos"), "")
            .set("class", theme),
        vec![
            el(
                "div",
                "Todos",
                &["composer"],
                vec![
                    h(
                        "div",
                        Props::new()
                            .set(generate_scope_id("TodoInput"), "")
                            .set("class", theme),
                        vec![h(
                            "input",
                            Props::new()
                                .set(generate_scope_id("TodoInput"), "")
                                .set("class", "input")
                                .set("type", "text")
                                .set("value", "")
                                .set("placeholder", "What needs to be done?"),
                            vec![],
                        )],
                    ),
                    on(
                        "button",
                        "Todos",
                        &["add"],
                        "todo_add",
                        vec![
                            el("span", "Todos", &["btn-icon"], vec![txt("+")]),
                            el("span", "Todos", &["btn-label"], vec![txt("Add")]),
                        ],
                    ),
                ],
            ),
            el("div", "Todos", &["list"], rows),
        ],
    )
}

fn confirm_dialog(dark: bool) -> VNode {
    let theme = if dark { "dark" } else { "" };
    h(
        "div",
        Props::new()
            .set(generate_scope_id("Confirm"), "")
            .set("class", theme),
        vec![on(
            "div",
            "Confirm",
            &["overlay"],
            "confirm_scrim",
            vec![on(
                "div",
                "Confirm",
                &["panel"],
                "confirm_panel",
                vec![
                    el(
                        "div",
                        "Confirm",
                        &["head"],
                        vec![
                            el(
                                "span",
                                "Confirm",
                                &["badge"],
                                vec![el("span", "Confirm", &["badge-mark"], vec![txt("!")])],
                            ),
                            el(
                                "h2",
                                "Confirm",
                                &["title"],
                                vec![txt("Clear completed tasks?")],
                            ),
                        ],
                    ),
                    el(
                        "p",
                        "Confirm",
                        &["message"],
                        vec![txt(
                            "Finished tasks are removed from the list. This cannot be undone.",
                        )],
                    ),
                    el(
                        "div",
                        "Confirm",
                        &["foot"],
                        vec![
                            on(
                                "button",
                                "Confirm",
                                &["cancel"],
                                "confirm_dismiss",
                                vec![txt("Keep them")],
                            ),
                            on(
                                "button",
                                "Confirm",
                                &["accept"],
                                "confirm_accept",
                                vec![txt("Clear them")],
                            ),
                        ],
                    ),
                ],
            )],
        )],
    )
}

fn modal_dialog(dark: bool) -> VNode {
    let theme = if dark { "dark" } else { "" };
    h(
        "div",
        Props::new()
            .set(generate_scope_id("Modal"), "")
            .set("class", theme),
        vec![on(
            "div",
            "Modal",
            &["overlay"],
            "modal_scrim",
            vec![on(
                "div",
                "Modal",
                &["panel"],
                "modal_panel",
                vec![
                    el(
                        "div",
                        "Modal",
                        &["head"],
                        vec![
                            el("h2", "Modal", &["title"], vec![txt("About this app")]),
                            on(
                                "button",
                                "Modal",
                                &["dismiss"],
                                "modal_dismiss",
                                vec![el("span", "Modal", &["dismiss-mark"], vec![txt("\u{d7}")])],
                            ),
                        ],
                    ),
                    el(
                        "div",
                        "Modal",
                        &["body"],
                        vec![el(
                            "p",
                            "Modal",
                            &["body-text"],
                            vec![txt(
                                "Every part of this window is a .vx single-file component.",
                            )],
                        )],
                    ),
                    el(
                        "div",
                        "Modal",
                        &["foot"],
                        vec![
                            on(
                                "button",
                                "Modal",
                                &["cancel"],
                                "modal_cancel",
                                vec![txt("Close")],
                            ),
                            on(
                                "button",
                                "Modal",
                                &["confirm"],
                                "modal_confirm",
                                vec![txt("Understood")],
                            ),
                        ],
                    ),
                ],
            )],
        )],
    )
}

/// `App.vx` with the two `v-if` flags as given.
fn app(dark: bool, clear_open: bool, about_open: bool) -> VNode {
    let theme = if dark { "dark" } else { "" };
    let mut children = vec![h(
        "div",
        Props::new()
            .set(generate_scope_id("App"), "")
            .set("class", "app"),
        vec![el(
            "div",
            "App",
            &["shell"],
            vec![
                masthead(dark),
                meta_row(),
                el("span", "App", &["rule"], vec![]),
                el("div", "App", &["work"], vec![todos(dark)]),
                el(
                    "p",
                    "App",
                    &["footnote"],
                    vec![txt(
                        "Everything above is live: add a task, tick a row, clear what is done, or \
                         open the About panel.",
                    )],
                ),
            ],
        )],
    )];
    if clear_open {
        children.push(confirm_dialog(dark));
    }
    if about_open {
        children.push(modal_dialog(dark));
    }
    h(
        "div",
        Props::new()
            .set(generate_scope_id("App"), "")
            .set("class", theme),
        children,
    )
}

fn styled(tree: &VNode) -> VNode {
    let sheet = real_sheet();
    velox_renderer::style_vnode_with_hover(tree, &sheet, &|_, _| false)
}

fn click_targets(tree: &VNode, w: i32, h_: i32) -> Vec<velox_renderer::events::ClickTarget> {
    let styled = styled(tree);
    let layout = compute_layout(&styled, w, h_);
    let mut out = Vec::new();
    let mut order = 0i32;
    velox_renderer::events::collect_click_targets(
        &styled,
        &layout,
        None,
        velox_renderer::events::StackCtx::ROOT,
        &mut order,
        &mut out,
    );
    out
}

/// The rect of the first element carrying `class`, paired with its tag.
fn rect_of(v: &VNode, l: &LayoutNode, class: &str) -> Option<(String, velox_dom::layout::Rect)> {
    if let VNode::Element {
        tag,
        props,
        children,
        ..
    } = v
    {
        if props
            .attrs
            .get("class")
            .is_some_and(|c| c.split_whitespace().any(|x| x == class))
        {
            return Some((tag.clone(), l.rect));
        }
        for c in l.children.iter() {
            if let Some(si) = c.source_index
                && let Some(cv) = children.get(si)
                && let Some(r) = rect_of(cv, c, class)
            {
                return Some(r);
            }
        }
    }
    None
}

/// THE GATE. Both dialogs must be fully inside the viewport at every window
/// height — before the fix the panel sat at `viewport_h + 299`.
#[test]
fn a_fixed_dialog_panel_is_inside_the_viewport() {
    for (w, h_) in [(900, 400), (900, 760), (900, 1000), (1280, 1800)] {
        for (label, c, a) in [("Confirm", true, false), ("Modal", false, true)] {
            let tree = app(false, c, a);
            let styled = styled(&tree);
            let layout = compute_layout(&styled, w, h_);
            let (_, overlay) = rect_of(&styled, &layout, "overlay")
                .unwrap_or_else(|| panic!("{label} at {w}x{h_}: no .overlay"));
            assert!(
                overlay.x >= 0 && overlay.y >= 0 && overlay.w > 0 && overlay.h > 0,
                "{label} overlay at {w}x{h_} = {overlay:?}",
            );
            let (tag, panel) = rect_of(&styled, &layout, "panel")
                .unwrap_or_else(|| panic!("{label} at {w}x{h_}: no .panel"));
            assert!(
                panel.y >= 0 && panel.y + panel.h <= h_,
                "{label} <{tag}> at {w}x{h_}: panel y={} h={} is outside the {h_}px \
                 viewport — the dialog opened below the fold and read as a dead button",
                panel.y,
                panel.h
            );
        }
    }
}

/// The panel's y must be the VIEWPORT's centre, not the page's foot. Before the
/// fix it was `viewport_h + 299`, i.e. it moved DOWN as the window grew.
#[test]
fn a_fixed_dialog_panel_is_centred_in_the_viewport() {
    for h_ in [400, 760, 1000, 1800] {
        let tree = app(false, false, true);
        let styled = styled(&tree);
        let layout = compute_layout(&styled, 900, h_);
        let (_, overlay) = rect_of(&styled, &layout, "overlay").expect("overlay");
        let (_, panel) = rect_of(&styled, &layout, "panel").expect("panel");
        let overlay_centre = overlay.y + overlay.h / 2;
        let panel_centre = panel.y + panel.h / 2;
        assert!(
            (panel_centre - overlay_centre).abs() <= 2,
            "at h={h_} panel centre {panel_centre} vs overlay centre {overlay_centre}"
        );
    }
}

/// The control case: the click path was never broken. A click at the centre of
/// either ghost button resolves to that button's handler.
#[test]
fn a_dead_button_is_still_collected_and_hit() {
    let tree = app(false, false, false);
    let targets = click_targets(&tree, 900, 760);
    for want in ["toggle_theme", "open_clear", "open_about"] {
        let t = targets
            .iter()
            .find(|t| t.handler == want)
            .unwrap_or_else(|| {
                panic!(
                    "{want} was never collected; got {:?}",
                    targets.iter().map(|t| &t.handler).collect::<Vec<_>>()
                )
            });
        let cx = t.rect.x as f32 + t.rect.w as f32 / 2.0;
        let cy = t.rect.y as f32 + t.rect.h as f32 / 2.0;
        let hit =
            velox_renderer::events::hit_test_click(&targets, cx, cy).map(|(h, _)| h.to_string());
        assert_eq!(
            hit.as_deref(),
            Some(want),
            "centre-click on {want} at ({cx},{cy}) resolved to {hit:?}"
        );
    }
}

/// With the dialog open, its OWN buttons must be clickable — which they were not
/// when the panel was parked below the fold, since `hit_test_click` rejects any
/// point outside the viewport's own bounds.
#[test]
fn an_open_dialog_own_buttons_are_clickable() {
    let tree = app(false, true, false);
    let targets = click_targets(&tree, 900, 760);
    for want in ["confirm_accept", "confirm_dismiss"] {
        let t = targets
            .iter()
            .find(|t| t.handler == want)
            .unwrap_or_else(|| panic!("{want} was never collected"));
        let cx = t.rect.x as f32 + t.rect.w as f32 / 2.0;
        let cy = t.rect.y as f32 + t.rect.h as f32 / 2.0;
        assert!(
            (0.0..=760.0).contains(&cy),
            "{want} centre y={cy} is off-screen; the dialog opened where it cannot be clicked"
        );
        let hit =
            velox_renderer::events::hit_test_click(&targets, cx, cy).map(|(h, _)| h.to_string());
        assert_eq!(
            hit.as_deref(),
            Some(want),
            "centre-click on {want} -> {hit:?}"
        );
    }
    // The scrim's CENTRE is covered by the panel, and `confirm_panel` is meant to
    // win that hit — `Confirm.vx` gives the panel its own handler precisely so a
    // click on its padding does not dismiss the dialog. Click a corner instead,
    // which is the scrim's own territory.
    let scrim = targets
        .iter()
        .find(|t| t.handler == "confirm_scrim")
        .expect("confirm_scrim");
    let hit = velox_renderer::events::hit_test_click(
        &targets,
        scrim.rect.x as f32 + 8.0,
        scrim.rect.y as f32 + 8.0,
    )
    .map(|(h, _)| h.to_string());
    assert_eq!(
        hit.as_deref(),
        Some("confirm_scrim"),
        "clicking the scrim outside the panel resolved to {hit:?}"
    );
}

/// ## The reported cause, measured
///
/// The report blamed a duplicated `source_index` in the inline machinery: two
/// layout children carrying one ordinal, the walk visiting the same VNode child
/// twice, and the sibling that "owns the other slot" never collected.
///
/// The FIRST half of that is real and reproducible: a text run that wraps onto
/// N lines emits N `LayoutNode`s that all carry the run's single ordinal. In the
/// scaffolded app, `.tagline` wraps to two lines and produces
/// `child[0] src_index=Some(0)` and `child[1] src_index=Some(0)`.
///
/// It is also the ONLY representable encoding — one VNode child has one index,
/// so N line fragments must all reference it. There is no second ordinal to
/// hand out.
///
/// The CONSEQUENCE, though, is not real, and that is what this test pins: the
/// fragments all belong to ONE VNode child, so re-walking it re-walks the same
/// element with the same rect. Every OTHER sibling keeps its own distinct
/// ordinal and is visited normally. No click target is lost, which is why
/// `collect_click_targets` needs no `visited` guard and why
/// `a_dead_button_is_still_collected_and_hit` passes.
#[test]
fn duplicated_ordinals_only_ever_belong_to_one_vnode_child() {
    /// Every duplicated ordinal at a level, with the element it resolves to.
    fn rec(v: &VNode, l: &LayoutNode, path: &str, bad: &mut Vec<String>) {
        if let VNode::Element { children, .. } = v {
            let mut by_ordinal: std::collections::BTreeMap<usize, usize> =
                std::collections::BTreeMap::new();
            for c in l.children.iter() {
                if let Some(s) = c.source_index {
                    *by_ordinal.entry(s).or_insert(0) += 1;
                }
            }
            for (s, n) in by_ordinal {
                if n > 1 {
                    // Legal only when that one VNode child is a text node: the
                    // fragments are its wrapped lines, and the walk's `Text` arm
                    // is a no-op, so nothing is double-collected and nothing is
                    // skipped.
                    let kind = match children.get(s) {
                        Some(VNode::Text(_)) => "text (wrapped lines)",
                        Some(other) => {
                            let tag = match other {
                                VNode::Element { tag, .. } => tag.clone(),
                                VNode::Text(_) => unreachable!(),
                            };
                            bad.push(format!(
                                "{path}: src_index {s} appears {n}x and resolves to a \
                                 NON-text <{tag}> — this would double-collect one element"
                            ));
                            continue;
                        }
                        None => {
                            bad.push(format!(
                                "{path}: src_index {s} appears {n}x and resolves to NOTHING"
                            ));
                            continue;
                        }
                    };
                    let _ = kind;
                }
            }
            for c in l.children.iter() {
                if let Some(si) = c.source_index
                    && let Some(cv) = children.get(si)
                {
                    rec(cv, c, &format!("{path}/{si}"), bad);
                }
            }
        }
    }

    let mut trees: Vec<(&str, VNode)> = vec![
        ("app/closed", app(false, false, false)),
        ("app/confirm-open", app(false, true, false)),
        ("app/modal-open", app(false, false, true)),
    ];
    // Inline-shaped reductions — the shape the report blamed.
    trees.push((
        "inline: text + spans",
        h(
            "div",
            Props::new().set("style", "width:200px;font-size:15px;"),
            vec![
                h("span", Props::new(), vec![text("a")]),
                text("plain"),
                h("span", Props::new(), vec![text("b")]),
            ],
        ),
    ));
    trees.push((
        "inline: empty element between texts",
        h(
            "div",
            Props::new().set("style", "width:200px;font-size:15px;"),
            vec![text("one"), h("span", Props::new(), vec![]), text("two")],
        ),
    ));
    trees.push((
        "inline: empty flex sibling, then a button",
        h(
            "div",
            Props::new().set("style", "width:200px;display:flex;gap:4px;"),
            vec![
                h("span", Props::new().set("style", "flex:1;"), vec![]),
                on("button", "", &[], "go", vec![text("Go")]),
            ],
        ),
    ));
    // A wrapping text run BESIDE an inline element: the exact shape that would
    // lose a sibling if the mechanism the report described were real.
    trees.push((
        "inline: wrapping text beside a button",
        h(
            "div",
            Props::new().set("style", "width:120px;font-size:15px;"),
            vec![
                on(
                    "button",
                    "",
                    &[],
                    "go",
                    vec![text("a button label long enough to wrap")],
                ),
                text("tail text that also wraps across lines here"),
            ],
        ),
    ));

    for (name, tree) in trees {
        let styled = styled(&tree);
        let layout = compute_layout(&styled, 900, 760);
        let mut bad = Vec::new();
        rec(&styled, &layout, name, &mut bad);
        assert!(bad.is_empty(), "{name}: {bad:?}");
    }
}

/// The consequence that actually matters: a wrapping text run must not cost its
/// SIBLING its click target. Measured directly — the button beside a four-line
/// text run is collected, exactly once, at its own rect.
#[test]
fn a_wrapping_run_does_not_cost_its_sibling_its_click_target() {
    let tree = h(
        "div",
        Props::new().set("style", "width:120px;font-size:15px;"),
        vec![
            on(
                "button",
                "",
                &[],
                "go",
                vec![text("a button label long enough to wrap")],
            ),
            text("tail text that also wraps across lines here"),
        ],
    );
    let targets = click_targets(&tree, 900, 760);
    let go: Vec<_> = targets.iter().filter(|t| t.handler == "go").collect();
    assert_eq!(
        go.len(),
        1,
        "the button beside a four-line text run was collected {} times, expected 1",
        go.len()
    );
    let (cx, cy) = (
        go[0].rect.x as f32 + go[0].rect.w as f32 / 2.0,
        go[0].rect.y as f32 + go[0].rect.h as f32 / 2.0,
    );
    assert_eq!(
        velox_renderer::events::hit_test_click(&targets, cx, cy)
            .map(|(h, _)| h.to_string())
            .as_deref(),
        Some("go"),
        "the sibling's target is unreachable"
    );
}

/// Every VNode element carrying `on:click` is collected — exactly as many
/// targets as there are clickable VNodes, with none lost or doubled. (Handler
/// NAMES repeat across the three todo rows, so the count is over the whole set.)
#[test]
fn every_clickable_vnode_is_visited_exactly_once() {
    fn clickables(v: &VNode, out: &mut usize) {
        if let VNode::Element {
            props, children, ..
        } = v
        {
            if props.attrs.contains_key("on:click") {
                *out += 1;
            }
            for c in children {
                clickables(c, out);
            }
        }
    }
    for (name, tree) in [
        ("app/closed", app(false, false, false)),
        ("app/confirm-open", app(false, true, false)),
        ("app/modal-open", app(false, false, true)),
    ] {
        let mut want = 0usize;
        clickables(&tree, &mut want);
        let got = click_targets(&tree, 900, 760).len();
        assert_eq!(
            got, want,
            "{name}: {got} click targets for {want} clickable elements"
        );
    }
}

/// The user-visible symptom, measured rather than argued about: before the fix
/// the open-dialog frame was BYTE-IDENTICAL to the closed one.
///
/// Needs the Skia raster path, so it only exists with `skia-native`. Without the
/// gate this test target fails to compile under `--features skia` alone, which is
/// exactly how the `renderer-features` job runs it.
#[cfg(feature = "skia-native")]
#[test]
fn opening_a_dialog_changes_the_frame() {
    let sheet = real_sheet();
    let closed = velox_renderer::render_vnode_to_raster_png_with_scale(
        &app(false, false, false),
        &sheet,
        900,
        760,
        1.0,
    )
    .expect("raster");
    for (name, c, a) in [("confirm", true, false), ("modal", false, true)] {
        let open = velox_renderer::render_vnode_to_raster_png_with_scale(
            &app(false, c, a),
            &sheet,
            900,
            760,
            1.0,
        )
        .expect("raster");
        let diff = closed
            .iter()
            .zip(open.iter())
            .filter(|(x, y)| x != y)
            .count();
        assert!(
            diff > 1_000,
            "{name}: only {diff}/{} bytes changed when the dialog opened — it is still \
             rendering where nobody can see it",
            closed.len()
        );
    }
}
