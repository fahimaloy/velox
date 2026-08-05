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
        move |_w: u32, _h: u32| -> (VNode, Stylesheet) {
            let vnode = app::render_with_state(Arc::clone(&state), |name| match name {
                "title" => state.title(),
                "count" => state.count().to_string(),
                _ => String::new(),
            });
            let sheet = Stylesheet::parse(app::STYLE);
            (vnode, sheet)
        }
    };

    let on_event = app::make_on_event(Arc::clone(&state));
    let get_title = || "Velox Counter".to_string();

    let _ = velox_renderer::run_window_vnode_skia("Velox Counter", make_view, on_event, get_title);
}
