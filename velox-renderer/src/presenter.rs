//! Softbuffer presenter for Skia rendering
//!
//! Bridges Skia raster surfaces to the display via softbuffer.

use softbuffer::{Context, Surface};
use velox_dom::VeloxError;
use winit::window::Window;

/// Presents Skia-rendered content to a window using softbuffer.
pub struct SoftbufferPresenter {
    _context: Context,
    surface: Surface,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl SoftbufferPresenter {
    /// Creates a new presenter for the given window with initial dimensions.
    ///
    /// # Errors
    /// Returns an error if softbuffer context or surface creation fails.
    pub fn new(window: &Window, width: u32, height: u32) -> Result<Self, VeloxError> {
        let context = unsafe {
            Context::new(window)
                .map_err(|e| VeloxError::Render(format!("softbuffer context failed: {e}")))?
        };
        let mut surface = unsafe {
            Surface::new(&context, window)
                .map_err(|e| VeloxError::Render(format!("softbuffer surface failed: {e}")))?
        };
        let w = width.max(1);
        let h = height.max(1);
        surface
            .resize(
                std::num::NonZeroU32::new(w).expect("w >= 1 guaranteed by .max(1)"),
                std::num::NonZeroU32::new(h).expect("h >= 1 guaranteed by .max(1)"),
            )
            .map_err(|e| format!("softbuffer resize failed: {}", e))?;
        Ok(Self {
            _context: context,
            surface,
            width: w,
            height: h,
            rgba: vec![0u8; (w as usize) * (h as usize) * 4],
        })
    }

    /// Resizes the presenter to new dimensions.
    ///
    /// No-op if dimensions haven't changed.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), VeloxError> {
        let w = width.max(1);
        let h = height.max(1);
        if w == self.width && h == self.height {
            return Ok(());
        }
        self.surface
            .resize(
                std::num::NonZeroU32::new(w).expect("w >= 1 guaranteed by .max(1)"),
                std::num::NonZeroU32::new(h).expect("h >= 1 guaranteed by .max(1)"),
            )
            .map_err(|e| format!("softbuffer resize failed: {}", e))?;
        self.width = w;
        self.height = h;
        self.rgba.resize((w as usize) * (h as usize) * 4, 0);
        Ok(())
    }

    /// Presents the contents of the Skia surface to the window.
    ///
    /// Reads pixels from the Skia surface, converts from RGBA to the
    /// softbuffer format, and presents to the display.
    pub fn present(
        &mut self,
        skia_surface: &mut crate::skia_surface::SkiaSurface,
    ) -> Result<(), VeloxError> {
        let width = skia_surface.width.max(1) as u32;
        let height = skia_surface.height.max(1) as u32;
        self.resize(width, height)?;

        let info = skia_safe::ImageInfo::new(
            (self.width as i32, self.height as i32),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Premul,
            None,
        );
        let row_bytes = (self.width * 4) as usize;
        if !skia_surface.read_pixels(&info, &mut self.rgba, row_bytes, (0, 0)) {
            return Err(VeloxError::Render("skia: read_pixels failed".into()));
        }

        let mut buffer = self
            .surface
            .buffer_mut()
            .map_err(|e| format!("softbuffer buffer_mut failed: {}", e))?;
        let pixels: &mut [u32] = &mut buffer;
        let pixel_count = (self.width as usize) * (self.height as usize);
        if pixels.len() < pixel_count {
            return Err(VeloxError::Render(
                "softbuffer: buffer smaller than expected".into(),
            ));
        }
        for (i, pixel) in pixels.iter_mut().take(pixel_count).enumerate() {
            let base = i * 4;
            let r = self.rgba[base] as u32;
            let g = self.rgba[base + 1] as u32;
            let b = self.rgba[base + 2] as u32;
            let a = self.rgba[base + 3] as u32;
            // Softbuffer uses ABGR format (little-endian: 0xAABBGGRR)
            // Skia outputs RGBA with premultiplied alpha
            *pixel = (a << 24) | (b << 16) | (g << 8) | r;
        }
        buffer
            .present()
            .map_err(|e| format!("softbuffer present failed: {}", e))?;
        Ok(())
    }
}
