# Template Syntax

A Velox component lives in a single `.vx` file — a Single-File Component (SFC) with a `<template>`, a `<script setup>`, and a `<style>` block. This page is the complete reference for the template language: structure, attributes, directives, expressions, and slots. Every construct here is compiled to Rust by the `velox-sfc` parser and codegen before your app ever runs.

## Anatomy of a `.vx` file

```html
<template>
  <div class="app">
    <h1 class="title">{{ title }}</h1>
    <p class="count">{{ count }}</p>
    <button class="btn" @click="increment">+1</button>
  </div>
</template>

<script setup>
use velox_core::ergonomics::Ref;

pub struct State {
    counter: Ref<i32>,
}

impl State {
    pub fn new() -> Self {
        Self { counter: velox_core::r#ref!(0) }
    }

    pub fn title(&self) -> String {
        String::from("Velox Counter")
    }

    pub fn count(&self) -> i32 {
        self.counter.get()
    }

    pub fn increment(&self) {
        self.counter.set(self.counter.get() + 1);
    }
}
</script>

<style scoped>
.app { width: 100%; min-height: 100vh; }
.btn { padding: 6px 12px; }
</style>
```

The file holds up to three block types:

| Block | Cardinality | Purpose |
|:---|:---|:---|
| `<template>` | 0 or 1 | The markup. One root element, directives, bindings, interpolations. |
| `<script setup>` | 0 or 1 | Component state and logic as Rust: a `State` struct plus its `impl`. Imports live here. |
| `<script>` | 0 or 1 | Plain Rust script block. Imports here do **not** register components. |
| `<style>` | 0 or 1 | CSS for the component. Declare `scoped` to confine rules to this component. |

A `<template>` block without any `<script>` or `<script setup>` block is reported as incomplete: the component would lack state and event handling.

> Note: the template must have **exactly one root element**. A file whose `<template>` holds two roots fails to compile with `template has multiple root elements` — wrap the markup in a single container element.

### The `State` struct

`<script setup>` is Rust, with two conventions the compiler relies on:

- A `pub struct State` holds the component's reactive fields.
- `pub fn` methods on `State` are what the template calls: interpolations read them, `@event` handlers dispatch to them, and `v-model` generates a setter that writes through them.

Reactive state is a `Ref<T>` (a wrapper around `Rc<Signal<T>>`), created with `r#ref!(initial_value)`. Reads go through `.get()`, writes through `.set(value)`.

```html
<script setup>
use velox_core::ergonomics::Ref;

pub struct State {
    counter: Ref<i32>,   // reactive scalar
    label: Ref<String>,  // v-model target
}
</script>
```

---

## Elements and attributes

The template language is HTML-like: nested elements, self-closing tags (`<input/>`), attributes, text, and `{{ interpolation }}` splits. Nesting is capped at **256 unclosed elements** — deeper markup fails with `template nesting depth 256 exceeded` rather than exhausting memory.

Every attribute falls into one of five kinds, chosen by its prefix:

