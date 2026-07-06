//! Test: render VNode with styles matching the template app
//! to verify the rendering pipeline produces visible output.

#[cfg(feature = "skia-native")]
#[test]
fn render_app_template() {
    use velox_dom::{h, text, Props, VNode};
    use velox_style::Stylesheet;

    // Build vnode matching the generated app template structure
    let title = "Velox App".to_string();
    let counter = "0".to_string();
    let positive = "false".to_string();

    let vnode = h(
        "div",
        Props::new().set("class", "app"),
        vec![
            h(
                "header",
                Props::new().set("class", "header"),
                vec![h("h1", Props::new(), vec![text(&title)])],
            ),
            h(
                "div",
                Props::new().set("class", "card"),
                vec![
                    h(
                        "p",
                        Props::new().set("class", "count"),
                        vec![text(&counter)],
                    ),
                    {
                        let is_positive =
                            positive == "true" || (!positive.is_empty() && positive != "false");
                        if is_positive {
                            h(
                                "p",
                                Props::new().set("class", "positive"),
                                vec![text("positive")],
                            )
                        } else {
                            h(
                                "p",
                                Props::new().set("class", "neutral"),
                                vec![text("not positive")],
                            )
                        }
                    },
                    h("button", Props::new().set("class", "btn"), vec![text("+1")]),
                    h("button", Props::new().set("class", "btn"), vec![text("-1")]),
                    h(
                        "button",
                        Props::new().set("class", "btn"),
                        vec![text("Reset")],
                    ),
                ],
            ),
        ],
    );

    // Parse the stylesheet from the template
    let css = r#"
        .app { display: flex; flex-direction: column; width: 100%; height: 100%; background: #1a1a2e; color: #e6edf3; font-family: system-ui, sans-serif; padding: 20px; }
        .header { padding: 20px; text-align: center; }
        .card { background: #16213e; padding: 24px; border-radius: 12px; text-align: center; }
        .count { font-size: 48px; font-weight: bold; margin: 0; }
        .positive { color: #3fb950; margin: 8px 0; }
        .neutral { color: #8b949e; margin: 8px 0; }
        .btn { padding: 10px 20px; font-size: 16px; background: #3478f6; color: white; border: none; border-radius: 6px; cursor: pointer; margin: 4px; }
    "#;
    let sheet = Stylesheet::parse(css);

    // Apply styles (same as apply_styles_with_hover without hover)
    let styled_vnode = velox_style::apply_styles(&vnode, &sheet);

    // Render to PNG using the public API (simple layout)
    let png_simple = match velox_renderer::render_vnode_to_raster_png(
        &styled_vnode,
        &Stylesheet::default(),
        800,
        600,
    ) {
        Ok(b) => b,
        Err(e) => panic!("render_vnode_to_raster_png failed: {}", e),
    };

    std::fs::create_dir_all("target").ok();
    std::fs::write("target/test_app_render_simple.png", &png_simple).expect("failed to write png");
    println!("Simple layout PNG: {} bytes", png_simple.len());
    assert!(
        png_simple.len() > 500,
        "Simple layout PNG too small - likely blank!"
    );

    // Also render using render_frame (same as used by run_window_vnode_skia)
    let png_frame = match velox_renderer::render_vnode_to_raster_png_with_scale(
        &styled_vnode,
        &Stylesheet::default(),
        800,
        600,
        1.0,
    ) {
        Ok(b) => b,
        Err(e) => panic!("render_vnode_to_raster_png_with_scale failed: {}", e),
    };

    std::fs::write("target/test_app_render_frame.png", &png_frame).expect("failed to write png");
    println!("Full layout PNG: {} bytes", png_frame.len());
    assert!(
        png_frame.len() > 500,
        "Full layout PNG too small - likely blank!"
    );
}
