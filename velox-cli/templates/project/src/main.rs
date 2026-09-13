use std::sync::Arc;
use velox_dom::VNode;
use velox_style::Stylesheet;

include!(concat!(env!("OUT_DIR"), "/app.rs"));

fn main() -> Result<(), String> {
    println!("Starting {}...", env!("CARGO_PKG_NAME"));

    let state = Arc::new(app::script_rs::State::new());

    // Viewport contract (X-H1 / 1A): the renderer calls make_view with the
    // current *logical* viewport size (w,h) — `logical_size(physical, scale)`.
    // Templates must acknowledge (w,h): even if the VNode itself is not
    // parameterized by them, the root style uses `width:100%` /
    // `min-height:100vh` (see src/App.vx `.app`) so compute_layout(w,h)
    // visibly reflows on every window resize / DPI change without hidden-when-visible.
    let make_view = {
        let state = Arc::clone(&state);
        move |w: u32, h: u32| -> (VNode, Stylesheet) {
            let _viewport = (w, h);
            let vnode = app::render_with_state(
                Arc::clone(&state),
                app::make_resolve(Arc::clone(&state)),
            );
            let sheet = Stylesheet::parse(app::STYLE);
            (vnode, sheet)
        }
    };

    let on_event = app::make_on_event(Arc::clone(&state));
    let get_title = {
        let state = Arc::clone(&state);
        move || state.title()
    };

    // Check for HMR mode — when running under the dev server, we use
    // the HMR-aware renderer so the dev server can send reload signals.
    if let Some(port) = velox_renderer::hmr_config() {
        let (hmr_tx, hmr_rx) = std::sync::mpsc::channel::<velox_renderer::HmrMessage>();
        let hmr_rx = Arc::new(std::sync::Mutex::new(hmr_rx));
        velox_renderer::run_hmr_client(port, hmr_tx);

        velox_renderer::run_window_vnode_skia_with_hmr(
            "Velox App",
            make_view,
            on_event,
            get_title,
            hmr_rx,
        )
    } else {
        velox_renderer::run_window_vnode_skia("Velox App", make_view, on_event, get_title)
    }
}
