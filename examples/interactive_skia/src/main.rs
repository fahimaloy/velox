use std::cell::RefCell;
use std::rc::Rc;

use velox_dom::{h, text, VNode};
use velox_style::Stylesheet;

fn main() {
    let count = Rc::new(RefCell::new(0i32));
    let count_for_view = Rc::clone(&count);
    let count_for_title = Rc::clone(&count);
    let count_for_event = Rc::clone(&count);

    let css = r#"
.btn { background-color: #F4C95D; color: #111111; border: 2px solid #111111; border-radius: 8px; padding: 8px 12px; }
.btn:hover { background-color: #FFE7A8; }
"#;
    let sheet = Stylesheet::parse(css);

    velox_renderer::run_window_vnode_skia(
        "Velox Skia Interactive",
        move |w, height| make_view(&count_for_view, w, height, &sheet),
        move |event, _payload| {
            if event == "increment" {
                *count_for_event.borrow_mut() += 1;
            }
        },
        move || format!("Velox Skia Interactive — count {}", *count_for_title.borrow()),
    );
}

fn make_view(count: &Rc<RefCell<i32>>, w: u32, height: u32, sheet: &Stylesheet) -> (VNode, Stylesheet) {
    let header = h(
        "div",
        vec![("style", "font-size:18px;color:#222222;margin-bottom:10px")],
        vec![text("Interactive Skia")],
    );

    let button = h(
        "button",
        vec![
            ("class", "btn"),
            ("on:click", "increment"),
            ("style", "margin-top:12px;width:160px;height:44px"),
        ],
        vec![text(format!("Clicks: {}", *count.borrow()))],
    );

    let body = h(
        "div",
        vec![("style", "background-color:#F6F4EF;padding:20px")],
        vec![header, button],
    );

    let root_style = format!("width:{}px;height:{}px", w, height);
    let root = h("div", vec![("style", root_style.as_str())], vec![body]);

    (root, sheet.clone())
}
