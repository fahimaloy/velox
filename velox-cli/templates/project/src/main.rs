use std::sync::Arc;
use velox_core::signal::Signal;
use velox_dom::VNode;
use velox_style::Stylesheet;

include!(concat!(env!("OUT_DIR"), "/App.rs"));

mod components;

fn main() {
    println!("Starting {}...", env!("CARGO_PKG_NAME"));

    let state = Arc::new(app::script_rs::State::new());

    let make_view = {
        let state = Arc::clone(&state);
        move |_w: u32, _h: u32| -> (VNode, Stylesheet) {
            let vnode = app::render_with_state(Arc::clone(&state), |name| match name {
                "title" => state.title.get(),
                "last_event" => state.last_event.get(),
                "new_task" => state.new_task.get(),
                _ => String::new(),
            });
            let sheet = Stylesheet::parse(app::STYLE);
            (vnode, sheet)
        }
    };

    let on_event = app::make_on_event(Arc::clone(&state));
    let get_title = {
        let state = Arc::clone(&state);
        move || state.title.get()
    };

    velox_renderer::run_window_vnode_skia("Velox App", make_view, on_event, get_title);
}
