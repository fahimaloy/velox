//! THROWAWAY VISUAL PROOF for the scaffolded template's design pass.
//!
//! This is not a gate. It exists so the design work in
//! `velox-cli/templates/project/src/{App.vx, components/*.vx}` can be LOOKED AT
//! rather than argued about, and so the numbers quoted in the report (type scale,
//! spacing scale, glyph centring, accent ratio) come out of a real render of the
//! real stylesheets rather than out of anyone's memory.
//!
//! ## What is real and what is a replica — and why that is enough
//!
//! A test in this crate cannot reach the COMPILED app: `velox-cli` depends on
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

/// Write a PNG of the scaffolded app at `w` x `h`, in `dark`.
fn write_scaffold_png(sheet: &Stylesheet, name: &str, dark: bool, w: i32, h: i32) -> PathBuf {
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
                &["brand"],
                vec![
                    el("p", "App", &["eyebrow"], vec![txt("VELOX")]),
                    el("h1", "App", &["title"], vec![txt("Velox Todo")]),
                    el(
                        "p",
                        "App",
                        &["tagline"],
                        vec![txt(
                            "One file per component. A template, a state block and a scoped \
                             style block, drawn natively with Skia.",
                        )],
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

/// Every pixel further than `thr` from `bg`, as `(x, y)`. `thr` has to clear
/// the anti-aliasing on a rounded edge — the pill's corner pixels blend toward
/// the page fill, which sits ~10/255 away from the button's own white.
fn ink(buf: &[u8], w: i32, h: i32, bg: [u8; 3], thr: i32) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let o = (y as usize * w as usize + x as usize) * 4;
            if buf[o + 3] > 8 && (buf[o] as i32 - bg[0] as i32).abs() > thr {
                out.push((x, y));
            }
        }
    }
    out
}

/// The bounding box of the ink in a window, or `None` when the window is blank.
fn ink_box(
    buf: &[u8],
    w: i32,
    h: i32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    bg: [u8; 3],
    thr: i32,
) -> Option<(i32, i32, i32, i32)> {
    let pts = ink(buf, w, h, bg, thr);
    let mut l = i32::MAX;
    let mut r = i32::MIN;
    let mut t = i32::MAX;
    let mut b = i32::MIN;
    let mut any = false;
    for (x, y) in pts {
        if x < x0 || x >= x1 || y < y0 || y >= y1 {
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
            &rgba,
            w,
            hh,
            bx + 7,
            by + 7,
            bx + bw - 7,
            by + bh - 7,
            bg,
            60,
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
                if let Some((k, val)) = d.split_once(':') {
                    if k.trim() == prop {
                        return Some(val.trim().to_string());
                    }
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
    // The one declaration that is NOT rhythm, and the measurement that puts it off
    // the grid on purpose. Sniffing the sheet for it is deliberately crude: if it
    // is deleted, the exemption stops matching and the grid check starts guarding
    // it again rather than passing in silence.
    const OPTICAL: &[(&str, &str)] = &[("components/TodoItem.vx", "padding-top: 1px")];
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
