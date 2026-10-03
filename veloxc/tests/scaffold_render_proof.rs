//! THROWAWAY VISUAL PROOF for the scaffolded template's design pass.
//!
//! This is not a gate. It exists so the design work in
//! `veloxc/templates/project/src/{App.vx, components/*.vx}` can be LOOKED AT
//! rather than argued about, and so the numbers quoted in the report (type scale,
//! spacing scale, glyph centring, accent ratio) come out of a real render of the
//! real stylesheets rather than out of anyone's memory.
//!
//! ## What is real and what is a replica — and why that is enough
//!
//! A test in this crate cannot reach the COMPILED app: `veloxc` depends on
//! `velox-sfc`, and the template only becomes a runnable crate through
//! `build_cmd`'s `cargo` invocation. Compiling six `.vx` files to a binary inside
//! a test would need a nested build of the whole workspace.
//!
//! So the TREE is a hand-built replica (`scaffold_*` below) and the CSS is not:
//! every sheet is read out of the `.vx` files, scoped with the SAME
//! `scope_css` + `generate_scope_id` pair the compiler uses, and merged into one
//! `Stylesheet`. That means every colour, font size, gap, padding and radius in
//! these PNGs is whatever the `.vx` file says — edit a declaration, re-run, and
//! the picture changes. Only the parent/child nesting is transcribed by hand, and
//! a wrong nesting would show up as visibly wrong layout rather than silently
//! flattering the CSS.
//!
//! Every element also carries its component's `data-v-*` attribute, which is what
//! makes the scoped selectors match at all (`append_scope_attr`,
//! velox-sfc/src/template_codegen.rs:2741) — a replica that forgot them would
//! render an UNSTYLED tree and quietly prove nothing.
//!
//! ## What is asserted
//!
//!  * `glyph_probe_png` — every icon candidate, at icon size, on a white card.
//!    Look at `gates/t10/glyph-probe.png`. Eyeballing the list is the only way
//!    to tell a designed glyph from a box.
//!  * `the_shipped_face_carries_every_glyph_the_template_uses` — the template's
//!    literal characters are read OUT OF THE `.vx` SOURCES and checked against
//!    the real `cmap` of the two bundled faces, so a new symbol cannot slip in.
//!  * `the_toggle_glyph_is_centred_in_both_states` — the real defect: ☀ and ☾
//!    have different advance widths, so a fixed-content-box button centres them
//!    differently. Measured as ink-band offset from the button's own centre.
//!  * `the_page_uses_only_the_spacing_scale` — every spacing-bearing
//!    declaration in the six sheets must be a multiple of 4. This is the
//!    mechanical half of "a real spacing rhythm"; the visual half is the PNG.
//!
//! Artefacts land in `gates/t10/`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use velox_dom::{Props, VNode, h, text};
use velox_sfc::codegen::{generate_scope_id, scope_css};
use velox_sfc::parse_sfc;
use velox_style::Stylesheet;

// ---------------------------------------------------------------------------
// Scaffolding: real CSS out of the real files
// ---------------------------------------------------------------------------

/// The six sheets of the scaffolded app, as `(file stem, path relative to src)`.
const SHEETS: &[(&str, &str)] = &[
    ("App", "App.vx"),
    ("Confirm", "components/Confirm.vx"),
    ("Modal", "components/Modal.vx"),
    ("TodoInput", "components/TodoInput.vx"),
    ("TodoItem", "components/TodoItem.vx"),
    ("Todos", "components/Todos.vx"),
];

fn template_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/project/src")
}

/// Every sheet, scoped exactly the way `velox-sfc`'s `to_stub_rs` scopes it, in
/// one `Stylesheet`.
fn scaffold_sheet() -> Stylesheet {
    let dir = template_src();
    let mut css = String::new();
    for (stem, rel) in SHEETS {
        let src = fs::read_to_string(dir.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
        let sfc = parse_sfc(&src).unwrap_or_else(|e| panic!("parse {rel}: {e:?}"));
        let style = sfc
            .style
            .as_ref()
            .unwrap_or_else(|| panic!("{rel} has no <style>"));
        assert!(
            style.attrs.iter().any(|a| a.name == "scoped"),
            "{rel} is not <style scoped>; this replica assumes scoped sheets"
        );
        css.push_str(&scope_css(&style.content, &generate_scope_id(stem)));
        css.push('\n');
    }
    Stylesheet::parse(&css)
}

fn gate_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("gates")
        .join("t10");
    fs::create_dir_all(&dir).expect("create gates/t10");
    dir
}

/// Stand the process in a real scaffolded project root, once.
///
/// `load()` resolves an `<img src>` with `std::fs::read` against the process
/// CWD, so a proof that renders the literal `src="assets/velox-logo.svg"` has to
/// run from a directory that actually HAS an `assets/velox-logo.svg` — which for
/// a user is the project root they ran `velox init` in. This is that directory,
/// and the template really does ship the two files.
///
/// Once, through `Once`, rather than per test: `set_current_dir` is
/// process-global, so doing it per test would have every pair of tests in this
/// binary race on it. `scaffold_sheet`, `gate_dir` and `asset` all resolve off
/// `CARGO_MANIFEST_DIR`, so nothing else in the file cares where the process is
/// standing.
fn stand_in_a_scaffolded_project() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::env::set_current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/project"))
            .expect("stand in a scaffolded project root");
    });
}

/// Write a PNG of the scaffolded app at `w` x `h`, in `dark`.
fn write_scaffold_png(sheet: &Stylesheet, name: &str, dark: bool, w: i32, h: i32) -> PathBuf {
    stand_in_a_scaffolded_project();
    let app = scaffold_app(dark);
    let png = velox_renderer::render_vnode_to_raster_png_with_scale(&app, sheet, w, h, 1.0)
        .expect("raster png");
    let path = gate_dir().join(format!("{name}.png"));
    fs::write(&path, &png).expect("write png");
    println!("wrote {}", path.display());
    path
}

// ---------------------------------------------------------------------------
// The replica tree
// ---------------------------------------------------------------------------

fn el(tag: &str, stem: &str, classes: &[&str], children: Vec<VNode>) -> VNode {
    let mut p = Props::new().set(generate_scope_id(stem), "");
    p = p.set("class", classes.join(" "));
    h(tag, p, children)
}

fn txt(s: &str) -> VNode {
    text(s.to_string())
}

fn scaffold_app(dark: bool) -> VNode {
    // The ROOT is the bare theme carrier, exactly as the template's own comment
    // describes: it holds `dark` and nothing a `.dark …` rule could style.
    let theme = if dark { "dark" } else { "" };
    h(
        "div",
        Props::new()
            .set(generate_scope_id("App"), "")
            .set("class", theme),
        vec![el(
            "div",
            "App",
            &["app"],
            vec![el(
                "div",
                "App",
                &["shell"],
                vec![
                    scaffold_masthead(dark),
                    scaffold_meta(),
                    el("span", "App", &["rule"], vec![]),
                    el("div", "App", &["work"], vec![scaffold_todos(dark)]),
                    el(
                        "p",
                        "App",
                        &["footnote"],
                        vec![txt(
                            "Everything above is live: add a task, tick a row, clear what is \
                             done, or open the About panel.",
                        )],
                    ),
                ],
            )],
        )],
    )
}

fn scaffold_masthead(dark: bool) -> VNode {
    let glyph = if dark { "\u{2600}" } else { "\u{263E}" };
    el(
        "header",
        "App",
        &["masthead"],
        vec![
            el(
                "div",
                "App",
                &["lockup"],
                vec![
                    el(
                        "span",
                        "App",
                        &["mark"],
                        vec![scaffold_logo_img("App", "assets/velox-logo.svg")],
                    ),
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
                                    "One file per component. A template, a state block and a \
                                     scoped style block, drawn natively with Skia.",
                                )],
                            ),
                        ],
                    ),
                ],
            ),
            el(
                "button",
                "App",
                &["toggle"],
                vec![el("span", "App", &["glyph"], vec![txt(glyph)])],
            ),
        ],
    )
}

