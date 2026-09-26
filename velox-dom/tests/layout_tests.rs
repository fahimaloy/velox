use velox_dom::{
    Props, h,
    layout::{Rect, compute_layout},
    text,
};

#[test]
fn block_stacks_children_and_uses_style_size() {
    // Use block-level divs instead of text nodes to test vertical stacking
    let root = h(
        "div",
        Props::new().set("style", "width: 300px; height: 100px;"),
        vec![
            h("div", Props::new().set("style", "height: 20px;"), vec![]),
            h("div", Props::new().set("style", "height: 30px;"), vec![]),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(
        lt.rect,
        Rect {
            x: 0,
            y: 0,
            w: 300,
            h: 100
        }
    );
    assert_eq!(lt.children.len(), 2);
    // First child at y=0 (with padding)
    assert_eq!(lt.children[0].rect.y, 0);
    // Second child should be below first (y >= first child's height)
    assert!(
        lt.children[1].rect.y >= lt.children[0].rect.h,
        "Expected children[1].rect.y ({}) >= children[0].rect.h ({})",
        lt.children[1].rect.y,
        lt.children[0].rect.h
    );
}

#[test]
fn block_boundary_collapses_whitespace_only_text() {
    let root = h(
        "div",
        Props::new(),
        vec![
            h("div", Props::new().set("style", "height:20px;"), vec![]),
            text(" "),
            h("div", Props::new().set("style", "height:20px;"), vec![]),
        ],
    );

    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 2);
    assert_eq!(lt.children[0].rect.y, 0);
    assert_eq!(lt.children[1].rect.y, lt.children[0].rect.h);
}

#[test]
fn block_boundary_preserves_whitespace_between_inline_participants() {
    let root = h(
        "div",
        Props::new(),
        vec![
            h("span", Props::new(), vec![text("a")]),
            text(" "),
            h("span", Props::new(), vec![text("b")]),
        ],
    );

    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 3);
    assert_eq!(lt.children[1].source_index, Some(1));
}

/// A whitespace-only text node sitting on a block boundary must not produce
/// a line box, exactly as in HTML/CSS. A `\n`-indented template is the real
/// shape of this bug: the text node collapses to a single space, and the
/// space is dropped because no line box can form on either side of it.
#[test]
fn block_flow_ignores_whitespace_only_text_between_blocks() {
    let root = h(
        "div",
        Props::new(),
        vec![
            h("div", Props::new().set("style", "height: 20px;"), vec![]),
            text(" "),
            h("div", Props::new().set("style", "height: 20px;"), vec![]),
        ],
    );

    let lt = compute_layout(&root, 800, 600);

    assert_eq!(
        lt.children.len(),
        2,
        "whitespace between two blocks must not create a third box, got {:?}",
        lt.children.iter().map(|c| c.rect).collect::<Vec<_>>()
    );
    assert_eq!(lt.children[0].rect.y, 0);
    assert_eq!(
        lt.children[1].rect.y, 20,
        "second block must not be pushed down"
    );
    assert_eq!(
        lt.children.iter().map(|c| c.rect.y + c.rect.h).max(),
        Some(40),
        "the phantom line box must not contribute to the block's content height"
    );
}

/// Negative control for the rule above: a whitespace-only text node between
/// two inline-level siblings is *collapsed*, not removed — browsers render
/// "a b", so a blanket "skip all whitespace-only text" rule would be wrong.
/// The whitespace must survive as a real line box here.
#[test]
fn inline_siblings_keep_their_collapsing_whitespace() {
    let root = h(
        "div",
        Props::new(),
        vec![
            h("span", Props::new(), vec![text("a")]),
            text(" "),
            h("span", Props::new(), vec![text("b")]),
        ],
    );

    let lt = compute_layout(&root, 800, 600);

    assert_eq!(
        lt.children.len(),
        3,
        "whitespace between inline siblings must collapse, not disappear; \
         got {:?}",
        lt.children.iter().map(|c| c.rect).collect::<Vec<_>>()
    );
    assert_eq!(lt.children[1].source_index, Some(1));
    assert!(
        lt.children[1].rect.h > 0,
        "the preserved whitespace must occupy a line box, got height {}",
        lt.children[1].rect.h
    );
}

