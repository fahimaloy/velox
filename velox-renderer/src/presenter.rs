//! Softbuffer presenter for Skia rendering
//!
//! Bridges Skia raster surfaces to the display via softbuffer.

use softbuffer::{Context, Surface};
use velox_dom::VeloxError;
use winit::window::Window;

/// Returns true if a display compositor appears to be available.
///
/// Checks `WAYLAND_DISPLAY` / `DISPLAY` / `VELOX_HEADLESS` env vars. This is
/// a best-effort heuristic — even if set, the compositor socket may still be
/// unreachable (EPIPE), which is handled separately in `present()`.
pub fn is_compositor_available() -> bool {
    if std::env::var("VELOX_HEADLESS").as_deref() == Ok("1") {
        return false;
    }
    let wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
    let x11 = std::env::var("DISPLAY").is_ok();
    wayland || x11
}

/// Checks if the WAYLAND_DISPLAY socket is actually connectable and not stale.
///
/// When `WAYLAND_DISPLAY` is set but the compositor has crashed or the session
/// ended, the socket file may still exist and even accept TCP connections, but
/// the Wayland protocol handshake will fail. winit 0.28's Wayland backend
/// calls `process::exit()` on such errors, which cannot be caught by
/// `catch_unwind`. This function does a lightweight connectivity check.
///
/// Returns `true` if the Wayland socket file exists and a Unix stream
/// connection can be established. Note: this only checks the socket layer —
/// it does NOT guarantee the Wayland protocol handshake will succeed.
fn is_wayland_socket_alive() -> bool {
    let display = match std::env::var("WAYLAND_DISPLAY") {
        Ok(d) => d,
        Err(_) => return false,
    };

    // WAYLAND_DISPLAY can be an absolute path or a socket name (relative to
    // XDG_RUNTIME_DIR or /tmp).
    let socket_path = if display.starts_with('/') {
        std::path::PathBuf::from(display)
    } else {
        let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
        std::path::PathBuf::from(runtime).join(&display)
    };

    // Quick check: does the socket file exist?
    if !socket_path.exists() {
        return false;
    }

    // Try to connect to the socket. If the compositor is alive, the Unix
    // stream connection succeeds; if it's stale/dead, we get ECONNREFUSED
    // or a similar error.
    use std::os::unix::net::UnixStream;
    UnixStream::connect(&socket_path).is_ok()
}

/// Sets `WINIT_UNIX_BACKEND` to prefer the specified backend, used before
/// creating a winit `EventLoop`. This forces winit to try only the named
/// backend instead of auto-probing Wayland-first.
///
/// This is critical because winit 0.28's Wayland backend calls `process::exit()`
/// on display errors (broken pipe), which **cannot** be caught by `catch_unwind`.
/// By forcing the X11 backend (which panics instead of exiting), we stay safe.
fn force_backend(backend: &str) {
    if cfg!(target_os = "linux") {
        unsafe { std::env::set_var("WINIT_UNIX_BACKEND", backend) }
    }
}

/// Prepares the winit backend selection to avoid Wayland `process::exit()` traps.
///
/// winit 0.28's Wayland backend calls `process::exit(err_code)` on display
/// errors (broken pipe), which **cannot** be caught by `catch_unwind`.
/// This function detects potentially problematic configurations and forces
/// a safer backend.
///
/// Strategy:
/// * If `VELOX_HEADLESS=1`, returns true (headless mode — no window).
/// * If both `DISPLAY` and `WAYLAND_DISPLAY` are set:
///   - X11 is available as a fallback. Force `x11` backend to avoid the
///     Wayland `process::exit()` trap, since we can't reliably detect
///     whether the Wayland compositor will actually work at protocol level.
/// * If only `WAYLAND_DISPLAY` is set (no X11):
///   - Check if the socket is alive (TCP-level check). If the socket exists
///     and accepts connections, we let winit try Wayland (no force). If the
///     socket is dead, there's no fallback — return true so caller goes headless.
/// * If only `DISPLAY` is set: no action needed (winit defaults to X11).
///
/// Returns `true` if the caller should proceed in headless mode (no window
/// creation attempted), `false` if window creation should be tried.
pub fn prepare_backend() -> bool {
    if std::env::var("VELOX_HEADLESS").as_deref() == Ok("1") {
        return true;
    }

    let has_display = std::env::var("DISPLAY").is_ok();
    let has_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();

    if has_display && has_wayland {
        // Both are set. Even if the Wayland socket appears alive, the Wayland
        // protocol handshake might still fail, and winit's Wayland backend
        // calls process::exit() on such errors (uncatchable by catch_unwind).
        // Since X11 is available, force the X11 backend.
        force_backend("x11");
    } else if has_wayland && !has_display {
        // Only Wayland is set (no X11 fallback). Check socket liveness.
        if !is_wayland_socket_alive() {
            // Wayland socket is dead and no X11 fallback.
            // Return true to signal headless mode.
            return true;
        }
        // Socket is alive — let winit try Wayland.
        // If it fails at protocol level, catch_unwind won't help (process::exit),
        // but this is the best we can do without an X11 fallback.
    }

    false
}

/// Returns true if the error string looks like a compositor / broken-pipe failure.
fn is_broken_pipe_error(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("broken pipe")
        || lower.contains("os error 32")
        || lower.contains("epipe")
        || lower.contains("no compositor")
        || lower.contains("failed to connect to wayland")
        || lower.contains("x11 display")
}

fn compositor_help(err_detail: &str) -> String {
    format!(
        "{err_detail} — no compositor available (headless environment?). \
         Set VELOX_HEADLESS=1 to run without a window, or ensure WAYLAND_DISPLAY/DISPLAY is set \
         and a Wayland/X11 compositor is running."
    )
}

