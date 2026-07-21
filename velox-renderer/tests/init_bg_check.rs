#![cfg(feature = "skia-native")]
use velox_dom::{h, Props, VNode};
use velox_style::Stylesheet;
use velox_renderer::render_vnode_to_rgba;

const STYLE: &str = r#".app { display: flex; flex-direction: column; width: 100%; height: 100%; background: #1a1a2e; color: #e6edf3; font-family: system-ui, sans-serif; padding: 20px; }"#;

fn build_tree() -> VNode {
    h("div", Props::new().set("class", "app"), vec![])
}

#[test]
fn init_app_bg_paint_check() {
    let sheet = Stylesheet::parse(STYLE);
    let tree = build_tree();
    let rgba = render_vnode_to_rgba(&tree, &sheet, 800, 600).expect("render");
    let w = 800usize;
    let h = 600usize;
    let px = &rgba[4 * (w/2 + h/2 * w)..][..4];
    let r = px[0] as usize;
    let g = px[1] as usize;
    let b = px[2] as usize;
    assert!(r < 50, "bg should be dark (navy), got r={r}");
}
