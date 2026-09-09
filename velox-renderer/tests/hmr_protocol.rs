//! Integration tests for the HMR message protocol.
//!
//! These tests verify that `HmrMessage` serializes and deserializes
//! correctly as newline-delimited JSON, which is the wire format used
//! between the dev server (in `velox-cli`) and the app (via
//! `run_hmr_client` in `velox-renderer::hmr`).

use velox_renderer::{HmrMessage, DEFAULT_HMR_PORT};

#[test]
fn default_hmr_port_is_well_known() {
    // The port is part of the HMR protocol contract. Apps and dev servers
    // must agree on it for the connection to work.
    assert_eq!(DEFAULT_HMR_PORT, 31313);
}

#[test]
fn full_reload_serializes_roundtrip() {
    let msg = HmrMessage::FullReload;
    let json = serde_json::to_string(&msg).expect("serialize FullReload");

    // The serialized form must be a single line (newline-delimited protocol).
    assert!(!json.contains('\n'), "serialized message must not contain newlines");

    let back: HmrMessage = serde_json::from_str(&json).expect("deserialize FullReload");
    assert_eq!(back, HmrMessage::FullReload);
}

#[test]
fn hot_reload_serializes_roundtrip() {
    let msg = HmrMessage::HotReload {
        module_path: "src/components/Counter.vx".to_string(),
    };
    let json = serde_json::to_string(&msg).expect("serialize HotReload");

    assert!(!json.contains('\n'));

    // The module_path field must appear in the JSON.
    assert!(json.contains("module_path"));
    assert!(json.contains("Counter.vx"));

    let back: HmrMessage = serde_json::from_str(&json).expect("deserialize HotReload");
    assert_eq!(back, msg);
}

#[test]
fn keep_window_serializes_roundtrip() {
    let msg = HmrMessage::KeepWindow;
    let json = serde_json::to_string(&msg).expect("serialize KeepWindow");

    assert!(!json.contains('\n'));

    let back: HmrMessage = serde_json::from_str(&json).expect("deserialize KeepWindow");
    assert_eq!(back, HmrMessage::KeepWindow);
}

#[test]
fn newline_delimited_protocol_parses() {
    // Simulate the newline-delimited JSON stream that the dev server
    // sends. Each message is on its own line.
    // Serialize each variant to determine the correct JSON format,
    // since serde's representation depends on the derive configuration.
    let lines: Vec<String> = [
        HmrMessage::FullReload,
        HmrMessage::HotReload {
            module_path: "src/App.vx".to_string(),
        },
        HmrMessage::KeepWindow,
    ]
    .iter()
    .map(|m| serde_json::to_string(m).expect("serialize"))
    .collect();

    // Each message must be on its own line (newline-delimited protocol).
    lines.iter().for_each(|l| {
        assert!(
            !l.contains('\n'),
            "serialized message must not contain newlines"
        );
    });

    let stream = lines.join("\n");
    let parsed: Vec<HmrMessage> = stream
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<HmrMessage>(l.trim()).ok())
        .collect();

    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0], HmrMessage::FullReload);
    assert_eq!(
        parsed[1],
        HmrMessage::HotReload {
            module_path: "src/App.vx".to_string()
        }
    );
    assert_eq!(parsed[2], HmrMessage::KeepWindow);
}

#[test]
fn hmr_config_disabled_by_default() {
    // When VELOX_HMR is not set, hmr_config should return None.
    // We can't easily test with the env var set (other tests may interfere),
    // but this test documents the contract.
    // Note: if VELOX_HMR is set in the environment, this would fail — it's
    // inherently environment-dependent.
    // SAFETY: we are testing that hmr_config returns None when the env
    // var is not set. remove_var is unsafe because it affects the process
    // environment, but in a test binary that's fine.
    unsafe {
        std::env::remove_var("VELOX_HMR");
        std::env::remove_var("VELOX_HMR_PORT");
    }
    assert!(velox_renderer::hmr_config().is_none());
}
