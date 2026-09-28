//! Characterisation test for `SoftbufferPresenter::present()` pixel conversion.
//!
//! `present()` historically asked Skia for `ColorType::RGBA8888` and then
//! hand-swizzled RGBA -> BGRA per pixel, because softbuffer's `&mut [u32]`
//! holds `0xAARRGGBB` (little-endian memory order `[B, G, R, A]`).
//! The surface itself is created with `raster_n32_premul`, so on a
//! little-endian target Skia's `kN32_SkColorType` is `kBGRA_8888` and the
//! read-back is *already* in softbuffer's memory order. The two swizzles
//! therefore cancel out: the net transform is the identity.
//!
//! This test pins that down at the byte level so a red/blue swap can never
//! slip through:
//!
//! 1. `old_pipeline_bytes_is_identical_to_new_pipeline` renders a real frame
//!    and compares the exact `u32` words the current code produces against
//!    the `N32` + bulk-copy version.
//! 2. `known_colours_map_to_argb_words` is the independent correctness check:
//!    pure red must land as `0xFFFF0000` and pure blue as `0xFF0000FF`.
//!    Without it, a bug that corrupts *both* pipelines identically would
//!    still satisfy test 1.
//!
//! These need only a CPU raster surface — no GPU, no window, no compositor.

#![cfg(all(feature = "skia-native", unix))]

use velox_renderer::skia_surface::SkiaSurface;

/// Surface size; also the quadrant size, so the surface is a 2x2 grid.
const Q: i32 = 64;
const W: i32 = Q * 2;
const H: i32 = Q * 2;

/// The conversion `present()` used before this change: read back as
/// `RGBA8888`, then swizzle each pixel into softbuffer's `0xAARRGGBB` word.
fn old_pipeline(surface: &mut SkiaSurface) -> Vec<u32> {
    let mut staging = vec![0u8; (W * H * 4) as usize];
    let info = skia_safe::ImageInfo::new(
        (W, H),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    assert!(
        surface.read_pixels(&info, &mut staging, (W * 4) as usize, (0, 0)),
        "read_pixels(RGBA8888) failed"
    );

    // Verbatim copy of the loop that was removed from `present()`.
    let mut pixels = vec![0u32; (W * H) as usize];
    for (i, pixel) in pixels.iter_mut().enumerate() {
        let base = i * 4;
        let r = staging[base] as u32;
        let g = staging[base + 1] as u32;
        let b = staging[base + 2] as u32;
        let a = staging[base + 3] as u32;
        *pixel = (a << 24) | (r << 16) | (g << 8) | b;
    }
    pixels
}

/// The conversion `present()` uses now: read back in the surface's native
/// `N32` order, which is already softbuffer's memory order, so a bulk copy
/// of the bytes is all that is left to do.
fn new_pipeline(surface: &mut SkiaSurface) -> Vec<u32> {
    let mut staging = vec![0u8; (W * H * 4) as usize];
    let info = skia_safe::ImageInfo::new(
        (W, H),
        skia_safe::ColorType::N32,
        skia_safe::AlphaType::Premul,
        None,
    );
    assert!(
        surface.read_pixels(&info, &mut staging, (W * 4) as usize, (0, 0)),
        "read_pixels(N32) failed"
    );

    // The bulk copy `present()` now performs. Staging is a `Vec<u8>`, so
    // reinterpret it into softbuffer's `&mut [u32]` view without unsafe by
    // going through a real aligned `Vec<u32>`.
    let mut pixels = vec![0u32; (W * H) as usize];
    let raw = bytemuck::cast_slice_mut::<u32, u8>(&mut pixels);
    raw.copy_from_slice(&staging);
    pixels
}

/// Draw the 2x2 quadrant fixture and return the surface.
fn fixture() -> SkiaSurface {
    let mut surface = SkiaSurface::new_raster(W, H).expect("new_raster");

    // Opaque base so every pixel is covered by exactly one Src paint.
    {
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::BLACK);
    }

    // Pure primaries, opaque: premultiplication is a no-op on these, so the
    // expected words are exact.
    let quadrants = [
        (
            (0.0, 0.0),
            (Q as f32, Q as f32),
            skia_safe::Color::from_rgb(255, 0, 0),
        ),
        (
            (Q as f32, 0.0),
            ((Q * 2) as f32, Q as f32),
            skia_safe::Color::from_rgb(0, 255, 0),
        ),
        (
            (0.0, Q as f32),
            (Q as f32, (Q * 2) as f32),
            skia_safe::Color::from_rgb(0, 0, 255),
        ),
        (
            (Q as f32, Q as f32),
            (W as f32, H as f32),
            skia_safe::Color::from_rgb(255, 255, 255),
        ),
    ];
    for (origin, far, color) in quadrants {
        let canvas = surface.canvas();
        let rect = skia_safe::Rect::new(origin.0, origin.1, far.0, far.1);
        let mut paint = skia_safe::Paint::default();
        paint.set_color(color);
        paint.set_anti_alias(false);
        canvas.draw_rect(rect, &paint);
    }
    surface
}

