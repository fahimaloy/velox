//! Softbuffer presenter for Skia rendering
//!
//! Bridges Skia raster surfaces to the display via softbuffer.

use softbuffer::{Context, Surface};
use velox_dom::VeloxError;
use winit::window::Window;

use crate::viewport::Viewport;

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
///
/// Unix-only: the check is a `std::os::unix::net::UnixStream` connect, which
/// exists on no other target. Off unix the same name is answered by the
/// `#[cfg(not(unix))]` twin below, so the call site in [`prepare_backend`]
/// needs no `cfg` of its own.
#[cfg(unix)]
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

/// Non-unix twin of [`is_wayland_socket_alive`]: always `false`.
///
/// The unix answer comes from a `UnixStream` connect, and there is no Unix
/// socket API on any other target, so there is nothing to connect with —
/// `false` ("no reachable Wayland socket") is the honest answer, not a
/// placeholder. It must not be `unreachable!()` or `todo!()`: this runs inside
/// the event loop, where a panic is a crash rather than a report.
///
/// The consequence is deliberate. In [`prepare_backend`], a `false` here with
/// `WAYLAND_DISPLAY` set and no `DISPLAY` fallback sends the caller down the
/// headless path. That is the safe direction to err in, because the whole
/// reason this function exists is that winit's Wayland backend `process::exit()`s
/// (uncatchably) when the socket is dead — declining to try beats exiting.
/// Off unix winit does not select the Wayland backend anyway, so in practice a
/// stray `WAYLAND_DISPLAY` costs a headless window rather than a usable one.
///
/// Deliberately not a `cfg!` arm inside one body: the `use` of
/// `std::os::unix::net::UnixStream` is a hard compile error off unix, so the
/// unix half has to disappear as a unit rather than be branched over.
#[cfg(not(unix))]
fn is_wayland_socket_alive() -> bool {
    false
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
        // SAFETY: `set_var` races with `getenv` in other threads, which is
        // why it is `unsafe`. This runs during startup window-bootstrap,
        // before the event loop (and any thread that reads
        // `WINIT_UNIX_BACKEND`) exists, so no concurrent read can observe a
        // torn value. The single-threaded test below serialises the env-var
        // tests for the same reason.
        unsafe { std::env::set_var("WINIT_UNIX_BACKEND", backend) }
    }
}

/// One-line compositor/backend diagnostic (CX-13 / F-23).
///
/// Pure helper: reads the backend-selection env vars and formats them with
/// `Debug`, so unset vars show as `Err(NotPresent)`. The
/// `VELOX_DEBUG_COMPOSITOR`-gated `eprintln!` in [`prepare_backend`] is a
/// thin wrapper around this — tests assert the exact output here.
pub fn debug_compositor_choice() -> String {
    format!(
        "[velox] backend={:?} wayland={:?} display={:?}",
        std::env::var("WINIT_UNIX_BACKEND"),
        std::env::var("WAYLAND_DISPLAY"),
        std::env::var("DISPLAY")
    )
}