/// `white-space: pre` makes whitespace non-collapsible, so the block-boundary
/// rule must not touch it even though both neighbours are block-level.
#[test]
fn block_flow_keeps_preserved_whitespace_between_blocks() {
    let root = h(
        "div",
        Props::new().set("style", "white-space: pre;"),
        vec![
            h("div", Props::new().set("style", "height: 20px;"), vec![]),
            text(" "),
            h("div", Props::new().set("style", "height: 20px;"), vec![]),
        ],
    );

    let lt = compute_layout(&root, 800, 600);

    assert_eq!(
        lt.children.len(),
        3,
        "white-space: pre whitespace is significant and must be preserved, got {:?}",
        lt.children.iter().map(|c| c.rect).collect::<Vec<_>>()
    );
    assert_eq!(lt.children[1].source_index, Some(1));
}

/// Layout-level coverage for the elements whose default display is `inline`
/// but which no example template uses, so nothing else in the suite exercises
/// them. No stylesheet is involved: this is the layout engine's own fallback
/// table, asserted through the whitespace rule it drives.
///
/// `img` is the interesting one. It used to sit in the inline-block table
/// while `default_display_for_tag` returned "block" for it, so the framework
/// contradicted itself; it is now in the inline table like every other tag the
/// guard treats as an inline-level participant.
#[test]
fn unstyled_inline_level_tags_keep_their_collapsing_whitespace() {
    for tag in ["span", "img", "output", "progress", "meter", "wbr"] {
        let root = h(
            "div",
            Props::new(),
            vec![
                h(tag, Props::new(), vec![]),
                text(" "),
                h("span", Props::new(), vec![text("x")]),
            ],
        );

        let lt = compute_layout(&root, 800, 600);

        assert_eq!(
            lt.children.len(),
            3,
            "an unstyled <{tag}> is inline-level, so the space beside it must \
             survive; got {:?}",
            lt.children.iter().map(|c| c.rect).collect::<Vec<_>>()
        );
        assert_eq!(lt.children[1].source_index, Some(1));
    }
}

/// The complement of the test above: an element that no browser makes inline
/// must still drop the space, so the previous test cannot pass by classifying
/// everything as inline-level.
#[test]
fn unstyled_block_level_tags_still_collapse_their_surrounding_whitespace() {
    for tag in ["div", "section", "p", "li", "blockquote", "h1"] {
        let root = h(
            "div",
            Props::new(),
            vec![
                h(tag, Props::new(), vec![]),
                text(" "),
                h("div", Props::new().set("style", "height: 20px;"), vec![]),
            ],
        );

        let lt = compute_layout(&root, 800, 600);

        assert_eq!(
            lt.children.len(),
            2,
            "an unstyled <{tag}> is block-level, so the phantom line box must be \
             dropped; got {:?}",
            lt.children.iter().map(|c| c.rect).collect::<Vec<_>>()
        );
    }
}

// ===== Flexbox Tests =====

#[test]
fn flex_row_basic() {
    // Flex row should place children horizontally
    let root = h(
        "div",
        Props::new().set("style", "display: flex; width: 300px; height: 100px;"),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 80px; height: 40px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 100px; height: 50px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.rect.w, 300);
    assert_eq!(lt.rect.h, 100);
    assert_eq!(lt.children.len(), 2);
    // Children should be placed horizontally
    assert_eq!(lt.children[0].rect.x, 0);
    assert!(
        lt.children[1].rect.x > lt.children[0].rect.x,
        "Second child should be to the right of first child"
    );
}

