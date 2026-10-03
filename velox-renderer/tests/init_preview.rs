//! Dump a PNG of the init-app tree so we can visually verify navy bg.
#![cfg(feature = "skia-native")]
use velox_dom::{Props, VNode, h};
use velox_style::{Stylesheet, apply_styles};

const STYLE: &str = r#"
.app { display: flex; flex-direction: column; width: 100%; height: 100%; background: #1a1a2e; color: #e6edf3; font-family: system-ui, sans-serif; padding: 20px; }
.header { padding: 20px; text-align: center; }
.title { font-size: 24px; }
.card { background: #16213e; padding: 24px; border-radius: 12px; text-align: center; min-width: 300px; }
.count { font-size: 32px; font-weight: bold; margin-bottom: 20px; }
.positive { color: #3fb950; }
.neutral { color: #8b949e; }
.btn { padding: 10px 20px; background: #3478f6; color: white; border: none; border-radius: 6px; }
"#;

fn build_tree() -> VNode {
    h(
        "div",
        Props::new().set("class", "app"),
        vec![
            h(
                "header",
                Props::new().set("class", "header"),
                vec![h("h1", Props::new().set("class", "title"), vec![])],
            ),
            h(
                "div",
                Props::new().set("class", "card"),
                vec![
                    h("p", Props::new().set("class", "count"), vec![]),
                    h("p", Props::new().set("class", "neutral"), vec![]),
                    h("button", Props::new().set("class", "btn"), vec![]),
                ],
            ),
        ],
    )
}

#[test]
fn dump_init_preview_png() {
    let sheet = Stylesheet::parse(STYLE);
    let styled = apply_styles(&build_tree(), &sheet);
    let png = velox_renderer::render_vnode_to_raster_png(&styled, &sheet, 600, 400).expect("png");
    std::fs::write("/tmp/velox_init_preview.png", png).expect("write");
    println!("wrote /tmp/velox_init_preview.png");
}
