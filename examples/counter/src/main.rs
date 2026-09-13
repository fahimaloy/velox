use std::sync::Arc;
use velox_dom::VNode;
use velox_style::Stylesheet;

include!(concat!(env!("OUT_DIR"), "/app.rs"));

#[allow(clippy::arc_with_non_send_sync)]
fn main() {
    println!("Starting {}...", env!("CARGO_PKG_NAME"));

    let state = Arc::new(app::script_rs::State::new());

    let make_view = {
        let state = Arc::clone(&state);
        // Viewport contract (X-H1): renderer passes logical viewport size (w,h).
        // The root .app in App.vx fills the viewport via `width:100%` /
        // `min-height:100vh`, so compute_layout(w,h) reflows visibly on resize.
        move |w: u32, h: u32| -> (VNode, Stylesheet) {
            let _viewport = (w, h); // acknowledge viewport — layout is viewport-driven
            let vnode =
                app::render_with_state(Arc::clone(&state), app::make_resolve(Arc::clone(&state)));
            let sheet = Stylesheet::parse(app::STYLE);
            (vnode, sheet)
        }
    };

    let on_event = app::make_on_event(Arc::clone(&state));
    let get_title = || "Velox Counter".to_string();

    // Check for HMR mode — when running under the dev server, use the
    // HMR-aware renderer so the dev server can send reload signals.
    if let Some(port) = velox_renderer::hmr_config() {
        let (hmr_tx, hmr_rx) = std::sync::mpsc::channel::<velox_renderer::HmrMessage>();
        let hmr_rx = Arc::new(std::sync::Mutex::new(hmr_rx));
        velox_renderer::run_hmr_client(port, hmr_tx);

        let _ = velox_renderer::run_window_vnode_skia_with_hmr(
            "Velox Counter",
            make_view,
            on_event,
            get_title,
            hmr_rx,
        );
    } else {
        let _ = velox_renderer::run_window_vnode_skia("Velox Counter", make_view, on_event, get_title);
    }
}
