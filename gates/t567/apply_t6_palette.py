#!/usr/bin/env python3
"""Apply the T6 palette to the six template components, role-aware.

Why a script and not a sed: several CURRENT values play two DIFFERENT roles in
the palette, so replacing by value would be wrong. `#333d45` is the resting
control border in `.dark .input` / `.dark .check` / `.dark .cancel` (-> control
border `#64727B`) and the HOVER border in `.dark .toggle:hover` /
`.dark .ghost:hover` / `.dark .dismiss:hover` (-> control hover `#7F8C94`).
Same in light mode, where `#dedbd3` is the card/panel/composer decorative border
(-> `#D8D4CB`) but the control border on `.input`, `.toggle`, `.ghost`, `.cancel`
(-> `#868D92`), and the hover border on `.dismiss:hover` (-> `#636A70`).

Each entry is keyed by (file, 1-based line, expected_old_substring). The script
refuses to write unless the expected old text is present on exactly that line,
so a stale line number is a hard error rather than a silent miss.
"""
import sys

ROOT = "velox-cli/templates/project/src/"

# role -> (old, new). Named so the table can be checked against T6 by eye.
P = {
    # ---------------- dark ----------------
    "page": ("#0f1316", "#0A0E11"),
    "surface-1": ("#161b1f", "#141A1E"),
    "surface-2": ("#181e22", "#1A2126"),
    "surface-3-hover": ("#1d2429", "#20282D"),
    "surface-3-hover-2": ("#222a2f", "#20282D"),
    "border-decorative": ("#242c31", "#364147"),
    "border-control": ("#333d45", "#64727B"),
    "border-control-hover": ("#333d45", "#7F8C94"),
    "border-control-hover-2": ("#414c55", "#7F8C94"),
    "text-primary": ("#e8ecec", "#EDF2F3"),
    "text-secondary": ("#a3aeb2", "#A8B4B9"),
    "text-muted-dark": ("#75818a", "#849299"),
    "accent": ("#4fd1c5", "#2DD4BF"),
    "accent-hover": ("#6fe0d5", "#5EEAD4"),
    "on-accent": ("#08110f", "#04211D"),
    "danger": ("#f08a80", "#FF8A80"),
    "danger-hover": ("#f7a49b", "#FFA9A1"),
    "on-danger": ("#1a0906", "#2B0704"),
    "badge-border": ("#3a2020", "#4A2B28"),
    "scrim-dark": ("rgba(3, 5, 6, 0.66)", "rgba(4, 7, 9, 0.72)"),
    # ---------------- light ----------------
    "border-decorative-light": ("#dedbd3", "#D8D4CB"),
    "border-control-light": ("#dedbd3", "#868D92"),
    "border-control-hover-light": ("#c4bfb4", "#636A70"),
    # `.check` is a control at REST and was the only control using the hover
    # token; des-1's table listed only `#dedbd3`, so this second spelling is
    # called out here.
    "border-control-light-2": ("#c4bfb4", "#868D92"),
    "border-control-hover-light-2": ("#dedbd3", "#636A70"),
    "text-muted-light": ("#6b7377", "#667076"),
    "hover-surface": ("#efece5", "#EFEDE8"),
    "hover-surface-2": ("#f6f5f2", "#EFEDE8"),
}

