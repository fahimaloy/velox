use std::sync::Arc;

use velox_dom::VNode;
use velox_style::Stylesheet;

// Include the generated code from build.rs.
include!(concat!(env!("OUT_DIR"), "/app.rs"));

#[allow(clippy::arc_with_non_send_sync)]
fn main() {
    let state = Arc::new(app::script_rs::State::new());

    // make_view: react to window resize.
    let make_view = {
        let state = Arc::clone(&state);
        move |w: u32, h: u32| -> (VNode, Stylesheet) {
            let _viewport = (w, h);
            let vnode =
                app::render_with_state(Arc::clone(&state), app::make_resolve(Arc::clone(&state)));
            let sheet = Stylesheet::parse(app::STYLE);
            (vnode, sheet)
        }
    };

    let on_event = app::make_on_event(Arc::clone(&state));

    let get_title = || "Velox Todo".to_string();

    // HMR when the dev server is running; otherwise a normal window.
    if let Some(port) = velox_renderer::hmr_config() {
        let (hmr_tx, hmr_rx) = std::sync::mpsc::channel::<velox_renderer::HmrMessage>();
        let hmr_rx = Arc::new(std::sync::Mutex::new(hmr_rx));
        velox_renderer::run_hmr_client(port, hmr_tx);
        let _ = velox_renderer::run_window_vnode_skia_with_hmr(
            "Velox Todo",
            make_view,
            on_event,
            get_title,
            hmr_rx,
        );
    } else {
        let _ = velox_renderer::run_window_vnode_skia("Velox Todo", make_view, on_event, get_title);
    }
}