/// Ensures the `VELOX_DEBUG_COMPOSITOR` diagnostic is printed once per
/// process even though `prepare_backend` runs for both window entry points.
static DEBUG_COMPOSITOR_LOG: std::sync::Once = std::sync::Once::new();

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
/// If `VELOX_DEBUG_COMPOSITOR` is set (any value), the backend choice is
/// logged once via stderr — diagnostics only, never a behavior change.
///
/// Returns `true` if the caller should proceed in headless mode (no window
/// creation attempted), `false` if window creation should be tried.
pub fn prepare_backend() -> bool {
    if std::env::var("VELOX_DEBUG_COMPOSITOR").is_ok() {
        DEBUG_COMPOSITOR_LOG.call_once(|| {
            eprintln!("{}", debug_compositor_choice());
        });
    }

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
        //
        // Intentionally NOT `#[cfg]`-gated: `is_wayland_socket_alive` has a
        // `#[cfg(not(unix))]` twin that answers `false`, so this call resolves
        // on every target. Adding a `cfg` here would leave that twin uncalled
        // off unix and turn it into a `dead_code` warning, which this repo's
        // `-D warnings` gate would then fail on the non-unix build.
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

/// Process-global guard so the EPIPE degrade warning surfaces exactly once
/// per session (F-23), even if more than one presenter instance degrades —
/// per-frame spam is suppressed.
static EPIPE_WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Largest softbuffer staging buffer, in bytes (`w*h*4`).
///
/// 128 MiB is a 5792x5792 RGBA surface: anything above it is a degenerate
/// resize, not a window, and `vec!` would abort the process trying. Callers
/// that exceed it get a `VeloxError::Render` before a single byte is
/// allocated — the same error shape every other presenter failure uses.
const MAX_STAGING_BYTES: usize = 128 * 1024 * 1024;

/// `NonZeroU32` without the panic: `w`/`h` arrive `.max(1)`-clamped at every
/// call site, so the fallback is unreachable in practice, but `expect` would
/// still abort the event loop on the one caller that forgot the clamp.
/// `MIN` (1) is the same value the clamp would have produced.
fn nonzero_dim(v: u32) -> std::num::NonZeroU32 {
    std::num::NonZeroU32::new(v).unwrap_or(std::num::NonZeroU32::MIN)
}

/// Checked `w * h * 4` for the `read_pixels` staging buffer.
///
/// `checked_mul` throughout: `w`/`h` are `u32` and a `0xFFFFFFFF x
/// 0xFFFFFFFF` resize overflows `usize` multiplication in debug (panic) and
/// wraps in release (a too-small staging buffer `read_pixels` then overruns).
fn staging_len(w: u32, h: u32) -> Result<usize, VeloxError> {
    let len = (w as usize)
        .checked_mul(h as usize)
        .and_then(|px| px.checked_mul(4))
        .ok_or_else(|| {
            VeloxError::Render(
                "softbuffer: presenter dimensions overflow the staging buffer".into(),
            )
        })?;
    if len > MAX_STAGING_BYTES {
        return Err(VeloxError::Render(format!(
            "softbuffer: presenter {w}x{h} needs {len} bytes, over the {MAX_STAGING_BYTES}-byte cap"
        )));
    }
    Ok(len)
}

/// Claims the right to report an EPIPE degrade against `flag`.
/// Returns `true` only on the first call; every later call returns `false`.
fn claim_epipe_warning(flag: &std::sync::atomic::AtomicBool) -> bool {
    !flag.swap(true, std::sync::atomic::Ordering::SeqCst)
}

/// Surfaces a compositor broken-pipe (EPIPE) error once per session.
///
/// The presenter still degrades to a no-op so rendering continues offscreen,
/// but the error is no longer silently masked: `eprintln!` reaches stderr
/// even when no logger is initialized, and the static guard prevents
/// per-frame spam.
fn warn_epipe_once(detail: &str) {
    if claim_epipe_warning(&EPIPE_WARNED) {
        eprintln!(
            "[velox] compositor connection lost (broken pipe) — presenter degraded to no-op, \
             rendering continues offscreen. Error: {detail}"
        );
        eprintln!(
            "[velox] hint: set VELOX_HEADLESS=1 to run without a window, or ensure \
             WAYLAND_DISPLAY/DISPLAY points to a running compositor."
        );
    }
}

/// Presents Skia-rendered content to a window using softbuffer.
pub struct SoftbufferPresenter {
    _context: Context,
    surface: Surface,
    viewport: Viewport,
    /// Legacy aliases — kept in sync with viewport.
    width: u32,
    height: u32,
    /// Staging buffer for `read_pixels`, in the surface's native `N32` order
    /// ([B, G, R, A] little-endian), which is already softbuffer's word order.
    /// Written once per frame and then bulk-copied — no per-pixel work.
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
        // SAFETY: `Context::new` is `unsafe` because softbuffer cannot verify
        // the window handle outlives the context. `window` is borrowed from
        // the winit event loop, which outlives this presenter by construction
        // (the presenter is created and dropped inside the loop's lifetime),
        // so the handle is valid for the whole `Self`. A creation failure is
        // a `Result::Err`, not undefined behaviour, and is mapped below.
        let context = unsafe {
            Context::new(window).map_err(|e| {
                let msg = e.to_string();
                if is_broken_pipe_error(&msg) {
                    VeloxError::Render(compositor_help(&format!(
                        "softbuffer context failed: {msg}"
                    )))
                } else {
                    VeloxError::Render(format!("softbuffer context failed: {msg}"))
                }
            })?
        };
        // SAFETY: same lifetime contract as `Context::new` above — `window`
        // outlives `Self`, and `Surface::new` only reads the handle during
        // this call. Failure is a mapped `Err`, never UB.
        let mut surface = unsafe {
            Surface::new(&context, window).map_err(|e| {
                let msg = e.to_string();
                if is_broken_pipe_error(&msg) {
                    VeloxError::Render(compositor_help(&format!(
                        "softbuffer surface failed: {msg}"
                    )))
                } else {
                    VeloxError::Render(format!("softbuffer surface failed: {msg}"))
                }
            })?
        };
        let w = width.max(1);
        let h = height.max(1);
        if let Err(e) = surface.resize(nonzero_dim(w), nonzero_dim(h)) {
            let msg = e.to_string();
            if is_broken_pipe_error(&msg) {
                return Err(VeloxError::Render(compositor_help(&format!(
                    "softbuffer resize failed: {msg}"
                ))));
            } else {
                return Err(VeloxError::Render(format!(
                    "softbuffer resize failed: {}",
                    msg
                )));
            }
        }
        let viewport = Viewport::new(w, h, 1.0);
        // Sized through `staging_len` BEFORE allocating: `w`/`h` are raw
        // `u32` and a degenerate resize would otherwise overflow the
        // multiplication (debug panic) or allocate until the OOM killer
        // arrives. An over-cap size is a `Render` error like any other
        // presenter failure, returned before a single byte is allocated.
        let rgba = vec![0u8; staging_len(w, h)?];
        Ok(Self {
            _context: context,
            surface,
            viewport,
            width: w,
            height: h,
            rgba,
            degraded: false,
        })
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
        // Checked BEFORE touching the surface: on failure nothing has been
        // resized yet, so the presenter keeps its old (working) geometry
        // instead of a resized surface paired with a stale staging buffer.
        let len = staging_len(w, h)?;
        if let Err(e) = self.surface.resize(nonzero_dim(w), nonzero_dim(h)) {
            let msg = e.to_string();
            if is_broken_pipe_error(&msg) {
                warn_epipe_once(&msg);
                self.degraded = true;
                return Ok(());
            } else {
                return Err(VeloxError::Render(format!(
                    "softbuffer resize failed: {}",
                    msg
                )));
            }
        }
        self.viewport.set_physical(w, h);
        self.width = w;
        self.height = h;
        self.rgba.resize(len, 0);
        Ok(())
    }

    /// Presents the contents of the Skia surface to the window.
    ///
    /// Reads pixels from the Skia surface into the staging buffer and
    /// bulk-copies them into the softbuffer buffer, then presents to the
    /// display. No per-pixel colour conversion is needed: the surface's
    /// native `N32` order already matches softbuffer's (see the comment at
    /// the `ImageInfo` below).
    ///
    /// If the compositor connection is broken (EPIPE / broken pipe) the
    /// presenter degrades to a no-op and subsequent calls return `Ok(())`
    /// instead of propagating the error — this prevents the event loop from
    /// crashing in headless / CI environments. The rendering itself still
    /// runs offscreen into the raster Skia surface. The first degrade
    /// surfaces the error once per session on stderr (see `warn_epipe_once`).
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

        // Read back in the surface's own `N32` order. The surface is created
        // with `raster_n32_premul`, and on a little-endian target Skia's
        // `kN32_SkColorType` is `kBGRA_8888` — i.e. the surface already holds
        // `[B, G, R, A]` in memory, which is exactly softbuffer's word order.
        //
        // Previously this asked for `ColorType::RGBA8888`, which made Skia
        // swizzle BGRA -> RGBA on the way out, and the per-pixel loop below
        // then swizzled straight back RGB -> BGR. The two transforms cancelled
        // out, so the whole per-pixel pass existed only to undo Skia's copy.
        // Asking for `N32` makes `read_pixels` a straight copy and leaves a
        // bulk memcpy below.
        //
        // This is verified at the byte level by
        // `tests/presenter_pixel_bytes.rs`, which also pins that N32 really is
        // BGRA on this target (a build that flipped it would fail there
        // rather than silently presenting swapped channels).
        let info = skia_safe::ImageInfo::new(
            (self.width as i32, self.height as i32),
            skia_safe::ColorType::N32,
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
                    warn_epipe_once(&msg);
                    self.degraded = true;
                    return Ok(());
                } else {
                    return Err(VeloxError::Render(format!(
                        "softbuffer buffer_mut failed: {}",
                        msg
                    )));
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
        // The staging bytes from `read_pixels` are already in softbuffer's
        // layout (0xAARRGGBB, little-endian memory order [B, G, R, A]), so
        // this is a straight byte copy rather than a per-pixel swizzle.
        let dst = bytemuck::cast_slice_mut::<u32, u8>(&mut pixels[..pixel_count]);
        dst.copy_from_slice(&self.rgba[..pixel_count * 4]);
        if let Err(e) = buffer.present() {
            let msg = e.to_string();
            if is_broken_pipe_error(&msg) {
                warn_epipe_once(&msg);
                self.degraded = true;
                return Ok(());
            } else {
                return Err(VeloxError::Render(format!(
                    "softbuffer present failed: {}",
                    msg
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RAII guard that restores an env var to its prior value (or removes it)
    /// on drop — including on assertion failure — so tests never leak state.
    struct EnvVarGuard {
        key: &'static str,
        prev: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prev = std::env::var(key).ok();
            // SAFETY: process-global env mutation. All env-var assertions in
            // this module live in ONE test (`debug_flag_does_not_change_...`),
            // and no other test in this crate touches these keys, so no
            // thread can read the var mid-mutation. See that test's note.
            unsafe { std::env::set_var(key, value) };
            Self { key, prev }
        }

        /// Removes the var now; restores the prior value on drop.
        fn remove(key: &'static str) -> Self {
            let prev = std::env::var(key).ok();
            // SAFETY: same single-test serialisation as `set` above.
            unsafe { std::env::remove_var(key) };
            Self { key, prev }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match self.prev.take() {
                // SAFETY: same single-test serialisation as `set` above.
                Some(v) => unsafe { std::env::set_var(self.key, v) },
                None => unsafe { std::env::remove_var(self.key) },
            };
        }
    }

    /// CX-13 / F-23: `VELOX_DEBUG_COMPOSITOR` must log the backend choice but
    /// must NOT change the degraded (headless / force-x11) code path.
    ///
    /// NOTE on parallelism: `std::env::set_var` is process-global and Rust
    /// runs unit tests in the same binary on parallel threads. All env-var
    /// assertions in this module are collected into this single test, and no
    /// other unit test in velox-renderer reads or writes `WAYLAND_DISPLAY` /
    /// `DISPLAY` / `WINIT_UNIX_BACKEND` / `VELOX_HEADLESS` /
    /// `VELOX_DEBUG_COMPOSITOR`, which contains the race.
    #[test]
    fn debug_flag_does_not_change_degraded_path() {
        let _g_backend = EnvVarGuard::remove("WINIT_UNIX_BACKEND");
        let _g_wayland = EnvVarGuard::set("WAYLAND_DISPLAY", "wayland-test-0");
        let _g_display = EnvVarGuard::set("DISPLAY", ":99");
        let _g_debug = EnvVarGuard::set("VELOX_DEBUG_COMPOSITOR", "1");
        let _g_headless = EnvVarGuard::remove("VELOX_HEADLESS");

        // With both sockets configured the debug flag must not alter the
        // existing force-x11 choice (Wayland process::exit() trap avoidance).
        assert!(!prepare_backend());
        assert_eq!(
            std::env::var("WINIT_UNIX_BACKEND").as_deref(),
            Ok("x11"),
            "force-x11 must be kept when DISPLAY and WAYLAND_DISPLAY are both set"
        );

        // The pure helper renders the exact diagnostic format from the task
        // brief, including the backend forced above.
        assert_eq!(
            debug_compositor_choice(),
            "[velox] backend=Ok(\"x11\") wayland=Ok(\"wayland-test-0\") display=Ok(\":99\")"
        );

        // Unset vars render as Err(NotPresent).
        {
            let _g = EnvVarGuard::remove("WINIT_UNIX_BACKEND");
            let s = debug_compositor_choice();
            assert!(s.contains("backend=Err(NotPresent)"), "got: {s}");
        }

        // Headless degraded path is unchanged by the debug flag: compositor
        // reports unavailable and prepare_backend still selects headless —
        // the env-gated eprintln must log without panicking. Repeated calls
        // exercise the "logged once" Once guard and must not panic either.
        let _g_headless_on = EnvVarGuard::set("VELOX_HEADLESS", "1");
        assert!(!is_compositor_available());
        assert!(prepare_backend());
        assert!(prepare_backend());
    }

    /// F-23: the EPIPE degrade warning must be claimable exactly once per
    /// session (this drives `warn_epipe_once`'s no-spam guarantee).
    /// Env-free and race-free.
    #[test]
    fn epipe_warning_claimed_once_per_session() {
        let flag = std::sync::atomic::AtomicBool::new(false);
        assert!(claim_epipe_warning(&flag));
        assert!(!claim_epipe_warning(&flag));
        assert!(!claim_epipe_warning(&flag));
    }
}
