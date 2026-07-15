//! Renders the inited-app tree with styles applied and samples the center
//! pixel to verify the `.app` background actually paints navy (#1a1a2e),
//! not the default brown/clear color (catches the "brown window" bug).

#![cfg(feature = "skia-native")]

use velox_dom::{h, Props, VNode};
use velox_style::{apply_styles, Stylesheet};

const STYLE: &str = r#"
.app { display: flex; flex-direction: column; width: 100%; height: 100%; background: #1a1a2e; color: #e6edf3; font-family: system-ui, sans-serif; padding: 20px; }
.header { padding: 20px; text-align: center; }
.card { background: #16213e; padding: 24px; border-radius: 12px; text-align: center; }
.count { font-size: 48px; font-weight: bold; margin: 0; }
.positive { color: #3fb950; margin: 8px 0; }
.neutral { color: #8b949e; margin: 8px 0; }
.btn { padding: 10px 20px; font-size: 16px; background: #3478f6; color: white; border: none; border-radius: 6px; cursor: pointer; margin: 4px; }
"#;

fn build_tree() -> VNode {
    h(
        "div",
        Props::new().set("class", "app"),
        vec![
            h(
                "header",
                Props::new().set("class", "header"),
                vec![h("h1", Props::new(), vec![])],
            ),
            h(
                "div",
                Props::new().set("class", "card"),
                vec![
                    h("p", Props::new().set("class", "count"), vec![]),
                    h("p", Props::new().set("class", "neutral"), vec![]),
                    h(
                        "button",
                        Props::new()
                            .set("class", "btn")
                            .set("on:click", "increment"),
                        vec![],
                    ),
                ],
            ),
        ],
    )
}

#[test]
fn app_root_background_paints_navy_not_brown() {
    let sheet = Stylesheet::parse(STYLE);
    let styled = apply_styles(&build_tree(), &sheet);

    let w = 200i32;
    let hgt = 150i32;
    let rgba = velox_renderer::render_vnode_to_rgba(&styled, &sheet, w, hgt)
        .expect("render to rgba");

    let cx = (w / 2) as usize;
    let cy = (hgt / 2) as usize;
    let base = (cy * w as usize + cx) * 4;
    let (r, g, b) = (rgba[base], rgba[base + 1], rgba[base + 2]);
    println!("center pixel rgb = ({}, {}, {})", r, g, b);
    // #1a1a2e => (26, 26, 46). Must be blue-dominant (navy), not brown (~(80,40,20)).
    assert!(
        b > r && b > 20,
        "expected navy/blue-dominant background, got rgb=({},{},{})",
        r,
        g,
        b
    );
}
