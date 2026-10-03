# velox-sfc — review fixes

Branch `feat/premium-boilerplate-core-components`. Scope of this pass: every
finding in the CRITICAL / IMPORTANT 1-3 / MINOR list, in `velox-sfc/**` only.
Nothing outside the crate was touched; nothing was staged or committed.

**Verification command for every result below:**

```
cargo test -j 3 -p velox-sfc
```

Final state: **36 test targets (35 binaries + doc-tests), 463 passed, 0 failed**
(5 pre-existing `#[ignore]`s). `cargo build -j 3 -p velox-sfc` is clean, no warnings.

---

## CRITICAL — invalid Rust emitted for a legal template

**fixed.**

`emit_node_with_ctx_state` emitted `emit_dispatch_registrations` unconditionally,
so every child component tag inside a `v-for` loop body got a handler
registration. That registration closes over `state`
(`std::sync::Arc::downgrade(&state)`), but in Props mode `render_with_props`
binds `state` as a plain `script_rs::State` — or binds nothing at all when the
component has no script — so the generated crate failed with E0308 or E0425.

The mode is now threaded through the function and the registration is gated on
it, exactly as the sibling `emit_node_with_mode` already did.

| Change | file:line |
| --- | --- |
| `emit_node_with_ctx_state` gains a `mode: TransformMode` parameter | `velox-sfc/src/template_codegen.rs:3044` |
| the loop-body `dispatch` is gated on `mode == TransformMode::State`, with a comment saying why (registration closes over `state`; only State binds `state` as `Arc<State>`; Props yields E0425 or E0308; Resolve has no `State`) | `velox-sfc/src/template_codegen.rs:3233` |
| external caller in `emit_node_with_mode` passes its local `mode` | `velox-sfc/src/template_codegen.rs:2285` |
| external caller in `emit_children_with_mode` passes its local `mode` | `velox-sfc/src/template_codegen.rs:2957` |
| internal recursions pass `mode` through: slot fallback / `v-show` / `v-if` / `slots_expr` / plain-element children | `:3105`, `:3135`, `:3191`, `:3245`, `:3301` |

New tests — `velox-sfc/tests/props_loop_dispatch_compiles.rs`. These **build** the
generated crate rather than grepping the generated string, because E0308/E0425
is a compile-time class of bug and a string assertion cannot see it.

| Test | line |
| --- | --- |
| `a_bound_child_tag_in_a_props_loop_generates_code_that_compiles` — `Props { items: Vec<String> }` + `v-for="item in items"` + `<RowChild :label="item" @confirm="on_confirm" />` | `:337` |
| `a_fn_prop_child_tag_in_a_props_loop_generates_code_that_compiles` — same shape with `:on_confirm="on_confirm"` (the function-prop spelling) | `:352` |

**Falsification M1** — reverted the gate to the unconditional call. Both tests
went RED:

```
test a_bound_child_tag_in_a_props_loop_generates_code_that_compiles ... FAILED
test a_fn_prop_child_tag_in_a_props_loop_generates_code_that_compiles ... FAILED
error[E0308]: mismatched types
  ... std::sync::Arc::downgrade(&state) ...
  expected `&Arc<_, _>`, found `&State`
```

Mutation reverted; both tests green again.

---

## IMPORTANT 1 — the predicate accepted signatures the builder cannot build

**fixed, by narrowing + a deliberate diagnostic** (not by widening the builder).

`is_function_prop_type` advertised `Fn()`, `FnMut(String)`, `Fn(A, B)`;
`function_prop_value` emits exactly one shape — a `move |__vx_payload: &str|`
closure — wrapped per the declared type. A parent binding a method name to a prop
declared `Option<Box<dyn Fn(String) -> bool>>` therefore produced an opaque type
error inside generated code the author never wrote.

The generator has no value for a `Fn(String) -> bool` and cannot invent one, so
the honest fix is the same one the call-binding case already uses
(`template_codegen.rs:2457`): say so, by name, in the author's own prop type.