/// Centre pixel index of each quadrant.
fn centre(x: i32, y: i32) -> usize {
    (y as usize) * (W as usize) + (x as usize)
}

#[test]
fn old_pipeline_bytes_is_identical_to_new_pipeline() {
    let mut surface = fixture();
    let old = old_pipeline(&mut surface);
    let new = new_pipeline(&mut surface);

    assert_eq!(old.len(), new.len(), "pipelines disagree on pixel count");

    // Compare as raw bytes: this is what softbuffer actually blits, so a
    // byte-level difference here is exactly the failure mode we care about.
    let old_bytes: Vec<u8> = old.iter().flat_map(|w| w.to_ne_bytes()).collect();
    let new_bytes: Vec<u8> = new.iter().flat_map(|w| w.to_ne_bytes()).collect();

    if old_bytes != new_bytes {
        let first_diff = old_bytes
            .iter()
            .zip(new_bytes.iter())
            .position(|(a, b)| a != b)
            .expect("lengths equal but bytes differ");
        let pixel = first_diff / 4;
        panic!(
            "N32 + bulk copy is NOT byte-identical to RGBA8888 + swizzle.\n\
             first differing byte at {first_diff} (pixel {pixel} = x{}, y{})\n\
             old = {:#010x}, new = {:#010x}\n\
             => the two swizzles do not cancel; reverting to ColorType::RGBA8888",
            pixel % (W as usize),
            pixel / (W as usize),
            old[pixel],
            new[pixel],
        );
    }
}

/// The `N32` pipeline must still produce the words softbuffer expects.
/// Without this, a bug that corrupted both pipelines identically would
/// satisfy the equality test above.
#[test]
fn known_colours_map_to_argb_words() {
    let mut surface = fixture();
    let new = new_pipeline(&mut surface);

    // Pure red at top-left, green top-right, blue bottom-left.
    assert_eq!(new[centre(Q / 2, Q / 2)], 0xFFFF_0000, "pure red");
    assert_eq!(new[centre(Q + Q / 2, Q / 2)], 0xFF00_FF00, "pure green");
    assert_eq!(new[centre(Q / 2, Q + Q / 2)], 0xFF00_00FF, "pure blue");
    assert_eq!(new[centre(Q + Q / 2, Q + Q / 2)], 0xFFFF_FFFF, "white");

    // Every pixel must be fully opaque: the fixture only paints opaque
    // colours, so alpha must have been carried through the top byte.
    for (i, word) in new.iter().enumerate() {
        assert_eq!(word >> 24, 0xFF, "pixel {i} lost its alpha: {word:#010x}");
    }
}

/// Guards the premise of the whole optimisation: the surface the presenter
/// reads from is created as N32, and on this target N32 is `kBGRA_8888`.
/// If a future Skia build flipped N32 to RGBA order, `present()` would need
/// the swizzle back and this test would start failing loudly rather than
/// silently shipping swapped channels.
#[test]
fn n32_is_bgra_8888_on_this_target() {
    use skia_safe::ColorType;
    assert_eq!(ColorType::N32.bytes_per_pixel(), 4, "N32 must be 4 bytes");

    // Encode a known pixel into N32 and read the bytes back.
    let mut surface = SkiaSurface::new_raster(1, 1).expect("new_raster");
    {
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::from_rgb(0x11, 0x22, 0x33));
    }
    let mut staging = vec![0u8; 4];
    let info =
        skia_safe::ImageInfo::new((1, 1), ColorType::N32, skia_safe::AlphaType::Premul, None);
    assert!(surface.read_pixels(&info, &mut staging, 4, (0, 0)));

    // N32 == BGRA means the bytes are [B, G, R, A].
    assert_eq!(
        staging,
        vec![0x33, 0x22, 0x11, 0xFF],
        "kN32_SkColorType is not BGRA_8888 on this target; \
         ColorType::N32 is no longer a drop-in for softbuffer's word order"
    );
}