#[test]
fn flex_column_basic() {
    // Flex column should stack children vertically
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; flex-direction: column; width: 200px; height: 300px;",
        ),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 100px; height: 80px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 120px; height: 60px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.rect.w, 200);
    assert_eq!(lt.rect.h, 300);
    assert_eq!(lt.children.len(), 2);
    // Children should be stacked vertically
    assert_eq!(lt.children[0].rect.y, 0);
    assert!(
        lt.children[1].rect.y > lt.children[0].rect.y,
        "Second child should be below first child"
    );
}

#[test]
fn flex_grow_distributes_space() {
    // Two items with flex-grow: 1 should split space equally
    let root = h(
        "div",
        Props::new().set("style", "display: flex; width: 400px; height: 100px;"),
        vec![
            h(
                "div",
                Props::new().set("style", "flex-grow: 1; height: 40px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "flex-grow: 1; height: 40px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 2);
    // Both items should have roughly equal width (200px each)
    let w0 = lt.children[0].rect.w;
    let w1 = lt.children[1].rect.w;
    assert!(
        (w0 - w1).abs() <= 2,
        "Items with equal flex-grow should have similar widths: {} vs {}",
        w0,
        w1
    );
    assert!(
        (190..=210).contains(&w0),
        "Item width should be ~200px, got {}",
        w0
    );
}

#[test]
fn flex_grow_proportional() {
    // flex-grow: 1 and flex-grow: 2 should give 1/3 and 2/3 of space
    let root = h(
        "div",
        Props::new().set("style", "display: flex; width: 300px; height: 100px;"),
        vec![
            h(
                "div",
                Props::new().set("style", "flex-grow: 1; height: 40px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "flex-grow: 2; height: 40px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 2);
    let w0 = lt.children[0].rect.w;
    let w1 = lt.children[1].rect.w;
    // Second item should be roughly twice the width of first
    let ratio = w1 as f32 / w0 as f32;
    assert!(
        ratio > 1.5 && ratio < 2.5,
        "Second item should be ~2x wider: w0={}, w1={}, ratio={}",
        w0,
        w1,
        ratio
    );
}

#[test]
fn flex_gap_spacing() {
    // Flex with gap should space items properly
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; width: 400px; height: 100px; gap: 20px;",
        ),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 100px; height: 40px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 100px; height: 40px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 100px; height: 40px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 3);
    // Items should have gaps between them
    let gap1 = lt.children[1].rect.x - (lt.children[0].rect.x + lt.children[0].rect.w);
    let gap2 = lt.children[2].rect.x - (lt.children[1].rect.x + lt.children[1].rect.w);
    assert!(
        gap1 >= 15,
        "Gap between items should be ~20px, got {}",
        gap1
    );
    assert!(
        gap2 >= 15,
        "Gap between items should be ~20px, got {}",
        gap2
    );
}

#[test]
fn flex_justify_content_center() {
    // justify-content: center should center items
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; justify-content: center; width: 400px; height: 100px;",
        ),
        vec![h(
            "div",
            Props::new().set("style", "width: 100px; height: 40px;"),
            vec![],
        )],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 1);
    // Item should be centered horizontally
    let item = &lt.children[0];
    let offset = item.rect.x;
    assert!(
        offset > 100 && offset < 350,
        "Item should be centered, x={}, expected ~150",
        offset
    );
}

#[test]
fn flex_align_items_center() {
    // align-items: center should center items on cross axis
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; align-items: center; width: 300px; height: 200px;",
        ),
        vec![h(
            "div",
            Props::new().set("style", "width: 80px; height: 60px;"),
            vec![],
        )],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 1);
    let item = &lt.children[0];
    // Item should be vertically centered
    let offset = item.rect.y;
    assert!(
        offset > 50 && offset < 150,
        "Item should be vertically centered, y={}, expected ~70",
        offset
    );
}

