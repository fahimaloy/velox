//! Integration tests for Velox
//!
//! Tests the full workflow:
//! 1. Create a project with init
//! 2. Build .vx files
//! 3. Verify styling is applied
//! 4. Test event binding

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use velox_renderer::event_binding::{EventBinder, EventData, EventType};
    use velox_sfc::parse_sfc;
    use velox_style::{Color, ComputedStyle, Display, Length, Stylesheet};

    #[test]
    fn test_complete_sfc_parsing() {
        let sfc_content = r#"
<template>
  <div class="container">
    <h1>{{ title }}</h1>
    <button @click="increment">Click me</button>
  </div>
</template>

<script setup>
pub struct State {
  pub count: std::cell::Cell<i32>,
}
</script>

<style>
.container { display: flex; flex-direction: column; }
.container h1 { font-size: 2rem; }
</style>
"#;

        let sfc = parse_sfc(sfc_content).expect("Failed to parse SFC");

        assert!(sfc.template.is_some());
        assert!(sfc.script_setup.is_some());
        assert!(sfc.style.is_some());

        let template = sfc.template.unwrap();
        assert!(template.content.contains("button"));
        assert!(template.content.contains("@click"));
    }

    #[test]
    fn test_css_property_application() {
        let css = r#"
.button {
  display: flex;
  flex-direction: column;
  padding: 10px;
  margin: 5px;
  background-color: #3478f6;
  color: white;
  font-size: 16px;
  font-weight: bold;
  border-radius: 8px;
  box-shadow: 0 2px 5px rgba(0,0,0,0.2);
}
"#;

        let stylesheet = Stylesheet::parse(css);
        assert!(!stylesheet.rules.is_empty());

        // Verify a rule was parsed
        let rule = &stylesheet.rules[0];
        assert_eq!(rule.selector.parts[0].class, "button");

        // Check that properties were captured
        assert!(rule.decls.contains_key("display"));
        assert!(rule.decls.contains_key("padding"));
        assert!(rule.decls.contains_key("background-color"));
    }

    #[test]
    fn test_event_binding_workflow() {
        let mut binder = EventBinder::new();
        let click_count = Rc::new(RefCell::new(0));
        let count_clone = click_count.clone();

        // Register click handler
        binder.on_click(
            "btn1",
            Rc::new(RefCell::new(move |_: &EventData| {
                *count_clone.borrow_mut() += 1;
            })),
        );

        // Simulate clicks
        let event1 = EventData::new(EventType::Click, "btn1");
        binder.dispatch(&event1);
        assert_eq!(*click_count.borrow(), 1);

        let event2 = EventData::new(EventType::Click, "btn1");
        binder.dispatch(&event2);
        assert_eq!(*click_count.borrow(), 2);

        // Non-matching event shouldn't trigger
        let event3 = EventData::new(EventType::Click, "btn2");
        binder.dispatch(&event3);
        assert_eq!(*click_count.borrow(), 2);
    }

    #[test]
    fn test_input_event_handling() {
        let mut binder = EventBinder::new();
        let last_value = Rc::new(RefCell::new(String::new()));
        let value_clone = last_value.clone();

        binder.on_input(
            "text-input",
            Rc::new(RefCell::new(move |event: &EventData| {
                if let Some(val) = &event.value {
                    *value_clone.borrow_mut() = val.clone();
                }
            })),
        );

        let event = EventData::new(EventType::Input, "text-input").with_value("hello");

        binder.dispatch(&event);
        assert_eq!(*last_value.borrow(), "hello");
    }

    #[test]
    fn test_hover_event() {
        let mut binder = EventBinder::new();
        let is_hovered = Rc::new(RefCell::new(false));
        let hovered_clone = is_hovered.clone();

        binder.on_hover(
            "item",
            Rc::new(RefCell::new(move |_: &EventData| {
                *hovered_clone.borrow_mut() = true;
            })),
        );

        let hover_event = EventData::new(EventType::Hover, "item");
        binder.dispatch(&hover_event);

        assert!(*is_hovered.borrow());
    }

    #[test]
    fn test_multiple_handlers_same_target() {
        let mut binder = EventBinder::new();
        let counter1 = Rc::new(RefCell::new(0));
        let counter2 = Rc::new(RefCell::new(0));

        let c1 = counter1.clone();
        let c2 = counter2.clone();

        binder.on_click(
            "btn",
            Rc::new(RefCell::new(move |_event: &EventData| {
                *c1.borrow_mut() += 1;
            })),
        );

        binder.on_click(
            "btn",
            Rc::new(RefCell::new(move |_event: &EventData| {
                *c2.borrow_mut() += 1;
            })),
        );

        let event = EventData::new(EventType::Click, "btn");
        binder.dispatch(&event);

        assert_eq!(*counter1.borrow(), 1);
        assert_eq!(*counter2.borrow(), 1);
    }

    #[test]
    fn test_computed_style_from_css() {
        let mut style = ComputedStyle::new();

        // Apply inline styles
        style.apply_inline_style("display: flex; padding: 10px; color: #ff0000;");

        assert_eq!(style.display, Display::Flex);
        assert_eq!(style.padding.top, Length::Px(10.0));
        assert_eq!(style.color, Color::parse("#ff0000").unwrap());
    }

    #[test]
    fn test_flexbox_properties() {
        let mut style = ComputedStyle::new();

        style.apply_inline_style(
            "display: flex; flex-direction: column; justify-content: center; gap: 10px;",
        );

        assert_eq!(style.display, Display::Flex);
        assert_eq!(style.gap, Length::Px(10.0));
    }

    #[test]
    fn test_typography_properties() {
        let mut style = ComputedStyle::new();

        style.apply_inline_style(
            "font-size: 18px; font-weight: bold; line-height: 1.5; text-align: center;",
        );

        assert_eq!(style.font_size, Length::Px(18.0));
        assert_eq!(style.font_weight.to_number(), 700);
    }

    #[test]
    fn test_positioning_properties() {
        let mut style = ComputedStyle::new();

        style.apply_inline_style("position: absolute; top: 10px; left: 20px; z-index: 10;");

        assert_eq!(style.position, velox_style::Position::Absolute);
        assert_eq!(style.top, Length::Px(10.0));
        assert_eq!(style.z_index, Some(10));
    }

    #[test]
    fn test_event_data_with_position() {
        let event = EventData::new(EventType::Click, "element").with_position(100.5, 50.3);

        assert_eq!(event.x, Some(100.5));
        assert_eq!(event.y, Some(50.3));
    }

    #[test]
    fn test_event_data_with_key() {
        let event = EventData::new(EventType::KeyPress, "input").with_key(13); // Enter key

        assert_eq!(event.key_code, Some(13));
    }
}
