; Velox Tree-sitter highlight queries for Zed
; These captures map to Zed's built-in highlight names

; Template section
(template
  (tag_name) @tag)

; Script section - delegate to Rust highlights
(script
  (script_content) @rust)

; Style section - delegate to CSS highlights
(style
  (style_content) @css)

; HTML-like elements in template
(element
  (tag_name) @tag)

; Attributes
(attribute
  (attribute_name) @attribute)

; Vue-style directives
(directive
  name: (directive_name) @keyword)

; Event bindings (@click, @input, etc.)
(event_binding
  "@" @keyword
  event: (event_name) @attribute)

; Property bindings (:value, :class, etc.)
(prop_binding
  ":" @keyword
  prop: (prop_name) @attribute)

; Interpolation {{ expr }}
(interpolation
  "{{" @punctuation.special
  "}}" @punctuation.special
  (#set! "priority" 105))
