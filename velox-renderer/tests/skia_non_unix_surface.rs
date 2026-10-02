//! The `skia-native` GPU path has a real implementation only on unix, so every
//! gate in this repo ran on Linux and the rest of the feature was never compiled
//! on a non-unix target. It did not build there at all:
//! `skia_surface.rs` calls `crate::skia_gl::create_context_from_winit`, which
//! existed only inside `skia_gl`'s `unix` block, and `skia_surface.rs` /
//! `presenter.rs` name `raw_window_handle` / `egl` / `glow` / `softbuffer`
//! unconditionally while `Cargo.toml` declared those four under
//! `[target.'cfg(unix)'.dependencies]`.
//!
//! What this file does is *not* replace a non-unix build — the `not(unix)` body
//! is only compiled by a real non-unix target, which is the
//! `skia-native-non-unix` job in `.github/workflows/ci.yml` (run on
//! `windows-latest`). This is the half that a Linux gate can still catch: that
//! the stub keeps the API the shared call site is written against, that no
//! entry point there panics or claims success, and that the platform crates
//! stay reachable from every target.
//!
//! Source text rather than types, because the stub does not exist to be
//! referenced on a unix host — there is no way to *name* `cfg(not(unix))` items
//! from a test running on unix. The same convention is used elsewhere in this
//! crate (`skia_render.rs` asserts on its own source text), so it is not a new
//! idea here; it is the only available one, and it fails loudly the moment the
//! two files disagree.

const SKIA_GL: &str = include_str!("../src/skia_gl.rs");
const SKIA_SURFACE: &str = include_str!("../src/skia_surface.rs");
const CARGO_TOML: &str = include_str!("../Cargo.toml");

/// Marker that starts the non-unix stub in `skia_gl.rs`. Everything from here
/// to the end of the file is the part a non-unix target compiles.
const STUB_MARKER: &str = "mod non_unix_stub";

/// The source text of the non-unix configuration: `skia_gl.rs` from its stub
/// module onwards.
fn non_unix_section() -> &'static str {
    let start = SKIA_GL
        .find(STUB_MARKER)
        .expect("skia_gl.rs must keep a `mod non_unix_stub` — without it the feature does not build off unix");
    &SKIA_GL[start..]
}

/// The identifier that follows `marker` on this line, if any.
///
/// Matches mid-line because the interesting calls are usually inside a larger
/// expression (`match crate::skia_gl::create_context_from_winit(window) {`,
/// `match gl_ctx.into_direct_context() {`).
fn identifier_after(line: &str, marker: &str) -> Option<String> {
    let mut from = 0usize;
    while let Some(offset) = line[from..].find(marker) {
        let rest = &line[from + offset + marker.len()..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            return Some(name);
        }
        from += offset + marker.len();
    }
    None
}

/// Every `fn` name `skia_surface.rs` reaches for on `skia_gl`.
///
/// Pulled out of the call site rather than hardcoded, so adding a new
/// `skia_gl` call in `skia_surface.rs` makes this test demand a stub counterpart
/// without anyone updating this list.
fn fn_names_called_on_skia_gl() -> Vec<String> {
    let mut names = Vec::new();
    for line in SKIA_SURFACE.lines() {
        let code = match line.split_once("//") {
            Some((code, _comment)) => code,
            None => line,
        };
        for marker in ["crate::skia_gl::", "gl_ctx.", "owned."] {
            if let Some(name) = identifier_after(code, marker) {
                names.push(name);
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

#[test]
fn the_non_unix_stub_provides_every_entry_point_the_shared_call_site_uses() {
    let stub = non_unix_section();
    let names = fn_names_called_on_skia_gl();
    assert!(
        !names.is_empty(),
        "the extractor found nothing in skia_surface.rs; a broken extractor \
         would make this test pass for the wrong reason"
    );
    let missing: Vec<&String> = names
        .iter()
        .filter(|n| {
            !stub.contains(&format!("fn {n}("))
                && !stub.contains(&format!("fn {n}<"))
                && !stub.contains(&format!("struct {n} "))
                && !stub.contains(&format!("struct {n}\n"))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "`skia_surface.rs` calls {names:?}, but the non-unix stub of `skia_gl.rs` \
         defines no {missing:?}. The feature will not compile off unix."
    );
}

#[test]
fn no_non_unix_entry_point_panics_or_claims_a_context_it_cannot_make() {
    let stub = non_unix_section();
    for banned in ["panic!", "unreachable!", "todo!", "unimplemented!"] {
        assert!(
            !stub.contains(banned),
            "the non-unix stub uses {banned}. Off-unix the correct answer is a \
             typed error: a panic in an event loop is a crash, not a report."
        );
    }
    // And the shape of that answer: the context constructor returns `None` and
    // every free function returns `Err`, rather than a fake success.
    assert!(
        stub.contains("pub fn into_direct_context(self) -> Option<GlDirectContext> {")
            && stub.contains("None"),
        "`into_direct_context` must exist and must answer `None` off unix — there \
         is no GL context for a `DirectContext` to belong to."
    );
    assert!(
        stub.contains("UNSUPPORTED: &str"),
        "the stub's error must be one named constant, so every entry point says \
         the same thing about the same cause."
    );
}

#[test]
fn the_platform_crates_stay_reachable_on_every_target() {
    // These four are named unconditionally by `skia_surface.rs` /
    // `presenter.rs`, which compile on every target. Declaring them under
    // `[target.'cfg(unix)'.dependencies]` makes the `skia-native` feature
    // uncompilable off unix, which is the bug this file exists to keep fixed.
    assert!(
        !CARGO_TOML.contains("[target.'cfg(unix)'.dependencies]"),
        "velox-renderer must not target-gate its dependencies to `cfg(unix)`"
    );
    for crate_name in ["raw-window-handle", "egl", "glow", "softbuffer"] {
        let declared = CARGO_TOML
            .lines()
            .any(|l| l.starts_with(crate_name) && l.contains("optional = true"));
        assert!(
            declared,
            "`{crate_name}` must be an optional dependency of `[dependencies]`, \
             not target-gated: `skia_surface.rs` / `presenter.rs` name it on \
             every target."
        );
    }
}
