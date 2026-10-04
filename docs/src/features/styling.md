# Styling

Velox styles elements with CSS. A component declares styles in its `<style>` block, elements can carry inline `style` attributes, and a built-in user-agent stylesheet provides sensible HTML defaults. All three layers run through one cascade on every frame — there is no separate build step for CSS.

## Where styles live

| Layer | Declared in | Precedence |
|:---|:---|:---|
| User-agent | Built in (`velox-style` `ua.css`) | Lowest |
| Author | `<style>` blocks in `.vx` files | Middle |
| Inline | `style` attributes on elements | Highest |

Inline styles win over stylesheet rules, which win over the UA sheet — the same ordering browsers apply.

```html
<template>
  <div class="card" style="margin-top: 12px">…</div>
</template>

<style scoped>
.card {
  width: 100%;
  padding: 16px;
  background-color: #1a1a2e;
}
</style>
```

### Scoped and global styles

Declare the block `<style scoped>` to confine every rule to that component's own elements:

- The component gets a unique `data-v-<hash>` scope id derived from its name.
- Each selector is rewritten with the matching attribute selector — `.header` becomes `.header[data-v-abc1ef23]`.
- The scope id is exposed as `SCOPE_ID` so rendered VNodes are tagged with the same attribute the scoped selectors rely on.

An unscoped `<style>` block passes through unchanged and applies globally. Inside a scoped block, `@keyframes` inner selectors (`from`, `to`, percent offsets) stay unscoped while the outer rules are scoped.

---

## The cascade

`apply_with_cascade` composes the layers **UA < author < inline** and runs the result over the whole tree once per frame:

- Matching is by compound selector against the element and, for multi-part selectors, its ancestors.
- `:hover` matches when the pointer is over the element (the renderer supplies the hover predicate).
- `::placeholder` declarations are written to a separate attribute (`style:placeholder`) so they can never repaint the element's own value.

## Selectors

| Selector | Example | Matches |
|:---|:---|:---|
| Tag | `button` | Elements with that tag |
| Universal | `*` | Any element |
| Class | `.btn` | Elements whose `class` attribute contains the token |
| Compound | `h1.title` | Tag **and** class on the same element |
| Attribute | `[disabled]` / `[type="text"]` | Elements carrying the attribute (optionally with an exact value) |
| Descendant | `.header h1` | An `h1` with any `.header` ancestor |
| Child | `div > .card` | A `.card` exactly one level below a `div` |
| Hover | `button:hover` | The element while the pointer is over it |
| Pseudo-element | `input::placeholder` | The placeholder of an `<input>` that is currently showing it |

> Note: `::placeholder` is the only pseudo-element Velox supports, and it matches only an `<input>` — not a `textarea` — that carries a non-empty `placeholder` attribute and an empty value. A filled field shows the value, not the placeholder, so a `::placeholder` rule never wins there.

---

## Supported properties

The table below lists every property the style engine parses and which rendering reader honors it. The live path has three readers — the flexbox/table layout pass, the box painter, and the text painter — and a property is honest only if at least one of them looks it up. Properties marked *parsed only* survive the cascade but are then dropped; `velox lint` reports them.

### Layout and box model

| Property | Values | Rendered by |
|:---|:---|:---|
| `display` | `block`, `inline`, `inline-block`, `flex`, `grid`, `none` (`hidden` alias) | Layout + box painter |
| `position` | `static`, `relative`, `absolute`, `fixed`, `sticky` | Layout + box painter |
| `z-index` | Integer | Stacking context |
| `opacity` | `0.0`–`1.0` (clamped) | Layout + box painter |
| `width` / `height` | Length | Layout |
| `min-width` / `min-height` | Length | Layout |
| `max-width` / `max-height` | Length | Layout |
| `margin` | Length shorthand (1–4 values) + `margin-top`/`-right`/`-bottom`/`-left` | Layout |
| `padding` | Length shorthand (1–4 values) + `padding-top`/`-right`/`-bottom`/`-left` | Layout + box painter |
| `top` / `right` / `bottom` / `left` | Length | Layout |
| `box-sizing` | `content-box`, `border-box` | Layout (UA default is `border-box`) |

### Flexbox

| Property | Values | Rendered by |
|:---|:---|:---|
| `flex-direction` | `row`, `row-reverse`, `column`, `column-reverse` | Layout |
| `flex-wrap` | `nowrap`, `wrap`, `wrap-reverse` | Layout |
| `justify-content` | `flex-start` (`start`), `flex-end` (`end`), `center`, `space-between`, `space-around`, `space-evenly` | Layout |
| `align-items` | `flex-start` (`start`), `flex-end` (`end`), `center`, `baseline`, `stretch` | Layout |
| `align-self` | `auto` + the `align-items` values | Layout |
| `flex-grow` / `flex-shrink` | Number | Layout |
| `flex-basis` | Length | Layout |
| `gap` (`grid-gap`) | Length (sets both axes) | Layout |
| `row-gap` / `column-gap` | Length | Layout |

### Backgrounds and borders