/// Presents Skia-rendered content to a window using softbuffer.
pub struct SoftbufferPresenter {
    _context: Context,
    surface: Surface,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    /// Once we observe a broken-pipe / compositor-lost error the presenter
    /// degrades to a no-op so the app can keep rendering offscreen without
    /// crashing the event loop.
    degraded: bool,
}

impl SoftbufferPresenter {
    /// Creates a new presenter for the given window with initial dimensions.
    ///
    /// # Errors
    /// Returns an error if softbuffer context or surface creation fails.
    /// When no compositor is detected the error message includes actionable
    /// guidance about `VELOX_HEADLESS=1`.
    pub fn new(window: &Window, width: u32, height: u32) -> Result<Self, VeloxError> {
        if !is_compositor_available() {
            return Err(VeloxError::Render(compositor_help(
                "softbuffer: no display server detected (WAYLAND_DISPLAY and DISPLAY are both unset)",
            )));
        }
        let context = unsafe {
            Context::new(window).map_err(|e| {
                let msg = e.to_string();
                if is_broken_pipe_error(&msg) {
                    VeloxError::Render(compositor_help(&format!("softbuffer context failed: {msg}")))
                } else {
                    VeloxError::Render(format!("softbuffer context failed: {msg}"))
                }
            })?
        };
        let mut surface = unsafe {
            Surface::new(&context, window).map_err(|e| {
                let msg = e.to_string();
                if is_broken_pipe_error(&msg) {
                    VeloxError::Render(compositor_help(&format!("softbuffer surface failed: {msg}")))
                } else {
                    VeloxError::Render(format!("softbuffer surface failed: {msg}"))
                }
            })?
        };
        let w = width.max(1);
        let h = height.max(1);
        if let Err(e) = surface.resize(
            std::num::NonZeroU32::new(w).expect("w >= 1 guaranteed by .max(1)"),
            std::num::NonZeroU32::new(h).expect("h >= 1 guaranteed by .max(1)"),
        ) {
            let msg = e.to_string();
            if is_broken_pipe_error(&msg) {
                return Err(VeloxError::Render(compositor_help(&format!(
                    "softbuffer resize failed: {msg}"
                ))));
            } else {
                return Err(VeloxError::Render(format!("softbuffer resize failed: {}", msg)));
            }
        }
        Ok(Self {
            _context: context,
            surface,
            width: w,
            height: h,
            rgba: vec![0u8; (w as usize) * (h as usize) * 4],
            degraded: false,
        })
    }

    /// Whether this presenter has degraded to no-op after a broken-pipe error.
    pub fn is_degraded(&self) -> bool {
        self.degraded
    }

    /// Resizes the presenter to new dimensions.
    ///
    /// No-op if dimensions haven't changed. If the presenter has degraded
    /// (broken pipe) this is a no-op.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), VeloxError> {
        if self.degraded {
            return Ok(());
        }
        let w = width.max(1);
        let h = height.max(1);
        if w == self.width && h == self.height {
            return Ok(());
        }
        if let Err(e) = self.surface.resize(
            std::num::NonZeroU32::new(w).expect("w >= 1 guaranteed by .max(1)"),
            std::num::NonZeroU32::new(h).expect("h >= 1 guaranteed by .max(1)"),
        ) {
            let msg = e.to_string();
            if is_broken_pipe_error(&msg) {
                log::warn!("softbuffer resize failed (broken pipe) — degrading presenter to no-op: {msg}");
                self.degraded = true;
                return Ok(());
            } else {
                return Err(VeloxError::Render(format!("softbuffer resize failed: {}", msg)));
            }
        }
        self.width = w;
        self.height = h;
        self.rgba.resize((w as usize) * (h as usize) * 4, 0);
        Ok(())
    }

    /// Presents the contents of the Skia surface to the window.
    ///
    /// Reads pixels from the Skia surface, converts from RGBA to the
    /// softbuffer format, and presents to the display.
    ///
    /// If the compositor connection is broken (EPIPE / broken pipe) the
    /// presenter degrades to a no-op and subsequent calls return `Ok(())`
    /// instead of propagating the error — this prevents the event loop from
    /// crashing in headless / CI environments. The rendering itself still
    /// runs offscreen into the raster Skia surface.
    pub fn present(
        &mut self,
        skia_surface: &mut crate::skia_surface::SkiaSurface,
    ) -> Result<(), VeloxError> {
        if self.degraded {
            return Ok(());
        }
        let width = skia_surface.width.max(1) as u32;
        let height = skia_surface.height.max(1) as u32;
        self.resize(width, height)?;
        // resize may have degraded
        if self.degraded {
            return Ok(());
        }

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

        let mut buffer = match self.surface.buffer_mut() {
            Ok(b) => b,
            Err(e) => {
                let msg = e.to_string();
                if is_broken_pipe_error(&msg) {
                    log::warn!("softbuffer buffer_mut failed (broken pipe) — degrading presenter to no-op: {msg}");
                    self.degraded = true;
                    return Ok(());
                } else {
                    return Err(VeloxError::Render(format!("softbuffer buffer_mut failed: {}", msg)));
                }
            }
        };
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
            // Softbuffer uses ARGB8888 format (u32: 0xAARRGGBB, memory byte order: BGRA)
            // Skia outputs RGBA with premultiplied alpha
            *pixel = (a << 24) | (r << 16) | (g << 8) | b;
        }
        if let Err(e) = buffer.present() {
            let msg = e.to_string();
            if is_broken_pipe_error(&msg) {
                log::warn!("softbuffer present failed (broken pipe) — degrading presenter to no-op: {msg}");
                self.degraded = true;
                return Ok(());
            } else {
                return Err(VeloxError::Render(format!("softbuffer present failed: {}", msg)));
            }
        }
        Ok(())
    }
}
