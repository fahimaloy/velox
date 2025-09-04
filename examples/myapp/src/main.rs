use velox_dom::VNode;
use velox_style::{Stylesheet, apply_styles};
use velox_renderer::Renderer;

include!(concat!(env!("OUT_DIR"), "/App.rs"));

fn main() {
    // Render VNode from compiled template
    let vnode = render();
    // Apply styles from the SFC style block
    let sheet = Stylesheet::parse(app::STYLE);
    let styled = apply_styles(&vnode, &sheet);

    // Mount using the selected renderer
    let renderer = velox_renderer::new_selected_renderer();
    let tree = renderer.mount(&styled);
    println!("mounted nodes={}, texts={}", tree.node_count, tree.text_count);
    // Finally, open a window on the main thread (required by winit)
    velox_renderer::run_window("Velox App");
}
