//! `position: fixed` must carry its subtree with it.
//!
//! ## The defect
//!
//! `apply_absolute_position` runs as a TAIL pass, after an out-of-flow child's
//! children have already been laid out relative to where that child sat in flow
//! (`PendingAbsolute` holds the finished `LayoutNode`). It used to overwrite
//! `node.rect.x` / `node.rect.y` and nothing else, so the BOX moved to the
//! viewport and everything UNDER IT stayed at the page's foot.
//!
//! With the scaffolded app's dialogs — `position: fixed; top: 0; left: 0;
//! width: 100%; height: 100%` sitting after a `min-height: 100vh` shell — the
//! overlay landed at `(0, 0)` and its `.panel` at `y = viewport_h + 299`. The
//! dialog opened, its flag flipped, and it rendered one viewport BELOW the fold
//! at every window height, so the click looked like it had done nothing.
//!
//! ## Why a MOVE is the whole of it here
//!
//! A resize (`left: 0; right: 0`, `top`/`bottom`) still leaves children at their
//! old sizes: re-flowing them would mean re-entering `compute_layout`. That is
//! the same approximation `apply_sticky_position` makes at its own call site,
//! and these tests pin the translation, which is what the reported bug needed.

use velox_dom::layout::{LayoutNode, compute_layout};
use velox_dom::{Props, VNode, h, text};

/// Locate `class` in the laid-out tree, paired with its VNode.
fn locate(v: &VNode, l: &LayoutNode, class: &str) -> Option<(VNode, LayoutNode)> {
    if let VNode::Element {
        props, children, ..
    } = v
    {
        if props
            .attrs
            .get("class")
            .is_some_and(|c| c.split_whitespace().any(|x| x == class))
        {
            return Some((v.clone(), l.clone()));
        }
        for c in l.children.iter() {
            if let Some(si) = c.source_index
                && let Some(cv) = children.get(si)
                && let Some(r) = locate(cv, c, class)
            {
                return Some(r);
            }
        }
    }
    None
}

/// The dialog shape, reduced to the two rules that matter — and structured
/// exactly as `App.vx` nests it: a plain BLOCK carrier holding a
/// `min-height: 100vh` page and the fixed overlay as a later sibling.
///
/// `box-sizing: border-box` is stated rather than inherited: `compute_layout`
/// runs on the RAW tree, so it never sees the UA sheet that the cascade would
/// otherwise have applied (velox-dom/src/style.rs:2419).
fn dialog() -> VNode {
    h(
        "div",
        Props::new(),
        vec![
            h(
                "div",
                Props::new()
                    .set("class", "page")
                    .set("style", "min-height:100vh;background:#f6f5f2;"),
                vec![text("page")],
            ),
            h(
                "div",
                Props::new().set("class", "overlay").set(
                    "style",
                    "position:fixed;top:0;left:0;width:100%;height:100%;\
                         box-sizing:border-box;\
                         display:flex;align-items:center;justify-content:center;padding:24px;",
                ),
                vec![h(
                    "div",
                    Props::new()
                        .set("class", "panel")
                        .set("style", "width:400px;height:160px;"),
                    vec![text("panel")],
                )],
            ),
        ],
    )
}

#[test]
fn fixed_overlay_lands_on_the_viewport() {
    let layout = compute_layout(&dialog(), 900, 760);
    let (_, overlay) = locate(&dialog(), &layout, "overlay").expect("overlay");
    assert_eq!(
        (
            overlay.rect.x,
            overlay.rect.y,
            overlay.rect.w,
            overlay.rect.h
        ),
        (0, 0, 900, 760),
        "position:fixed top:0 left:0 100%x100% must resolve against the viewport"
    );
}

#[test]
fn fixed_overlay_panel_is_centred_not_below_the_fold() {
    for h_ in [400, 760, 1000, 1800] {
        let tree = dialog();
        let layout = compute_layout(&tree, 900, h_);
        let (_, panel) = locate(&tree, &layout, "panel").expect("panel");
        assert!(
            panel.rect.y >= 0 && panel.rect.y + panel.rect.h <= h_,
            "at h={h_} the panel landed at y={} h={} — one viewport below the fold, \
             so the dialog is invisible even though it opened",
            panel.rect.y,
            panel.rect.h
        );
        // Centred: `align-items: center` inside a 24px-padded viewport.
        let expect = 24 + (h_ - 48 - panel.rect.h) / 2;
        assert!(
            (panel.rect.y - expect).abs() <= 1,
            "at h={h_} panel y={} is not centred (expected ~{expect})",
            panel.rect.y
        );
    }
}

