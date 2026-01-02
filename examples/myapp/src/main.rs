use velox_dom::VNode;
use velox_style::Stylesheet;

include!(concat!(env!("OUT_DIR"), "/App.rs"));

fn main() {
    // Create component state from script section
    let state = app::script_rs::State::new();
    let state_ref = std::sync::Arc::new(state);
    let make_view = { let state = state_ref.clone(); move |_w: u32, _h: u32| -> (VNode, Stylesheet) {
        let vnode = app::render_with_state(state.clone(), |name| if name == "count" { state.count.get().to_string() } else { String::new() });
        let sheet = Stylesheet::parse(app::STYLE);
        (vnode, sheet)
    }};
    // use generated helper from SFC output to wire events to the component state
    let on_event = app::make_on_event(state_ref.clone());
    let get_title = { let state = state_ref.clone(); move || state.title.borrow().to_string() };
    velox_renderer::run_window_vnode_skia("Velox App", make_view, on_event, get_title);
}
