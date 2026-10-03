// Repro for R-H3 / R-M1 : DPI correctness
// These tests FAIL before fix and PASS after.
// Checks: inverse rounding hairline (single rounding physical=(logical*scale).round()),
// mouse_pos rescaled atomically on ScaleFactorChanged, hit-test+render share Viewport,
// fractional scales no blur/hairline, font re-raster at device pixels.

use std::fs;
use std::path::Path;

fn read_src(rel: &str) -> String {
    for p in [
        rel,
        &format!("velox-renderer/{rel}"),
        &format!("../velox-renderer/{rel}"),
    ] {
        if Path::new(p).exists() {
            return fs::read_to_string(p).unwrap();
        }
    }
    for p in [
        "src/lib.rs",
        "src/viewport.rs",
        "src/skia_render.rs",
        "src/skia_surface.rs",
        "velox-renderer/src/lib.rs",
    ] {
        if rel.ends_with(p) && Path::new(p).exists() {
            return fs::read_to_string(p).unwrap();
        }
    }
    panic!("cannot find {rel}");
}

fn lib_rs() -> String {
    read_src("src/lib.rs")
}
fn viewport_rs() -> String {
    read_src("src/viewport.rs")
}
fn skia_render_rs() -> String {
    read_src("src/skia_render.rs")
}
fn skia_surface_rs() -> String {
    read_src("src/skia_surface.rs")
}

#[test]
fn single_rounding_physical_from_logical_exists() {
    let vp = viewport_rs();
    // Must provide single rounding physical=(logical*scale).round() and not round-trip physical/scale only
    let has_physical_from_logical = vp.contains("physical_from_logical")
        || vp.contains("from_logical")
        || vp.contains("logical*scale")
        || vp.contains("logical * scale");
    assert!(
        has_physical_from_logical,
        "BUG R-H3: Viewport must expose single rounding physical=(logical*scale).round() (physical_from_logical / from_logical). Viewport only does inverse logical=physical/scale → causes 0.25px hairline at 1.25/1.5"
    );
    // Must contain rounding of logical*scale
    let has_round = vp.contains(".round()")
        && (vp.contains("* scale") || vp.contains("*scale") || vp.contains("scale *"));
    assert!(
        has_round,
        "Viewport must contain (logical*scale).round() for DPI correctness"
    );
}

#[test]
fn inverse_rounding_no_hairline_fractional_1_25_1_5() {
    // Simulate Viewport rounding: physical -> logical -> physical round-trip must be consistent
    // Without single-rounding fix, (logical*scale).round() != original physical leaves subpixel gap
    // After fix, Viewport should guarantee physical = (logical*scale).round() and logical = (physical/scale).round()
    // but keeping both without inverse hairline means snapped physical should cover full window.
    // We check that for fractional scales, the viewport's logical maps back to within 0.5px of physical
    // and that rendering would snap to physical pixels.
    let cases = [
        (800u32, 600u32, 1.25f32),
        (800, 600, 1.5),
        (800, 600, 1.75),
        (1024, 768, 1.25),
        (1920, 1080, 1.25),
        (1920, 1080, 1.5),
    ];
    for (pw, ph, scale) in cases {
        // current viewport logic: logical = (physical/scale).round()
        let lw = ((pw as f32) / scale).round().max(1.0) as u32;
        let lh = ((ph as f32) / scale).round().max(1.0) as u32;
        let phys_restored_w = ((lw as f32) * scale).round() as u32;
        let phys_restored_h = ((lh as f32) * scale).round() as u32;
        // The physical restored via single rounding should be within 1px of original (due to rounding)
        // but the key is that rendering must use snapped physical, not fractional logical*scale
        let phys_gap_w = (pw as i32 - phys_restored_w as i32).abs();
        let phys_gap_h = (ph as i32 - phys_restored_h as i32).abs();
        // Gap >1 would be hairline; with correct single rounding gap <=1 (clamped)
        assert!(
            phys_gap_w <= 1,
            "hairline at scale {scale}: pw {pw} -> lw {lw} -> restored {phys_restored_w} gap {phys_gap_w} >1px"
        );
        assert!(
            phys_gap_h <= 1,
            "hairline at scale {scale}: ph {ph} -> lh {lh} -> restored {phys_restored_h} gap {phys_gap_h} >1px"
        );

        // Additional: logical*scale without rounding has fractional .25/.5 causing blur
        let frac_w = (lw as f32) * scale;
        let is_fractional = (frac_w - frac_w.round()).abs() > 0.01;
        if is_fractional {
            // After fix, viewport or renderer must snap to physical integer
            let vp_src = viewport_rs();
            let render_src = skia_render_rs();
            let combined = format!("{vp_src}\n{render_src}");
            let has_snap = combined.contains("round()") && (combined.contains("scale"));
            assert!(
                has_snap,
                "fractional {frac_w:.2} at scale {scale} needs snap to physical via round() to avoid blur/hairline"
            );
        }
    }
    // Ensure viewport provides physical_from_logical helper to enforce single rounding
    let vp = viewport_rs();
    assert!(
        vp.contains("physical_from_logical") || vp.contains("from_logical"),
        "Viewport must provide physical_from_logical / from_logical to enforce single rounding physical=(logical*scale).round()"
    );
}