| Change | file:line |
| --- | --- |
| new shared `PROP_TYPE_WRAPPERS` table, read by BOTH the recogniser and the builder (longest marker wins, fully-qualified before bare) — one source of truth for how a declared type is peeled and rebuilt | `velox-sfc/src/template_codegen.rs:3501` |
| new `fn_prop_core(ty) -> Option<&str>` — peels wrappers + `dyn`, `None` when nothing peeled | `:3516` |
| `is_function_prop_type` = `fn_prop_core(ty).is_some_and(|c| c.starts_with("Fn"))` (still broad RECOGNITION) | `:3545` |
| new `is_buildable_function_prop_type(ty)` = exactly `Fn(&str)` / `FnMut(&str)` / `FnOnce(&str)` — what the builder can actually emit | `:3568` |
| new match arm: a recognised-but-unbuildable fn prop bound by method name emits a `compile_error!` naming the prop, quoting the declared type, and naming the signature that works | `:2452`, message at `:2457` |
| `wrap_by_declared_type` now reads `PROP_TYPE_WRAPPERS` instead of its own local list | `:3682` |

New tests in `velox-sfc/tests/optional_and_fn_props.rs`:

| Test | line |
| --- | --- |
| `a_callback_the_builder_cannot_build_is_refused_by_name` — asserts the message verbatim: `velox: <UnbuildableChild> declares prop \`on_confirm\` as \`Option<Box<dyn Fn(String) -> bool>>\`` plus `Fn(&str)`, `dispatches by name`, `f(payload)` | `:531` |
| `a_fn_prop_binding_by_method_name_compiles` — the buildable spelling still works | `:442` |
| `a_refcell_fn_mut_prop_binding_compiles` — `RefCell<Box<dyn FnMut(&str)>>` is recognised AND rebuilt | `:484` |
| `an_unbuildable_callback_left_unbound_still_compiles` — the refusal is about *binding by method name*, not about the type | `:595` |

**Falsification M13** — disabled the new arm. RED, and the failure mode is the
exact symptom the fix removes:

```
test a_callback_the_builder_cannot_build_is_refused_by_name ... FAILED
error[E0631]: type mismatch in closure arguments
```

**Falsification M14** — removed the bare `RefCell<` entry from
`PROP_TYPE_WRAPPERS`. RED: `a_refcell_fn_mut_prop_binding_compiles ... FAILED`
with 2× `error[E0308]: mismatched types`. Reverted.

---

## IMPORTANT 2 — ~150 lines of new codegen had no test

**fixed.** `slot_binding`, `strip_slot_binding`, `slot_content` and
`slots_map_expr` are now covered from both directions, because neither alone is
sufficient.

New integration tests — `velox-sfc/tests/slots_codegen.rs` (11). `slots_map_for`
extracts the `__slots = HashMap::from([…])` literal by **bracket matching**; a
naive "up to the next `;`" stops inside the block.

| Requirement from the finding | Test | line |
| --- | --- | --- |
| grouping by slot name | `several_children_naming_one_slot_become_one_entry` | `:95` |
| grouping by slot name | `two_different_slot_names_are_two_entries` | `:145` |
| the unnamed path still yields key `"default"` | `a_child_naming_no_slot_yields_the_default_key` | `:127` |
| `<template v-slot:x>` flattening to its children | `a_template_v_slot_flattens_to_its_children` | `:174` |
| the `slot:` attribute being stripped, content's own attributes kept | `the_contents_own_attributes_survive_the_slot_pass` | `:217` |

Plus: `a_camel_case_slot_name_is_stored_kebab_cased` (`:250`),
`the_hash_shorthand_and_v_slot_reach_the_same_entry` (`:275`),
`a_camel_case_slot_name_meets_the_camel_case_name_it_was_given` (`:302`),
`a_kebab_case_slot_name_is_left_alone` (`:323`),
`an_outlet_naming_nothing_looks_up_default` (`:336`), and a **running** test
`a_callers_slotted_content_reaches_the_outlet_it_named` (`:481`).

