//! The two brand files `velox init` writes into `assets/`, and the proof that
//! they are the Velox mark rather than two plausible-looking blobs.
//!
//! ## Why these need a gate at all
//!
//! An `<img>` that resolves to nothing is SILENT. `rasterize_svg` returning
//! `None` and `fs::read` failing on a missing file take the same path — the
//! element lays out, the frame paints, and the only evidence is a hole where a
//! logo should be. Nothing in the app, and nothing in the test output, says the
//! file is missing. So "the scaffold ships a working image example" is exactly
//! the claim that cannot be left to a screenshot, and these four tests are it.
//!
//!  1. `init_writes_both_brand_assets` — runs the real `velox init` binary and
//!     checks the bytes that landed on disk: both files present, both non-empty,
//!     the PNG carrying the eight-byte PNG signature, the SVG carrying `<svg`.
//!  2. `every_asset_the_template_points_at_is_shipped` — the direction that
//!     actually breaks: every `src="…"` in the six `.vx` sources is resolved
//!     against `templates/project/assets/` and must exist. Rename an asset and
//!     the template goes red instead of quietly shipping a blank box.
//!  3. `the_committed_png_is_the_svg_it_was_made_from` — both files are put
//!     through the SAME raster path the app uses and compared on what a viewer
//!     would see: ink present, and the two marks occupying the same box to
//!     within a pixel of slack. A white PNG, a truncated one, or an unrelated
//!     picture all fail it. Deliberately NOT byte equality against a fresh
//!     render — Skia's scaler and PNG encoder are not stable across toolchain
//!     bumps, and a gate that fails for a reason unrelated to the asset is a
//!     gate people learn to `--ignored`.
//!  4. `regenerate_the_png_from_the_svg` — `#[ignore]`d, because writing to the
//!     source tree is not something a test run should do behind your back. It
//!     exists so the committed PNG is not a mystery blob: the exact command is
//!     in the asset's own header comment and it is this.
//!
//! ## Why the PNG is generated through `velox-renderer` and not by an external
//! converter
//!
//! Because that is the only way to be sure the raster the app later decodes is
//! one this app's own rasteriser could produce. It also means regenerating needs
//! nothing installed: `rsvg-convert`, `cairosvg` and `inkscape` are all
//! optional on a contributor's machine and all produce subtly different
//! antialiasing, which is precisely the kind of drift nobody notices until a
//! design review. The cost is stated in the report: resvg rasterises SVG at its
//! intrinsic size and Skia scales at draw time, so a 128px asset from a 106px
//! source is a 1.2x resample. Invisible at the 48px it is drawn at, and the
//! price of not needing a converter.
//!
//! Artefacts land in `gates/t10/`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use velox_dom::{VNode, h};
use velox_style::Stylesheet;

// ---------------------------------------------------------------------------
// Paths and fixtures
// ---------------------------------------------------------------------------

/// `<crate>/templates/project` — the directory `velox init` copies from.
fn project_template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/project")
}

/// `<crate>/templates/project/assets`.
fn assets_dir() -> PathBuf {
    project_template().join("assets")
}

fn asset(name: &str) -> PathBuf {
    assets_dir().join(name)
}

/// The two files, spelled once so a rename cannot half-happen.
const SVG: &str = "velox-logo.svg";
const PNG: &str = "velox-logo.png";

/// The rasterisation size of the committed PNG, in pixels.
///
/// The mark is drawn at 48 logical px inside a 64px plate, so 128 is 2.7x the
/// largest on-screen size and comfortably past the 2x bar a HiDPI display sets.
/// The source is 106px, so this is a 1.2x resample. The alternative — 512, which
/// the repo's own `icons/` directory carries — is 26.7 KB for a mark that is never
/// drawn above 48px, and 3.26 KB is the file that actually ships.
const RASTER: i32 = 128;

fn gate_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("gates")
        .join("t10");
    fs::create_dir_all(&dir).expect("create gates/t10");
    dir
}

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

/// An `<img>` alone lays out 0x0 (the root path in `compute_layout` does not
/// reach replaced-element sizing), so every render here puts it in a sized box.
fn mark(img_src: &str) -> VNode {
    let size = format!("width:{RASTER}px;height:{RASTER}px");
    h(
        "div",
        vec![("style", size.as_str())],
        vec![h(
            "img",
            vec![("src", img_src), ("style", size.as_str())],
            vec![],
        )],
    )
}