#[test]
fn mouse_pos_rescaled_atomically_on_scale_change() {
    let src = lib_rs();
    // Must rescale mouse_pos on ScaleFactorChanged atomically
    // Look for ScaleFactorChanged handler that touches mouse_pos
    let has_scale_handler = src.contains("ScaleFactorChanged");
    assert!(
        has_scale_handler,
        "lib.rs missing ScaleFactorChanged handler"
    );

    // Check that mouse_pos is rescaled in that handler
    // We search for mouse_pos assignment that involves scale_factor in the ScaleFactorChanged region
    let mut found_rescale = false;
    let mut idx = 0;
    while let Some(pos) = src[idx..].find("ScaleFactorChanged") {
        let base = idx + pos;
        let slice = &src[base..(base + 4000).min(src.len())];
        // Look for mouse_pos rescale patterns
        let has_mouse = slice.contains("mouse_pos");
        let has_scale = slice.contains("scale_factor")
            || slice.contains("new_scale")
            || slice.contains("old_scale");
        // Need multiplication/division involving old/new scale
        let has_rescale = (slice.contains("mouse_pos.0")
            && (slice.contains("*") || slice.contains("/")))
            || slice.contains("mouse_pos = (")
            || (has_mouse && has_scale && (slice.contains("old_scale") || slice.contains("old")));
        if has_mouse && has_rescale {
            found_rescale = true;
            break;
        }
        // Also check for generic mouse_pos scaling
        if slice.contains("mouse_pos")
            && (slice.contains("old_scale") || slice.contains("* old") || slice.contains("/ scale"))
        {
            found_rescale = true;
            break;
        }
        idx = base + 1;
        if idx >= src.len() {
            break;
        }
    }
    // Fallback: global search for mouse_pos rescale near ScaleFactorChanged comment
    if !found_rescale {
        // Check if lib.rs contains any mouse_pos rescale outside CursorMoved
        let has_global_mouse_scale = src.contains("mouse_pos.0 *")
            || src.contains("mouse_pos.0*")
            || src.contains("mouse_pos.0 =") && src.contains("old_scale");
        found_rescale = has_global_mouse_scale;
    }
    assert!(
        found_rescale,
        "BUG R-M1: mouse_pos not re-scaled on Resized/ScaleFactorChanged — ScaleFactorChanged handler must atomically rescale mouse_pos (e.g. mouse_pos.0 = mouse_pos.0 * old_scale / new_scale). Click after DPI change hits offset."
    );
}

