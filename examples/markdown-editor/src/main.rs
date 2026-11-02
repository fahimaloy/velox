use std::sync::Arc;
use velox_dom::VNode;
use velox_style::Stylesheet;
include!(concat!(env!("OUT_DIR"), "/app.rs"));
#[allow(clippy::arc_with_non_send_sync)]
fn main() {
    let state = Arc::new(app::script_rs::State::new());
    let make_view = {
        let s = Arc::clone(&state);
        move |w: u32, h: u32| -> (VNode, Stylesheet) {
            let vnode = app::render_with_state(Arc::clone(&s), app::make_resolve(Arc::clone(&s)));
            let sheet = Stylesheet::parse(app::STYLE);
            (vnode, sheet)
        }
    };
    let on_event = app::make_on_event(Arc::clone(&state));
    let get_title = || "Velox Example".to_string();
    if let Some(port) = velox_renderer::hmr_config() {
        let (tx, rx) = std::sync::mpsc::channel();
        velox_renderer::run_hmr_client(port, tx);
        velox_renderer::run_window_vnode_skia_with_hmr(
            "Velox Example",
            make_view, on_event, get_title,
            Arc::new(std::sync::Mutex::new(rx))
        );
    } else {
        velox_renderer::run_window_vnode_skia("Velox Example", make_view, on_event, get_title);
    }
}
