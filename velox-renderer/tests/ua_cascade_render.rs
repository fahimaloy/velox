use velox_dom::{Props, h, text};
use velox_renderer::style_vnode_with_hover;
use velox_style::Stylesheet;

fn styled_layout(tree: &velox_dom::VNode, author: &Stylesheet) -> velox_dom::layout::LayoutNode {
    let styled = style_vnode_with_hover(tree, author, &|_, _| false);
    velox_dom::layout::compute_layout(&styled, 400, 300)
}

#[test]
fn production_cascade_applies_ua_default_margins() {
    let tree = h(
        "div",
        Props::new(),
        vec![
            h("h1", Props::new(), vec![text("Title")]),
            h("p", Props::new(), vec![text("Paragraph")]),
        ],
    );
    let layout = styled_layout(&tree, &Stylesheet::default());

    let h1 = &layout.children[0];
    let p = &layout.children[1];
    assert!(
        h1.rect.y > 0,
        "UA h1 top margin should offset its box: {:?}",
        h1.rect
    );
    assert!(
        p.rect.y > h1.rect.y + h1.rect.h,
        "UA sibling margins should create vertical spacing: h1={:?}, p={:?}",
        h1.rect,
        p.rect
    );
}

#[test]
fn author_and_inline_styles_override_ua_in_order() {
    let author = Stylesheet::parse(".title { margin: 3px 0; }");
    let author_tree = h(
        "div",
        Props::new(),
        vec![h(
            "h1",
            Props::new().set("class", "title"),
            vec![text("Title")],
        )],
    );
    let author_layout = styled_layout(&author_tree, &author);
    assert_eq!(author_layout.children[0].rect.y, 3);

    let inline_tree = h(
        "div",
        Props::new(),
        vec![h(
            "h1",
            Props::new()
                .set("class", "title")
                .set("style", "margin: 5px 0;"),
            vec![text("Title")],
        )],
    );
    let inline_layout = styled_layout(&inline_tree, &author);
    assert_eq!(inline_layout.children[0].rect.y, 5);
}