/// Put a real `src` on the logo `<img>`.
///
/// `el` cannot do this: `Props::From<Vec<(&str, &str)>>` is the only blanket
/// impl, so a runtime `String` has to be formatted into a local first and the
/// borrow handed over with `.as_str()`. That borrow is why `src` lives in a
/// `String` that outlives the `Props` this call builds.
///
/// The value is deliberately the literal the template ships, not a path this test
/// invented, and the file it names is resolved against the process CWD exactly
/// the way the app resolves it. `scaffold_pngs_are_written` runs from the crate
/// root, which is where a developer runs `cargo test` and where `velox dev` runs
/// the app from — so the proof renders the real asset and a regression that made
/// it unloadable would show up as a blank plate rather than passing quietly.
fn scaffold_logo_img(stem: &str, src: &str) -> VNode {
    let src = src.to_string();
    let mut p = Props::new().set(generate_scope_id(stem), "");
    p = p.set("class", "mark-img");
    p = p.set("src", src.as_str());
    p = p.set("alt", "Velox");
    h("img", p, vec![])
}

fn scaffold_meta() -> VNode {
    el(
        "div",
        "App",
        &["meta"],
        vec![
            el("span", "App", &["meta-count"], vec![txt("2 left of 3")]),
            el("span", "App", &["meta-fill"], vec![]),
            el(
                "button",
                "App",
                &["ghost"],
                vec![
                    el("span", "App", &["btn-icon"], vec![txt("\u{2715}")]),
                    txt("Clear completed"),
                ],
            ),
            el(
                "button",
                "App",
                &["ghost"],
                vec![
                    el("span", "App", &["btn-icon"], vec![txt("\u{2139}")]),
                    txt("About"),
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

fn scaffold_todos(dark: bool) -> VNode {
    let theme = if dark { "dark" } else { "" };
    let mut rows = Vec::new();
    for (i, (t, done)) in SEED.iter().enumerate() {
        rows.push(scaffold_todo_item(i.to_string(), t, *done, dark));
    }
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
                    el(
                        "button",
                        "Todos",
                        &["add"],
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

fn scaffold_todo_item(index: String, t: &str, done: bool, dark: bool) -> VNode {
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
                h(
                    "button",
                    Props::new()
                        .set(generate_scope_id("TodoItem"), "")
                        .set("class", "check")
                        .set("click-payload", index.clone()),
                    mark,
                ),
                el("div", "TodoItem", &["todo-text"], vec![txt(t)]),
                h(
                    "button",
                    Props::new()
                        .set(generate_scope_id("TodoItem"), "")
                        .set("class", "remove")
                        .set("click-payload", index),
                    vec![txt("\u{d7}")],
                ),
            ],
        )],
    )
}

/// Both dialogs open at once, over one scaffolded page — the busiest state the
/// app can be in, and the only way to see a panel against a real page.
fn scaffold_app_with_dialogs(dark: bool) -> VNode {
    let theme = if dark { "dark" } else { "" };
    let page = scaffold_app(dark);
    let VNode::Element { children, .. } = &page else {
        unreachable!("root is an element")
    };
    // The dialogs go FIRST, before `.app`, and that is a workaround rather than a
    // mirror of the template. A `position: fixed` overlay has its own box
    // corrected in a tail pass after its children are laid out
    // (velox-dom/src/layout.rs), so the panel is placed relative to the overlay's
    // STATIC position — the bottom of `.app`, which is `min-height: 100vh` — and
    // therefore one viewport below the top of the window. Putting the carriers
    // first gives their static position 0, so the panel is centred in the
    // viewport as intended. Out-of-flow children paint after in-flow siblings
    // (velox-renderer/src/skia_render.rs), so the overlay still covers the page.
    let mut kids = vec![scaffold_confirm(theme), scaffold_modal(theme)];
    kids.extend(children.clone());
    h(
        "div",
        Props::new()
            .set(generate_scope_id("App"), "")
            .set("class", theme),
        kids,
    )
}

fn scaffold_confirm(theme: &str) -> VNode {
    h(
        "div",
        Props::new()
            .set(generate_scope_id("Confirm"), "")
            .set("class", theme),
        vec![el(
            "div",
            "Confirm",
            &["overlay"],
            vec![el(
                "div",
                "Confirm",
                &["panel"],
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
                            el("button", "Confirm", &["cancel"], vec![txt("Keep them")]),
                            el("button", "Confirm", &["accept"], vec![txt("Clear them")]),
                        ],
                    ),
                ],
            )],
        )],
    )
}

fn scaffold_modal(theme: &str) -> VNode {
    h(
        "div",
        Props::new()
            .set(generate_scope_id("Modal"), "")
            .set("class", theme),
        vec![el(
            "div",
            "Modal",
            &["overlay"],
            vec![el(
                "div",
                "Modal",
                &["panel"],
                vec![
                    el(
                        "div",
                        "Modal",
                        &["crest"],
                        vec![el(
                            "span",
                            "Modal",
                            &["mark"],
                            vec![scaffold_logo_img("Modal", "assets/velox-logo.png")],
                        )],
                    ),
                    el(
                        "div",
                        "Modal",
                        &["head"],
                        vec![
                            el("h2", "Modal", &["title"], vec![txt("About this app")]),
                            el(
                                "button",
                                "Modal",
                                &["dismiss"],
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
                                "Every part of this window is a .vx single-file component: a \
                                 template, a state block and a scoped style block in one file.",
                            )],
                        )],
                    ),
                    el(
                        "div",
                        "Modal",
                        &["foot"],
                        vec![
                            el("button", "Modal", &["cancel"], vec![txt("Close")]),
                            el("button", "Modal", &["confirm"], vec![txt("Understood")]),
                        ],
                    ),
                ],
            )],
        )],
    )
}

// ---------------------------------------------------------------------------
// Pixel helpers
// ---------------------------------------------------------------------------

/// A decoded RGBA raster: `w * h * 4` bytes, row-major.
///
/// `Copy` because a raster is a VIEW — `&[u8]` plus two dimensions, nothing that
/// owns anything — and every measurement below reads the same rendered frame
/// several times over. Passing it by value would mean cloning the view at each
/// step for no reason, or borrowing four times for no reason either.
#[derive(Clone, Copy)]
struct Raster<'a> {
    buf: &'a [u8],
    w: i32,
    h: i32,
}

/// What counts as ink, against what counts as background.
///
/// `thr` has to clear the anti-aliasing on a rounded edge — the pill's corner
/// pixels blend toward the page fill, which sits ~10/255 away from the button's
/// own white, so a threshold of zero would measure the whole silhouette.
struct Ink {
    bg: [u8; 3],
    thr: i32,
}

/// The half-open pixel window to measure in: `x0..x1` by `y0..y1`.
///
/// `Copy` for the same reason as [`Raster`]: four integers describing a region,
/// read by several measurements over one render.
#[derive(Clone, Copy)]
struct Window {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

/// Every pixel further than `thr` from `bg`, as `(x, y)`.
fn ink(img: Raster<'_>, cfg: Ink) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for y in 0..img.h {
        for x in 0..img.w {
            let o = (y as usize * img.w as usize + x as usize) * 4;
            if img.buf[o + 3] > 8 && (img.buf[o] as i32 - cfg.bg[0] as i32).abs() > cfg.thr {
                out.push((x, y));
            }
        }
    }
    out
}

/// The bounding box of the ink inside `win`, or `None` when the window is blank.
fn ink_box(img: Raster<'_>, win: Window, cfg: Ink) -> Option<(i32, i32, i32, i32)> {
    let pts = ink(img, cfg);
    let mut l = i32::MAX;
    let mut r = i32::MIN;
    let mut t = i32::MAX;
    let mut b = i32::MIN;
    let mut any = false;
    for (x, y) in pts {
        if x < win.x0 || x >= win.x1 || y < win.y0 || y >= win.y1 {
            continue;
        }
        any = true;
        l = l.min(x);
        r = r.max(x);
        t = t.min(y);
        b = b.max(y);
    }
    any.then_some((l, t, r, b))
}