#[test]
fn flex_align_self_override() {
    // align-self should override align-items for individual item
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; align-items: flex-start; width: 300px; height: 200px;",
        ),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 80px; height: 60px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 80px; height: 60px; align-self: flex-end;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 2);
    // Second item should be lower than first due to align-self: flex-end
    assert!(
        lt.children[1].rect.y > lt.children[0].rect.y,
        "Item with align-self: flex-end should be lower: y1={}, y2={}",
        lt.children[0].rect.y,
        lt.children[1].rect.y
    );
}

#[test]
fn flex_shrink_on_overflow() {
    // Items should shrink when they exceed container width
    let root = h(
        "div",
        Props::new().set("style", "display: flex; width: 200px; height: 100px;"),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 150px; height: 40px; flex-shrink: 1;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 150px; height: 40px; flex-shrink: 1;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 2);
    // Total width of children should be <= container width
    let total_w = lt.children[0].rect.w + lt.children[1].rect.w;
    assert!(
        total_w <= 200,
        "Total width of shrunk items should fit container: {} > 200",
        total_w
    );
}

// ===== Text Layout Tests =====

#[test]
fn text_uses_font_metrics() {
    // Text dimensions should scale with font-size
    let root = h(
        "div",
        Props::new().set("style", "width: 400px; height: 200px;"),
        vec![velox_dom::VNode::Text("Hello World".to_string())],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 1);
    let text_node = &lt.children[0];
    // "Hello World" is 11 chars, at 16px font, char_width ~ 9.6
    // width should be roughly 11 * 9.6 ≈ 105
    assert!(
        text_node.rect.w > 80 && text_node.rect.w < 150,
        "Text width should be ~105px, got {}",
        text_node.rect.w
    );
    // Height should be ~1.2 * 16 = 19.2
    assert!(
        text_node.rect.h > 15 && text_node.rect.h < 25,
        "Text height should be ~19px, got {}",
        text_node.rect.h
    );
}

#[test]
fn text_larger_font() {
    // Larger font size should produce larger text dimensions
    let root = h(
        "div",
        Props::new().set("style", "width: 400px; height: 200px; font-size: 24px;"),
        vec![velox_dom::VNode::Text("Hello".to_string())],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 1);
    let text_node = &lt.children[0];
    // 5 chars at 24px font, char_width = 14.4, width ≈ 72
    assert!(
        text_node.rect.w > 50 && text_node.rect.w < 100,
        "Text width at 24px should be ~72px, got {}",
        text_node.rect.w
    );
    // Height should be ~1.2 * 24 = 28.8
    assert!(
        text_node.rect.h > 24 && text_node.rect.h < 35,
        "Text height at 24px should be ~29px, got {}",
        text_node.rect.h
    );
}

#[test]
fn text_align_center() {
    // text-align: center should center text lines
    let root = h(
        "div",
        Props::new().set("style", "width: 400px; height: 200px; text-align: center;"),
        vec![velox_dom::VNode::Text("Hello".to_string())],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 1);
    let text_node = &lt.children[0];
    // Text should be roughly centered
    let expected_center = (400 - text_node.rect.w) / 2;
    assert!(
        (text_node.rect.x - expected_center).abs() < 5,
        "Text should be centered, x={}, expected ~{}",
        text_node.rect.x,
        expected_center
    );
}

#[test]
fn text_align_right() {
    // text-align: right should right-align text
    let root = h(
        "div",
        Props::new().set("style", "width: 400px; height: 200px; text-align: right;"),
        vec![velox_dom::VNode::Text("Hello".to_string())],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 1);
    let text_node = &lt.children[0];
    // Text should be at the right edge
    let expected_x = 400 - text_node.rect.w;
    assert!(
        (text_node.rect.x - expected_x).abs() < 5,
        "Text should be right-aligned, x={}, expected ~{}",
        text_node.rect.x,
        expected_x
    );
}

// ===== Text Wrapping Tests =====