/// Render one asset through the real raster path and return premultiplied RGBA.
fn rasterise(img_src: &str) -> Vec<u8> {
    velox_renderer::render_vnode_to_rgba(&mark(img_src), &Stylesheet::default(), RASTER, RASTER)
        .expect("render_vnode_to_rgba")
}

/// `(left, top, right, bottom)` of every pixel with visible ink, or `None` when
/// the render is blank — which is what a missing, unparseable or empty file
/// produces, and is the failure this file exists to catch.
fn ink_bbox(rgba: &[u8], size: i32) -> Option<(i32, i32, i32, i32)> {
    let mut bounds: Option<(i32, i32, i32, i32)> = None;
    for y in 0..size {
        for x in 0..size {
            let a = rgba[((y * size + x) * 4 + 3) as usize];
            if a < 8 {
                continue;
            }
            bounds = Some(match bounds {
                None => (x, y, x, y),
                Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x), b.max(y)),
            });
        }
    }
    bounds
}

/// Fraction of the frame carrying ink. Compared loosely, never exactly.
fn coverage(rgba: &[u8]) -> f64 {
    let alpha: u64 = (0..rgba.len() / 4).map(|i| rgba[i * 4 + 3] as u64).sum();
    alpha as f64 / (255.0 * (rgba.len() / 4) as f64)
}

