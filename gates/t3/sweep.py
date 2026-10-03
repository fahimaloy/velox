#!/usr/bin/env python3
"""T3 mutation sweep.

Every mutation is applied to a COPY of the real source file, the mutated tree is
COMPILED, and only then is the suite run. A mutation that does not compile is
VACUOUS: the script exits 2 and records nothing, because "did not compile" is not
"not caught".

The script also refuses to proceed unless (a) the working tree is byte-identical
to the backups it took, (b) every needle occurs EXACTLY ONCE in the file it is
about to edit, and (c) the file's digest actually changed after the edit. A
silent no-op mutation is the failure mode that made two earlier sweeps in this
repo void.
"""
import hashlib
import subprocess
import sys
from pathlib import Path

ROOT = Path("/home/fahimaloy/Projects/personal/velox")
SRC = ROOT / "velox-renderer/src/skia_render.rs"
DOM = ROOT / "velox-dom/src/layout.rs"
SKIA_BACKUP = ROOT / "gates/t3/skia_render_fixed.rs"
DOM_BACKUP = ROOT / "gates/t3/layout_t3.rs"

# Each mutation is (id, description, file, edits) where `edits` is a list of
# (needle, replacement) applied in order, or None to restore the file from
# git HEAD (a straight revert of that one file).
MUTATIONS = [
    (
        "M1",
        "revert the whole fix: the pre-fix painter, which draws the whole "
        "string at every line node and breaks after line 0",
        SRC,
        None,
    ),
    (
        "M2",
        "dedupe instead of fix: paint only the FIRST node of a run and return "
        "early on the rest, so lines 1..N-1 are never drawn",
        SRC,
        [(
            "let key = node as *const VNode as usize;",
            "let key = node as *const VNode as usize;\n"
            "                        if text_word_offset.contains_key(&key) {\n"
            "                            return;\n"
            "                        }",
        )],
    ),
    (
        "M3",
        "drop the word offset: every line node starts at word 0, so each line "
        "paints the whole paragraph and overruns its box",
        SRC,
        [(
            "let from = text_word_offset.get(&key).copied().unwrap_or(0);",
            "let from = 0usize;",
        )],
    ),
    (
        "M4",
        "key the word tally on one constant instead of the VNode address: the "
        "key then collides between the tree's two paragraphs",
        SRC,
        [(
            "let key = node as *const VNode as usize;",
            "let key = 0usize;",
        )],
    ),
    (
        "M5",
        "break at the CONTAINER width again rather than at this line's own "
        "advance: for text inside a wrapped inline element the container is a "
        "per-line fragment box, so a different string is broken on every line",
        SRC,
        [(
            "let limit = layout.rect.w as f32;",
            "let limit = container_rect.width();",
        )],
    ),
    (
        "M6",
        "never carry the tally forward: store 0 whenever this is not the first "
        "line box, so every line node redraws line 0",
        SRC,
        [(
            "text_word_offset.insert(key, taken);",
            "text_word_offset.insert(key, if from == 0 { taken } else { 0 });",
        )],
    ),
    (
        "D1",
        "the design the T3 brief proposed, implemented: hoist `merged` above "
        "the per-line loop so one text VNode becomes ONE LayoutNode carrying "
        "every line, emitted once",
        DOM,
        [
            (
                "/// A piece of one line, merged back into the single `LayoutNode` the renderer\n"
                "/// expects for that VNode on that line.\nstruct MergedRun {",
                "/// A piece of one line, merged back into the single `LayoutNode` the renderer\n"
                "/// expects for that VNode on that line.\n"
                "#[derive(Clone)]\nstruct MergedRun {",
            ),
            (
                "    let mut y = cur_y;\n"
                "    let mut max_y_end = cur_y;\n"
                "    for (li, line) in lines.iter().enumerate() {",
                "    let mut y = cur_y;\n"
                "    let mut max_y_end = cur_y;\n"
                "    let mut merged_hoisted: Vec<MergedRun> = Vec::new();\n"
                "    for (li, line) in lines.iter().enumerate() {",
            ),
            (
                "        let mut merged: Vec<MergedRun> = Vec::new();",
                "        let mut merged: Vec<MergedRun> = merged_hoisted.clone();",
            ),
            (
                "        let mut nodes = inline_slots_to_nodes(&slots, &merged);\n"
                "        laid_children.append(&mut nodes);",
                "        let mut nodes = inline_slots_to_nodes(&slots, &merged);\n"
                "        if li + 1 == lines.len() {\n"
                "            laid_children.append(&mut nodes);\n"
                "        }\n"
                "        merged_hoisted = merged.clone();",
            ),
        ],
    ),
]