| Property | Values | Rendered by |
|:---|:---|:---|
| `background-color` | Color | Box painter |
| `background` | Color-only shorthand — the first color token is used | Box painter |
| `border` | `<width> <style> <color>` shorthand | Box painter |
| `border-width` | Length shorthand (1–4 values) | Box painter |
| `border-radius` | Length shorthand (1–4 values) | Box painter |
| `border-style` | Border style values | *Parsed only* |
| `border-color` | Color values | *Parsed only* |
| `background-image` | Value string | *Parsed only* |

### Typography

| Property | Values | Rendered by |
|:---|:---|:---|
| `color` | Color | Text painter (inherits) |
| `font-size` | Length | Text painter (inherits) |
| `font-family` | Family string (e.g. `system-ui`) | Text painter (inherits) |
| `font-weight` | `normal`, `bold`, `bolder`, `lighter`, or `100`–`900` | Text painter (inherits) |
| `line-height` | Number (multiplier), px length | Text painter (inherits) |
| `text-align` | `left`, `center`, `right`, `justify` | Text painter (inherits) |
| `vertical-align` | `baseline`, `top`, `bottom`, `middle` | Layout (inherits) |
| `text-decoration` | `none`, `underline`, `overline`, `line-through` | Text painter (inherits) |
| `font-style` | `normal`, `italic`, `oblique` | *Parsed only* |
| `letter-spacing` | Length | *Parsed only* |

### Overflow and misc

| Property | Values | Rendered by |
|:---|:---|:---|
| `overflow` | `visible`, `hidden`, `scroll`, `auto` | Layout + box painter |
| `overflow-x` | `visible`, `hidden`, `scroll`, `auto` | *Parsed only* — use the `overflow` shorthand |
| `overflow-y` | `visible`, `hidden`, `scroll`, `auto` | *Parsed only* — use the `overflow` shorthand |
| `white-space` | `normal`, `nowrap`, `pre`, `pre-wrap`, `pre-line` | Text painter (inherits) |
| `text-overflow` | `clip`, `ellipsis` | Text painter |
| `visibility` | `visible`, `hidden`, `collapse` | *Parsed only* — `visibility: hidden` hides nothing; use `display: none` or `v-show` |
| `transform` | Transform functions (`translateX`, `scaleX`, …) | *Parsed only* — forces a stacking context, no visual effect |
| `box-shadow` | `2px 2px 4px rgba(0,0,0,0.5)` form | *Parsed only* |
| `transition` | Comma-separated `<property> <duration> <timing>` | *Parsed only* — no reader plays a transition |

> Tip: `velox lint` reports exactly the *parsed only* properties above, by file and rule index. Unknown declarations are filtered out silently by the cascade, so lint deliberately does not flag them — vendor prefixes and custom properties stay quiet.

> Tip: to hide an element use `display: none` (or `v-show` in templates), not `visibility: hidden`. To round corners use `border-radius`, and give depth with `border` — not `box-shadow`.

---

## Units

All length values share one parser:

| Unit | Example | Resolves against |
|:---|:---|:---|
| `px` | `16px` | Physical pixels |
| `%` | `100%` | Parent size |
| `rem` | `1.5rem` | Root font size |
| `em` | `1.25em` | Parent font size |
| `vw` | `50vw` | Viewport width |
| `vh` | `100vh` | Viewport height |
| `dvw` / `dvh` | `100dvh` | Viewport width/height (treated as `vw`/`vh`) |
| `auto` | `auto` | Automatic (layout-dependent) |
| *(none)* | `0`, `16` | Unitless zero, plain numbers as px |

A responsive page fills the viewport with a root of `width: 100%; min-height: 100vh` — the canonical pattern in every scaffold and example, and one the layout engine reflows visibly on every window resize.

## Colors

| Form | Examples |
|:---|:---|
| Named | `black`, `white`, `red`, `green`, `blue`, `yellow`, `cyan`, `magenta`, `silver`, `gray`/`grey`, `maroon`, `olive`, `lime`, `aqua`, `teal`, `navy`, `fuchsia`, `purple`, `orange`, `pink`, `transparent` |
| Hex | `#abc`, `#aabbcc`, `#aabbccdd` (3, 6, or 8 digits) |
| Functional | `rgb(255, 0, 0)`, `rgba(255, 0, 0, 0.5)` |

## Inheritance

Inherited properties flow from parent to child per browser CSS behavior:

`color`, `font-size`, `font-family`, `font-weight`, `font-style`, `line-height`, `letter-spacing`, `text-align`, `vertical-align`, `white-space`, `visibility`, `cursor`, `text-decoration`

Everything else is non-inherited: each element reads its own declaration, so a descendant of a clipped box is not clipped unless it says so. `::placeholder` declarations never inherit — not even from their own element.

## Fonts

Font matching resolves against system fonts and custom font files. `font-weight` accepts the CSS keywords and the full 100–900 numeric scale (`Thin` 100 through `Black` 900, default `Normal` 400); `font-style` accepts `normal`, `italic`, and `oblique`; `line-height` accepts `normal` (browser default 1.2×), a number multiplier, or a px length.

## See also

- [Template Syntax](template-syntax.md) — the `<style scoped>` attribute and directive reference.
- [Renderer](renderer.md) — how the cascaded tree is laid out and painted.
- [Dev Workflow & HMR](hmr-dev-workflow.md) — editing styles live.
