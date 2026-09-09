//! Verifies that the velox-style public API (`compute_styles_for_node`) resolves
//! `position`, `overflow`, `transform`, `z-index`, `box-shadow`, and `transition`.

use velox_dom::{Props, h};
use velox_style::{
    BoxShadow, ComputedStyle, Display, Length, Overflow, Position, TransformOp, Transition,
};

fn computed(style: &str, css: Option<&str>) -> ComputedStyle {
    let node = h("div", Props::new().set("style", style), vec![]);
    let sheet = css.map(velox_style::Stylesheet::parse);
    velox_style::compute_styles_for_node(&node, Some(style), sheet.as_ref(), false, &[])
}

#[test]
fn resolves_position_and_overflow() {
    let cs = computed("position: absolute; overflow: auto;", None);
    assert_eq!(cs.position, Position::Absolute);
    assert_eq!(cs.overflow, Overflow::Auto);
    assert!(cs.creates_stacking_context());
}

#[test]
fn resolves_z_index_and_transform() {
    let cs = computed("position: relative; z-index: 5; transform: scale(1.5) rotate(90deg);", None);
    assert_eq!(cs.z_index, Some(5));
    assert_eq!(
        cs.transform.operations,
        vec![TransformOp::Scale(1.5, 1.5), TransformOp::Rotate(90.0)]
    );
}

#[test]
fn resolves_box_shadow_and_transitions() {
    let cs = computed(
        "box-shadow: 0 2px 8px rgba(0,0,0,0.3); transition: transform 0.2s ease;",
        None,
    );
    let shadow: Option<BoxShadow> = cs.box_shadow;
    let bs = shadow.expect("box-shadow parsed");
    assert_eq!(bs.offset_x, Length::Zero);
    assert_eq!(bs.offset_y, Length::Px(2.0));
    assert_eq!(bs.blur_radius, Length::Px(8.0));
    assert_eq!(cs.transitions.len(), 1);
    let tr: &Transition = &cs.transitions[0];
    assert_eq!(tr.property, "transform");
    assert_eq!(tr.duration, 0.2);
}

#[test]
fn display_none_and_fixed() {
    let cs = computed("display: none; position: fixed;", None);
    assert_eq!(cs.display, Display::None);
    assert_eq!(cs.position, Position::Fixed);
    assert!(cs.is_display_none());
}

#[test]
fn stylesheet_rules_resolve_new_properties() {
    let css = ".card { position: relative; overflow: hidden; box-shadow: 2px 2px 4px #000; }";
    let node = h("div", Props::new().set("class", "card"), vec![]);
    let sheet = velox_style::Stylesheet::parse(css);
    let cs = velox_style::compute_styles_for_node(&node, None, Some(&sheet), false, &[]);
    assert_eq!(cs.position, Position::Relative);
    assert_eq!(cs.overflow, Overflow::Hidden);
    assert!(cs.box_shadow.is_some());
}