#[test]
fn fixed_overlay_panel_y_tracks_the_viewport_not_the_page() {
    // THE regression shape. Before the fix the panel's y was
    // `viewport_h + 299`, so it moved DOWN as the window grew. It must move
    // down only as the viewport's own centre does.
    let ys: Vec<i32> = [400, 760, 1000, 1800]
        .iter()
        .map(|h_| {
            let tree = dialog();
            let layout = compute_layout(&tree, 900, *h_);
            let (_, p) = locate(&tree, &layout, "panel").expect("panel");
            p.rect.y
        })
        .collect();
    // Panel is 160 tall in a viewport `h`, padded 24: y = 24 + (h - 208) / 2.
    for (h_, y) in [400, 760, 1000, 1800].iter().zip(&ys) {
        let expect = 24 + (h_ - 208) / 2;
        assert_eq!(
            *y, expect,
            "panel y should be exactly centred at h={h_}, got {y}"
        );
    }
}

#[test]
fn absolute_overlay_panel_travels_with_its_parent() {
    // Same defect, `absolute` flavour: an overlay positioned at `top: 300px`
    // over a tall page must carry its panel down with it.
    let tree = h(
        "div",
        Props::new().set(
            "style",
            "display:flex;flex-direction:column;padding-top:500px;min-height:100vh;",
        ),
        vec![h(
            "div",
            Props::new().set("class", "overlay").set(
                "style",
                "position:absolute;top:300px;left:0;width:100%;height:200px;\
                     display:flex;align-items:center;justify-content:center;",
            ),
            vec![h(
                "div",
                Props::new()
                    .set("class", "panel")
                    .set("style", "width:100px;height:40px;"),
                vec![text("panel")],
            )],
        )],
    );
    let layout = compute_layout(&tree, 800, 600);
    let (_, overlay) = locate(&tree, &layout, "overlay").expect("overlay");
    let (_, panel) = locate(&tree, &layout, "panel").expect("panel");
    assert_eq!(overlay.rect.y, 300, "overlay must sit at top:300");
    let delta = panel.rect.y - overlay.rect.y;
    assert_eq!(
        delta,
        (overlay.rect.h - panel.rect.h) / 2,
        "panel must sit at the overlay's own centre (delta {delta}), not at its \
         static in-flow position"
    );
}

#[test]
fn fixed_overlay_clip_travels_with_the_box() {
    // An `overflow: hidden` fixed box has a clip measured from its pre-move
    // rect. If the box moves and the clip does not, hit-testing and paint both
    // disagree with the box's real geometry.
    //
    // The overlay follows REAL content so its static position is non-zero: a
    // flex container gives an out-of-flow child a static position of its own
    // content-box corner, which would make this test pass vacuously.
    let tree = h(
        "div",
        Props::new().set("style", "padding-top:300px;"),
        vec![
            text("page"),
            h(
                "div",
                Props::new().set("class", "overlay").set(
                    "style",
                    "position:fixed;top:0;left:0;width:200px;height:150px;\
                         overflow:hidden;",
                ),
                vec![text("clipped")],
            ),
        ],
    );
    let layout = compute_layout(&tree, 800, 600);
    let (_, overlay) = locate(&tree, &layout, "overlay").expect("overlay");
    assert_eq!((overlay.rect.x, overlay.rect.y), (0, 0));
    let clip = overlay.clip.expect("overflow:hidden must set a clip");
    assert_eq!(
        (clip.x, clip.y, clip.w, clip.h),
        (0, 0, 200, 150),
        "the box's own clip must move with the box, not stay at its static rect"
    );
}

#[test]
fn in_flow_layout_is_unchanged_by_the_tail_pass() {
    // Nothing but out-of-flow boxes reach `apply_absolute_position`, so a purely
    // in-flow tree must be untouched by it: the two flex items still sit at the
    // positions the in-flow pass gave them, in order.
    let tree = h(
        "div",
        Props::new().set("style", "display:flex;gap:8px;padding:16px;"),
        vec![
            h(
                "div",
                Props::new()
                    .set("class", "a")
                    .set("style", "width:100px;height:40px;"),
                vec![text("a")],
            ),
            h(
                "div",
                Props::new()
                    .set("class", "b")
                    .set("style", "width:100px;height:40px;"),
                vec![text("b")],
            ),
        ],
    );
    let layout = compute_layout(&tree, 800, 600);
    let (_, a) = locate(&tree, &layout, "a").expect("a");
    let (_, b) = locate(&tree, &layout, "b").expect("b");
    assert_eq!((a.rect.x, a.rect.y), (16, 16));
    assert_eq!(
        (b.rect.x, b.rect.y),
        (124, 16),
        "gap of 8 must be preserved"
    );
}