New unit tests appended to `velox-sfc/src/codegen_unit_tests.rs`:

| Test | line |
| --- | --- |
| `a_template_fragment_contributes_its_children_not_itself` | `:306` |
| `a_plain_element_is_its_own_content_and_keeps_its_own_attributes` | `:329` |
| `the_slot_binding_is_stripped_and_nothing_else_is` | `:351` |
| `the_slot_name_is_read_off_the_directive_unchanged` | `:378` |

`slot_binding` / `strip_slot_binding` / `slot_content` are now `pub(crate)` for
these tests. Why the strip assertion lives in unit tests and not in the
integration test: `emit_props_in_loop` emits only `AttrKind::Static` and
`AttrKind::Bind` and drops `AttrKind::Directive` anyway, so the strip is
currently a **latent** guarantee — the generated output is identical whether or
not `strip_slot_binding` does its job. Proven, not assumed: mutation **M7**
(no-strip) left the integration test PASSING; mutation **M7b** (same mutation,
run against the unit tests) turned `the_slot_binding_is_stripped_and_nothing_else_is`
RED. The doc comment on `strip_slot_binding` now says the strip is latent rather
than claiming a guarantee the current call path does not provide.

Supporting changes: `slot_binding` `velox-sfc/src/template_codegen.rs:3852`,
`strip_slot_binding` `:3887`, `slot_content` `:3920`, `slots_map_expr` `:3963`.

**Falsification M5** (remove grouping) → `several_children_naming_one_slot_become_one_entry`
+ `the_hash_shorthand_and_v_slot_reach_the_same_entry` RED.
**M6 / M6b** (no flattening in `slot_content`) → `a_template_v_slot_flattens_to_its_children`
RED; against the unit tests, `a_template_fragment_contributes_its_children_not_itself`
and `the_slot_binding_is_stripped_and_nothing_else_is` RED.
**M8** (`slot:` prefix → `slotX:`) → 3 unit tests RED.
**M9** (`strip_slot_binding` drops ALL attributes) → `the_contents_own_attributes_survive_the_slot_pass` RED.
**M10** (default key renamed `"DEFAULT_SLOT"`) → `a_child_naming_no_slot_yields_the_default_key` RED.
**M11** (broken kebab fold, `{name}-x`) → 3 slot tests RED. All reverted.

---

## IMPORTANT 3 — slot-name normalisation was asymmetric and silent

**fixed.** The parent key was normalised; the child looked up the RAW static
attribute, so `<template v-slot:footerBar>` + `<slot name="footerBar">` stored
`"footer-bar"` and looked up `"footerBar"` → `render_slot` returned the fallback
with no diagnostic.

| Change | file:line |
| --- | --- |
| new `normalize_slot_name` = `template_parse::normalize_directive_name(name.trim())`, so the child folds exactly as the parent does | `velox-sfc/src/template_codegen.rs:1561` |
| new `slot_outlet_name(attrs)` — static `name` attr → normalised → else `"default"` | `:1567` |
| `emit_slot_node` uses it (was reading the raw attribute) | `:1595` |
| the in-loop-body slot site uses it (was reading the raw attribute) | `:3100` |
| `normalize_directive_name` is now `pub(crate)`, with a doc comment naming `template_codegen::normalize_slot_name` as the child-side caller and stating that drift is invisible | `velox-sfc/src/template_parse.rs:471` |

Pinned by `a_camel_case_slot_name_is_stored_kebab_cased`,
`the_hash_shorthand_and_v_slot_reach_the_same_entry`,
`a_camel_case_slot_name_meets_the_camel_case_name_it_was_given`,
`a_kebab_case_slot_name_is_left_alone`, and the running test.

**Falsification M4** — dropped `normalize_slot_name` from `slot_outlet_name`.
RED on both the string test and the running test, and the running test's stdout
reproduced the exact silent-fallback symptom:

```
test a_camel_case_slot_name_meets_the_camel_case_name_it_was_given ... FAILED
test a_callers_slotted_content_reaches_the_outlet_it_named ... FAILED
TEXT:|FALLBACK_HEADER||FALLBACK_FOOTER|
```

