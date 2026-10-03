//! With NO image backend registered, a replaced element has no intrinsic size:
//! a `<img src=…>` is a zero-size box.
//!
//! This lives in its own binary for the reason `tests/common/mod.rs` documents:
//! `set_intrinsic_size_probe` is a process global with no unregister, so a test
//! binary whose sibling tests register a probe can never observe the
//! no-backend fallback. `replaced_img_layout.rs` covers everything else about the
//! replaced rules, including a probe that is registered but cannot answer a
//! given `src`; only the unregistered case needs a process to itself.

use velox_dom::layout::compute_layout;
use velox_dom::{Props, h};

/// Nothing registered, so nothing knows what `logo.png` measures: the box is
/// zero, and -- the half that matters -- it is zero rather than the full width
/// of its container.
#[test]
fn an_img_is_a_zero_size_box_with_no_backend_registered() {
    let tree = h(
        "div",
        Props::new().set("style", "width: 400px;"),
        vec![h("img", Props::new().set("src", "logo.png"), vec![])],
    );

    let laid = compute_layout(&tree, 800, 600);
    let image = &laid.children[0];

    assert_eq!(
        (image.rect.w, image.rect.h),
        (0, 0),
        "no probe and no declared or hinted size leaves nothing to size the box \
         from; got {:?}",
        image.rect
    );
}

/// A declaration is still enough on its own, so the seam is an INPUT and not a
/// prerequisite: an `<img>` laid out without any backend registered takes its
/// declared size exactly as a `<div>` does.
#[test]
fn a_declared_size_needs_no_backend_at_all() {
    let tree = h(
        "div",
        Props::new().set("style", "width: 400px;"),
        vec![h(
            "img",
            Props::new()
                .set("src", "logo.png")
                .set("style", "width: 120px; height: 40px;"),
            vec![],
        )],
    );

    let laid = compute_layout(&tree, 800, 600);

    assert_eq!(
        (laid.children[0].rect.w, laid.children[0].rect.h),
        (120, 40)
    );
}
