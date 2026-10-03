//! Viewport — single source of truth for physical/logical size and scale.
//! Shared by SkiaSurface, SoftbufferPresenter and the window loop (R-H1/R-H3).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogicalSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Physical pixels (window inner size, clamped to >=1).
    pub physical: PhysicalSize,
    /// Logical pixels: (physical / scale).round().max(1).
    pub logical: LogicalSize,
    /// Device pixel ratio.
    pub scale: f32,
}

// Public to satisfy `visible_private_types` lint when re-exported via lib.rs.
pub type ViewportPhysicalSize = PhysicalSize;
pub type ViewportLogicalSize = LogicalSize;

impl Viewport {
    /// Create a new Viewport clamping physical to max(1) and deriving logical.
    pub fn new(physical_width: u32, physical_height: u32, scale: f32) -> Self {
        let pw = physical_width.max(1);
        let ph = physical_height.max(1);
        let s = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let (lw, lh) = Self::logical_from_physical(pw, ph, s);
        Self {
            physical: PhysicalSize {
                width: pw,
                height: ph,
            },
            logical: LogicalSize {
                width: lw,
                height: lh,
            },
            scale: s,
        }
    }

    /// Create from i32 physical (SkiaSurface API) — clamps to max(1).
    pub fn from_i32(physical_width: i32, physical_height: i32, scale: f32) -> Self {
        Self::new(
            physical_width.max(1) as u32,
            physical_height.max(1) as u32,
            scale,
        )
    }

    #[inline]
    fn logical_from_physical(pw: u32, ph: u32, scale: f32) -> (u32, u32) {
        let lw = ((pw as f32) / scale).round().max(1.0) as u32;
        let lh = ((ph as f32) / scale).round().max(1.0) as u32;
        (lw, lh)
    }

    /// Single rounding: physical = (logical * scale).round().max(1) — no round-trip.
    /// This is the canonical forward mapping that must be used for DPI-correct rendering
    /// to avoid 0.25px subpixel edges at fractional scales (1.25/1.5/1.75).
    #[inline]
    pub fn physical_from_logical(logical_w: u32, logical_h: u32, scale: f32) -> (u32, u32) {
        let s = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let pw = ((logical_w as f32) * s).round().max(1.0) as u32;
        let ph = ((logical_h as f32) * s).round().max(1.0) as u32;
        (pw, ph)
    }

    /// Create a Viewport from logical size + scale — derives physical via single rounding
    /// `physical = (logical * scale).round()`. Useful for tests and render snapping.
    pub fn from_logical(logical_width: u32, logical_height: u32, scale: f32) -> Self {
        let s = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let (pw, ph) = Self::physical_from_logical(logical_width, logical_height, s);
        Self {
            physical: PhysicalSize {
                width: pw,
                height: ph,
            },
            logical: LogicalSize {
                width: logical_width.max(1),
                height: logical_height.max(1),
            },
            scale: s,
        }
    }

    /// Snap a logical coordinate/value to the nearest physical pixel grid for the current scale.
    /// Ensures `((logical * scale).round() / scale)` so edges align to device pixels
    /// and no 0.25px hairline/blur at 1.25/1.5 appears after `canvas.scale(scale)`.
    #[inline]
    pub fn snap_logical_to_physical_grid(value: f32, scale: f32) -> f32 {
        let s = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        (value * s).round() / s
    }

    /// Update physical size, recomputing logical. Clamps to >=1.
    pub fn set_physical(&mut self, width: u32, height: u32) {
        self.physical.width = width.max(1);
        self.physical.height = height.max(1);
        let (lw, lh) =
            Self::logical_from_physical(self.physical.width, self.physical.height, self.scale);
        self.logical.width = lw;
        self.logical.height = lh;
    }

    /// Update physical from i32 (skia api).
    pub fn set_physical_i32(&mut self, width: i32, height: i32) {
        self.set_physical(width.max(1) as u32, height.max(1) as u32);
    }

    /// Update scale, recomputing logical.
    pub fn set_scale(&mut self, scale: f32) {
        if scale.is_finite() && scale > 0.0 {
            self.scale = scale;
            let (lw, lh) =
                Self::logical_from_physical(self.physical.width, self.physical.height, self.scale);
            self.logical.width = lw;
            self.logical.height = lh;
        }
    }

    #[inline]
    pub fn physical_width_i32(&self) -> i32 {
        self.physical.width as i32
    }

    #[inline]
    pub fn physical_height_i32(&self) -> i32 {
        self.physical.height as i32
    }

    #[inline]
    pub fn logical_size(&self) -> (u32, u32) {
        (self.logical.width, self.logical.height)
    }

    #[inline]
    pub fn physical_size_u32(&self) -> (u32, u32) {
        (self.physical.width, self.physical.height)
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new(800, 600, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::Viewport;

    #[test]
    fn clamp_zero() {
        let v = Viewport::new(0, 0, 1.0);
        assert_eq!(v.physical.width, 1);
        assert_eq!(v.physical.height, 1);
        assert_eq!(v.logical.width, 1);
        assert_eq!(v.logical.height, 1);
    }

    #[test]
    fn fractional_scale_rounding() {
        let v = Viewport::new(800, 600, 1.5);
        // 800/1.5 = 533.333 -> 533, 600/1.5=400
        assert_eq!(v.logical.width, 533);
        assert_eq!(v.logical.height, 400);
    }

    #[test]
    fn update_physical_recomputes_logical() {
        let mut v = Viewport::new(800, 600, 2.0);
        assert_eq!(v.logical.width, 400);
        v.set_physical(0, 0);
        assert_eq!(v.physical.width, 1);
        assert_eq!(v.logical.width, 1);
    }

    #[test]
    fn set_scale_recomputes() {
        let mut v = Viewport::new(800, 600, 1.0);
        v.set_scale(2.0);
        assert_eq!(v.logical.width, 400);
        // invalid scale ignored
        v.set_scale(0.0);
        assert_eq!(v.scale, 2.0);
    }
}