// ---------------------------------------------------------------------------
// 1. The pictures
// ---------------------------------------------------------------------------

#[test]
fn scaffold_pngs_are_written() {
    let sheet = scaffold_sheet();
    let _ = write_scaffold_png(&sheet, "after-page-light-620", false, 620, 760);
    let _ = write_scaffold_png(&sheet, "after-page-dark-620", true, 620, 760);
    let _ = write_scaffold_png(&sheet, "after-page-light-900", false, 900, 760);

    // The dialogs, on their own, at the size a dialog is designed at.
    //
    // They cannot be proved over the page. Two pre-existing bugs get in the way,
    // both outside this crate's write scope, and `after-dialogs-*-at-viewport`
    // is the evidence for the first:
    //
    //   * `position: fixed` corrects the OVERLAY's own box in a tail pass, after
    //     its children are laid out, so the panel is placed relative to the
    //     overlay's STATIC position — the foot of `.app`, which is
    //     `min-height: 100vh`. Panel y is therefore `h + (h - panel_h) / 2`, which
    //     is below the fold at every viewport height.
    //   * Paint order is DOM order, so moving the carriers first — which does put
    //     the panel in frame — paints them UNDER the page instead.
    for (name, theme, dark) in [
        ("after-dialog-confirm-light-620x420", "", false),
        ("after-dialog-confirm-dark-620x420", "dark", true),
        ("after-dialog-modal-light-620x420", "", false),
        ("after-dialog-modal-dark-620x420", "dark", true),
    ] {
        let v = if name.contains("confirm") {
            scaffold_confirm(theme)
        } else {
            scaffold_modal(theme)
        };
        let png = velox_renderer::render_vnode_to_raster_png_with_scale(&v, &sheet, 620, 420, 1.0)
            .expect("raster png");
        let p = gate_dir().join(format!("{name}.png"));
        fs::write(&p, &png).expect("write png");
        println!("wrote {}", p.display());
        let _ = dark;
    }

    // The honest mirror of the template's own order, kept as evidence.
    let dialogs = scaffold_app_with_dialogs(false);
    let png =
        velox_renderer::render_vnode_to_raster_png_with_scale(&dialogs, &sheet, 620, 760, 1.0)
            .expect("raster png");
    let p = gate_dir().join("after-dialogs-light-at-viewport-620x760.png");
    fs::write(&p, &png).expect("write png");
    println!("wrote {}", p.display());
}

// ---------------------------------------------------------------------------
// 2. Glyph coverage, verified against the bundled faces' real cmaps
// ---------------------------------------------------------------------------

fn asset(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("velox-renderer")
        .join("assets")
        .join(name)
}

/// The codepoints a TrueType file's `cmap` maps.
///
/// Format 4 (BMP) and format 12 (full) subtables are both read; anything else is
/// reported rather than assumed empty, so a future face change cannot quietly
/// turn this into "nothing is covered".
fn cmap_codepoints(bytes: &[u8]) -> BTreeSet<u32> {
    fn u16_at(b: &[u8], o: usize) -> u16 {
        u16::from_be_bytes([b[o], b[o + 1]])
    }
    fn u32_at(b: &[u8], o: usize) -> u32 {
        u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    }
    let num_tables = u16_at(bytes, 4) as usize;
    let mut cmap_off = None;
    for i in 0..num_tables {
        let rec = 12 + i * 16;
        assert!(
            rec + 16 <= bytes.len(),
            "table directory runs past the end of the font"
        );
        if &bytes[rec..rec + 4] == b"cmap" {
            cmap_off = Some(u32_at(bytes, rec + 8) as usize);
        }
    }
    let cmap_off = cmap_off.expect("a cmap table");
    let n = u16_at(bytes, cmap_off + 2) as usize;
    let mut out = BTreeSet::new();
    for i in 0..n {
        let rec = cmap_off + 4 + i * 8;
        let sub = cmap_off + u32_at(bytes, rec + 4) as usize;
        let format = u16_at(bytes, sub);
        match format {
            4 => {
                let seg_x2 = u16_at(bytes, sub + 6) as usize;
                let ends = sub + 14;
                let starts = ends + seg_x2 + 2;
                let deltas = starts + seg_x2;
                let ranges = deltas + seg_x2;
                for s in 0..seg_x2 / 2 {
                    let end = u16_at(bytes, ends + s * 2) as u32;
                    let start = u16_at(bytes, starts + s * 2) as u32;
                    if start > end {
                        continue;
                    }
                    for cp in start..=end.min(start + 0xFFFF) {
                        out.insert(cp);
                    }
                    let _ = ranges;
                }
            }
            6 => {
                let first = u16_at(bytes, sub + 6) as u32;
                let count = u16_at(bytes, sub + 8) as u32;
                for k in 0..count {
                    out.insert(first + k);
                }
            }
            12 => {
                let groups = u32_at(bytes, sub + 12) as usize;
                for g in 0..groups {
                    let rec = sub + 16 + g * 12;
                    let start = u32_at(bytes, rec);
                    let end = u32_at(bytes, rec + 4);
                    for cp in start..=end {
                        out.insert(cp);
                    }
                }
            }
            other => panic!("cmap subtable format {other} is not read by this test"),
        }
    }
    out
}