**Falsification M3** — see "Not in the review" below; it is what surfaced the
`emit_slot_node` compile bug.

**Deliberate non-action:** an unmatched slot name is still a silent fallback, and
`normalize_slot_name`'s doc comment says why that is correct rather than an
oversight — the fallback *is* the feature (a `<slot>` with no content supplied
renders its own children), and the map is built by the caller in a different
file, so an empty lookup result cannot distinguish "nobody passed this" from
"passed under a different name". Making it a diagnostic would need the child's
declared slot names checked against the caller's map at one join point, which
does not exist. Recorded here as a known limitation, not fixed.

---

## MINOR items

### 1. Tautological assertion — **fixed**

`velox-sfc/tests/emit_system_tests.rs` asserted
`!a || !b`, where the first disjunct was always FALSE and the second always TRUE.

Replaced with whitespace-stripped assertions that cannot pass by construction
and do not depend on the wrapper's indentation (`velox-sfc/tests/emit_system_tests.rs:346`):

```rust
let compact: String = rs.chars().filter(|c| !c.is_whitespace()).collect();
assert!(compact.contains("fnrender_with_slots_only(props:PropsArg,slots:"));   // :348
assert!(!compact.contains("fnrender_with_slots_only(props:PropsArg,callbacks")); // :354
```

**Falsification M2** — added a `callbacks: &[&str]` parameter to
`render_with_slots_only`. RED:
`the_two_slots_entry_points_do_not_share_a_name ... FAILED`.

### 2. Comment naming a call site that does not exist — **fixed**

`sanitize_ident` was `pub(crate)` because `template_codegen::function_prop_value`
was said to name generated locals with it. There are exactly two references in
the crate — the definition and one call site, both in `codegen.rs` — and
`template_codegen.rs` has zero references to `sanitize_ident` and zero to
`codegen::`.

`velox-sfc/src/codegen.rs:713-725`: reverted to private, and the doc comment now
states the stated consumer does not exist, that a justification naming an absent
consumer is worse than none because the next reader trusts it, and that it
should become `pub(crate)` again **with a real caller named**.

### 3. A test that races itself — **fixed**

`velox-sfc/tests/keydown_focus_e2e.rs`: both `#[test]`s called `scaffold()`,
which wrote to the stable path `target/keydown-focus-e2e`, and each then ran
`cargo run` there — same binary, parallel threads, concurrent rewrites of
`Cargo.toml` / `src/main.rs` / `src/app.rs` / `src/reorder.rs` and two cargo
invocations in one target dir.

| Change | line |
| --- | --- |
| `scaffold()` returns `ScratchCrate { root }`; root is `target/keydown-focus-e2e/run-{nanos}` | `:427` |
| `struct ScratchCrate` + `impl Drop` removing the run dir | `:498`, `:502` |
| `cargo_run(scratch: &ScratchCrate)` | `:508` |

The unique path removes the race. The `Drop` is not cosmetic: the stable path had
accumulated **338 MB** of build output. Sharing one cargo target dir between the
two tests was considered and rejected — two path-packages with the same crate name
in one target dir collide on output filenames.

The same `ScratchCrate`+`Drop` pattern was later added to
`velox-sfc/tests/slots_codegen.rs:448` (it had the same leak: its `remove_dir_all`
sat *after* the assertions, so a failing run left ~300 MB behind — and a failing
run is the run that matters). Also applied to `props_loop_dispatch_compiles.rs`
and `optional_and_fn_props.rs`.

### 4. Untested relaxation, previously fatal — **fixed**

An unbound `Option` prop silently became `None` (`:2438` in the old numbering)
with no test pinning it either way. `is_optional_type` matched only `Option<`, not
`std::option::Option<`.

| Change | file:line |
| --- | --- |
| `is_optional_type` now matches any wrapper marker ending in `Option<`, so the fully-qualified spelling counts too | `velox-sfc/src/template_codegen.rs:3580` |