| Kind | Syntax | Resolves to | Example |
|:---|:---|:---|:---|
| Static | `name="value"` | Literal attribute value | `class="btn"` |
| Bind | `:name="expr"` | Expression evaluated against `State` | `:disabled="!is_valid"` |
| Event | `@event="handler"` | Method name on `State` | `@click="increment"` |
| Directive | `v-name="expr"` | Structural directive | `v-if="count > 0"` |
| Slot shorthand | `#name="expr"` | Named slot fill (see [Slots](#slots)) | `#footer="frag"` |

> Note: attribute values must be quoted — `"double"` or `'single'`. There is no unquoted-value form; a bare token after `=` is skipped, not guessed.

### Bindings

A `:name="expr"` bind evaluates `expr` on every render and writes the result into the element's attributes. Binds work on any attribute — including `:class`, `:style`, and `:key`:

```html
<li v-for="todo in todos" :key="todo.id" :class="todo.completed ? 'done' : ''">
  {{ todo.text }}
</li>
```

Inside a `v-for` body, binds may reference the loop item and its index directly (`:key="todo.id"`, `:key="i"`).

### Interpolation

`{{ expr }}` inserts the value of `expr` as text at that point in the tree:

```html
<p>Clicked {{ count }} times</p>
```

The expression is re-evaluated on every render, so state changes appear on the next frame. An interpolation that is never closed is a **hard error** — the compiler reports `unclosed '{{' — expected a matching '}}'` with a caret pointing at the opener.

---

## Directives

Directives are the `v-`-prefixed attributes that give an element structure. The full set is small and deliberate — five directives, one alias, one slot spelling:

| Directive | Value form | Behavior |
|:---|:---|:---|
| `v-if` | `expr` | Render the element only when `expr` is truthy; otherwise emit an empty text node. |
| `v-else-if` | `expr` | Continuation of a `v-if` chain. Alias: `v-elseif`. |
| `v-else` | *(none)* | Final arm of a `v-if` chain. |
| `v-show` | `expr` | Always renders the element; toggles an inline `display: none` style when `expr` is falsy. |
| `v-for` | `item in items` or `(item, index) in items` | Render the element once per entry in the collection. |
| `v-model` | `field` | Two-way bind on an `<input>`: desugars to `:value="field"` plus an `@input` handler. |
| `v-slot:name` | `expr` | Fill a child component's named slot. Shorthand: `#name`. |

> Note: an element cannot carry both `v-for` and `v-else-if`; that combination is rejected by the compiler.

### `v-if` / `v-else-if` / `v-else`

```html
<p v-if="count > 10">Many</p>
<p v-else-if="count > 0">Some</p>
<p v-else>None</p>
```

The chain is positional: a `v-else-if` or `v-else` attaches to the nearest preceding `v-if` sibling. Whitespace text nodes between chain elements are valid. `v-if` removes the element from the tree entirely when its condition is false — use `v-show` when the element must stay laid out and merely hide.

### `v-show`

```html
<div v-show="is_open" class="panel">…</div>
```

`v-show` always renders the element and toggles CSS `display: none` based on the expression, preserving the element in the tree. If the element already carries a `style` attribute, the display value is merged into it.

### `v-for`

```html
<li v-for="item in items" :key="item.id">{{ item.text }}</li>
<li v-for="(item, index) in items" :key="index">{{ index }}: {{ item.text }}</li>
```

- The value splits on ` in `: the left side names the item variable, optionally destructured as `(item, index)`.
- Without destructuring, the index is still available as `index` when written; the internal default is `__idx`.
- Add a `:key` on loop elements — either `:key="item.id"` or `:key="{{ item.id }}"`; both spellings are normalized to the same binding.
- A `v-model` rooted at the loop item or index is **read-only by design**: the value bind renders, and the missing write is reported as a warning, because a write to a loop item cannot be routed through the `State`-rooted dispatcher.

### `v-model`

```html
<input class="label-input" v-model="label"/>
```

`v-model="field"` desugars into a `:value="field"` bind plus an `@input` handler that calls the generated setter `__vmodel_set_field` — for `v-model="form.name"`, the setter is `__vmodel_set_form_name` and writes `self.form.name`. The target field needs a `VModel` implementation on its type; `Ref<T>` implements it.

---

## Expressions

Directive values, binds, and interpolations share one expression language. It is a small, typed expression grammar compiled to Rust:

| Category | Syntax |
|:---|:---|
| Literals | `true`, `false`, numbers (`42`, `3.14`), strings (`"done"`) |
| Identifiers | `count`, `is_visible`, `form.name` (field access chains) |
| Unary | `!expr`, `-expr` |
| Comparison | `==`, `!=`, `>`, `<`, `>=`, `<=` |
| Logical | `&&`, `\|\|` |
| Arithmetic | `+`, `-`, `*`, `/`, `%` |
| Ternary | `cond ? then : else` |
| Method calls | `obj.method(args)` |
| Field access | `expr.field` |
| Index access | `expr[index]` |
| Grouping | `(expr)` |

```html
<p v-if="count > 0 && !is_hidden">visible with items</p>
<p>{{ items[0].text }}</p>
<p>{{ items.length > 3 ? "many" : "few" }}</p>
```

Comparisons need numeric operands and logical operators need boolean operands; identifiers used in a boolean context are wrapped in truthy checks during compilation.

---

## Components

A tag starting with an uppercase ASCII letter is a **component reference**. Import the component in `<script setup>` and use its PascalCase name as the tag:

```html
<template>
  <div class="app">
    <TodoItem v-for="todo in todos" :key="todo.id" :todo="todo" @remove="remove"/>
  </div>
</template>

<script setup>
import TodoItem from './components/TodoItem.vx';
</script>
```

- Both import forms are supported: `import X from './path.vx'` and `import { A, B } from './path.vx'`.
- Imports are a Velox DSL, not Rust — the compiler rewrites them into module declarations with `#[path]` attributes.
- Attributes on a component tag bind props into the child (`:todo="todo"`); an `@event` attaches a handler the child can emit back to.
- A PascalCase tag that is not an import produces an `unknown component` warning naming the tag with a line/column and the fix: import it in `<script setup>` or fix the tag name. An unregistered component renders as an inert element rather than the intended component.

---

## Slots

A `<slot>` element in a component template is an outlet for content passed from the parent:

```html
<!-- Card.vx -->
<template>
  <div class="card">
    <slot name="header"></slot>
    <slot></slot>
    <slot name="footer">Fallback content</slot>
  </div>
</template>
```

The parent fills slots with a nested `<template v-slot:name>` or its `#name` shorthand:

```html
<Card #header="h">
  <h1>{{ h }}</h1>
</Card>
```

- A `<slot>` with no `name` attribute is the **default** slot.
- Slot names are folded to kebab-case on both sides of the component boundary: `name="footerBar"`, `name="footer_bar"`, and `v-slot:footerBar` all resolve to the same slot. A `<slot name>` on the child and a `v-slot:` on the parent can never disagree on spelling.
- The **fallback is the feature**: a slot's own children render when the caller passed nothing under that name. A mismatch is a naming mistake, not a broken component.
- The `#name="slotProps"` value is parsed but not bound — slot props are not yet wired from the child back into the caller's fragment, so the shorthand's value cannot be resolved and is reported as such rather than silently rendering empty.

---

## Comments

| Context | Syntax | Behavior |
|:---|:---|:---|
| `<style>` block | `/* comment */` | Recognized and stripped before scoping. |
| `<script setup>` | `// line`, `/* block */` | Ordinary Rust comments. |
| `<template>` | `<!-- comment -->` | **Not supported** — see below. |

> Warning: the template parser does not recognize `<!-- -->` comments. It tolerates them without crashing, but treats them as malformed markup that is silently dropped — and everything after the comment inside the enclosing element is dropped with it. Do not use HTML comments in templates; put a CSS comment in the `<style>` block or a Rust comment in `<script setup>` instead.

---

## Syntax summary

| Construct | Syntax | Notes |
|:---|:---|:---|
| SFC blocks | `<template>`, `<script setup>`, `<script>`, `<style scoped>` | One root element required in `<template>` |
| Static attribute | `name="value"` | Quoted values only |
| Bind | `:name="expr"` | Re-evaluated every render |
| Event | `@event="handler"` | Modifiers: `.stop` `.prevent` `.once` `.capture` `.self` |
| Conditional | `v-if`, `v-else-if`, `v-elseif`, `v-else` | Positional chain |
| Visibility | `v-show="expr"` | Toggles inline `display: none` |
| Loop | `v-for="item in items"` / `(item, index) in items` | Pair with `:key` |
| Two-way bind | `v-model="field"` | Desugars to `:value` + `@input` |
| Slot outlet | `<slot name="name">` | Unnamed = default slot; children are fallback |
| Slot fill | `<template v-slot:name>` or `#name` | Kebab-case slot names |
| Component | `PascalCase` tag + `import X from './X.vx'` | Unknown tags warn |
| Interpolation | `{{ expr }}` | Unclosed `{{` is a hard error |
| Nesting limit | 256 unclosed elements | Deeper markup fails with an actionable error |

## See also

- [Events](events.md) — `@event` bindings, payloads, and dispatch.
- [Styling](styling.md) — the `<style>` block, cascade, and supported properties.
- [Dev Workflow & HMR](hmr-dev-workflow.md) — editing `.vx` files live.