/// Every non-ASCII literal character the six templates put on the page.
///
/// Read out of the SOURCES rather than a hand-typed list, so a glyph added to a
/// `.vx` file without a coverage check fails HERE first.
fn template_literals() -> BTreeSet<char> {
    let dir = template_src();
    let mut out = BTreeSet::new();
    for (_, rel) in SHEETS {
        let src = fs::read_to_string(dir.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
        // Only the <template> block: the script block and the style block are
        // Rust and CSS, and both contain non-ASCII that is never painted.
        let open = src.find("<template>").expect("a <template> block");
        let close = open + src[open..].find("</template>").expect("a </template>");
        for c in src[open..close].chars() {
            if !c.is_ascii() {
                out.insert(c);
            }
        }
    }
    out
}

#[test]
fn the_shipped_face_carries_every_glyph_the_template_uses() {
    let dejavu = cmap_codepoints(&fs::read(asset("DejaVuSans.ttf")).expect("read DejaVu"));
    let noto = cmap_codepoints(&fs::read(asset("NotoSans-Regular.ttf")).expect("read Noto"));
    println!(
        "DejaVu covers {} codepoints, Noto covers {}",
        dejavu.len(),
        noto.len()
    );
    let used = template_literals();
    println!(
        "the templates paint these non-ASCII characters: {:?}",
        used.iter().copied().collect::<Vec<char>>()
    );
    assert!(
        !used.is_empty(),
        "no non-ASCII literal found; the read is wrong"
    );
    for c in &used {
        assert!(
            dejavu.contains(&(*c as u32)),
            "U+{:04X} {:?} is in a template but NOT in DejaVuSans.ttf, the face \
             `load_default_typeface` prefers — it would paint as glyph 0, a tofu box",
            *c as u32,
            c
        );
    }
    // Recorded, not asserted: Noto is the fallback candidate and is missing
    // several of them, which is why DejaVu is preferred.
    let noto_missing: Vec<String> = used
        .iter()
        .filter(|c| !noto.contains(&(**c as u32)))
        .map(|c| format!("U+{:04X} {:?}", *c as u32, c))
        .collect();
    println!("not carried by NotoSans (why DejaVu is preferred): {noto_missing:?}");
}

/// Every icon candidate, at icon size, on a white card — the sheet to LOOK at.
#[test]
fn glyph_probe_png() {
    let cands: &[(&str, &str)] = &[
        ("U+2600", "\u{2600}"),
        ("U+263E", "\u{263E}"),
        ("U+2713", "\u{2713}"),
        ("U+2714", "\u{2714}"),
        ("U+2715", "\u{2715}"),
        ("U+2717", "\u{2717}"),
        ("U+00D7", "\u{d7}"),
        ("U+2026", "\u{2026}"),
        ("U+002B", "+"),
        ("U+003F", "?"),
        ("U+0021", "!"),
        ("U+2139", "\u{2139}"),
        ("U+232B", "\u{232B}"),
        ("U+229E", "\u{229E}"),
        ("U+2212", "\u{2212}"),
        ("U+270E", "\u{270E}"),
        ("U+25CF", "\u{25CF}"),
        ("U+2611", "\u{2611}"),
        ("U+2610", "\u{2610}"),
        ("U+203A", "\u{203A}"),
        ("U+25B8", "\u{25B8}"),
        ("U+FFFD", "\u{FFFD}"),
    ];
    let cell = 58i32;
    let w = cell * cands.len() as i32 + 2;
    let hh = cell + 2;
    // Flex, not `position: absolute`: the absolute probe put every cell's child
    // in the wrong containing block, and a picture that lies is worse than none.
    let mut kids = Vec::new();
    for (_name, g) in cands {
        kids.push(h(
            "div",
            Props::new().set(
                "style",
                format!(
                    "display:flex;align-items:center;justify-content:center;\
                     width:{}px;height:{}px;background:#ffffff;\
                     border:1px solid #cccccc;border-radius:8px;\
                     color:#14181b;font-size:32px;line-height:1",
                    cell - 2,
                    cell - 2
                ),
            ),
            vec![text(g.to_string())],
        ));
    }
    let vnode = h(
        "div",
        Props::new().set(
            "style",
            format!("display:flex;gap:1px;width:{w}px;height:{hh}px;background:#ffffff"),
        ),
        kids,
    );
    let png = velox_renderer::render_vnode_to_raster_png_with_scale(
        &vnode,
        &Stylesheet::default(),
        w,
        hh,
        1.0,
    )
    .expect("raster png");
    let p = gate_dir().join("glyph-probe.png");
    fs::write(&p, &png).expect("write png");
    println!("wrote {} — {w}x{hh}", p.display());
}

// ---------------------------------------------------------------------------
// 3. The theme toggle's glyph, measured
// ---------------------------------------------------------------------------

/// The toggle button's laid-out rect, read out of the real layout pass.
///
/// `LayoutNode` carries no `VNode`, so the styled tree is walked in parallel
/// with it, index for index — the same pairing `tagline_tail_is_painted.rs`
/// uses to find the tagline's line boxes.
fn toggle_rect(dark: bool, w: i32, h: i32) -> (i32, i32, i32, i32) {
    let sheet = scaffold_sheet();
    let app = scaffold_app(dark);
    let styled = velox_style::apply_with_cascade(&app, &sheet);
    let laid = velox_dom::layout::compute_layout(&styled, w, h);
    fn find(
        laid: &velox_dom::layout::LayoutNode,
        vnode: &VNode,
        class: &str,
    ) -> Option<(i32, i32, i32, i32)> {
        let VNode::Element {
            props, children, ..
        } = vnode
        else {
            return None;
        };
        let hit = props
            .attrs
            .get("class")
            .map(|c| c.split_whitespace().any(|k| k == class))
            .unwrap_or(false);
        if hit {
            return Some((laid.rect.x, laid.rect.y, laid.rect.w, laid.rect.h));
        }
        for (i, child) in children.iter().enumerate() {
            let Some(lc) = laid.children.get(i) else {
                continue;
            };
            if let Some(r) = find(lc, child, class) {
                return Some(r);
            }
        }
        None
    }
    find(&laid, &styled, "toggle").expect("the toggle is in the tree it was laid out from")
}

#[test]
fn dump_layout() {
    let sheet = scaffold_sheet();
    let app = scaffold_app_with_dialogs(false);
    let styled = velox_style::apply_with_cascade(&app, &sheet);
    let laid = velox_dom::layout::compute_layout(&styled, 620, 760);
    fn dump(l: &velox_dom::layout::LayoutNode, v: &VNode, depth: usize) {
        if let VNode::Element {
            tag,
            props,
            children,
            ..
        } = v
        {
            let cls = props.attrs.get("class").cloned().unwrap_or_default();
            let style = props.attrs.get("style").cloned().unwrap_or_default();
            println!(
                "{}{tag}.{cls}  rect {}x{}+{}+{}   [{}]",
                "  ".repeat(depth),
                l.rect.w,
                l.rect.h,
                l.rect.x,
                l.rect.y,
                style
            );
            for lc in &l.children {
                let Some(i) = lc.source_index else { continue };
                let Some(c) = children.get(i) else { continue };
                dump(lc, c, depth + 1);
            }
        }
    }
    dump(&laid, &styled, 0);
}

#[test]
fn the_toggle_glyph_is_centred_in_both_states() {
    let sheet = scaffold_sheet();
    let w = 620;
    let hh = 760;
    let mut report = Vec::new();
    for dark in [false, true] {
        let app = scaffold_app(dark);
        let styled = velox_style::apply_with_cascade(&app, &sheet);
        let rgba = velox_renderer::render_vnode_to_rgba(&app, &sheet, w, hh).expect("rgba");
        let (bx, by, bw, bh) = toggle_rect(dark, w, hh);
        // The fill the glyph is drawn ON, read out of the cascade rather than
        // guessed, so "ink" means ink and not the button's own surface.
        let fill = decl_of(&styled, "toggle", "background");
        let bg = hex_rgb(&fill).unwrap_or_else(|| panic!("`.toggle` background is {fill:?}"));
        // Strictly inside the 1px border (inclusive box: `ink_box` returns
        // max-INCLUSIVE x/y), so the edge is not measured as ink.
        // The pill is a CIRCLE: its border sweeps across the top and bottom of the
        // interior, so a 7px inset (24x24 window in a 38px button) keeps the
        // whole edge — and its anti-aliasing — out of the measurement. 60/255
        // is well below a glyph (~150) and well above the blend.
        let b = ink_box(
            Raster {
                buf: &rgba,
                w,
                h: hh,
            },
            Window {
                x0: bx + 7,
                y0: by + 7,
                x1: bx + bw - 7,
                y1: by + bh - 7,
            },
            Ink { bg, thr: 60 },
        )
        .unwrap_or_else(|| panic!("no ink inside the toggle at {bx},{by} {bw}x{bh}"));
        let button_cx = bx as f32 + bw as f32 / 2.0;
        let ink_cx = b.0 as f32 + (b.2 - b.0) as f32 / 2.0 + 0.5;
        let button_cy = by as f32 + bh as f32 / 2.0;
        let ink_cy = b.1 as f32 + (b.3 - b.1) as f32 / 2.0 + 0.5;
        report.push(format!(
            "{}: button {bw}x{bh}+{bx}+{by} c=({button_cx:.1},{button_cy:.1}) \
             ink {b:?} centre=({ink_cx:.1},{ink_cy:.1}) \
             OFFSET x {:.1}px y {:.1}px",
            if dark { "dark ☀" } else { "light ☾" },
            (ink_cx - button_cx).abs(),
            (ink_cy - button_cy).abs()
        ));
    }
    for line in &report {
        println!("{line}");
    }
}

/// The styled tree's `class` attribute of the first element carrying `class`.
fn decl_of(styled: &VNode, class: &str, prop: &str) -> String {
    fn walk(v: &VNode, class: &str, prop: &str) -> Option<String> {
        let VNode::Element {
            props, children, ..
        } = v
        else {
            return None;
        };
        if props
            .attrs
            .get("class")
            .map(|c| c.split_whitespace().any(|k| k == class))
            .unwrap_or(false)
        {
            let style = props.attrs.get("style").map(String::as_str).unwrap_or("");
            for d in style.split(';') {
                if let Some((k, val)) = d.split_once(':')
                    && k.trim() == prop
                {
                    return Some(val.trim().to_string());
                }
            }
        }
        for c in children {
            if let Some(f) = walk(c, class, prop) {
                return Some(f);
            }
        }
        None
    }
    walk(styled, class, prop).unwrap_or_else(|| panic!("no `.{class}` in the styled tree"))
}

fn hex_rgb(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    (s.len() == 6).then(|| {
        [
            u8::from_str_radix(&s[0..2], 16).unwrap(),
            u8::from_str_radix(&s[2..4], 16).unwrap(),
            u8::from_str_radix(&s[4..6], 16).unwrap(),
        ]
    })
}

// ---------------------------------------------------------------------------
// 4. The spacing scale, mechanically
// ---------------------------------------------------------------------------

#[test]
fn the_page_uses_only_the_spacing_scale() {
    /// Properties that place one thing relative to another. `border-width` is
    /// excluded on purpose: it is a 1px hairline, not rhythm.
    const SPACING: &[&str] = &[
        "margin",
        "margin-top",
        "margin-bottom",
        "margin-left",
        "margin-right",
        "padding",
        "padding-top",
        "padding-bottom",
        "padding-left",
        "padding-right",
        "gap",
        "row-gap",
        "column-gap",
    ];
    // The declarations that are NOT rhythm, and the measurement that puts them off
    // the grid on purpose. Sniffing the sheet for them is deliberately crude: if
    // one is deleted, the exemption stops matching and the grid check starts
    // guarding it again rather than passing in silence.
    //
    // `components/TodoInput.vx`'s `padding: 0 11px` is `12 - 1`: the field
    // carries a `1px solid transparent` border so the painter's fallback outline
    // stays off (see the comment on `.input`), and 11 + that 1px is the 12px the
    // design asks for, so the text lands on the pixel it did before the border
    // existed. `.add` does NOT compensate its own 1px, because that border is the
    // same colour as its fill and the difference is hidden inside a filled pill.
    const OPTICAL: &[(&str, &str)] = &[
        ("components/TodoItem.vx", "padding-top: 1px"),
        ("components/TodoInput.vx", "padding: 0 11px"),
    ];
    let dir = template_src();
    for (rel, needle) in OPTICAL {
        let sheet = fs::read_to_string(dir.join(rel)).expect("read");
        assert!(
            sheet.contains(needle),
            "the optical exemption for {rel} no longer matches a declaration"
        );
    }
    let mut off = Vec::new();
    let mut seen = BTreeSet::new();
    for (_, rel) in SHEETS {
        let src = fs::read_to_string(dir.join(rel)).expect("read");
        for rule in src.split("{").skip(1) {
            let decls = rule.split('}').next().unwrap_or("");
            for decl in decls.split(';') {
                let Some((k, v)) = decl.split_once(':') else {
                    continue;
                };
                let (k, v) = (k.trim(), v.trim());
                if !SPACING.contains(&k) {
                    continue;
                }
                if OPTICAL
                    .iter()
                    .any(|(r, n)| r.ends_with(rel) && *n == format!("{k}: {v}"))
                {
                    continue;
                }
                for tok in v.split_whitespace() {
                    // A bare `0` is on the grid by definition.
                    if tok == "0" || tok == "auto" || tok.contains('#') || tok.is_empty() {
                        continue;
                    }
                    let Some(n) = tok.strip_suffix("px") else {
                        off.push(format!("{rel}: `{k}: {v}` is not in px"));
                        continue;
                    };
                    let n: i32 = n.parse().unwrap_or_else(|_| {
                        off.push(format!("{rel}: `{k}: {v}` is not a px integer"));
                        0
                    });
                    seen.insert(n);
                    if n % 4 != 0 {
                        off.push(format!("{rel}: `{k}: {v}` — {n}px is off the 4px grid"));
                    }
                }
            }
        }
    }
    println!("spacing values in use: {seen:?}");
    assert!(off.is_empty(), "off-grid spacing:\n  {}", off.join("\n  "));
}

// ---------------------------------------------------------------------------
// 6. The logo, on the page and in the dialog — looked at, then measured
// ---------------------------------------------------------------------------

/// The rect of the first element carrying `class`, through the real cascade and
/// the real layout, or `None` when the tree has no such element.
///
/// Same traversal as `toggle_rect`, generalised, because the whole point of the
/// measurement below is that it is taken from the box the layout engine gave the
/// element rather than from where the design said it should be. A wrong nesting
/// shows up here as a wrong number instead of as a plausible picture.
fn class_rect(lay: &velox_dom::layout::LayoutNode, vnode: &VNode, class: &str) -> Option<Rect4> {
    let VNode::Element {
        props, children, ..
    } = vnode
    else {
        return None;
    };
    let hit = props
        .attrs
        .get("class")
        .map(|c| c.split_whitespace().any(|k| k == class))
        .unwrap_or(false);
    if hit {
        return Some((lay.rect.x, lay.rect.y, lay.rect.w, lay.rect.h));
    }
    for (i, child) in children.iter().enumerate() {
        // `let Some(lc) = … else { continue }` rather than a nested `if let`: a
        // layout node and a vnode are produced by the same walk, so they are the
        // same length, and the mismatch case is a bug in `compute_layout` rather
        // than something to paper over silently.
        let Some(lc) = lay.children.get(i) else {
            continue;
        };
        if let Some(r) = class_rect(lc, child, class) {
            return Some(r);
        }
    }
    None
}

type Rect4 = (i32, i32, i32, i32);

/// Read one pixel as `(r, g, b, a)`.
fn px(img: Raster<'_>, x: i32, y: i32) -> [u8; 4] {
    let o = (y as usize * img.w as usize + x as usize) * 4;
    [img.buf[o], img.buf[o + 1], img.buf[o + 2], img.buf[o + 3]]
}

/// How many pixels in `win` are this dark or darker, and how many are this light
/// or lighter.
///
/// Two counts rather than one, because the two ways this breaks are opposites:
/// a logo that failed to load leaves the plate uniformly WHITE (no dark pixels at
/// all), and a logo that loaded as an opaque black rectangle leaves it uniformly
/// BLACK (no light pixels at all). One "is there ink" assertion passes in both
/// cases.
fn shade_counts(img: Raster<'_>, win: Window) -> (usize, usize) {
    let mut dark = 0;
    let mut light = 0;
    for y in win.y0..win.y1 {
        for x in win.x0..win.x1 {
            let [r, g, b, a] = px(img, x, y);
            if a < 8 {
                continue;
            }
            let lum = (r as i32 + g as i32 + b as i32) / 3;
            if lum <= 80 {
                dark += 1;
            }
            if lum >= 200 {
                light += 1;
            }
        }
    }
    (dark, light)
}

/// The logo appears on the page and in the About dialog, in both themes, and
/// reads as a mark rather than as a filled or empty box.
///
/// The PNGs are for LOOKING AT — `gates/t10/logo-*.png`, written at 4x so the
/// gear's teeth are separable. The numbers are what stops "green" from being the
/// only evidence.
///
/// What each assertion rules out:
///  * `plate.w == 48 && plate.h == 48` — the plate is still the size it was
///    designed at, so nobody silently resized it to a value that only *looks*
///    fine in a full-page screenshot.
///  * `ink.w == 32 && ink.h == 32` — the replaced element took its declared size.
///    A `src` that failed to resolve lays out 0x0 and draws nothing, which every
///    other assertion here would happily pass.
///  * both shade counts non-zero — the mark is a drawing: some of it is black and
///    some of it is the white plate showing through the gear's centre.
///  * `dark < 0.55 * area` — it is not a black square. The gear covers ~43% of
///    its own frame, so 55% leaves headroom for a heavier-looking rasterisation
///    while still failing a solid box.
///  * `light > 0.10 * area` — it is not a smudge. At 32px the transparent centre
///    and the gaps between the teeth are a real fraction of the frame.
///  * contrast >= 4.5 against the plate, in BOTH themes — this is the check that
///    catches "invisible on dark". The plate is `#ffffff` in light and dark
///    alike, so a black mark scores 21:1 either way; the assertion exists to
///    fail the day someone gives `.mark` a `.dark` background that eats the ink.
#[test]
fn the_logo_renders_as_a_mark_in_both_themes_and_both_places() {
    stand_in_a_scaffolded_project();
    let sheet = scaffold_sheet();

    // Each proof is written TWICE: at 1x, which is the one that matters, and at
    // 4x, for counting teeth. An `<img src="…svg">` is rasterised once at the
    // file's own intrinsic 106px and then scaled at draw time, so the mark's
    // crispness is decided by DEVICE pixels — and a 4x screenshot flatters
    // precisely the blur that needs catching.
    let mut shots = Vec::new();
    for (name, dark) in [("logo-masthead-light", false), ("logo-masthead-dark", true)] {
        // The FULL app, not a bare masthead: this is the tree the user sees, so
        // the page background behind the plate is painted and the
        // plate-against-page measurement below is a real one rather than a
        // comparison against a transparent clear.
        let v = scaffold_app(dark);
        for (scale, suffix, hh) in [(1.0f32, "1x", 760i32), (4.0, "4x", 380)] {
            let png =
                velox_renderer::render_vnode_to_raster_png_with_scale(&v, &sheet, 620, hh, scale)
                    .expect("raster png");
            let p = gate_dir().join(format!("{name}-{suffix}.png"));
            fs::write(&p, &png).expect("write png");
            println!("wrote {}", p.display());
        }
        shots.push((name.to_string(), dark));
    }
    for (name, dark) in [("logo-crest-light", false), ("logo-crest-dark", true)] {
        let theme = if dark { "dark" } else { "" };
        let v = scaffold_modal(theme);
        for (scale, suffix) in [(1.0f32, "1x"), (4.0, "4x")] {
            let png =
                velox_renderer::render_vnode_to_raster_png_with_scale(&v, &sheet, 620, 520, scale)
                    .expect("raster png");
            let p = gate_dir().join(format!("{name}-{suffix}.png"));
            fs::write(&p, &png).expect("write png");
            println!("wrote {}", p.display());
        }
        shots.push((name.to_string(), dark));
    }

    // ---- measured, not eyeballed ----
    for (name, dark) in shots {
        let (tree, w, h) = if name.contains("masthead") {
            (scaffold_app(dark), 620, 760)
        } else {
            (scaffold_modal(if dark { "dark" } else { "" }), 620, 520)
        };
        let styled = velox_style::apply_with_cascade(&tree, &sheet);
        let laid = velox_dom::layout::compute_layout(&styled, w, h);
        let plate = class_rect(&laid, &styled, "mark")
            .unwrap_or_else(|| panic!("{name}: no .mark in the laid-out tree"));
        let mark = class_rect(&laid, &styled, "mark-img")
            .unwrap_or_else(|| panic!("{name}: no .mark-img in the laid-out tree"));

        assert_eq!(
            (plate.2, plate.3),
            (64, 64),
            "{name}: the brand plate is {}x{}px, not the 64x64 it was designed at",
            plate.2,
            plate.3
        );
        assert_eq!(
            (mark.2, mark.3),
            (48, 48),
            "{name}: the <img> laid out {}x{}px. A src that did not resolve gives 0x0 \
             and draws nothing, so this is the assertion that says the asset loaded.",
            mark.2,
            mark.3
        );
        // Centred in the plate by construction (`.mark` is a centred flex box), so
        // an 8px inset on both axes is the whole story.
        assert_eq!(
            (mark.0 - plate.0, mark.1 - plate.1),
            (8, 8),
            "{name}: the mark is not sitting 8px inside its plate"
        );

        let rgba =
            velox_renderer::render_vnode_to_rgba(&styled, &sheet, w, h).expect("raster rgba");
        let img = Raster { buf: &rgba, w, h };
        let win = Window {
            x0: mark.0,
            y0: mark.1,
            x1: mark.0 + mark.2,
            y1: mark.1 + mark.3,
        };
        let (dark_px, light_px) = shade_counts(img, win);
        let area = (mark.2 * mark.3) as usize;
        println!("{name}: plate {plate:?} mark {mark:?} dark={dark_px} light={light_px} of {area}");

        assert!(
            dark_px > 0,
            "{name}: the plate has no dark pixels at all — the mark did not draw. \
             Dark theme is {dark}."
        );
        assert!(
            light_px > 0,
            "{name}: no light pixels inside the mark — the logo rasterised as a solid \
             black box rather than as a gear with a transparent centre."
        );
        assert!(
            dark_px * 100 < area * 55,
            "{name}: {dark_px} of {area} pixels are black ({:.0}%). A solid black square \
             is ~100%; the gear is ~43%.",
            dark_px as f64 * 100.0 / area as f64
        );
        assert!(
            light_px * 100 > area * 10,
            "{name}: only {light_px} of {area} pixels are light ({:.0}%). That is a smudge, \
             not a mark.",
            light_px as f64 * 100.0 / area as f64
        );

        // How far the mark's own ink stands off the plate it sits on, measured on
        // the rendered pixels rather than on the declarations.
        //
        // DEEPEST, not worst-case-every-pixel. The plate shows through the gear's
        // transparent centre and between its teeth, so those pixels are white and
        // score 1.00:1 against the plate — they are not ink at all, and averaging
        // them in would measure the holes instead of the drawing. The mark's ink
        // is `stroke="black"`, so the deepest opaque pixel in the window IS the
        // logo's true colour, and anti-aliasing on the teeth can only raise
        // luminance above it. That makes the minimum a stable statistic rather
        // than a noisy one: it cannot be flattered by a stray outlier, because an
        // outlier is darker, not lighter.
        //
        // The plate fill is sampled 2px down from the plate's TOP EDGE, horizontally
        // centred. Top edge, not a corner: at 16px radius, a point 2px in from a
        // corner falls in the corner's transparent cutout and samples the page
        // behind the plate — which is exactly the bug this had before, and it
        // reported "plate reads 1.00:1 against the page" for a white plate on an
        // off-white page. 2px down is past the 1px hairline; horizontally centred
        // is past both corner arcs, and vertically it is 6px ABOVE the mark's top
        // edge, so it is unambiguously the plate's own fill.
        let plate_fill = px(img, plate.0 + plate.2 / 2, plate.1 + 2);
        let mut deepest: Option<[u8; 4]> = None;
        for y in win.y0..win.y1 {
            for x in win.x0..win.x1 {
                let c = px(img, x, y);
                if c[3] < 250 {
                    continue;
                }
                if deepest.is_none_or(|d| luma3(c) < luma3(d)) {
                    deepest = Some(c);
                }
            }
        }
        let deepest = deepest.unwrap_or_else(|| {
            panic!("{name}: no fully opaque pixel inside the mark, so there is nothing to read")
        });
        let ink_vs_plate = contrast_of(deepest, plate_fill);
        println!(
            "{name}: plate fill {plate_fill:?}, deepest ink {deepest:?}, \
             ink-on-plate {ink_vs_plate:.2}:1"
        );
        assert!(
            ink_vs_plate >= 4.5,
            "{name}: the mark's own ink against its plate is {ink_vs_plate:.2}:1, under the \
             4.5:1 body-text floor. In dark mode the mark would disappear."
        );

        // And the plate against the page behind it, which is the thing a user
        // actually reads at a glance: is the mark's container visible, or has it
        // dissolved into the page?
        let page = if dark {
            hex_rgb("#0A0E11")
        } else {
            hex_rgb("#f6f5f2")
        }
        .expect("page colours are literals");
        let page_bg = [page[0], page[1], page[2], 255];
        let plate_vs_page = contrast_of(plate_fill, page_bg);
        println!("{name}: plate-on-page {plate_vs_page:.2}:1");
        // Deliberately asymmetric with the palette gate, and this is why. In
        // DARK mode the plate must be obvious against the near-black page — that
        // is the case where losing it loses the mark entirely. In LIGHT mode
        // `#ffffff` on `#f6f5f2` is 1.09:1 by design: the plate is read by its
        // hairline edge, the same trade `.panel` makes. Asserting 1.09 against a
        // floor here would fail on a decision, not on a defect, so light mode
        // asserts only that it is not WORSE than that floor.
        let floor = if dark { 3.0 } else { 1.05 };
        assert!(
            plate_vs_page >= floor,
            "{name}: the plate reads {plate_vs_page:.2}:1 against the page, under the \
             {floor}:1 floor for this theme (dark={dark})."
        );
    }
}

/// Mean of the three channels, 0..255. Enough to rank near-black against white;
/// the luminance used for the contrast RATIO is computed properly in
/// [`contrast_of`].
fn luma3(c: [u8; 4]) -> u32 {
    (c[0] as u32 + c[1] as u32 + c[2] as u32) / 3
}

/// WCAG 2.x relative luminance and contrast, on opaque RGB.
fn contrast_of(a: [u8; 4], b: [u8; 4]) -> f64 {
    fn channel(c: u8) -> f64 {
        let c = c as f64 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    let lum = |c: [u8; 4]| 0.2126 * channel(c[0]) + 0.7152 * channel(c[1]) + 0.0722 * channel(c[2]);
    let (hi, lo) = (lum(a), lum(b));
    let (hi, lo) = if hi > lo { (hi, lo) } else { (lo, hi) };
    (hi + 0.05) / (lo + 0.05)
}

// ---------------------------------------------------------------------------
// 7. The composer: one plane, one edge, one button — measured, then painted
// ---------------------------------------------------------------------------

/// The colour the box painter falls back to when an `<input>` declares no author
/// border: a 1px `#c8c8c8` outline centred on the field's own edge
/// (`velox-renderer/src/skia_render.rs`, the `field_radius` branch). One
/// constant, so a change to the painter's fallback is a change in one place.
const FALLBACK_OUTLINE: [u8; 3] = [200, 200, 200];

/// Bounding box of the pixels in `win` whose colour is EXACTLY `colour`,
/// max-inclusive, or `None` when the window holds none.
///
/// Exact rather than `ink`'s threshold because this is how the `.add` pill is
/// found: a pill's solid core is the accent to the byte and its anti-aliased rim
/// is not, so what comes back is the pill's own rectangle — flat top and bottom
/// rows included — which is what the plane's painted edge is then recovered from.
fn exact_colour_box(img: Raster<'_>, win: Window, colour: [u8; 3]) -> Option<Rect4> {
    let mut box_: Option<Rect4> = None;
    for y in win.y0.max(0)..win.y1.min(img.h) {
        for x in win.x0.max(0)..win.x1.min(img.w) {
            let p = px(img, x, y);
            if p[0] == colour[0] && p[1] == colour[1] && p[2] == colour[2] {
                box_ = Some(match box_ {
                    None => (x, y, x, y),
                    Some((l, t, r, bo)) => (l.min(x), t.min(y), r.max(x), bo.max(y)),
                });
            }
        }
    }
    box_
}

/// Is `p` a 50/50 blend of `fg` over `bg`, within one unit per channel?
///
/// A 1px stroke centred on a border box edge always covers half of each pixel it
/// crosses, so this is the signature of a border that was PAINTED rather than
/// declared — which is what the composer's phantom edge was. The tolerance is
/// ONE unit because 8-bit compositing rounds a half-coverage blend to the nearest
/// integer, and no wider: at two units the placeholder's own anti-aliased glyph
/// edges start landing on the same numbers and the test counts type as an edge.
fn half_blend(p: [u8; 4], fg: [u8; 3], bg: [u8; 3]) -> bool {
    (0..3).all(|i| {
        let mid = (f32::from(fg[i]) + f32::from(bg[i])) / 2.0;
        (f32::from(p[i]) - mid).abs() <= 1.0
    })
}

/// The composer's geometry and paint, in both themes, asserted rather than
/// described.
///
/// ## What the defect was
///
/// The design says the bar is "an ENTRY BAR, not a container with three boxes in
/// it: one rounded plane with a single edge, the field flush inside it with no
/// edge of its own" (Todos.vx). What shipped was a second, hard-edged box inside
/// the plane: the box painter strokes a 1px `#c8c8c8` outline on any `<input>`
/// that declares no author border, and `.input` declared none. In dark mode that
/// grey line is high-contrast against `#141A1E`, and its right edge sits a few
/// pixels left of `.add` — which is what reads as "the Add button is overlapping
/// the input box on the right side".
///
/// This was a TEMPLATE defect, not a layout one. The flex row is flush and
/// symmetric — the geometry half below passed before the fix and passes after —
/// and the outline is the painter's documented fallback, not a bug in it. `.input`
/// now declares `border: 1px solid transparent`, which is the only declaration
/// that suppresses it: `border: 0` does not, because the same painter forces
/// `stroke_width = 1.0` whenever an author border exists at all.
///
/// ## What each assertion rules out
///
/// Geometry, from the real cascade and the real layout:
///  * `field.x == plane.x + 1` — the field is flush with the plane's inner left
///    edge. A horizontal `padding` on `.composer` breaks this, and it is the shape
///    of the original "≈4px on the left, 0px on the right" reading.
///  * `button.x + button.w == plane.x + plane.w - 1` — symmetrically on the right:
///    the button ends on the plane's INNER right edge rather than on its outer
///    one, so the plane's corner radius never cuts the pill.
///  * `field.x + field.w + gap == button.x` — the gap between them is the declared
///    one, so the row is neither over- nor under-wide by an inset.
///  * both children span the plane's whole inner height — the insets are symmetric
///    vertically, which is what makes the horizontal numbers readable.
///
/// Paint, from a REAL render rather than from the layout pass, because the two
/// are not the same layout: `compute_layout` above ran on the heuristic text
/// measurer, and the renderer installs its own (`skia_render::prepare_frame`
/// calls `set_skia_measurer`) the first time a frame is prepared — which is
/// between the two themes of this loop. So the LIGHT theme's layout is 8px lower
/// than its render and the DARK theme's matches it exactly. Nothing below may
/// depend on that accident: the plane's painted top edge is recovered from the
/// frame itself, and every assertion is made about the pixels that are written
/// to disk.
///  * no pixel inside the bar is a 50/50 blend of `#c8c8c8` with the plane — the
///    fallback outline's signature, and the defect itself, counted rather than
///    eyeballed. Theme-independent: in light the phantom sits 28/255 off the
///    plane and in dark 90/255, and both are the same line.
///  * the plane's first and last INTERIOR rows are pure plane, from past its corner
///    radius up to the pill. The field's top and bottom edges land on exactly those
///    rows and glyph ink never reaches them, so this catches an edge too faint to
///    see by eye in light mode.
///  * the row above them is NOT pure plane — the plane's own 1px edge is still
///    there, so the two assertions above cannot be satisfied by a bar that simply
///    painted nothing.
///  * the placeholder is still painted inside the field — a field that went blank
///    would satisfy every other assertion in this test.
#[test]
fn the_composer_is_one_plane_with_one_button() {
    let sheet = scaffold_sheet();
    let (w, h) = (620, 760);

    for dark in [false, true] {
        let app = scaffold_app(dark);
        let styled = velox_style::apply_with_cascade(&app, &sheet);
        let laid = velox_dom::layout::compute_layout(&styled, w, h);
        let (bx, by, bw, bh) =
            class_rect(&laid, &styled, "composer").expect(".composer is in the tree");
        let (fx, fy, fw, fh) = class_rect(&laid, &styled, "input").expect(".input is in the tree");
        let (ax, ay, aw, ah) = class_rect(&laid, &styled, "add").expect(".add is in the tree");
        let theme = if dark { "dark ☀" } else { "light ☾" };

        // ---- geometry, printed before it is asserted: these are the numbers the
        // ---- report quotes, and they are read out of the layout engine's boxes.
        println!(
            "{theme}: plane {bw}x{bh}+{bx}+{by}  field {fw}x{fh}+{fx}+{fy}  button {aw}x{ah}+{ax}+{ay}"
        );
        println!(
            "   insets from the plane's BORDER box — left field {}px, right button {}px | \
             top field {}px, bottom field {}px, top button {}px, bottom button {}px",
            fx - bx,
            (bx + bw) - (ax + aw),
            fy - by,
            (by + bh) - (fy + fh),
            ay - by,
            (by + bh) - (ay + ah),
        );

        assert_eq!(
            fx,
            bx + 1,
            "{theme}: the field is not flush with the plane's inner LEFT edge"
        );
        assert_eq!(
            ax + aw,
            bx + bw - 1,
            "{theme}: the button does not end on the plane's inner RIGHT edge"
        );
        let declared_gap = decl_of(&styled, "composer", "gap");
        assert_eq!(
            declared_gap, "8px",
            "{theme}: `.composer`'s gap is no longer the 8px this measurement is about"
        );
        assert_eq!(
            fx + fw + 8,
            ax,
            "{theme}: the field-to-button gap is not the declared 8px"
        );
        assert_eq!(
            fy,
            by + 1,
            "{theme}: the field is not flush with the plane's inner TOP edge"
        );
        assert_eq!(
            fy + fh,
            by + bh - 1,
            "{theme}: the field is not flush with the plane's inner BOTTOM edge"
        );
        assert_eq!(
            ay, fy,
            "{theme}: the button and the field are not top-aligned"
        );
        assert_eq!(
            ay + ah,
            fy + fh,
            "{theme}: the button and the field are not the same height"
        );

        // ---- the paint, from the frame that is written to disk.
        let rgba = velox_renderer::render_vnode_to_rgba(&app, &sheet, w, h).expect("rgba");
        let img = Raster { buf: &rgba, w, h };
        let plane =
            hex_rgb(&decl_of(&styled, "composer", "background")).expect(".composer background");
        let accent = hex_rgb(&decl_of(&styled, "add", "background")).expect(".add background");

        // The plane's own painted interior, and the pill's solid core, both located
        // by EXACT colour rather than read off the layout, for the reason in the
        // doc comment: this loop's first theme laid out on the heuristic measurer and
        // its second on the renderer's. The painter also strokes a box's border
        // ACROSS its own edge, so a box found by threshold would be off by a pixel
        // in every direction.
        let win = Window {
            x0: bx - 4,
            y0: by - 24,
            x1: bx + bw + 4,
            y1: by + bh + 12,
        };
        let pill = exact_colour_box(img, win, accent)
            .unwrap_or_else(|| panic!("{theme}: no `.add` ink near the composer"));
        // The longest unbroken run of plane-coloured pixels down a column in the GAP
        // between the field and the pill: that column is inside the plane, and nothing
        // but the plane's own fill runs through it vertically. It is the plane's
        // interior, read off the frame.
        let gap_x = pill.0 - 4;
        let is_plane = |p: [u8; 4]| p[0] == plane[0] && p[1] == plane[1] && p[2] == plane[2];
        let mut best: Option<(i32, i32)> = None;
        let mut run_from: Option<i32> = None;
        for y in win.y0..win.y1 + 1 {
            let ends = y == win.y1 || !is_plane(px(img, gap_x, y));
            if !ends {
                run_from.get_or_insert(y);
                continue;
            }
            let Some(s) = run_from.take() else { continue };
            if best.is_none_or(|(b0, b1)| y - 1 - s > b1 - b0) {
                best = Some((s, y - 1));
            }
        }
        let (plane_y, plane_bottom) =
            best.unwrap_or_else(|| panic!("{theme}: no plane fill in the gap column x={gap_x}"));
        assert_eq!(
            plane_bottom - plane_y + 1,
            bh - 2,
            "{theme}: the plane's painted interior is {} rows at x={gap_x} (pill {pill:?}), not its \
             inner height {}. The first rows off that run: {:?}",
            plane_bottom - plane_y + 1,
            bh - 2,
            (plane_bottom + 1..win.y1)
                .map(|y| (y, px(img, gap_x, y)))
                .take(4)
                .collect::<Vec<_>>()
        );
        // The pill's box is the plane's content box. Its SOLID core is two rows inside
        // its own 1px border, so it is compared against `bh - 4`, not `bh - 2`.
        assert_eq!(
            pill.3 - pill.1 + 1,
            bh - 4,
            "{theme}: the pill's solid core is {pill:?}, {} rows tall",
            pill.3 - pill.1 + 1
        );

        // The fallback outline, counted. Every pixel it paints is a 50/50 blend of
        // `#c8c8c8` with whatever it was drawn over, so one signature finds it in
        // either theme and at any sub-pixel position.
        let mut phantom: Vec<(i32, i32)> = Vec::new();
        for y in plane_y..plane_y + bh {
            for x in bx..bx + bw {
                let p = px(img, x, y);
                if half_blend(p, FALLBACK_OUTLINE, plane) {
                    phantom.push((x, y));
                }
            }
        }
        assert!(
            phantom.is_empty(),
            "{theme}: {n} pixels inside the bar are a 50/50 blend of the #c8c8c8 fallback outline \
             with the plane's own fill — first few at {first:?}. The field is painting an edge of \
             its own.",
            n = phantom.len(),
            first = &phantom[..phantom.len().min(8)]
        );

        // The field's own top and bottom edges would land on the plane's
        // first and last interior rows, so those rows must be pure plane from past
        // the corner radius up to the pill. Glyph ink sits ~9px in from them, and
        // the pill's anti-aliased cap starts two columns before `pill.0`.
        let row_x0 = bx + 12;
        let row_x1 = pill.0 - 2;
        assert!(
            row_x0 < row_x1,
            "{theme}: no room between the plane's corner and the pill to measure the field"
        );
        for (label, row) in [("first", plane_y), ("last", plane_bottom)] {
            let offenders: Vec<(i32, [u8; 4])> = (row_x0..row_x1)
                .map(|x| (x, px(img, x, row)))
                .filter(|(_, p)| p[0] != plane[0] || p[1] != plane[1] || p[2] != plane[2])
                .take(8)
                .collect();
            assert!(
                offenders.is_empty(),
                "{theme}: the plane's {label} interior row (y={row}) is not pure plane — \
                 {offenders:?}. Something is painting an edge inside the bar."
            );
        }

        // The plane's own 1px edge is still painted on all four sides, just outside its
        // interior — so "pure plane inside" is a statement about the interior and
        // not about a bar that painted nothing at all, and the content box really
        // is 1px in from the border box.
        for (label, p) in [
            ("above", px(img, bx + 12, plane_y - 1)),
            ("below", px(img, bx + 12, plane_bottom + 1)),
            ("left", px(img, bx, plane_y + 12)),
            ("right", px(img, bx + bw - 1, plane_y + 12)),
        ] {
            assert!(
                !is_plane(p),
                "{theme}: the plane paints no edge of its own {label} of its interior"
            );
        }

        // And the field is not blank: the placeholder is painted in it.
        assert!(
            ink_box(
                img,
                Window {
                    x0: row_x0 + 8,
                    y0: plane_y + 6,
                    x1: pill.0 - 8,
                    y1: plane_bottom - 6
                },
                Ink { bg: plane, thr: 60 },
            )
            .is_some(),
            "{theme}: no placeholder ink inside the field"
        );

        println!(
            "   painted: interior rows {plane_y}..{plane_bottom} of a {bw}x{bh} plane at \
             x={bx}, pill {pill:?}, no fallback outline anywhere inside the bar"
        );
    }
}
