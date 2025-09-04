use velox_core::signal::{Signal, effect};
use std::rc::Rc;
use velox_dom::{h, text, Props, VNode};

fn view(count: i32) -> VNode {
    h("div", Props::new().set("class", "app"), vec![text(format!("{}", count))])
}

fn main() {
    // Open a window (no drawing yet)
    std::thread::spawn(|| {
        velox_renderer::run_window("Velox App");
    });

    // Reactive state demo in stdout
    let count = Rc::new(Signal::new(0));
    effect({
        let c = count.clone();
        let count_get = move || c.get();
        move || {
            let v = view(count_get());
            if let VNode::Element { children, .. } = &v { if let VNode::Text(t) = &children[0] { println!("count={}", t); } }
        }
    });
    for i in 1..=3 { count.set(i); }
}
