# Events

Velox components respond to user input through declarative event bindings in the template, backed by a typed event system in the renderer.

## Attaching handlers

Attach a handler to any element with the `@<event>` syntax, pointing at a method on your `State`:

```html
<template>
  <div>
    <button @click="increment">Clicked {{ count }} times</button>
  </div>
</template>

<script setup>
let count = signal(0);

fn increment() {
  count.set(count.get() + 1);
}
</script>
```

## Supported events

| Event | Fires when |
|:---|:---|
| `click` | Left-button press on the element (hit-tested against layout geometry) |
| `dblclick` | Two clicks on the same element within 400 ms |
| `input` | A text field's value changes |
| `change` | A text field commits its value (e.g. on submit) |
| `focus` / `blur` | A text field gains or loses focus |
| `keydown` / `keyup` / `keypress` | A key is pressed, released, or pressed-and-released |
| `mousedown` / `mouseup` / `mousemove` | Mouse buttons and movement |
| `mouseenter` / `mouseleave` | The cursor enters or leaves the element |
| `hover` | The cursor moves over a hoverable element |
| `submit` | A form is submitted |

## Event payloads

You can attach an explicit payload to an event using `on:<event>-payload`, and handlers will receive it. Alternatively, use inline Rust closures in the script to receive the payload (or the event name if no payload was provided).

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

Add `on:click-payload` when you want to pass extra data (IDs, quantities) from the template to the handler. Use inline closures in `<script setup>` when you want to handle the raw payload string directly.

## How dispatch works

- The SFC codegen produces a helper `make_on_event(state)` that returns a closure with signature `FnMut(&str, Option<&str>)` — event name, optional payload. Every template handler becomes an arm of that closure; handlers declared with a payload parameter receive it, others are called zero-arg. Handlers owned by a persistent child component state route to `state.{owner}.{method}`.
- At runtime, the renderer hit-tests clicks against the laid-out tree (`collect_click_targets` / `hit_test_click` in `velox-renderer/src/events.rs`). When a hit matches, the handler is invoked through `make_on_event`.
- **Payload forwarding:** an explicit `on:<event>-payload` attribute is forwarded as the payload string when present (`velox-renderer/src/lib.rs:2128-2131`). Otherwise, pointer events receive a JSON object with the mouse coordinates — `{"x":<x>,"y":<y>}`.

## Event modifiers

Velox recognizes the Vue-style modifiers `.stop`, `.prevent`, `.once`, `.capture`, and `.self`, parsed from the handler name (e.g. `@click.stop="submit"`). Modifiers are emitted inline by the codegen alongside the handler call.

## Keyboard input

`@keydown` handlers receive a stable, author-facing **key name** as the payload — a string, not a winit enum — so templates never leak a dependency type. The vocabulary follows the DOM `KeyboardEvent.key` convention: `"Enter"` (not `"Return"`), `"Space"`, `"Escape"`, with upper-case bare letters for shifted and unshifted keys alike (`"a"` and `"A"` share a name). Anything outside the table becomes `"Unidentified"`.

While a text field is focused, editing keys (backspace, delete, arrows, Home/End) edit the value and dispatch `input`; `Enter` dispatches `change`; `Escape` ends the editing session, dropping focus and clearing the selection.
