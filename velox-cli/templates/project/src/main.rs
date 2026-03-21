use velox_core::*;
use velox_dom::*;
use velox_renderer::*;
use velox_style::*;

mod App;

fn main() {
    println!("🚀 Starting {}...", env!("CARGO_PKG_NAME"));
    
    // Create initial state
    let app = create_signal(0i32);
    
    // Render the app
    velox_renderer::run(move || {
        App::render()
    });
}
