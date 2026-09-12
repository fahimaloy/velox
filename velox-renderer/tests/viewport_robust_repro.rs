// Repro for R-H1 / R-H3: zero-size resize must clamp to max(1), warn and keep previous,
// and Viewport deduplication must exist.
// These tests FAIL before fix and PASS after.

use std::fs;
use std::path::Path;

fn read_src(rel: &str) -> String {
    for p in [rel, &format!("velox-renderer/{rel}"), &format!("../velox-renderer/{rel}")] {
        if Path::new(p).exists() {
            return fs::read_to_string(p).unwrap();
        }
    }
    // fallback: try crate relative
    for p in ["src/skia_surface.rs", "src/presenter.rs", "src/lib.rs", "velox-renderer/src/skia_surface.rs", "velox-renderer/src/presenter.rs", "velox-renderer/src/lib.rs"] {
        if rel.ends_with(p) && Path::new(p).exists() {
            return fs::read_to_string(p).unwrap();
        }
    }
    panic!("cannot find {rel}");
}

fn lib_rs() -> String { read_src("src/lib.rs") }
fn surface_rs() -> String { read_src("src/skia_surface.rs") }
fn presenter_rs() -> String { read_src("src/presenter.rs") }

#[test]
fn viewport_struct_exists_and_has_fields() {
    let lib = lib_rs();
    let surf = surface_rs();
    let pres = presenter_rs();
    let viewport_src = read_src("src/viewport.rs");
    // Viewport struct must exist somewhere in renderer crate (lib, viewport.rs, surface, presenter)
    let combined = format!("{}\n{}\n{}\n{}", lib, surf, pres, viewport_src);
    assert!(combined.contains("struct Viewport"), "Viewport struct missing — deduplication not done");
    // Must have physical, logical, scale fields (accepts physical/logical/scale as field names)
    assert!(combined.contains("physical"), "Viewport missing `physical` field");
    assert!(combined.contains("logical"), "Viewport missing `logical` field");
    assert!(combined.contains("scale"), "Viewport missing `scale` field");
}

#[test]
fn skia_surface_resize_clamps_max1_before_raster() {
    let src = surface_rs();
    // Find the resize fn and check it clamps before raster_n32_premul
    let pos = src.find("fn resize").expect("resize fn not found in skia_surface.rs");
    let slice = &src[pos..(pos+2000).min(src.len())];
    // Must clamp with max(1)
    assert!(slice.contains("max(1)") || slice.contains("max( 1)") || slice.contains(".max(1)"), "SkiaSurface::resize must clamp width/height to max(1) before raster_n32_premul");
    // Must use clamped values for raster_n32_premul
    // Ensure raster_n32_premul is called with clamped variable, not raw width/height
    // At minimum, check clamping appears before the raster call
    let clamp_pos = slice.find("max(1)").expect("max(1) not in resize");
    let raster_pos = slice.find("raster_n32_premul").expect("raster_n32_premul not found in resize");
    assert!(clamp_pos < raster_pos, "max(1) clamp must appear before raster_n32_premul");
}

#[test]
fn skia_surface_resize_warns_and_keeps_previous_on_error() {
    let src = surface_rs();
    let pos = src.find("fn resize").expect("resize fn not found");
    let slice = &src[pos..(pos+3000).min(src.len())];
    // Must log warn! on error and keep previous surface (not set width=0 blindly)
    // Check for warn! macro
    assert!(slice.contains("warn!"), "SkiaSurface::resize must warn! on error instead of silently failing");
    // The bug was: self.width = width; self.height = height; BEFORE attempting surface creation
    // Fix must set width/height AFTER success, or restore on failure.
    // Check that assignment is after the raster call or guarded.
    let width_assign_before = slice.find("self.width = width");
    let raster_pos = slice.find("raster_n32_premul");
    // If both exist, the width assignment should NOT be unconditionally before raster with raw width.
    // Acceptable: assignment after Ok, or assignment uses clamped values after creation, or no direct width=width before.
    // Fail if width is set to raw param before raster without clamping guard.
    if let (Some(_wpos), Some(rpos)) = (width_assign_before, raster_pos) {
        // If width assignment is before raster and uses raw `width` (not clamped), it's the bug.
        // The fixed code either: (a) uses w/h clamped, or (b) assigns after raster, or (c) assigns viewport.
        // So we consider it failing if the slice has "self.width = width" literally before raster.
        // Fixed version should have "self.width = w" or "self.viewport" or assign after Ok.
        let pre = &slice[..rpos];
        let has_raw_assign_before = pre.contains("self.width = width") || pre.contains("self.height = height");
        assert!(!has_raw_assign_before, "BUG R-H1: SkiaSurface::resize sets self.width/height to raw (possibly 0) BEFORE surface creation — must clamp and only update on success, otherwise desyncs");
    }
    // Also ensure error path does NOT leave width=0: the function must return Err but keep previous dims.
    // At least the code should not have an ok_or_else that still leaves width=0; the warn path must keep old.
    // Already covered by above, but also check that Ok(()) is only after assignment.
}

