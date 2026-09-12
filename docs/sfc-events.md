Event payloads
===============

You can attach an explicit payload to an event using `on:<event>-payload` and handlers will receive it. Alternatively, use inline Rust closures in the script to receive the payload (or the event name if no payload was provided).

Example SFC snippet:

```html
<template>
  <div>
    <!-- explicit payload string -->
    <button @click="inc" on:click-payload="amount:5">Add 5</button>

    <!-- inline closure receives payload (or name) -->
    <button @click="|p| state.handle_payload(p)">Handle Payload</button>
  </div>
</template>

<script setup>
pub struct State;
impl State {
  pub fn handle_payload(&self, payload: &str) {
    // payload may be the explicit `on:click-payload` string, or the event name when absent
    println!("payload={} ", payload);
  }
}
</script>
```

Notes
-----
- The SFC codegen produces a helper `make_on_event(state)` that returns a closure with signature `FnMut(&str, Option<&str>)` (event name, optional payload).
- The renderer forwards an explicit `on:<event>-payload` when present, otherwise it forwards a JSON object containing mouse coordinates for pointer events.

Usage
-----
- Add `on:click-payload` when you want to pass extra data (IDs, quantities) from the template to the handler.
- Use inline closures in `<script setup>` when you want to handle the raw payload string directly.
