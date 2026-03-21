//! Event binding and handler system for Velox components
//!
//! Maps template events (@click, @input, etc.) to component callbacks
//! Enables reactive component behavior

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

/// Event type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    Click,
    DoubleClick,
    Input,
    Change,
    Focus,
    Blur,
    KeyDown,
    KeyUp,
    KeyPress,
    MouseDown,
    MouseUp,
    MouseMove,
    MouseEnter,
    MouseLeave,
    Hover,
    Submit,
}

impl EventType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "click" => Some(EventType::Click),
            "dblclick" | "double-click" => Some(EventType::DoubleClick),
            "input" => Some(EventType::Input),
            "change" => Some(EventType::Change),
            "focus" => Some(EventType::Focus),
            "blur" => Some(EventType::Blur),
            "keydown" | "key-down" => Some(EventType::KeyDown),
            "keyup" | "key-up" => Some(EventType::KeyUp),
            "keypress" | "key-press" => Some(EventType::KeyPress),
            "mousedown" | "mouse-down" => Some(EventType::MouseDown),
            "mouseup" | "mouse-up" => Some(EventType::MouseUp),
            "mousemove" | "mouse-move" => Some(EventType::MouseMove),
            "mouseenter" | "mouse-enter" => Some(EventType::MouseEnter),
            "mouseleave" | "mouse-leave" => Some(EventType::MouseLeave),
            "hover" => Some(EventType::Hover),
            "submit" => Some(EventType::Submit),
            _ => None,
        }
    }
}

/// Event data passed to handlers
#[derive(Debug, Clone)]
pub struct EventData {
    pub event_type: EventType,
    pub target_id: String,
    pub value: Option<String>,
    pub key_code: Option<u32>,
    pub x: Option<f32>,
    pub y: Option<f32>,
}

impl EventData {
    pub fn new(event_type: EventType, target_id: impl Into<String>) -> Self {
        Self {
            event_type,
            target_id: target_id.into(),
            value: None,
            key_code: None,
            x: None,
            y: None,
        }
    }
    
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }
    
    pub fn with_position(mut self, x: f32, y: f32) -> Self {
        self.x = Some(x);
        self.y = Some(y);
        self
    }
    
    pub fn with_key(mut self, key_code: u32) -> Self {
        self.key_code = Some(key_code);
        self
    }
}

/// Event handler callback type
pub type EventHandler = Rc<RefCell<dyn for<'a> Fn(&'a EventData) + 'static>>;

/// Event binding - maps event to handler
#[derive(Clone)]
pub struct EventBinding {
    pub event_type: EventType,
    pub handler_name: String,
    pub handler: Option<EventHandler>,
}

impl EventBinding {
    pub fn new(event_type: EventType, handler_name: impl Into<String>) -> Self {
        Self {
            event_type,
            handler_name: handler_name.into(),
            handler: None,
        }
    }
    
    pub fn with_handler(mut self, handler: EventHandler) -> Self {
        self.handler = Some(handler);
        self
    }
}

impl std::fmt::Debug for EventBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBinding")
            .field("event_type", &self.event_type)
            .field("handler_name", &self.handler_name)
            .field("has_handler", &self.handler.is_some())
            .finish()
    }
}

/// Event binder - manages event bindings for a component
pub struct EventBinder {
    bindings: HashMap<String, Vec<EventBinding>>,
}

impl EventBinder {
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }
    
    /// Register an event binding
    pub fn on(&mut self, target_id: impl Into<String>, binding: EventBinding) {
        let id = target_id.into();
        self.bindings.entry(id).or_insert_with(Vec::new).push(binding);
    }
    
    /// Register a click handler
    pub fn on_click(&mut self, target_id: impl Into<String>, handler: EventHandler) {
        self.on(target_id, EventBinding::new(EventType::Click, "click").with_handler(handler));
    }
    
    /// Register an input handler
    pub fn on_input(&mut self, target_id: impl Into<String>, handler: EventHandler) {
        self.on(target_id, EventBinding::new(EventType::Input, "input").with_handler(handler));
    }
    
    /// Register a change handler
    pub fn on_change(&mut self, target_id: impl Into<String>, handler: EventHandler) {
        self.on(target_id, EventBinding::new(EventType::Change, "change").with_handler(handler));
    }
    
    /// Register a keypress handler
    pub fn on_keypress(&mut self, target_id: impl Into<String>, handler: EventHandler) {
        self.on(target_id, EventBinding::new(EventType::KeyPress, "keypress").with_handler(handler));
    }
    
    /// Register a hover handler
    pub fn on_hover(&mut self, target_id: impl Into<String>, handler: EventHandler) {
        self.on(target_id, EventBinding::new(EventType::Hover, "hover").with_handler(handler));
    }
    
    /// Get bindings for a target
    pub fn get_bindings(&self, target_id: &str) -> Option<&[EventBinding]> {
        self.bindings.get(target_id).map(|v| v.as_slice())
    }
    
    /// Dispatch an event to all matching handlers
    pub fn dispatch(&self, event: &EventData) -> bool {
        if let Some(bindings) = self.bindings.get(&event.target_id) {
            let mut handled = false;
            for binding in bindings {
                if binding.event_type == event.event_type {
                    if let Some(handler) = &binding.handler {
                        handler.borrow()(event);
                        handled = true;
                    }
                }
            }
            handled
        } else {
            false
        }
    }
    
    /// Get all registered target IDs
    pub fn target_ids(&self) -> Vec<&str> {
        self.bindings.keys().map(|k| k.as_str()).collect()
    }
}

impl Default for EventBinder {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for EventBinder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBinder")
            .field("targets", &self.bindings.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    
    #[test]
    fn test_event_type_parsing() {
        assert_eq!(EventType::from_str("click"), Some(EventType::Click));
        assert_eq!(EventType::from_str("keypress"), Some(EventType::KeyPress));
        assert_eq!(EventType::from_str("unknown"), None);
    }
    
    #[test]
    fn test_event_data() {
        let event = EventData::new(EventType::Click, "btn1")
            .with_position(100.0, 50.0);
        
        assert_eq!(event.event_type, EventType::Click);
        assert_eq!(event.x, Some(100.0));
    }
    
    #[test]
    fn test_event_binder() {
        let mut binder = EventBinder::new();
        let called = Arc::new(Mutex::new(false));
        let called_clone = called.clone();
        
        let handler = Rc::new(RefCell::new(move |_: &EventData| {
            *called_clone.lock().unwrap() = true;
        }));
        
        binder.on_click("btn1", handler);
        
        let event = EventData::new(EventType::Click, "btn1");
        binder.dispatch(&event);
        
        assert!(*called.lock().unwrap());
    }
    
    #[test]
    fn test_event_binder_no_handler() {
        let binder = EventBinder::new();
        let event = EventData::new(EventType::Click, "btn1");
        
        let handled = binder.dispatch(&event);
        assert!(!handled);
    }
}
