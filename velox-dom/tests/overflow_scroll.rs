//! Scrollable overflow model (CX-07, F-08/F-10/F-12/F-22):
//! scrollHeight separation, is_scrollable, clip behavior, ScrollState
//! wheel clamping, and synthetic scroll-left/scroll-top compat.

use velox_dom::layout::{ScrollState, compute_layout, is_scrollable};
use velox_dom::{Props, h};

fn props(s: &str) -> Props {
    Props::from_inline(s)
}

#[test]
fn overflow_auto_becomes_scrollable_when_content_exceeds() {
    let lo = compute_layout(
        &h(
            "div",
            props("height:100px; overflow:auto"),
            vec![h("div", props("height:300px"), vec![])],
        ),
        800,
        600,
    );
    assert!(lo.scrollable, "expected scrollable when content exceeds");
    assert_eq!(lo.scroll_height, 300);
    assert_eq!(lo.max_scroll_y, 200);
    // scrollable => clip set to the container rect
    assert!(lo.clip.is_some());
    let clip = lo.clip.unwrap();
    assert_eq!((clip.x, clip.y, clip.w, clip.h), (0, 0, 800, 100));
}

#[test]
fn overflow_hidden_not_scrollable_but_clips() {
    let lo = compute_layout(
        &h(
            "div",
            props("height:100px; overflow:hidden"),
            vec![h("div", props("height:300px"), vec![])],
        ),
        800,
        600,
    );
    assert!(!lo.scrollable, "hidden should not be scrollable");
    assert!(lo.clip.is_some(), "hidden should clip");
    assert_eq!(lo.scroll_height, 300);
    // overflow:hidden stays programmatically scrollable in CSS terms:
    // max offset is still scrollHeight - rect height.
    assert_eq!(lo.max_scroll_y, 200);
}

#[test]
fn overflow_visible_not_scrollable_no_clip() {
    let lo = compute_layout(
        &h(
            "div",
            props("height:100px; overflow:visible"),
            vec![h("div", props("height:300px"), vec![])],
        ),
        800,
        600,
    );
    assert!(!lo.scrollable);
    assert!(lo.clip.is_none());
}

#[test]
fn overflow_scroll_scrollable_when_content_exceeds() {
    let lo = compute_layout(
        &h(
            "div",
            props("height:100px; overflow:scroll"),
            vec![h("div", props("height:300px"), vec![])],
        ),
        800,
        600,
    );
    assert!(
        lo.scrollable,
        "overflow:scroll with exceeding content is scrollable"
    );
    assert_eq!(lo.scroll_height, 300);
    assert_eq!(lo.max_scroll_y, 200);
    assert!(lo.clip.is_some());
}

#[test]
fn overflow_auto_not_scrollable_when_content_fits() {
    let lo = compute_layout(
        &h(
            "div",
            props("height:100px; overflow:auto"),
            vec![h("div", props("height:50px"), vec![])],
        ),
        800,
        600,
    );
    assert!(!lo.scrollable, "content fits -> not scrollable");
    assert_eq!(lo.max_scroll_y, 0);
    // Per the specified formula clip = scrollable || hidden; auto with
    // fitting content therefore produces no layout clip.
    assert!(lo.clip.is_none());
}

#[test]
fn scroll_height_includes_padding_and_border() {
    // Raw layout (no UA stylesheet) sizes height content-box: the declared
    // 100px is the content box, so rect_h = 100 + 10 + 10 = 120 and
    // scrollHeight = content_h(300) + pt + pb = 320, max = 320 - 120 = 200.
    let lo = compute_layout(
        &h(
            "div",
            props("height:100px; padding-top:10px; padding-bottom:10px; overflow:auto"),
            vec![h("div", props("height:300px"), vec![])],
        ),
        800,
        600,
    );
    assert!(lo.scrollable);
    assert_eq!(lo.rect.h, 120);
    assert_eq!(lo.scroll_height, 320);
    assert_eq!(lo.max_scroll_y, 200);
}

#[test]
fn synthetic_scroll_top_still_shifts_children() {
    // Deprecated scroll-top keeps its layout-time effect for compat.
    let lo = compute_layout(
        &h(
            "div",
            props("height:100px; overflow:auto; scroll-top:50px"),
            vec![h("div", props("height:300px"), vec![])],
        ),
        800,
        600,
    );
    assert_eq!(lo.scroll_y, 50, "synthetic scroll-top stored raw");
    assert_eq!(lo.children[0].rect.y, -50, "children positioned scrolled");
}

#[test]
fn synthetic_scroll_top_on_visible_keeps_head_behavior() {
    // overflow:visible + scroll-top must keep pre-scroll-model behavior:
    // children shift even though the node is not scrollable (no clamping).
    let lo = compute_layout(
        &h(
            "div",
            props("height:100px; scroll-top:40px"),
            vec![h("div", props("height:20px"), vec![])],
        ),
        800,
        600,
    );
    assert!(!lo.scrollable);
    assert_eq!(lo.scroll_y, 40);
    assert_eq!(lo.children[0].rect.y, -40);
}

#[test]
fn wheel_scroll_clamps() {
    let mut s = ScrollState {
        offset_y: 0.0,
        max_y: 200.0,
    };
    s.scroll_by(300.0);
    assert_eq!(s.offset_y, 200.0);
    s.scroll_by(-500.0);
    assert_eq!(s.offset_y, 0.0);
}

#[test]
fn wheel_scroll_clamps_via_on_wheel() {
    let mut s = ScrollState {
        offset_y: 0.0,
        max_y: 200.0,
    };
    s.on_wheel(300.0);
    assert_eq!(s.offset_y, 200.0);
    s.on_wheel(-500.0);
    assert_eq!(s.offset_y, 0.0);
}

#[test]
fn is_scrollable_helper() {
    assert!(is_scrollable("auto", 300.0, 100.0));
    assert!(is_scrollable("scroll", 300.0, 100.0));
    assert!(!is_scrollable("auto", 50.0, 100.0));
    assert!(!is_scrollable("hidden", 300.0, 100.0));
    assert!(!is_scrollable("visible", 300.0, 100.0));
}