EDITS = [
    # ---------------- App.vx ----------------
    ("App.vx", 331, "border-decorative-light", "control"),      # .toggle rest
    ("App.vx", 337, "border-control-hover-light", ""),           # .toggle:hover
    ("App.vx", 338, "hover-surface", ""),                        # .toggle:hover bg
    ("App.vx", 378, "border-control-light", ""),                 # .ghost rest
    ("App.vx", 387, "border-control-hover-light", ""),           # .ghost:hover
    ("App.vx", 388, "hover-surface", ""),                        # .ghost:hover bg
    ("App.vx", 395, "text-muted-light", ""),                     # .footnote
    ("App.vx", 403, "page", ""),                                 # .dark .app bg
    ("App.vx", 404, "text-primary", ""),                         # .dark .app color
    ("App.vx", 407, "accent", ""),                               # .dark .eyebrow
    ("App.vx", 410, "text-primary", ""),                         # .dark .title
    ("App.vx", 413, "text-secondary", ""),                       # .dark .tagline
    ("App.vx", 416, "border-decorative", ""),                    # .dark .toggle
    ("App.vx", 417, "surface-1", ""),                            # .dark .toggle bg
    ("App.vx", 418, "text-secondary", ""),                       # .dark .toggle color
    ("App.vx", 421, "border-control-hover", ""),                 # .dark .toggle:hover
    ("App.vx", 422, "surface-3-hover", ""),                      # .dark .toggle:hover bg
    ("App.vx", 423, "text-primary", ""),                         # .dark .toggle:hover color
    ("App.vx", 426, "text-secondary", ""),                       # .dark .meta-count
    ("App.vx", 429, "border-decorative", ""),                    # .dark .rule (divider)
    ("App.vx", 432, "border-decorative", ""),                    # .dark .ghost
    ("App.vx", 433, "surface-1", ""),                            # .dark .ghost bg
    ("App.vx", 434, "text-secondary", ""),                       # .dark .ghost color
    ("App.vx", 437, "border-control-hover", ""),                 # .dark .ghost:hover
    ("App.vx", 438, "surface-3-hover", ""),                      # .dark .ghost:hover bg
    ("App.vx", 439, "text-primary", ""),                         # .dark .ghost:hover color
    ("App.vx", 442, "text-muted-dark", ""),                      # .dark .footnote
    # ---------------- Todos.vx ----------------
    ("components/Todos.vx", 205, "border-decorative-light", "composer well"),
    ("components/Todos.vx", 235, "border-decorative-light", "empty-state panel"),
    ("components/Todos.vx", 238, "text-muted-light", ""),
    ("components/Todos.vx", 247, "border-decorative", ""),
    ("components/Todos.vx", 248, "surface-1", ""),
    ("components/Todos.vx", 251, "accent", ""),
    ("components/Todos.vx", 252, "accent", ""),
    ("components/Todos.vx", 253, "on-accent", ""),
    ("components/Todos.vx", 256, "accent-hover", ""),
    ("components/Todos.vx", 257, "accent-hover", ""),
    ("components/Todos.vx", 260, "border-decorative", ""),
    ("components/Todos.vx", 261, "surface-1", ""),
    ("components/Todos.vx", 262, "text-muted-dark", ""),
    # ---------------- TodoInput.vx ----------------
    ("components/TodoInput.vx", 96, "border-control-light", "the input IS the control"),
    ("components/TodoInput.vx", 105, "text-muted-light", ""),
    ("components/TodoInput.vx", 110, "border-control", ""),
    ("components/TodoInput.vx", 111, "surface-1", ""),
    ("components/TodoInput.vx", 112, "text-primary", ""),
    ("components/TodoInput.vx", 115, "text-muted-dark", ""),
    # ---------------- TodoItem.vx ----------------
    ("components/TodoItem.vx", 116, "border-decorative-light", "card"),
    ("components/TodoItem.vx", 131, "border-control-light-2", "check at rest"),
    ("components/TodoItem.vx", 165, "text-muted-light", ""),
    ("components/TodoItem.vx", 198, "text-muted-light", ""),
    ("components/TodoItem.vx", 206, "border-decorative", ""),
    ("components/TodoItem.vx", 207, "surface-1", ""),
    ("components/TodoItem.vx", 210, "border-control", ""),
    ("components/TodoItem.vx", 211, "surface-1", ""),
    ("components/TodoItem.vx", 212, "on-accent", ""),
    ("components/TodoItem.vx", 215, "accent", ""),
    ("components/TodoItem.vx", 218, "on-accent", ""),
    ("components/TodoItem.vx", 221, "text-primary", ""),
    ("components/TodoItem.vx", 225, "surface-1", ""),
    ("components/TodoItem.vx", 226, "text-muted-dark", ""),
    ("components/TodoItem.vx", 229, "danger", ""),
    ("components/TodoItem.vx", 230, "danger", ""),
    ("components/TodoItem.vx", 231, "on-danger", ""),
    ("components/TodoItem.vx", 234, "accent", ""),
    ("components/TodoItem.vx", 235, "accent", ""),
    ("components/TodoItem.vx", 238, "accent-hover", ""),
    ("components/TodoItem.vx", 239, "accent-hover", ""),
    ("components/TodoItem.vx", 242, "text-muted-dark", ""),
    # ---------------- Modal.vx ----------------
    ("components/Modal.vx", 196, "border-decorative-light", "panel"),
    ("components/Modal.vx", 228, "text-muted-light", ""),
    ("components/Modal.vx", 231, "border-control-hover-light-2", "dismiss:hover"),
    ("components/Modal.vx", 232, "hover-surface-2", ""),
    ("components/Modal.vx", 263, "border-control-light", ""),
    ("components/Modal.vx", 272, "border-control-hover-light", ""),
    ("components/Modal.vx", 273, "hover-surface-2", ""),
    ("components/Modal.vx", 296, "scrim-dark", ""),
    ("components/Modal.vx", 299, "border-decorative", ""),
    ("components/Modal.vx", 300, "surface-2", ""),
    ("components/Modal.vx", 301, "text-primary", ""),
    ("components/Modal.vx", 304, "text-primary", ""),
    ("components/Modal.vx", 309, "text-muted-dark", ""),
    ("components/Modal.vx", 312, "border-control-hover", ""),
    ("components/Modal.vx", 313, "surface-3-hover-2", ""),
    ("components/Modal.vx", 314, "text-primary", ""),
    ("components/Modal.vx", 317, "text-secondary", ""),
    ("components/Modal.vx", 320, "border-control", ""),
    ("components/Modal.vx", 321, "surface-2", ""),
    ("components/Modal.vx", 322, "text-secondary", ""),
    ("components/Modal.vx", 325, "border-control-hover-2", ""),
    ("components/Modal.vx", 326, "surface-3-hover-2", ""),
    ("components/Modal.vx", 327, "text-primary", ""),
    ("components/Modal.vx", 330, "accent", ""),
    ("components/Modal.vx", 331, "accent", ""),
    ("components/Modal.vx", 332, "on-accent", ""),
    ("components/Modal.vx", 335, "accent-hover", ""),
    ("components/Modal.vx", 336, "accent-hover", ""),
    # ---------------- Confirm.vx ----------------
    ("components/Confirm.vx", 153, "border-decorative-light", "panel"),
    ("components/Confirm.vx", 212, "border-control-light", ""),
    ("components/Confirm.vx", 221, "border-control-hover-light", ""),
    ("components/Confirm.vx", 222, "hover-surface-2", ""),
    ("components/Confirm.vx", 245, "scrim-dark", ""),
    ("components/Confirm.vx", 248, "border-decorative", ""),
    ("components/Confirm.vx", 249, "surface-2", ""),
    ("components/Confirm.vx", 250, "text-primary", ""),
    ("components/Confirm.vx", 253, "badge-border", ""),
    ("components/Confirm.vx", 255, "danger", ""),
    ("components/Confirm.vx", 258, "text-primary", ""),
    ("components/Confirm.vx", 261, "text-secondary", ""),
    ("components/Confirm.vx", 264, "border-control", ""),
    ("components/Confirm.vx", 265, "surface-2", ""),
    ("components/Confirm.vx", 266, "text-secondary", ""),
    ("components/Confirm.vx", 269, "border-control-hover-2", ""),
    ("components/Confirm.vx", 270, "surface-3-hover-2", ""),
    ("components/Confirm.vx", 271, "text-primary", ""),
    ("components/Confirm.vx", 274, "danger", ""),
    ("components/Confirm.vx", 275, "danger", ""),
    ("components/Confirm.vx", 276, "on-danger", ""),
    ("components/Confirm.vx", 279, "danger-hover", ""),
    ("components/Confirm.vx", 280, "danger-hover", ""),
]

applied = 0
errors = []
by_file = {}
for fname, lineno, role, note in EDITS:
    by_file.setdefault(fname, []).append((lineno, role, note))

for fname, edits in by_file.items():
    path = ROOT + fname
    lines = open(path, encoding="utf-8").read().split("\n")
    for lineno, role, note in edits:
        old, new = P[role]
        idx = lineno - 1
        if idx >= len(lines):
            errors.append(f"{fname}:{lineno} line does not exist (file has {len(lines)})")
            continue
        if old not in lines[idx]:
            errors.append(
                f"{fname}:{lineno} expected role {role!r} old {old!r} on the line, got: "
                f"{lines[idx]!r}"
            )
            continue
        lines[idx] = lines[idx].replace(old, new, 1)
        applied += 1
        print(f"{fname}:{lineno:<4} {role:<28} {old} -> {new}   {lines[idx].strip()}")
    open(path, "w", encoding="utf-8").write("\n".join(lines))

if errors:
    print("\nERRORS (nothing was written for those lines):", file=sys.stderr)
    for e in errors:
        print("  " + e, file=sys.stderr)
    sys.exit(2)
print(f"\napplied {applied} edits")
