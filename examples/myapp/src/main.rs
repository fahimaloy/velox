use velox_dom::VNode;
use velox_style::{Stylesheet, apply_styles};
use velox_renderer::Renderer;

include!(concat!(env!("OUT_DIR"), "/App.rs"));

fn main() {
    use std::cell::Cell;
    use std::rc::Rc;
    let count = Rc::new(Cell::new(0));
    // view factory uses current count value
    let make_view = { let count = count.clone(); move |w: u32, _h: u32| -> VNode {
        let c = count.clone();
        let vnode = render_with(|name| if name == "count" { c.get().to_string() } else { String::new() });
        let sheet = Stylesheet::parse(app::STYLE);
        apply_styles(&vnode, &sheet)
    }};
    // on_click increments count and triggers re-render through the window loop
    let on_click = { let count = count.clone(); move || { count.set(count.get() + 1); } };
    velox_renderer::run_window_vnode("Velox App", make_view, on_click);
}