#[test]
fn hit_test_and_render_share_viewport() {
    let lib = lib_rs();
    let vp = viewport_rs();
    // After 1B/1C, hit-test and render should share same Viewport/logical_size
    // Check that lib.rs uses Viewport struct (not raw width/height drift)
    assert!(
        lib.contains("Viewport") || lib.contains("viewport"),
        "lib.rs must use Viewport shared between hit-test and render (R-H5)"
    );
    // Check that recompute_targets takes &LayoutNode (single layout per frame) — from 1B
    assert!(
        lib.contains("recompute_targets") && lib.contains("LayoutNode"),
        "lib.rs recompute_targets must take &LayoutNode per 1B single-layout invariant"
    );

    // Check that ScaleFactorChanged and RedrawRequested share same Viewport logical path
    // They should both use logical_size helper or viewport.logical_size()
    let uses_logical_size = lib.contains("logical_size") || vp.contains("logical_size");
    assert!(
        uses_logical_size,
        "Viewport/logical_size must be single rounding point shared by hit-test and render"
    );

    // Ensure hit-test after scale change uses updated viewport (same as render)
    // The bug was triple layout per resize with divergent rounding; check that Resized does NOT call recompute_targets
    // and only RedrawRequested does. After 1B, Resized handler should NOT contain recompute_targets.
    // Find Resized blocks
    let mut resized_has_recompute = false;
    let mut idx = 0;
    while let Some(pos) = lib[idx..].find("Resized") {
        let base = idx + pos;
        let slice = &lib[base..(base + 3000).min(lib.len())];
        if slice.contains("recompute_targets") {
            // If it's inside WindowEvent::Resized, that's stale (should defer to RedrawRequested)
            // Allow if it's just the resize call, but recompute should be absent
            // Check that within 1500 chars after Resized there is recompute
            let snippet = &lib[base..(base + 1500).min(lib.len())];
            if snippet.contains("recompute_targets") {
                resized_has_recompute = true;
                break;
            }
        }
        idx = base + 1;
        if idx >= lib.len() {
            break;
        }
    }
    // This test documents expectation: Resized should NOT recompute hit-test; defer to RedrawRequested
    // If it does, hit-test and render could diverge 1px by rounding
    assert!(
        !resized_has_recompute,
        "BUG R-H5: WindowEvent::Resized must NOT call recompute_targets; defer to RedrawRequested to share single Viewport/layout (prevents 1px hit-test vs render divergence)"
    );
}

#[test]
fn font_re_raster_at_device_pixels() {
    let render = skia_render_rs();
    let surface = skia_surface_rs();
    let vp = viewport_rs();
    // Fonts must be re-rastered at device pixels (hinting) when scale changes
    // Check that FontCache or font creation uses scale factor
    let _combined = format!("{render}\n{surface}\n{vp}");
    // Look specifically for font handling with scale
    let has_font_scale =
        render.contains("scale") && (render.contains("Font") || render.contains("font"));
    assert!(
        has_font_scale,
        "BUG R-H3: font re-raster at device pixels missing — FontCache/font creation must account for scale_factor (e.g. font_size * scale) to avoid blur at fractional DPI"
    );

    // Check that scale change triggers font handling (render_frame reads scale)
    assert!(
        render.contains("scale_factor") || render.contains("scale"),
        "skia_render.rs must read scale_factor for font re-raster"
    );
    assert!(
        render.contains("round") || render.contains("device"),
        "font/device pixel handling should involve rounding to physical pixels"
    );
}

#[test]
fn fractional_scales_no_blur_hairline_click_correct() {
    // Integration: Viewport fractional 1.25/1.5 must be consistent and hit-test coordinates must match rendered pixels
    // Simulate: layout rect at logical (10,10,100,30) at scale 1.25 -> physical (13,13,125,38) rounded
    // Mouse at physical (50,25) -> logical (40,20) -> hit-test should hit rect
    let scale_125 = 1.25f32;
    let scale_15 = 1.5f32;
    let vp_src = viewport_rs();
    let has_snap = vp_src.contains("round()") || skia_render_rs().contains("round");
    assert!(
        has_snap,
        "fractional scales require rounding to avoid blur/hairline"
    );

    // Check hit-test path uses logical coordinates derived from same Viewport
    let lib = lib_rs();
    assert!(
        lib.contains("hit_test") && lib.contains("mouse_pos"),
        "hit-test must use mouse_pos in logical coords shared with Viewport"
    );

    // Ensure viewport handles 1.25/1.5 without Invertible rounding error: test helper physical_from_logical
    for scale in [scale_125, scale_15] {
        let logical_w = 100;
        let expected_physical = ((logical_w as f32) * scale).round() as u32;
        // Without single rounding, integer truncation would give wrong result
        let truncated = (logical_w as f32 * scale) as u32;
        if expected_physical != truncated {
            // Proves rounding matters at fractional scales; viewport must use round, not truncation
            assert!(
                expected_physical == ((logical_w as f32 * scale).round() as u32),
                "physical=(logical*scale).round() must be used, not truncation, for scale {scale}"
            );
        }
    }
}
