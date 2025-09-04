use velox_dom::VNode;
use velox_style::Stylesheet;
use velox_renderer::Renderer;

include!(concat!(env!("OUT_DIR"), "/App.rs"));

fn main() {
    // Create component state from script section
    let state = app::script_rs::State::new();
    let state_ref = std::sync::Arc::new(state);
    let make_view = { let state = state_ref.clone(); move |_w: u32, _h: u32| -> (VNode, Stylesheet) {
        let vnode = render_with(|name| if name == "count" { state.count.get().to_string() } else { String::new() });
        let sheet = Stylesheet::parse(app::STYLE);
        (vnode, sheet)
    }};
    let on_event = { let state = state_ref.clone(); move |name: &str| { match name { "inc" => state.inc(), "dec" => state.dec(), _ => {} } } };
    let get_title = { let state = state_ref.clone(); move || state.title.borrow().to_string() };
    velox_renderer::run_window_vnode("Velox App", make_view, on_event, get_title);
}