**Decision on `std::option::Option<`: accepted.** It is the same type, and
`PROP_TYPE_WRAPPERS` now already has both spellings for every wrapper, so
recognising one and not the other was an accident of `starts_with` rather than a
decision.

New tests in `velox-sfc/tests/optional_and_fn_props.rs`:

| Test | line |
| --- | --- |
| `an_unbound_optional_prop_becomes_none_and_compiles` — pins the NEW `None` relaxation | `:293` |
| `an_unbound_required_prop_is_still_refused` — pins that the refusal survives, and asserts the **verbatim** `velox: <Child> declares prop \`required\` and this parent did not bind it` plus both remedies | `:332` |
| `a_fully_qualified_optional_is_optional_too` — every prop bound except the `std::option::Option<String>` one, so the omission is the qualified spelling and nothing else | `:394` |

**Falsification M12** (`is_optional_type` back to the short spelling only) → 3 tests
RED. **M15** (unbound `Option` → `compile_error!` again) → 4 tests RED.

**Falsification M16 — the first attempt did NOT fail, and the reason matters.**
Making *every* unbound prop `None` left all 7 tests green. The test was asserting
`stderr.contains("required")`, and `required: None` against a `String` field also
fails the build and also prints the word "required" — a pass for entirely the wrong
reason. The assertion was rewritten to match the deliberate `velox:` sentence
(and both remedies), after which **M16b** went RED:

```
test an_unbound_required_prop_is_still_refused ... FAILED
```

### 5. Unbounded recursion inside pest — **fixed with a bound outside the grammar**

`grammar.pest:51-52` spells nesting as mutual recursion with no ceiling:

```
nested_template = { template_open ~ template_body ~ "</template>" }
template_body  = @{ (nested_template | !"</template>" ~ ANY)* }
```

The crate's only ceiling, `template_parse::MAX_TEMPLATE_DEPTH = 256`
(`velox-sfc/src/template_parse.rs:15`, enforced at `:245`), is checked *after*
pest has returned and bounds the hand-written builder's open-element `Vec` — a
different stack. pest offers no way to parameterise a rule's depth, so the bound
has to live outside the grammar, in `parse_sfc`.

| Change | file:line |
| --- | --- |
| new `pub const MAX_NESTED_TEMPLATE_DEPTH: usize = 256`, with a doc comment giving the soundness argument, the deliberate trade, and why it is NOT `MAX_TEMPLATE_DEPTH` | `velox-sfc/src/sfc.rs:236` |
| new `nested_template_openers(source) -> (count, offending_offset)` — a linear `char_indices` scan counting `<template` that **can** open a nested block, plus the byte offset of the first one past the ceiling | `velox-sfc/src/sfc.rs:245` |
| new `check_nested_template_depth(source)` — the refusal, rendered through the crate's own `diagnostic::render_parse_error` with a caret on the offending opener and a suggestion | `velox-sfc/src/sfc.rs:288` |
| `parse_sfc` calls it **before** handing the source to pest | `velox-sfc/src/sfc.rs:318` |

**Why counting occurrences is sound.** Every level of `nested_template` consumes a
distinct literal `<template` — that is what `template_open` starts with — so the
count is an **upper bound** on the grammar's real recursion depth. The guard
therefore cannot wave a recursion bomb through. It is deliberately a
single-pass `char_indices` walk: a byte counter slices a `&str` across a
character boundary and panics on any file containing non-ASCII text.

**The trade, stated plainly.** The scan makes no attempt to exclude `<template`
text that the grammar would not open on — inside an attribute value, a comment, or
a `<script>` body holding a code sample. Proving a position inert requires
modelling the grammar's own state, and getting that wrong in a guard whose only
job is to be an upper bound turns the guard into the hazard. So a file carrying
more than 256 `<template` strings **anywhere** is refused with an actionable
message. One exclusion *is* safe and was made: a `<template` not followed by
whitespace or `>` cannot be what `template_open` matches, so `<templates>` does not
count.

