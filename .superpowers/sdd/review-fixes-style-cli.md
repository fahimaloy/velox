# Review fixes — velox-style, velox-cli, velox-dom tests, todo tests, CI

Branch `feat/premium-boilerplate-core-components`. Write scope honoured: only
`velox-style/**`, `velox-cli/**`, `.github/**`, `examples/todo/tests/**`,
`velox-dom/tests/**`, plus `CHANGELOG.md` (the CI section, to stop the
`--fail-under 0` claim from outliving the change). Nothing under
`velox-renderer/**` or `velox-sfc/**` was touched. No commit, no `git add`,
no staging.

Every line citation below was re-verified at source after the concurrent edits;
where the review's number had drifted, the correct one is given.

| Finding | Status |
| --- | --- |
| A1 `::placeholder` targeting | **fixed** + tests + 2 falsification mutations |
| A2 untested `ua.css` `input` rule | **fixed** (documented deviation, not conditioned) + 5 tests + 3 mutations |
| A3 `add.rs` non-atomic, symlink-following guard | **fixed** (atomic `create_new`) + 4 tests + 2 mutations |
| A4 two unfailable CI jobs | **fixed both** — probe now covers `examples/`; coverage measured and gated |
| A5 vacuous staleness tripwire | **fixed** + 1 falsification mutation |
| A6 unreachable `_` arm | **fixed** + 1 falsification mutation |
| `velox_web` / `.gitignore` | **open — not mine, escalated** (see "Open") |
| `flex` shorthand has no `set_property` arm | **already correct — working code, left alone** |
| CI security waiver comment | **already correct — one-line clarification added** |

---

## A1 — `::placeholder` targeting was keyed off the wrong selector part

**Status: fixed.**

### Defect

`velox-style/src/lib.rs:842` (was `:814`) routed a rule's declarations into the
placeholder accumulator or the element's own accumulator on a single question:
is the LAST part a placeholder?

```rust
let target = if rule.selector.parts.last().is_some_and(|p| p.placeholder) {
```

`.a::placeholder .b` parses to `SelectorPart{class:"a", placeholder:true,
combinator:None}` + `SelectorPart{class:"b", placeholder:false,
combinator:Descendant}`. The last part has no placeholder flag, so `.a`'s
placeholder declarations landed in `.b`'s OWN `style`.

Confirmed empirically. With the old routing, a `span.b` inside an
`input.a::placeholder` received
`style="box-sizing: border-box; color: #ff0000; display: inline;"`.

### Fix

`velox-style/src/lib.rs:191` — new `CompoundSelector::placeholder_is_subject`,
plus `has_placeholder` at the same impl. Routing at `:842`:

```rust
let target = if rule.selector.has_placeholder() {
    if !rule.selector.placeholder_is_subject() {
        continue;
    }
    &mut pseudo_acc
} else {
    &mut acc
};
```

The rule is: a `::placeholder` part must be the selector's SUBJECT (rightmost
part) and its ONLY `::placeholder` part. `::placeholder` is a pseudo-ELEMENT, so
CSS only ever allows it as the subject; a pseudo-element has no box and cannot be
a link in a descendant chain. When it is a chain link, the selector matches
nothing in real CSS, so the declarations now reach nobody rather than reaching
the subject.

Accepted (real CSS, kept working): `input::placeholder`,
`.field::placeholder`, `*::placeholder`, `.wrap input::placeholder`,
`.wrap > input::placeholder`, `.a .b::placeholder`.
Rejected: `.a::placeholder .b`, `.a::placeholder > .b`, and
`.a::placeholder .b::placeholder` (a link disqualifies it even though the subject
also carries one).

### Also fixed — the same defect in a second function

