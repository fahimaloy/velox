---
name: velox-css-scoping
description: Implements CSS scoping for per-component style isolation in Velox
instructions: default
model: sonnet
tools:
  - Bash
  - Read
  - Grep
  - Glob
  - Write
  - Edit
---

You are implementing CSS scoping (style isolation) for the Velox project.

## Task
Implement per-component CSS scoping — each component's styles should only affect its own template, not leak to other components or the global scope.

## Current State
- CSS is defined in `<style>` blocks in `.vx` SFC files
- Currently all styles are likely applied globally
- A TODO in `codegen.rs` marks this as not implemented

## Implementation Approach

### 1. Identify where styles are processed
- `velox-sfc/src/codegen.rs` — SFC to Rust codegen, has CSS scoping TODO
- `velox-style/src/` — CSS processing crate

### 2. Generate scoped class names
For each component, generate a unique scope ID (e.g., `data-v-<hash>`)
- Modify the codegen to wrap element classes with scope-specific selectors
- OR add scope attributes to elements and use CSS attribute selectors

### 3. Transform CSS rules
For each component's stylesheet:
- Prefix all selectors with `[data-v-<scope-id>]`
- Example: `button { color: red; }` → `[data-v-abc123] button { color: red; }`

### 4. Apply to VNode
- The scoped styles should be embedded/inlined into the component's VNode
- Or stored in a component-level stylesheet registry

## Files to Modify
- `velox-sfc/src/codegen.rs` — Add scope ID generation and CSS transform
- `velox-style/src/` — May need CSS selector transformation logic

## Verification
- Build succeeds: `cargo build`
- Component styles don't leak between components
- Run example: `cargo run -p velox-example-todo`

## Review Checklist
- [ ] Scope ID is unique per component
- [ ] CSS selectors properly prefixed
- [ ] No global CSS pollution
- [ ] Build passes