**New tests** — `velox-sfc/tests/nested_template_depth.rs` (9). The depths there
are literal on purpose: a fixture derived from `MAX_NESTED_TEMPLATE_DEPTH + 4`
moves its own goalposts, so raising the constant would leave the bomb tripping the
(now larger) limit and the test could not tell whether the guard does anything.

| Test | line |
| --- | --- |
| `a_recursion_bomb_is_refused_before_pest_sees_it` — **properly closed** nesting at 260 levels, because an unclosed run of openers makes pest fail on its own after one level and would leave the guard untested | `:60` |
| `a_source_with_multibyte_characters_is_scanned_safely` | `:86` |
| `a_multibyte_source_is_scanned_safely_even_when_the_guard_fires` | `:94` |
| `the_published_ceiling_is_the_one_the_guard_enforces` | `:118` |
| `the_refusal_says_what_to_do_instead` | `:130` |
| `the_refusal_points_at_the_opening_tag_that_overran_the_limit` — asserts the caret is on the overrunning opener and **not** on line 1, the top-level `<template>` every file has | `:143` |
| `a_file_that_merely_names_templates_is_not_penalised` — 400 mentions of the word, one real opener, must parse | `:171` |
| `tags_that_only_start_with_template_do_not_count_towards_the_ceiling` — 400 `<templates>` must not count | `:184` |
| `the_grammar_ceiling_is_never_below_the_element_builder_ceiling` — the invariant that keeps a future ceiling change from silently tightening the surface | `:198` |

**Falsification M17** — raised the ceiling to `100_000`. Four tests RED, including
the bomb, which then **parsed successfully** (pest handled 260 recursive frames
without incident, confirming the chosen ceiling is generous rather than near the
wire). Reverted.
**M18** — always report byte 0 instead of the offending offset. RED:
`the_refusal_points_at_the_opening_tag_that_overran_the_limit`.
**M19** — drop the whitespace-or-`>` condition and count plain substrings. RED:
`tags_that_only_start_with_template_do_not_count_towards_the_ceiling`.
**M20** — lower the ceiling to 16. Three tests RED, including
`the_grammar_ceiling_is_never_below_the_element_builder_ceiling`.
**M21** — `char_indices` back to a byte counter. Two tests RED.

### **What the pest item did and did not prove**

**Proved:** the guard fires before pest is reached; it refuses a file that pest
*would* have parsed; the count is an upper bound on real nesting depth, so a
source past the guard cannot be one that recurses without limit; the scan is
character-safe; the ceiling is never tighter than the builder's.

**Not proved, deliberately:** no test asserts that pest survives an *unbounded*
recursion. Demonstrating that requires actually letting it recurse until the stack
dies — a process-killing experiment this suite will not run, and the orchestrator
explicitly ruled it out. The evidence that 260 levels is survivable is incidental
(it fell out of M17), not a claim about where the limit actually is. No claim is
made here about pest's true stack ceiling.

---

## Not in the review

### A compile error for every `<slot>` that had fallback content — **fixed**

Found while writing the running slot test. `emit_slot_node` emitted

```rust
render_slot(name, || { { let mut __children: Vec<VNode> = ...; __children } })
```

`render_slot` takes `impl FnOnce() -> VNode`, so the closure returned a
`Vec<VNode>` — **E0308 for every slot outlet with fallback children**. The fixture
corpus never reached it because `render_slot`'s fallback arm was untested.

Fixed at `velox-sfc/src/template_codegen.rs:1587-1611` to
`render_slot({name}, || velox_dom::h("slot", velox_dom::Props::new(), {fallback}))`,
which is exactly what the ctx variant and `slots_map_expr` already do. The comment
at `:1598` records why the wrapper is load-bearing: the fallback has to be ONE node,
and `h("slot", …)` is what makes several children into one.

**Falsification M3** — reverted to the `Vec` form. RED:
`a_callers_slotted_content_reaches_the_outlet_it_named ... FAILED` with 6×
`error[E0308]: mismatched types`.

### A byte-counter panic in my own new guard — **fixed**

