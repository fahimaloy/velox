use velox_dom::VNode;

include!(concat!(env!("OUT_DIR"), "/App.rs"));

fn main() {
    std::thread::spawn(|| velox_renderer::run_counter_window());
    let v = render();
    if let VNode::Element { children, .. } = &v {
        if let VNode::Text(t) = &children[0] {
            println!("rendered: {}", t);
        }
    }
}