#[test]
fn text_wrap_splits_lines() {
    // Long text should wrap into multiple lines
    let root = h(
        "div",
        Props::new().set("style", "width: 200px; height: 300px;"),
        vec![velox_dom::VNode::Text(
            "This is a longer piece of text that should wrap".to_string(),
        )],
    );
    let lt = compute_layout(&root, 800, 600);

    // Text should wrap into at least 2 lines
    assert!(
        lt.children.len() >= 2,
        "Long text should wrap into multiple lines, got {} lines",
        lt.children.len()
    );
}

// ===== Layout Context Tests =====

#[test]
fn layout_uses_full_width_for_root() {
    // Root div should fill viewport width
    let root = h(
        "body",
        Props::new(),
        vec![h("div", Props::new().set("style", "height: 50px;"), vec![])],
    );
    let lt = compute_layout(&root, 1024, 768);

    // Root should fill viewport
    assert_eq!(lt.rect.w, 1024, "Root should fill viewport width");
}

#[test]
fn flex_column_renders_children() {
    let root = h(
        "div",
        Props::new().set(
            "style",
            "display: flex; flex-direction: column; width: 300px; height: 400px;",
        ),
        vec![
            h(
                "div",
                Props::new().set("style", "width: 100px; height: 100px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 100px; height: 100px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "width: 100px; height: 100px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 3);
    // Each child should be below the previous
    for i in 1..lt.children.len() {
        assert!(
            lt.children[i].rect.y > lt.children[i - 1].rect.y,
            "Child {} should be below child {}",
            i,
            i - 1
        );
    }
}

// ===== Margin Collapsing Tests =====

#[test]
fn block_margin_collapse_adjacent_siblings() {
    // Adjacent block siblings: vertical margins should collapse (max of margins)
    let root = h(
        "div",
        Props::new().set("style", "width: 400px; height: 400px;"),
        vec![
            h(
                "div",
                Props::new().set("style", "height: 50px; margin: 10px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "height: 50px; margin: 10px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 2);
    // First child: margin-top = 10px
    assert_eq!(
        lt.children[0].rect.y, 10,
        "First child should have margin-top applied"
    );
    // Second child: collapsed margin = max(10, 10) = 10px
    // Position = first_child_y + first_child_h + collapsed_margin = 10 + 50 + 10 = 70
    assert_eq!(
        lt.children[1].rect.y, 70,
        "Margins should collapse: max(10,10)=10, not 20"
    );
}

#[test]
fn block_margin_collapse_different_margins() {
    // Adjacent siblings with different margins: collapse to max
    let root = h(
        "div",
        Props::new().set("style", "width: 400px; height: 400px;"),
        vec![
            h(
                "div",
                Props::new().set("style", "height: 50px; margin-bottom: 30px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "height: 50px; margin-top: 10px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 2);
    // Collapsed margin = max(30, 10) = 30px
    // Second child y = 0 + 50 + 30 = 80
    assert_eq!(
        lt.children[1].rect.y, 80,
        "Margins should collapse to max(30,10)=30"
    );
}

#[test]
fn block_margin_no_collapse_with_parent_padding() {
    // Parent padding prevents margin collapsing with first child
    let root = h(
        "div",
        Props::new().set("style", "width: 400px; height: 400px; padding: 20px;"),
        vec![
            h(
                "div",
                Props::new().set("style", "height: 50px; margin: 10px;"),
                vec![],
            ),
            h(
                "div",
                Props::new().set("style", "height: 50px; margin: 10px;"),
                vec![],
            ),
        ],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 2);
    // First child: parent padding (20) + child margin-top (10) = 30
    assert_eq!(
        lt.children[0].rect.y, 30,
        "First child: padding + margin-top = 30"
    );
    // Second child: collapsed margin = max(10, 10) = 10
    // Position = 30 + 50 + 10 = 90
    assert_eq!(
        lt.children[1].rect.y, 90,
        "Subsequent children still collapse"
    );
}

// ===== Flex Fit-Content (Indefinite Cross-Size) Tests =====

#[test]
fn flex_row_fit_content_height() {
    // Flex row without explicit height should size to content (fit-content)
    let root = h(
        "div",
        Props::new().set("style", "width: 400px; height: 400px;"),
        vec![h(
            "div",
            Props::new().set("style", "display: flex; gap: 10px;"),
            vec![
                h(
                    "div",
                    Props::new().set("style", "padding: 10px 20px; font-size: 16px;"),
                    vec![velox_dom::VNode::Text("Button 1".to_string())],
                ),
                h(
                    "div",
                    Props::new().set("style", "padding: 10px 20px; font-size: 16px;"),
                    vec![velox_dom::VNode::Text("Button 2".to_string())],
                ),
            ],
        )],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 1);
    let flex_container = &lt.children[0];
    // Flex container should size to content (~50-60px), not fill available height (400px)
    assert!(
        flex_container.rect.h > 40 && flex_container.rect.h < 100,
        "Flex row without height should fit content (~50-60px), got {}",
        flex_container.rect.h
    );
    // Children should have natural height, not stretched to 400px
    for child in &flex_container.children {
        assert!(
            child.rect.h > 40 && child.rect.h < 100,
            "Flex children should have natural height, got {}",
            child.rect.h
        );
    }
}

#[test]
fn flex_row_definite_height_stretches_children() {
    // Flex row WITH explicit height should stretch children (align-items: stretch default)
    let root = h(
        "div",
        Props::new().set("style", "width: 400px; height: 400px;"),
        vec![h(
            "div",
            Props::new().set("style", "display: flex; height: 200px; gap: 10px;"),
            vec![
                h(
                    "div",
                    Props::new().set("style", "padding: 10px 20px; font-size: 16px;"),
                    vec![velox_dom::VNode::Text("Button 1".to_string())],
                ),
                h(
                    "div",
                    Props::new().set("style", "padding: 10px 20px; font-size: 16px;"),
                    vec![velox_dom::VNode::Text("Button 2".to_string())],
                ),
            ],
        )],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 1);
    let flex_container = &lt.children[0];
    // Flex container should have explicit height
    assert_eq!(
        flex_container.rect.h, 200,
        "Flex container should have explicit height"
    );
    // Children should be stretched to fill cross-size (200px - padding)
    for child in &flex_container.children {
        // Child height should be close to container's content height (200 - padding)
        assert!(
            child.rect.h > 150 && child.rect.h <= 200,
            "Flex children should stretch to fill definite cross-size, got {}",
            child.rect.h
        );
    }
}

#[test]
fn flex_row_align_items_flex_start_no_stretch() {
    // Flex row with align-items: flex-start should NOT stretch children even with definite height
    let root = h(
        "div",
        Props::new().set("style", "width: 400px; height: 400px;"),
        vec![h(
            "div",
            Props::new().set(
                "style",
                "display: flex; height: 200px; align-items: flex-start; gap: 10px;",
            ),
            vec![
                h(
                    "div",
                    Props::new().set("style", "padding: 10px 20px; font-size: 16px;"),
                    vec![velox_dom::VNode::Text("Button 1".to_string())],
                ),
                h(
                    "div",
                    Props::new().set("style", "padding: 10px 20px; font-size: 16px;"),
                    vec![velox_dom::VNode::Text("Button 2".to_string())],
                ),
            ],
        )],
    );
    let lt = compute_layout(&root, 800, 600);

    assert_eq!(lt.children.len(), 1);
    let flex_container = &lt.children[0];
    assert_eq!(
        flex_container.rect.h, 200,
        "Flex container should have explicit height"
    );
    // Children should have natural height (not stretched) due to align-items: flex-start
    for child in &flex_container.children {
        assert!(
            child.rect.h > 40 && child.rect.h < 100,
            "Flex children with align-items: flex-start should NOT stretch, got {}",
            child.rect.h
        );
    }
}
