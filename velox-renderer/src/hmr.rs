//! HMR (Hot Module Replacement) support for the Velox dev server.
//!
//! The dev server (in `velox-cli`) starts a TCP listener on
//! `DEFAULT_HMR_PORT`. When an app runs with `VELOX_HMR=1`,
//! [`hmr_config`] reads the port from the `VELOX_HMR_PORT` environment
//! variable and [`run_hmr_client`] spawns a background thread that
//! connects to the dev server and sends [`HmrMessage`] values through
//! a channel.
//!
//! The message protocol is newline-delimited JSON. The current protocol
//! supports three message types:
//!
//! - `FullReload` — the app should rebuild and re-mount.
//! - `HotReload { module_path }` — the app should re-execute a specific
//!   module (future use, currently falls back to FullReload behavior).
//! - `KeepWindow` — a no-op that the dev server can send to probe the
//!   connection without restarting.

use serde::{Deserialize, Serialize};

/// The default TCP port for HMR communication between the dev server
/// and the app. The dev server listens on this port; apps connect to it.
pub const DEFAULT_HMR_PORT: u16 = 31313;

/// Messages exchanged between the dev server and the app over the HMR
/// TCP channel. Serialized as newline-delimited JSON.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HmrMessage {
    /// Signal the app to rebuild and restart. The app receives this,
    /// exits with code 0, and the dev server restarts it with a fresh build.
    FullReload,

    /// Signal the app to hot-reload a specific module. Currently
    /// treated as a FullReload (module-level HMR not yet implemented).
    HotReload {
        module_path: String,
    },

    /// A keep-alive / no-op message.
    KeepWindow,
}

/// Check whether the current process is running under the Velox dev
/// server's HMR mode. When `VELOX_HMR=1` is set in the environment,
/// this returns `Some(port)` where `port` is read from
/// `VELOX_HMR_PORT` (defaulting to [`DEFAULT_HMR_PORT`] if unset).
/// Otherwise returns `None`.
pub fn hmr_config() -> Option<u16> {
    let hmr_enabled =
        std::env::var("VELOX_HMR").as_deref() == Ok("1");
    if !hmr_enabled {
        return None;
    }
    let port = std::env::var("VELOX_HMR_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(DEFAULT_HMR_PORT);
    Some(port)
}

/// Start a background thread that connects to the HMR dev server at
/// `127.0.0.1:port` and forwards received [`HmrMessage`] values through
/// the provided channel sender.
///
/// The thread runs for the lifetime of the process (or until the
/// channel sender is dropped). It handles reconnection: if the
/// connection to the dev server is lost, it retries every 500ms.
///
/// On `HmrMessage::FullReload`, the thread also forces the process to
/// exit with code 0, which the dev server detects (via the child
/// process exiting) and then restarts with a fresh build.
pub fn run_hmr_client(port: u16, tx: std::sync::mpsc::Sender<HmrMessage>) {
    std::thread::spawn(move || {
        loop {
            match std::net::TcpStream::connect(("127.0.0.1", port)) {
                Ok(stream) => {
                    eprintln!(
                        "[velox] HMR client connected to dev server on port {}",
                        port
                    );
                    let reader = std::io::BufReader::new(stream);
                    for line in std::io::BufRead::lines(reader) {
                        match line {
                            Ok(json) => match serde_json::from_str::<HmrMessage>(&json) {
                                Ok(msg) => {
                                    if tx.send(msg.clone()).is_err() {
                                        // Receiver dropped — stop the thread.
                                        return;
                                    }
                                    if msg == HmrMessage::FullReload {
                                        std::process::exit(0);
                                    }
                                }
                                Err(e) => {
                                    eprintln!(
                                        "[velox] HMR: failed to parse message: {}",
                                        e
                                    );
                                }
                            },
                            Err(e) => {
                                eprintln!("[velox] HMR connection error: {}", e);
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[velox] HMR client: connection to port {} failed: {} — retrying...",
                        port, e
                    );
                }
            }
            // Retry interval between connection attempts.
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    });
}
