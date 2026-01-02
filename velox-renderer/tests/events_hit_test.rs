use velox_dom::h;

#[test]
fn hit_test_click_targets() {
    let vnode = h(
        "div",
        vec![("style", "width:120px;height:80px")],
        vec![
            h(
                "button",
                vec![
                    ("on:click", "btn-a"),
                    ("on:click-payload", "payload-a"),
                    ("style", "width:60px;height:30px"),
                ],
                vec![],
            ),
            h(
                "button",
                vec![
                    ("on:click", "btn-b"),
                    ("style", "width:60px;height:30px"),
                ],
                vec![],
            ),
        ],
    );

    let layout = velox_dom::layout::compute_layout(&vnode, 120, 80);
    let mut targets = Vec::new();
    velox_renderer::events::collect_click_targets(&vnode, &layout, &mut targets);

    let hit_first = velox_renderer::events::hit_test_click(&targets, 10.0, 10.0);
    assert_eq!(hit_first, Some(("btn-a", Some("payload-a"))));

    let hit_second = velox_renderer::events::hit_test_click(&targets, 10.0, 45.0);
    assert_eq!(hit_second, Some(("btn-b", None)));

    let hit_none = velox_renderer::events::hit_test_click(&targets, 200.0, 200.0);
    assert_eq!(hit_none, None);
}