/// Write a PNG of one asset to `gates/t10/` for the eyeball pass.
fn dump(img_src: &str, name: &str) {
    let png = velox_renderer::render_vnode_to_raster_png_with_scale(
        &mark(img_src),
        &Stylesheet::default(),
        RASTER,
        RASTER,
        1.0,
    )
    .expect("render_vnode_to_raster_png_with_scale");
    let path = gate_dir().join(format!("{name}.png"));
    fs::write(&path, &png).expect("write png");
    println!("wrote {}", path.display());
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `velox init` must write both files, and they must be real files: a PNG
/// without its signature is not a PNG, and an SVG without `<svg` is not markup.
#[test]
fn init_writes_both_brand_assets() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join(format!("velox-logo-scratch-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("clear scratch");
    }
    // `Command::current_dir` fails with `NotFound` — the error that is easy to
    // misread as "velox is not installed" — if the directory is not there yet.
    fs::create_dir_all(&dir).expect("create scratch dir");

    let out = Command::new(env!("CARGO_BIN_EXE_veloxc"))
        .arg("init")
        .arg("logo-scratch")
        .current_dir(&dir)
        .output()
        .expect("run velox init");
    assert!(
        out.status.success(),
        "velox init failed: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    for name in [SVG, PNG] {
        let written = dir.join("logo-scratch/assets").join(name);
        let bytes =
            fs::read(&written).unwrap_or_else(|e| panic!("read {}: {e}", written.display()));
        assert!(
            bytes.len() > 512,
            "{} is {} bytes — that is not an asset",
            name,
            bytes.len()
        );
    }

    // The PNG signature. `fs::write` of an empty vec, or a stale text file, both
    // produce a file of the right length and neither one is a PNG.
    let png = fs::read(dir.join("logo-scratch/assets").join(PNG)).expect("png");
    assert_eq!(
        &png[..8],
        &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
        "{PNG} does not start with the PNG signature"
    );

    // And the SVG is markup rather than the same bytes under another name.
    let svg = String::from_utf8(fs::read(dir.join("logo-scratch/assets").join(SVG)).expect("svg"))
        .expect("svg is utf-8");
    assert!(
        svg.contains("<svg") && svg.contains("</svg>"),
        "{SVG} is not an SVG document"
    );

    fs::remove_dir_all(&dir).ok();
}

/// The directory that breaks in practice: a template points `src` at a path
/// that `init` does not write. The render is silent; this is not.
///
/// Scans the `<template>` block ONLY, and that restriction is load-bearing rather
/// than tidy. `src="…"` appears in these files' own prose — this file's sibling
/// comment in `App.vx` writes `src="/…"` while explaining why a rooted path does
/// not work — and a whole-file scan resolves that to a file named `…`, which fails
/// the test on its own documentation. A `src` attribute only means anything inside
/// a template, so that is the only place worth looking.
#[test]
fn every_asset_the_template_points_at_is_shipped() {
    let src_dir = project_template().join("src");
    let mut referenced: BTreeSet<String> = BTreeSet::new();
    for rel in [
        "App.vx",
        "components/Confirm.vx",
        "components/Modal.vx",
        "components/Todos.vx",
        "components/TodoInput.vx",
        "components/TodoItem.vx",
    ] {
        let text =
            fs::read_to_string(src_dir.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
        let start = text
            .find("<template>")
            .unwrap_or_else(|| panic!("{rel} has no <template>"));
        let end = text[start..]
            .find("</template>")
            .map(|i| start + i)
            .unwrap_or_else(|| panic!("{rel} has no </template>"));
        for (at, _) in text[start..end].match_indices("src=\"") {
            let rest = &text[(start + at + "src=\"".len()).min(text.len())..];
            let close = rest.find('"').expect("unterminated src attribute");
            referenced.insert(rest[..close].to_string());
        }
    }

    assert!(
        !referenced.is_empty(),
        "no `src=` found in the template — this test is guarding nothing"
    );
    for r in &referenced {
        let name = r.rsplit('/').next().unwrap_or(r);
        let shipped = assets_dir().join(name);
        assert!(
            shipped.is_file(),
            "the template points src=\"{r}\" but {} does not exist",
            shipped.display()
        );
    }
}

/// The committed PNG and the committed SVG are the same picture.
///
/// Both go through the same decoder the app uses, so this also proves the PNG
/// survives velox's own raster path — which is the half a magic-byte check
/// cannot see. The comparison is on INK, not bytes: the SVG is resampled on its
/// way in, so a pixel-exact match would be testing the toolchain, not the asset.
#[test]
fn the_committed_png_is_the_svg_it_was_made_from() {
    let svg_png = rasterise(&asset(SVG).to_string_lossy());
    let png_png = rasterise(&asset(PNG).to_string_lossy());

    let svg_box =
        ink_bbox(&svg_png, RASTER).expect("the SVG renders no ink — resvg is not resolving it");
    let png_box = ink_bbox(&png_png, RASTER).expect("the committed PNG renders no ink");

    assert_eq!(
        png_box.2 - png_box.0,
        svg_box.2 - svg_box.0,
        "the PNG is {}px wide where the SVG is {}px",
        png_box.2 - png_box.0,
        svg_box.2 - svg_box.0
    );
    assert_eq!(
        png_box.3 - png_box.1,
        svg_box.3 - svg_box.1,
        "the PNG is {}px tall where the SVG is {}px",
        png_box.3 - png_box.1,
        svg_box.3 - svg_box.1
    );

    let svg_cov = coverage(&svg_png);
    let png_cov = coverage(&png_png);
    assert!(
        (svg_cov - png_cov).abs() < 0.01,
        "the PNG covers {:.3} of the frame where the SVG covers {:.3} — different pictures",
        png_cov,
        svg_cov
    );

    println!(
        "svg ink {svg_box:?} coverage {svg_cov:.3}; png ink {png_box:?} coverage {png_cov:.3}"
    );
    dump(&asset(SVG).to_string_lossy(), "asset-velox-logo-svg");
    dump(&asset(PNG).to_string_lossy(), "asset-velox-logo-png");
}

/// Regenerate `templates/project/assets/velox-logo.png` from the sibling SVG.
///
/// Ignored by default. Run it when the SVG changes:
///
/// ```text
/// cargo test -p veloxc --test logo_assets -- --ignored regenerate --nocapture
/// ```
#[test]
#[ignore = "writes into the source tree on purpose; run it deliberately"]
fn regenerate_the_png_from_the_svg() {
    let bytes = velox_renderer::render_vnode_to_raster_png_with_scale(
        &mark(&asset(SVG).to_string_lossy()),
        &Stylesheet::default(),
        RASTER,
        RASTER,
        1.0,
    )
    .expect("render the svg");
    fs::write(asset(PNG), &bytes).expect("write the png");
    println!("wrote {} ({} bytes)", asset(PNG).display(), bytes.len());
    assert!(
        ink_bbox(&rasterise(&asset(PNG).to_string_lossy()), RASTER).is_some(),
        "the regenerated PNG renders no ink — check the SVG, not the encoder"
    );
}
