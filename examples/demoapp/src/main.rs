use velox_core::signal::{Signal, effect};
use velox_dom::{h, text, Props, VNode};

fn view(count: i32) -> VNode {
    h("div", Props::new().set("class", "app"), vec![text(format!("{}", count))])
}

fn main() -> anyhow::Result<()> {
    // Open a window (no drawing yet)
    #[cfg(feature = "wgpu")]
    {
        std::thread::spawn(|| {
            let _ = velox_renderer::run_window("Velox App");
        });
    }

    // Reactive state demo in stdout
    let count = Signal::new(0);
    effect({
        let count_get = || count.get();
        move || {
            let v = view(count_get());
            if let VNode::Element { children, .. } = &v { if let VNode::Text(t) = &children[0] { println!("count={}", t); } }
        }
    });
    for i in 1..=3 { count.set(i); }
    Ok(())
}
