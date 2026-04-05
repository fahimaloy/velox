use velox_dom::VNode;
use velox_style::Stylesheet;
use velox_renderer::event_binding::EventBinder;

include!(concat!(env!("OUT_DIR"), "/App.rs"));

fn main() {
    println!("🚀 Starting Velox Counter App...");
    
    // Create component state
    let state = app::script_rs::State::new();
    
    // Render the component
    let vnode = render_with(|name| {
        match name {
            "count" => state.count.get().to_string(),
            "title" => state.title.get(),
            _ => String::new(),
        }
    });
    
    // Parse and apply styles
    let sheet = Stylesheet::parse(app::STYLE);
    
    // Set up event bindings
    let mut events = EventBinder::new();
    
    let state_clone = &state;
    events.on_click("btn-increment", {
        let state = state_clone.clone();
        std::rc::Rc::new(std::cell::RefCell::new(move |_| {
            state.increment();
        }))
    });
    
    println!("✅ App rendered successfully!");
    println!("📊 Initial count: {}", state.count.get());
    println!("🎨 Styles loaded: {} rules", sheet.rules.len());
}
