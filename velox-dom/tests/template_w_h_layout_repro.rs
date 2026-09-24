//! 1A repro: resize visibly reflows (simulated screenshot diff 400x300 vs 800x600).
//! Must live in velox-dom which has compute_layout.

use std::collections::HashMap;
use velox_dom::layout::compute_layout;
use velox_dom::{Props, VNode};

fn mk_app(style: &str) -> VNode {
    VNode::Element {
        tag: "div".into(),
        props: Props {
            attrs: {
                let mut m = HashMap::new();
                m.insert("style".into(), style.into());
                m.insert("class".into(), "app".into());
                m
            },
        },
        children: vec![VNode::Element {
            tag: "div".into(),
            props: Props {
                attrs: {
                    let mut m = HashMap::new();
                    m.insert("style".into(), "width: 50%; height: 40px;".into());
                    m
                },
            },
            children: vec![VNode::Text("hi".into())],
        }],
    }
}

#[test]
fn layout_reflows_with_viewport_size() {
    let app_style = "width: 100%; min-height: 100vh; display: block;";
    let vnode = mk_app(app_style);
    let small = compute_layout(&vnode, 400, 300);
    let large = compute_layout(&vnode, 800, 600);
    assert!(
        large.rect.w > small.rect.w,
        "layout must reflow: small w {} large w {}",
        small.rect.w,
        large.rect.w
    );
    assert!(
        (390..=410).contains(&small.rect.w),
        "small viewport root width should be ~400, got {}",
        small.rect.w
    );
    assert!(
        (790..=810).contains(&large.rect.w),
        "large viewport root width should be ~800, got {}",
        large.rect.w
    );
    let small_child_w = small.children.first().map(|c| c.rect.w).unwrap_or(0);
    let large_child_w = large.children.first().map(|c| c.rect.w).unwrap_or(0);
    assert!(
        small_child_w > 0 && large_child_w > 0,
        "children must be visible at both sizes"
    );
    assert!(
        large_child_w > small_child_w,
        "child should scale with viewport (50%): small {} large {}",
        small_child_w,
        large_child_w
    );
}