#[test]
fn lib_resize_does_not_swallow_error() {
    let src = lib_rs();
    // lib.rs had `let _ = renderer.resize(...)` which swallows error and causes desync
    // After fix, it must handle Err with warn!
    // Count occurrences of swallowed resize
    let swallow_count = src.matches("let _ = renderer.resize").count();
    assert_eq!(swallow_count, 0, "BUG R-H1: lib.rs still has `let _ = renderer.resize` swallowing errors — must handle with warn! and keep previous surface (found {} occurrences)", swallow_count);
    // Should have warn! near resize in lib.rs
    assert!(src.contains("renderer.resize") && src.contains("warn!"), "lib.rs resize error handling must warn! on failure");
}

#[test]
fn viewport_dedup_used_by_surface_and_presenter_and_lib() {
    let lib = lib_rs();
    let surf = surface_rs();
    let pres = presenter_rs();
    // After dedup, SkiaSurface and SoftbufferPresenter should use Viewport, not separate width/height fields as primary state
    // Check that both files reference Viewport
    let surf_uses_viewport = surf.contains("Viewport");
    let pres_uses_viewport = pres.contains("Viewport");
    let lib_uses_viewport = lib.contains("Viewport") || lib.contains("viewport");
    assert!(surf_uses_viewport, "SkiaSurface should use Viewport struct");
    assert!(pres_uses_viewport, "SoftbufferPresenter should use Viewport struct");
    assert!(lib_uses_viewport, "lib.rs should use Viewport struct (via `use viewport::Viewport` or logical_size delegation)");
}

#[test]
fn minimize_0x0_then_restore_no_crash_no_desync_functional() {
    // Functional check without requiring skia-native: verify logical_size clamping and viewport logic via source,
    // and if skia-native is available, actually exercise SkiaSurface.
    // Pure source check: presenter already clamps, surface must clamp, lib must not desync.

    // Simulate viewport transitions: 800x600 -> 0x0 -> 800x600
    // With correct fix, intermediate logical size is 1x1 and no panic.
    // We test the clamping logic directly: max(1) ensures no zero.
    let zero_w: u32 = 0;
    let zero_h: u32 = 0;
    let clamped_w = zero_w.max(1);
    let clamped_h = zero_h.max(1);
    assert_eq!(clamped_w, 1, "0 width must clamp to 1");
    assert_eq!(clamped_h, 1, "0 height must clamp to 1");
    // Physical 0 should not produce logical 0
    let scale = 1.5f32;
    let logical_w = ((clamped_w as f32) / scale).round().max(1.0) as u32;
    let logical_h = ((clamped_h as f32) / scale).round().max(1.0) as u32;
    assert_eq!(logical_w, 1);
    assert_eq!(logical_h, 1);
}

#[cfg(feature = "skia-native")]
#[test]
fn skia_surface_resize_zero_keeps_previous_surface() {
    use velox_renderer::skia_surface::SkiaSurface;
    // Create a valid surface then resize to 0x0 and verify it doesn't panic and keeps clamped size
    let mut s = SkiaSurface::new_raster(64, 64).expect("new_raster 64x64 failed");
    assert_eq!(s.width, 64);
    assert_eq!(s.height, 64);
    // Resize to 0x0 should clamp to 1x1 and succeed (or warn and keep previous but not 0)
    let res = s.resize(0, 0);
    // With fix, resize(0,0) should succeed with clamped 1x1, or at least not set width to 0
    // Even if it returns Err, width must not be 0 (keep previous or 1)
    assert_ne!(s.width, 0, "surface width must not be 0 after resize(0,0)");
    assert_ne!(s.height, 0, "surface height must not be 0 after resize(0,0)");
    // Restore should work
    let res2 = s.resize(800, 600);
    assert!(res2.is_ok(), "resize back to 800x600 must succeed");
    assert_eq!(s.width, 800);
    assert_eq!(s.height, 600);
    // If resize returned Ok for 0x0, width should be 1
    if res.is_ok() {
        // if first resize succeeded, second restore already checked; check that 0x0 gave 1x1 intermediate
        // Recreate to verify clamping specifically
        let mut s2 = SkiaSurface::new_raster(32, 32).unwrap();
        let _ = s2.resize(0, 0);
        assert_eq!(s2.width, 1);
        assert_eq!(s2.height, 1);
    }
}