The first version of `nested_template_openers` advanced one byte at a time, so
`source[i..]` landed inside a multi-byte character. Caught by the full suite:
`persistent_child_tests` (2 tests) panicked at `sfc.rs:263`. Fixed with
`char_indices`, and pinned by two new tests (`:86`, `:94`).

---

## Deliberately not done

1. **No diagnostic for an unmatched slot name.** See IMPORTANT 3. The fallback is
   the feature; the join point needed to distinguish "no content" from "wrong name"
   does not exist.
2. **No shared cargo target dir between the two `keydown_focus_e2e` tests.**
   Two path-packages with the same crate name in one target dir collide on output
   filenames.
3. **The `strip_slot_binding` guarantee stays latent.** Making it real means
   teaching `emit_props_in_loop` to emit `AttrKind::Directive` attributes, which is
   a change to generated output beyond this review's scope. The doc comment now
   says so rather than claiming otherwise.
4. **No `#[allow]` added anywhere.** The only `#[allow]` in my new code is
   `#[allow(clippy::ptr_arg)]` at `velox-sfc/src/template_parse.rs:140`, which is
   pre-existing. `is_buildable_function_prop_type` and `fn_prop_core` are both
   genuinely called; there is no instrumentation, no debug counter and no scratch
   file left in the tree.
5. **Did not narrow the `is_function_prop_type` doc comment to match the builder.**
   It stays broad, because it is still used for RECOGNITION (to decide that a
   prop *is* a callback and needs the dispatch treatment at all). Narrowing it
   would silently change behaviour for `Option<Box<dyn Fn(String) -> bool>>`
   bindings that are not method references. The builder/recogniser split is now
   explicit in code (`is_function_prop_type` vs `is_buildable_function_prop_type`)
   and the doc comment says which is which.
6. **Did not run the workspace test suite.** `velox-renderer/**` and
   `velox-style/**` have concurrent owners with work in flight; a workspace run
   would report their in-flight state as my breakage. The new `parse_sfc` guard is
   the one change with a blast radius outside this crate, so **the orchestrator
   should run the full workspace suite after the writers land.**

---

## File manifest

Modified (`velox-sfc` only):

- `velox-sfc/src/template_codegen.rs` — CRITICAL, IMPORTANT 1, IMPORTANT 2, IMPORTANT 3, the `emit_slot_node` bug
- `velox-sfc/src/template_parse.rs` — `normalize_directive_name` visibility, slot-key normalisation
- `velox-sfc/src/sfc.rs` — the grammar-recursion bound (**new file in this change set's scope; was already listed as modified by the named-slot branch work**)
- `velox-sfc/src/codegen.rs` — `sanitize_ident` comment
- `velox-sfc/src/codegen_unit_tests.rs` — 4 new unit tests
- `velox-sfc/tests/emit_system_tests.rs` — tautology fix
- `velox-sfc/tests/keydown_focus_e2e.rs` — unique run dir + cleanup

Created:

- `velox-sfc/tests/props_loop_dispatch_compiles.rs` (2 tests)
- `velox-sfc/tests/slots_codegen.rs` (11 tests)
- `velox-sfc/tests/optional_and_fn_props.rs` (7 tests)
- `velox-sfc/tests/nested_template_depth.rs` (9 tests)

**Not mine, present in the working tree from the named-slot branch work**
(`git status` shows these modified/untracked alongside my changes):
`velox-sfc/src/grammar.pest`, `velox-sfc/tests/fixtures/*.rs`,
`velox-sfc/tests/props_collection.rs`, `velox-sfc/tests/emit_invokes_parent.rs`,
`velox-sfc/tests/keydown_binding.rs`, `velox-sfc/tests/slot_grammar.rs`.

Scratch build directories left behind by earlier revisions of these tests
(~1.2 GB, all created by this pass) were removed; the pre-existing
`velox-sfc/target/keydown-focus-e2e/{Cargo.lock,Cargo.toml,src,target}` and
`velox-sfc/target/emit-invokes/` were left alone.

Nothing was staged, committed, reverted or cleaned with git.