`velox-style/src/lib.rs:929`, in `compute_styles_for_node`, had **no**
placeholder separation at all: any matching `::placeholder` rule fed its
declarations straight into the element's `ComputedStyle`. Same failure mode the
`pseudo_acc` split exists to prevent (the element's own style is what the painter
reads the VALUE's colour from). It now skips any rule that has a placeholder
part, since that function has one accumulator and no second attribute to put
them in.

This is **beyond what the review reported.** It has no production caller
(`grep -rn compute_styles_for_node --include='*.rs'` → only its definition and
`velox-style/tests/computed_properties_tests.rs`), so it is a latent trap rather
than a live bug — but the reviewer's concern applied to it verbatim and it is
three lines to close.

### Tests

`velox-style/tests/placeholder_targeting.rs` (new, 6 tests) — end-to-end through
`apply_with_cascade`: the bare form, a descendant chain before the subject, three
rejected shapes, and a control proving an unrelated `.b` rule still applies
alongside a rejected one.

Six inline unit tests in `velox-style/src/lib.rs:1154+` pin the routing table
directly through `placeholder_is_subject`.

One of my own new test cases was wrong and the test caught it: I had listed
`.a .b::placeholder` as a *rejected* shape. It ends in the placeholder, so it is
the subject and is valid CSS. Corrected, and the case is kept as
`a_placeholder_in_the_chain_order_does_not_change_the_subject` because it is the
one that shows which end of the chain decides.

### Falsification

**M1 — drop the `count() == 1` clause** from `placeholder_is_subject`, leaving
only the last-part check.

```
test placeholder_tests::a_second_placeholder_part_disqualifies_the_selector ... FAILED
panicked at velox-style/src/lib.rs:1239
```

**M2 — restore the old routing** (`parts.last().is_some_and(|p| p.placeholder)`):

```
test a_placeholder_as_a_descendant_link_does_not_repaint_the_descendant ... FAILED
test a_rejected_placeholder_chain_does_not_disturb_the_rules_around_it ... FAILED

`.a::placeholder .b` matched no element in CSS — there is no node for
`.a::placeholder` to be an ancestor of — so it must not paint `.b`.
Got style Some("box-sizing: border-box; color: #ff0000; display: inline;")
```

Both reverted.

**A fixture bug this exposed, worth knowing:** my first version of these tests
used `input.a` and `span.b` as SIBLINGS and passed against both old and new
code. `apply_rec` (`velox-style/src/lib.rs:876-879`) builds `child_ancestors`
as `[node, ...ancestors]` — the element ITSELF occupies index 0, so the first
candidate `match_prefix` examines for a descendant combinator is the target, not
its parent. A sibling is never reachable as a chain link, and the control test
(`.a .b` with no placeholder) failed loudly, which is what caught it. Any
descendant-selector test in this repo must nest.

### Verification

```
cargo test -j 3 -p velox-style
```
27 + 15 + 5 + 1 + 6 + 3 + 10 + 4 + 1 + 3 lib/integration binaries, all `ok`,
0 failed.

---

## A2 — a new `ua.css` rule with no test anywhere

**Status: fixed. The deviation is DOCUMENTED, not conditioned.**

### The rule

`velox-style/src/ua.css:65`:
```css
input { padding: 6px 10px; min-height: 24px; color: #000000; }
```

The review's verification holds: `min-height` has a `set_property` arm
(`velox-dom/src/style.rs:1433`) and is consumed from the style string in block
flow (`velox-dom/src/layout.rs:5364-5387`), `color` has its arm at
`velox-dom/src/style.rs:1603`, and the rule adds no `display` so the
`ua.css` / `INLINE_BY_DEFAULT_TAGS` sync is untouched — correct, since `input` is
deliberately absent from `velox-dom/src/layout.rs:2579`.

### The checkbox/radio question: I checked, and the answer is NO

The review asked me to check whether velox can express `:not([type=…])` rather
than assume. It cannot, and writing it is **worse than leaving the rule
unconditioned**, because the condition comes out INVERTED:

```text
written:  input:not([type=checkbox])
parsed:   SelectorPart { tag: "input", attr_name: "type",
                        attr_value: Some("checkbox"), placeholder: false }
i.e.:     input[type=checkbox]
```

`split_pseudos` (`velox-style/src/lib.rs:351-380`) recognises exactly two pseudos,
`hover` and `placeholder`, and drops anything else via its `_ => {}` arm at
`:372`. `parse_selector_part` (`:382-395`) then independently hoists the first
`[...]` group out as a REQUIREMENT. Composed, `:not()` vanishes and the bracket
survives as a positive condition.

So conditioning the rule would apply the padding and the forced black to the
**checkbox** and not to the text field. I therefore documented the deviation
instead, and pinned the reason.

Pinned by `velox-style/tests/unsupported_not_pseudo.rs` (new, 3 tests):
`the_not_pseudo_is_unsupported_and_inverts_the_condition` asserts the parsed
selector EQUALS the parsed `input[type=checkbox]` selector, so the test FAILS the
moment `:not()` is implemented — which is the signal to revisit the deviation.
Plus `a_not_bracket_group_is_kept_as_a_requirement` and
`a_not_without_a_bracket_group_broadens_to_the_bare_tag`.

Documented at `velox-style/src/ua.css:54-64` (the `KNOWN DEVIATION` block) and in
the test file.

### Per-declaration pins

`velox-dom/tests/ua_defaults_live.rs`, 5 new tests. This file is the right home:
its module docs state the invariant that a `ua.css` declaration needs a
`set_property` arm AND a reader, and every assertion goes through the real
cascade.

Each of the three declarations is pinned **on its own**, because they are
independent and one can be removed without the others noticing:

- `an_input_takes_the_ua_padding` (`:505`) — `padding` on the line box it insets
  (10px horizontal, 6px vertical). `LayoutNode` carries no padding field.
- `an_input_keeps_a_24px_floor_even_with_its_padding_removed` (`:541`) —
  `min-height` is a FLOOR and is invisible while the UA padding alone exceeds it
  (a `padding: 0` field is 6+6+22 = 34px tall from padding alone), so the test
  overrides the author's padding to 0. Then the only thing that can produce 24px
  is `min-height`; without it the field collapses to its 22px line box.
- `an_input_takes_the_ua_text_colour` (`:569`) — `color` has no layout reader,
  so per this file's convention the observable is the post-cascade style string,
  byte-for-byte what `parse_text_style` receives.
- `an_author_colour_still_beats_the_ua_input_colour` (`:584`) — the other half of
  the `color` rationale, and a cascade fact rather than a `ua.css` fact.
- `the_input_rule_is_unconditioned_on_input_type_and_that_deviation_is_pinned`
  (`:620`) — asserts the deviation for `text`/`password`/`checkbox`/`radio`, so
  it is recorded as deliberate and a future reader cannot mistake it for an
  oversight.

### Falsification — mutated `ua.css` itself, one declaration at a time

**M7 — `padding: 6px 10px` → `9px 10px`:**
```
test an_input_takes_the_ua_padding ... FAILED
assertion `left == right` failed: ua.css `input { padding: 6px 10px }` must inset the content 6px vertically
  left: 9
```

**M8 — `min-height: 24px` → `31px`:**
```
test an_input_keeps_a_24px_floor_even_with_its_padding_removed ... FAILED
test the_input_rule_is_unconditioned_on_input_type_and_that_deviation_is_pinned ... FAILED
  left: 31   right: 24
```

**M9 — `color: #000000` → `#123456`:**
```
test an_input_takes_the_ua_text_colour ... FAILED
test the_input_rule_is_unconditioned_on_input_type_and_that_deviation_is_pinned ... FAILED
  left: Some("#123456")   right: Some("#000000")
```

All three reverted; `ua.css:65` is back to `padding: 6px 10px; min-height: 24px;
color: #000000`.

### Verification

```
cargo test -j 3 -p velox-dom --test ua_defaults_live
```
`test result: ok. 24 passed; 0 failed` (19 pre-existing + 5 new).

---

## A3 — `add.rs` no-overwrite guard was non-atomic and followed symlinks

**Status: fixed.**

`velox-cli/src/commands/add.rs` — the old `:199-204`:

```rust
if path.exists() {
    anyhow::bail!("component already exists: {}", path.display());
}
let content = component_template(&struct_name, &kebab, &slots);
fs::write(&path, content).with_context(|| format!("write {}", path.display()))?;
```

Three distinct defects, all now closed by one atomic syscall at
`velox-cli/src/commands/add.rs:234-248`:

1. A **dangling** symlink at `path` makes `Path::exists()` return false (it
   resolves), then `fs::write` → `File::create` follows the link and CREATES
   whatever it points at.
2. `exists()` then `write` is check-then-act: a file created in between is
   silently **truncated**, because `write` opens with `.truncate(true)`.
3. A live symlink was refused only because its TARGET existed — the refusal was a
   side effect of the target, not a property of the path.

```rust
let mut file = match fs::OpenOptions::new()
    .write(true)
    .create_new(true)
    .open(&path)
{
    Ok(f) => f,
    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
        anyhow::bail!("component already exists: {}", path.display());
    }
    Err(e) => return Err(e).with_context(|| format!("create {}", path.display())),
};
file.write_all(content.as_bytes())
    .with_context(|| format!("write {}", path.display()))?;
```

`create_new` fails if the path exists AT ALL and does not resolve the final
component, so it refuses both the existing file and the existing symlink in one
call. **Error text preserved verbatim** — `"component already exists: {}"`, which
`add_component_tests.rs` asserts on.

`create_dir_all` had the shape too (`:207-213`): it succeeds when the directory
exists as a symlink to a real directory, so the component would be written
through it. Now refused with an explicit message.

### Tests

4 new tests in `velox-cli/tests/add_component_tests.rs`, all `#[cfg(unix)]` except
the control:
- `a_pre_existing_real_file_is_still_refused` — control, so the symlink tests
  cannot pass merely because the command stopped writing at all. Also asserts
  content is not clobbered.
- `a_dangling_symlink_at_the_component_path_is_refused_and_not_written_through`
- `a_live_symlink_at_the_component_path_is_refused_and_its_target_is_untouched`
- `a_symlinked_components_directory_is_refused`

### Falsification

**M3 — restore `if path.exists() { bail }` + `fs::write`:**
```
test a_dangling_symlink_at_the_component_path_is_refused_and_not_written_through ... FAILED
panicked at velox-cli/tests/add_component_tests.rs:251
assertion failed: !out.status.success()
```
(the command SUCCEEDED — it wrote through the dangling link)

**M4 — remove the `symlink_metadata` directory guard:**
```
test a_symlinked_components_directory_is_refused ... FAILED
panicked at velox-cli/tests/add_component_tests.rs:298
```

Both reverted.

### Verification

```
cargo test -j 3 -p velox-cli
```
All 12 binaries `ok`, 0 failed. `add_component_tests`: 13 passed
(9 pre-existing + 4 new). `cargo clippy -j 3 -p velox-cli --all-targets` clean.

---

## A4 — two CI jobs that can never fail

**Status: both fixed.**

### `property-tests` — probe could not see `examples/`

`ci.yml` `:224-238`. The probe listed six directories and omitted
`examples/counter`, `examples/todo`, `examples/showcase`.

The review's claim was overstated in one respect and I verified it: the *claim
that the job goes green having run one `grep`* is correct, and the intent
(disclosed, opt-in) is right. The gap is real.

**Fixed:** the probe now also searches `examples`, with a comment saying why the
list must cover every crate.

**Falsification (M11).** Planted `examples/todo/tests/zz_m11_probe.rs` containing
`proptest!(...)`:
```
new probe (with examples):  present=true
old six-dir probe:          present=false   <-- M11 CONFIRMED
```
Probe file removed.

The second half of the finding — that turning it on costs an unbudgeted second
native Skia build through the same feature unification `build-test` already pays
for — is real but is not yet a defect, since the job cannot run. I recorded it at
the `Run proptests` step instead of leaving it to be discovered: either add
`cargo proptest` to `build-test`, which already builds Skia so the cost is
incremental, or budget this job separately.

### `coverage` — `--fail-under 0`, and a second instrumented Skia build

**Recommendation was to delete the job or set a real threshold. I set a real
threshold, so I measured first.**

Installed the toolchain the job uses (`rustup component add llvm-tools-preview`,
`cargo install cargo-llvm-cov --locked`) and ran
`cargo llvm-cov --workspace --summary-only --no-fail-fast`, then read the total
out of the collected profile data with `cargo llvm-cov report --summary-only`.

**Measured: 77.52% lines / 77.21% regions** (24,977 lines, 5,616 missed;
37,229 regions, 8,484 missed).

Caveat on the measurement, and it matters: the first run failed to compile
`velox-renderer` mid-measurement because a sibling agent was editing
`velox-renderer/src/skia_surface.rs:160` (that file is outside my write scope, so
I did not touch it), and `-p velox-renderer --lib` also has one pre-existing
failing test, `skia_render::skia_impl::tests::border_value_resolves_relative_lengths`
(`velox-renderer/src/skia_render.rs:3998`, `left: 16.0, right: 18.0`). A target
whose tests fail is EXCLUDED from the total, so **77.52% is a slight
under-count** — `velox-renderer`'s contribution is missing or partial. That
sentence is now in the workflow comment, because it is the kind of thing that
makes a coverage number look better than it is.

**`COVERAGE_MIN: 76`**, deliberately BELOW the measured 77.52% so ordinary work
does not redden CI. The review's warning applies and I respected it: a
`--fail-under` above the measured value turns CI red for everyone on the next
unrelated change. 76 still catches the real regression — a large block losing its
tests moves the number by whole points.

Added `--no-fail-fast` to the measurement step, for the reason above: without it
one red target costs you the number entirely.

`CHANGELOG.md` updated so the old `--fail-under 0` claim does not outlive the
change.

**Not done:** I did not delete the job. The review gave a recommendation, not an
instruction, and having measured a real baseline the job now enforces something.

### Verification

```
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"
```
`YAML OK`. `jobs:` still `['build-test', 'coverage', 'dependency-audit',
'property-tests', 'renderer-features', 'skia-native-non-unix', 'docker']`.
`COVERAGE_MIN: 76`. Both `run:` blocks read back as intended.

---

## A5 — a staleness tripwire that passed vacuously

**Status: fixed.**

`examples/todo/tests/render_proof.rs:406-426`. The old code scored `Greater` +1,
`Less` -1, `Equal` 0, and asserted the SUM was 0. If `dark_pixels_in` ever came
to count only the background — precisely the failure the doc at `:391-401`
describes — both themes go `Equal`, both contribute 0, and the assertion passes
having detected nothing.

Now `:407-445`: each theme's direction is asserted by name, against a named
expected ordering.

```rust
assert_eq!(light.2.cmp(&light.1), Ordering::Greater, "on the light theme ...");
assert_eq!(dark.2.cmp(&dark.1), Ordering::Less, "on the dark theme ...");
```

**M6 — `dark_pixels_in` returns a constant `42`**, i.e. exactly the described
vacuity (a measure blind to the theme, so both orderings become `Equal`):
```
test the_ink_measure_is_theme_independent ... FAILED
assertion `left == right` failed: on the light theme typing must RAISE the
dark-pixel count (42 -> 42). If it did not, `dark_pixels_in` is no longer
measuring what this test's doc comment says it measures.
  left: Equal
 right: Greater
```
Under the old sum-based assertion this would have passed. Reverted.

**The load-bearing half is untouched and still runs for both themes:**
`typed_ink > blank_ink + 5` at `:586`, plus the zero-ink-in-blank check at
`:604-607` and the value-coloured check at `:605-609`.

### Verification

```
cargo test -j 3 -p velox-example-todo --test render_proof
```
`test result: ok. 6 passed; 0 failed`.

---

## A6 — a `_` arm that was unreachable

**Status: fixed.**

`velox-cli/src/commands/add.rs:185` (was):
```rust
let first = struct_name.chars().next().unwrap_or('_');
if !first.is_ascii_alphabetic() && first != '_' {
```

Both halves were dead. `to_pascal_case` (`:53-66`) substitutes `"Component"` for
an empty result, so `struct_name` is never empty and `unwrap_or('_')` cannot fire
— and it invented a character that was not in the name. `split_words` (`:25-48`)
emits only lowercased ASCII alphanumerics, so the first character is always an
ASCII letter or digit and `first != '_'` cannot fire either.

The review is right that the GUARD is load-bearing — it is what rejects a leading
digit — and right that the `_` arm is decoration. Simplified to exactly the check
that works (`:197`):

```rust
if !struct_name.starts_with(|c: char| c.is_ascii_alphabetic()) {
    anyhow::bail!("invalid component name '{name}': must start with a letter");
}
```

No `#[allow]`. The comment above records why both former arms were unreachable,
so the simplification does not read as an oversight.

**M5 — widen the guard to `is_ascii_alphanumeric()`** (which accepts the leading
digit):
```
test invalid_component_name_is_still_refused ... FAILED
panicked at velox-cli/tests/add_component_tests.rs:184
```
The existing test (`add_component_tests.rs:180-187`, driving `add 1foo`) is kept
and still pins the behaviour. Reverted.

---

## Already correct — evidence

**`flex` shorthand has no `set_property` arm.** Working code, as stated. 12
`flex:` uses across the six template components are handled by
`velox-dom/src/layout.rs:4076-4078` parsing the shorthand from the raw cascaded
style string; `velox-dom/src/style.rs:1549-1562` covers only the longhands and
the catch-all at `:1761` is `_ => {}`. Noted, not changed.

**CI security waiver comment.** Left substantively alone as instructed. Added
the one-line clarification that was invited, at `ci.yml` `:124-131`: the waived
versions are the ones that path pins today and a `cargo update` can move any of
them; and `Cargo.lock` also contains a **`wayland-scanner 0.29.5`** line which
depends on `xml-rs`, not `quick-xml` — verified:

```
$ grep -n 'name = "wayland-scanner"' -A 8 Cargo.lock
2782:name = "wayland-scanner"
2783-version = "0.29.5"
2786-dependencies = [ "proc-macro2", "quote", "xml-rs" ]
2793:name = "wayland-scanner"
2794-version = "0.31.8"
2797-dependencies = [ "proc-macro2", "quick-xml", "quote" ]
```

So the 0.29.5 entry is a different path from the one waived, and the comment now
says so rather than leaving a reader to infer it.

---

## Open — escalated, not mine to decide

**`velox_web` and `.gitignore:67-70`.** The claim that `velox_web` "is its own
git repository (separate remote, separate history) nested inside this one" was
reported FALSE at review time. **It is TRUE now**, apparently fixed during this
window — current state:

```
$ ls -a velox_web/.git
COMMIT_EDITMSG  config  description  HEAD  hooks  index  info  logs  objects  refs

$ git -C velox_web log --oneline | wc -l
4

$ git -C velox_web remote -v
origin  git@github.com:fahimaloy/velox-web.git (fetch/push)

$ git -C velox_web ls-files | wc -l
93     # 24 .tsx, 22 .ts, 22 .mdx, 5 .svg, 5 .json, 4 .md, ...
```

The nested repo exists, has its own separate remote and 4 commits, and all 93
source files ARE versioned inside it. `/velox_web/` in `.gitignore` is therefore
correct — not an accidental hide of unversioned files. The orchestrator/user
appears to have acted on the escalation between the review and now.

Worth a glance: the ignore comment does not state that the files are versioned in
the NESTED repo rather than here, which is the detail that made it look like a
blind spot.

---

## Full verification, final state

```
cargo test -j 3 -p velox-style                 all binaries ok, 0 failed
cargo test -j 3 -p velox-dom                  25 binaries ok, 0 failed
cargo test -j 3 -p velox-cli                  12 binaries ok, 0 failed
cargo test -j 3 -p velox-example-todo \
  --test render_proof                         6 passed, 0 failed
cargo clippy -j 3 -p velox-style --all-targets -- -D warnings    clean
cargo clippy -j 3 -p velox-cli --all-targets                     clean
cargo fmt --all -- --check                    no diff in any file I touched
```

`cargo fmt --all -- --check` still reports diffs in `velox-renderer/**` and
`velox-sfc/**` — sibling agents' files, mid-edit, outside my scope. I formatted
only the seven files I wrote.

**Not run:** the full `cargo test --workspace` and
`cargo clippy --workspace` clean end-to-end. `velox-renderer` currently has one
failing test and two clippy `doc list item without indentation` errors in
`velox-sfc`, all in files owned by sibling agents. Neither is mine to fix, and
both should be re-run once those agents land.

**Instrumentation:** all removed. No `zz_*` scratch files, no `MUTATION` markers,
no `#[allow]` added anywhere. Verified by `grep` across all four write-scope
trees.