# The suite each mutation must turn red. Every entry includes the gate artefact,
# so the screenshot proof is falsified too and not just the unit tests.
SUITES = {
    "M1": ["wrapped_text_each_line_once", "text_decoration_render", "skia_text_wrap_render",
           "tagline_tail_is_painted"],
    "M2": ["wrapped_text_each_line_once", "tagline_tail_is_painted"],
    "M3": ["wrapped_text_each_line_once", "tagline_tail_is_painted"],
    "M4": ["wrapped_text_each_line_once", "tagline_tail_is_painted"],
    "M5": ["wrapped_text_each_line_once", "tagline_tail_is_painted"],
    "M6": ["wrapped_text_each_line_once", "tagline_tail_is_painted"],
    "D1": ["text_wrap_one_node_per_line", "layout_golden",
           "wrapped_text_each_line_once", "skia_text_wrap_render",
           "tagline_tail_is_painted"],
}

DOM_TESTS = {"text_wrap_one_node_per_line", "layout_golden"}


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def build() -> tuple[bool, str]:
    r = subprocess.run(
        ["cargo", "build", "-p", "velox-renderer", "--features", "skia-native", "-j", "3"],
        cwd=ROOT, capture_output=True, text=True, timeout=3600,
    )
    return r.returncode == 0, (r.stdout + r.stderr)[-4000:]


def run_tests(tests: list[str]) -> str:
    out = []
    for t in tests:
        pkg = "velox-dom" if t in DOM_TESTS else "velox-renderer"
        cmd = ["cargo", "test", "-p", pkg, "--test", t, "-j", "3", "--no-fail-fast"]
        if pkg == "velox-renderer":
            cmd += ["--features", "skia-native"]
        r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, timeout=3600)
        out.append(f"$ cargo test -p {pkg} --test {t}\nRC={r.returncode}\n"
                   + r.stdout + r.stderr)
    return "\n".join(out)


def main() -> int:
    for live, backup, name in ((SRC, SKIA_BACKUP, "skia_render.rs"),
                               (DOM, DOM_BACKUP, "layout.rs")):
        if not backup.exists():
            print(f"FATAL: {backup} missing")
            return 2
        if sha(live) != sha(backup):
            print(f"FATAL: {name} differs from its backup; refusing to sweep a "
                  f"tree I cannot restore")
            return 2
    print(f"skia_render.rs sha256 {sha(SRC)}")
    print(f"layout.rs      sha256 {sha(DOM)}")
    print()

    table = []
    only = set(sys.argv[1:])
    for mid, desc, target, edits in MUTATIONS:
        if only and mid not in only:
            continue
        before = sha(target)
        original = target.read_text()
        if edits is None:
            mutated = subprocess.run(
                ["git", "show", f"HEAD:{target.relative_to(ROOT)}"],
                cwd=ROOT, capture_output=True, text=True, check=True,
            ).stdout
        else:
            mutated = original
            for needle, repl in edits:
                n = mutated.count(needle)
                if n != 1:
                    print(f"{mid}: NEEDLE {needle[:60]!r} occurs {n} times, expected "
                          f"exactly 1 — ABORT, the mutation would be ambiguous")
                    target.write_text(original)
                    return 2
                mutated = mutated.replace(needle, repl, 1)
        target.write_text(mutated)
        if sha(target) == before:
            print(f"{mid}: digest unchanged — ABORT, the mutation was a no-op")
            target.write_text(original)
            return 2
        print(f"=== {mid}: {desc}")
        ok, blog = build()
        if not ok:
            print("  COMPILED: NO   <-- VACUOUS MUTATION, aborting")
            print(blog[-2000:])
            target.write_text(original)
            return 2
        print("  COMPILED: yes")
        tlog = run_tests(SUITES[mid])
        (ROOT / f"gates/t3/sweep_{mid}.txt").write_text(tlog)
        red = sorted({ln.split()[1] for ln in tlog.splitlines()
                      if ln.startswith("test ") and " FAILED" in ln
                      and not ln.startswith("test result")})
        rcs = [ln for ln in tlog.splitlines() if ln.startswith("RC=")]
        print(f"  suite RC: {rcs}")
        print(f"  RED: {red}")
        table.append((mid, desc, "yes", red))
        target.write_text(original)
        if sha(target) != before:
            print(f"FATAL: restore of {target} failed")
            return 2
        print("  restored, digest verified")

    print("\n=================== SWEEP TABLE ====================")
    for mid, desc, comp, red in table:
        verdict = "CAUGHT" if red else "NOT CAUGHT"
        print(f"{mid:3} | compiled={comp:3} | {verdict:11} | RED: {red}")
        print(f"     | {desc}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
