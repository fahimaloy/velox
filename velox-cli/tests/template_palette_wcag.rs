//! The shipped project's palette, asserted.
//!
//! T6 replaced the dark mode's colours wholesale. Two things had to be pinned
//! afterwards, and they are different kinds of test:
//!
//! * **Contrast.** WCAG 2.1 SC 1.4.3 (4.5:1 for body text) and SC 1.4.11
//!   (3:1 for the boundary of a control you can operate). Every pair below is
//!   named by SELECTOR, never by value, so the numbers are resolved out of the
//!   sheets at run time: editing a colour in a `.vx` file can never make this
//!   test silently agree with itself, and a selector that disappears is a hard
//!   failure rather than a skipped check.
//!
//! * **One value per role.** The rot T6 had to clean up was not "a wrong
//!   colour" but "the same role spelled three ways in three files" — two
//!   ghost-button hovers that disagreed, a `#333d45` that meant one thing on
//!   `.input` and another on `.ghost:hover`. `ROLES` classifies every one of the
//!   template's colour declarations by the ROLE it plays, and
//!   `one_value_per_role` holds each (role, mode) to exactly one value. A new
//!   colour has to be given a role before it can be committed, which is the only
//!   thing that stops this specific rot from coming back.
//!
//! ## What is deliberately NOT claimed
//!
//! * Decorative edges (a card, a panel, a divider, a badge ring) are held to
//!   `DECORATIVE_FLOOR` — "you can see it" — and not to 3:1. None of them
//!   identifies a component or a state: a card is identified by its surface and a
//!   divider by being a divider. Two of them do not reach 1.3 either — the light
//!   `.rule` (`#e3e1db` on `#f6f5f2` = 1.20:1) and the light `.badge` ring
//!   (`#f0d6d2` on its own `#f7e9e7` tint = 1.16:1) — so the floor is 1.15 and the
//!   numbers are stated in the module docs instead of hidden.
//! * Large-text relief (SC 1.4.3's 3:1 for >=24px, or >=18.66px bold) is not
//!   claimed anywhere. `.title` at 27px/700 would qualify; demanding 4.5:1 of it
//!   anyway costs nothing and keeps the table honest.
//! * The `.overlay` scrims get no contrast ratio: they dim whatever is beneath
//!   them and carry no text of their own.
//!   `the_scrims_are_not_transparent` pins that both stay meaningfully opaque and
//!   that the two overlays dim by the same amount.
//! * The checkbox tick's RESTING state is excluded on purpose: `.check` paints a
//!   `#ffffff` tick on a `#ffffff` box and `.dark .check` a `#04211D` tick on a
//!   `#141A1E` box, because an unchecked box is supposed to show no tick. The pair
//!   that matters is the tick on the FILLED box, and that one is asserted —
//!   `.check-mark` on `.completed .check`, 6.11:1 light and 9.09:1 dark.
//!
//! Source of truth: `velox-cli/templates/project/src/{App.vx, components/*.vx}`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// The only properties this template may carry a colour in. A colour token in any
/// OTHER property (`box-shadow`, `text-decoration-color`, …) fails
/// `no_colour_hides_in_an_unread_property` rather than going unchecked.
const COLOUR_PROPS: &[&str] = &[
    "background",
    "background-color",
    "border",
    "border-color",
    "border-bottom",
    "border-left",
    "border-right",
    "border-top",
    "color",
    "outline",
    "outline-color",
];

/// The six sheets, as paths relative to `templates/project/src`.
const SHEETS: &[&str] = &[
    "App.vx",
    "components/Confirm.vx",
    "components/Modal.vx",
    "components/TodoInput.vx",
    "components/TodoItem.vx",
    "components/Todos.vx",
];

/// A card's edge, a panel's edge, a divider, a badge ring: visible, but not a
/// control boundary. See the module docs for the two pairs that sit below 1.3.
const DECORATIVE_FLOOR: f32 = 1.15;

/// Every colour-bearing declaration in the six sheets, with the ROLE it plays.
/// One row per declaration, keyed by (file, selector, property, mode).
///
/// Roles are the unit of consistency: two declarations share a role when they
/// mean the same thing, and `one_value_per_role` requires all the rows of a role
/// to agree on the value. A value may serve two roles only when it is listed in
/// `ALLOWED_SHARINGS` — a fill and the text drawn on it are the same token on
/// purpose.
const ROLES: &[(&str, &str, &str, &str, &str, &str)] = &[
    (
        "App.vx",
        ".app",
        "background",
        "light",
        "surface-page-light",
        "#f6f5f2",
    ),
    (
        "App.vx",
        ".app",
        "color",
        "light",
        "text-primary-light",
        "#14181b",
    ),
    (
        "App.vx",
        ".eyebrow",
        "color",
        "light",
        "text-accent-light",
        "#0d6e66",
    ),
    (
        "App.vx",
        ".title",
        "color",
        "light",
        "text-primary-light",
        "#14181b",
    ),
    (
        "App.vx",
        ".tagline",
        "color",
        "light",
        "text-secondary-light",
        "#4A555B",
    ),
    (
        "App.vx",
        ".toggle",
        "border",
        "light",
        "border-control-light",
        "#868D92",
    ),
    (
        "App.vx",
        ".toggle",
        "background",
        "light",
        "card-light",
        "#ffffff",
    ),
    (
        "App.vx",
        ".toggle",
        "color",
        "light",
        "text-secondary-light",
        "#4A555B",
    ),
    (
        "App.vx",
        ".toggle:hover",
        "border",
        "light",
        "border-control-hover-light",
        "#636A70",
    ),
    (
        "App.vx",
        ".toggle:hover",
        "background",
        "light",
        "surface-hover-light",
        "#EFEDE8",
    ),
    (
        "App.vx",
        ".toggle:hover",
        "color",
        "light",
        "text-primary-light",
        "#14181b",
    ),
    (
        "App.vx",
        ".meta-count",
        "color",
        "light",
        "text-secondary-light",
        "#4A555B",
    ),
    (
        "App.vx",
        ".rule",
        "background",
        "light",
        "divider-light",
        "#dcdad3",
    ),
    (
        "App.vx",
        ".ghost",
        "border",
        "light",
        "border-control-light",
        "#868D92",
    ),
    (
        "App.vx",
        ".ghost",
        "background",
        "light",
        "card-light",
        "#ffffff",
    ),
    (
        "App.vx",
        ".ghost",
        "color",
        "light",
        "text-secondary-light",
        "#4A555B",
    ),
    (
        "App.vx",
        ".ghost:hover",
        "border",
        "light",
        "border-control-hover-light",
        "#636A70",
    ),
    (
        "App.vx",
        ".ghost:hover",
        "background",
        "light",
        "surface-hover-light",
        "#EFEDE8",
    ),
    (
        "App.vx",
        ".ghost:hover",
        "color",
        "light",
        "text-primary-light",
        "#14181b",
    ),
    (
        "App.vx",
        ".footnote",
        "color",
        "light",
        "text-muted-light",
        "#5F6A70",
    ),
    (
        "App.vx",
        ".dark .app",
        "background",
        "dark",
        "surface-page-dark",
        "#0A0E11",
    ),
    (
        "App.vx",
        ".dark .app",
        "color",
        "dark",
        "text-primary-dark",
        "#EDF2F3",
    ),
    (
        "App.vx",
        ".dark .eyebrow",
        "color",
        "dark",
        "text-accent-dark",
        "#2DD4BF",
    ),
    (
        "App.vx",
        ".dark .title",
        "color",
        "dark",
        "text-primary-dark",
        "#EDF2F3",
    ),
    (
        "App.vx",
        ".dark .tagline",
        "color",
        "dark",
        "text-secondary-dark",
        "#A8B4B9",
    ),
    (
        "App.vx",
        ".dark .toggle",
        "border",
        "dark",
        "border-control-dark",
        "#64727B",
    ),
    (
        "App.vx",
        ".dark .toggle",
        "background",
        "dark",
        "card-dark",
        "#141A1E",
    ),
    (
        "App.vx",
        ".dark .toggle",
        "color",
        "dark",
        "text-secondary-dark",
        "#A8B4B9",
    ),
    (
        "App.vx",
        ".dark .toggle:hover",
        "border",
        "dark",
        "border-control-hover-dark",
        "#7F8C94",
    ),
    (
        "App.vx",
        ".dark .toggle:hover",
        "background",
        "dark",
        "surface-hover-dark",
        "#20282D",
    ),
    (
        "App.vx",
        ".dark .toggle:hover",
        "color",
        "dark",
        "text-primary-dark",
        "#EDF2F3",
    ),
    (
        "App.vx",
        ".dark .meta-count",
        "color",
        "dark",
        "text-secondary-dark",
        "#A8B4B9",
    ),
    (
        "App.vx",
        ".dark .rule",
        "background",
        "dark",
        "divider-dark",
        "#313B42",
    ),
    (
        "App.vx",
        ".dark .ghost",
        "border",
        "dark",
        "border-control-dark",
        "#64727B",
    ),
    (
        "App.vx",
        ".dark .ghost",
        "background",
        "dark",
        "card-dark",
        "#141A1E",
    ),
    (
        "App.vx",
        ".dark .ghost",
        "color",
        "dark",
        "text-secondary-dark",
        "#A8B4B9",
    ),
    (
        "App.vx",
        ".dark .ghost:hover",
        "border",
        "dark",
        "border-control-hover-dark",
        "#7F8C94",
    ),
    (
        "App.vx",
        ".dark .ghost:hover",
        "background",
        "dark",
        "surface-hover-dark",
        "#20282D",
    ),
    (
        "App.vx",
        ".dark .ghost:hover",
        "color",
        "dark",
        "text-primary-dark",
        "#EDF2F3",
    ),
    (
        "App.vx",
        ".dark .footnote",
        "color",
        "dark",
        "text-muted-dark",
        "#849299",
    ),
    (
        "components/Confirm.vx",
        ".overlay",
        "background",
        "light",
        "scrim-light",
        "rgba(20, 24, 27, 0.34)",
    ),
    (
        "components/Confirm.vx",
        ".panel",
        "border",
        "light",
        "border-decorative-light",
        "#DFDBD2",
    ),
    (
        "components/Confirm.vx",
        ".panel",
        "background",
        "light",
        "card-light",
        "#ffffff",
    ),
    (
        "components/Confirm.vx",
        ".panel",
        "color",
        "light",
        "text-primary-light",
        "#14181b",
    ),
    (
        "components/Confirm.vx",
        ".badge",
        "border",
        "light",
        "badge-border-light",
        "#f0d6d2",
    ),
    (
        "components/Confirm.vx",
        ".badge",
        "background",
        "light",
        "badge-surface-light",
        "#f7e9e7",
    ),
    (
        "components/Confirm.vx",
        ".badge",
        "color",
        "light",
        "text-danger-light",
        "#8a2a23",
    ),
    (
        "components/Confirm.vx",
        ".title",
        "color",
        "light",
        "text-primary-light",
        "#14181b",
    ),
    (
        "components/Confirm.vx",
        ".message",
        "color",
        "light",
        "text-secondary-light",
        "#4A555B",
    ),
    (
        "components/Confirm.vx",
        ".cancel",
        "border",
        "light",
        "border-control-light",
        "#868D92",
    ),
    (
        "components/Confirm.vx",
        ".cancel",
        "background",
        "light",
        "card-light",
        "#ffffff",
    ),
    (
        "components/Confirm.vx",
        ".cancel",
        "color",
        "light",
        "text-secondary-light",
        "#4A555B",
    ),
    (
        "components/Confirm.vx",
        ".cancel:hover",
        "border",
        "light",
        "border-control-hover-light",
        "#636A70",
    ),
    (
        "components/Confirm.vx",
        ".cancel:hover",
        "background",
        "light",
        "surface-hover-light",
        "#EFEDE8",
    ),
    (
        "components/Confirm.vx",
        ".cancel:hover",
        "color",
        "light",
        "text-primary-light",
        "#14181b",
    ),
    (
        "components/Confirm.vx",
        ".accept",
        "border",
        "light",
        "danger-light",
        "#a3342c",
    ),
    (
        "components/Confirm.vx",
        ".accept",
        "background",
        "light",
        "danger-light",
        "#a3342c",
    ),
    (
        "components/Confirm.vx",
        ".accept",
        "color",
        "light",
        "on-filled-light",
        "#ffffff",
    ),
    (
        "components/Confirm.vx",
        ".accept:hover",
        "border",
        "light",
        "danger-hover-light",
        "#8a2a23",
    ),
    (
        "components/Confirm.vx",
        ".accept:hover",
        "background",
        "light",
        "danger-hover-light",
        "#8a2a23",
    ),
    (
        "components/Confirm.vx",
        ".dark .overlay",
        "background",
        "dark",
        "scrim-dark",
        "rgba(4, 7, 9, 0.72)",
    ),
    (
        "components/Confirm.vx",
        ".dark .panel",
        "border",
        "dark",
        "border-decorative-dark",
        "#313B42",
    ),
    (
        "components/Confirm.vx",
        ".dark .panel",
        "background",
        "dark",
        "surface-raised-dark",
        "#1A2126",
    ),
    (
        "components/Confirm.vx",
        ".dark .panel",
        "color",
        "dark",
        "text-primary-dark",
        "#EDF2F3",
    ),
    (
        "components/Confirm.vx",
        ".dark .badge",
        "border",
        "dark",
        "badge-border-dark",
        "#4A2B28",
    ),
    (
        "components/Confirm.vx",
        ".dark .badge",
        "background",
        "dark",
        "badge-surface-dark",
        "#2a1614",
    ),
    (
        "components/Confirm.vx",
        ".dark .badge",
        "color",
        "dark",
        "text-danger-dark",
        "#FF8A80",
    ),
    (
        "components/Confirm.vx",
        ".dark .title",
        "color",
        "dark",
        "text-primary-dark",
        "#EDF2F3",
    ),
    (
        "components/Confirm.vx",
        ".dark .message",
        "color",
        "dark",
        "text-secondary-dark",
        "#A8B4B9",
    ),
    (
        "components/Confirm.vx",
        ".dark .cancel",
        "border",
        "dark",
        "border-control-dark",
        "#64727B",
    ),
    (
        "components/Confirm.vx",
        ".dark .cancel",
        "background",
        "dark",
        "surface-raised-dark",
        "#1A2126",
    ),
    (
        "components/Confirm.vx",
        ".dark .cancel",
        "color",
        "dark",
        "text-secondary-dark",
        "#A8B4B9",
    ),
    (
        "components/Confirm.vx",
        ".dark .cancel:hover",
        "border",
        "dark",
        "border-control-hover-dark",
        "#7F8C94",
    ),
    (
        "components/Confirm.vx",
        ".dark .cancel:hover",
        "background",
        "dark",
        "surface-hover-dark",
        "#20282D",
    ),
    (
        "components/Confirm.vx",
        ".dark .cancel:hover",
        "color",
        "dark",
        "text-primary-dark",
        "#EDF2F3",
    ),
    (
        "components/Confirm.vx",
        ".dark .accept",
        "border",
        "dark",
        "danger-dark",
        "#FF8A80",
    ),
    (
        "components/Confirm.vx",
        ".dark .accept",
        "background",
        "dark",
        "danger-dark",
        "#FF8A80",
    ),
    (
        "components/Confirm.vx",
        ".dark .accept",
        "color",
        "dark",
        "on-danger-dark",
        "#2B0704",
    ),
    (
        "components/Confirm.vx",
        ".dark .accept:hover",
        "border",
        "dark",
        "danger-hover-dark",
        "#FFA9A1",
    ),
    (
        "components/Confirm.vx",
        ".dark .accept:hover",
        "background",
        "dark",
        "danger-hover-dark",
        "#FFA9A1",
    ),
    (
        "components/Modal.vx",
        ".overlay",
        "background",
        "light",
        "scrim-light",
        "rgba(20, 24, 27, 0.34)",
    ),
    (
        "components/Modal.vx",
        ".panel",
        "border",
        "light",
        "border-decorative-light",
        "#DFDBD2",
    ),
    (
        "components/Modal.vx",
        ".panel",
        "background",
        "light",
        "card-light",
        "#ffffff",
    ),
    (
        "components/Modal.vx",
        ".panel",
        "color",
        "light",
        "text-primary-light",
        "#14181b",
    ),
    (
        "components/Modal.vx",
        ".title",
        "color",
        "light",
        "text-primary-light",
        "#14181b",
    ),
    (
        "components/Modal.vx",
        ".dismiss",
        "border",
        "light",
        "border-none",
        "transparent",
    ),
    (
        "components/Modal.vx",
        ".dismiss",
        "background",
        "light",
        "card-light",
        "#ffffff",
    ),
    (
        "components/Modal.vx",
        ".dismiss",
        "color",
        "light",
        "text-muted-light",
        "#5F6A70",
    ),
    (
        "components/Modal.vx",
        ".dismiss:hover",
        "border",
        "light",
        "border-control-hover-light",
        "#636A70",
    ),
    (
        "components/Modal.vx",
        ".dismiss:hover",
        "background",
        "light",
        "surface-hover-light",
        "#EFEDE8",
    ),
    (
        "components/Modal.vx",
        ".dismiss:hover",
        "color",
        "light",
        "text-primary-light",
        "#14181b",
    ),
    (
        "components/Modal.vx",
        ".body-text",
        "color",
        "light",
        "text-secondary-light",
        "#4A555B",
    ),
    (
        "components/Modal.vx",
        ".cancel",
        "border",
        "light",
        "border-control-light",
        "#868D92",
    ),
    (
        "components/Modal.vx",
        ".cancel",
        "background",
        "light",
        "card-light",
        "#ffffff",
    ),
    (
        "components/Modal.vx",
        ".cancel",
        "color",
        "light",
        "text-secondary-light",
        "#4A555B",
    ),
    (
        "components/Modal.vx",
        ".cancel:hover",
        "border",
        "light",
        "border-control-hover-light",
        "#636A70",
    ),
    (
        "components/Modal.vx",
        ".cancel:hover",
        "background",
        "light",
        "surface-hover-light",
        "#EFEDE8",
    ),
    (
        "components/Modal.vx",
        ".cancel:hover",
        "color",
        "light",
        "text-primary-light",
        "#14181b",
    ),
    (
        "components/Modal.vx",
        ".confirm",
        "border",
        "light",
        "accent-light",
        "#0d6e66",
    ),
    (
        "components/Modal.vx",
        ".confirm",
        "background",
        "light",
        "accent-light",
        "#0d6e66",
    ),
    (
        "components/Modal.vx",
        ".confirm",
        "color",
        "light",
        "on-filled-light",
        "#ffffff",
    ),
    (
        "components/Modal.vx",
        ".confirm:hover",
        "border",
        "light",
        "accent-hover-light",
        "#0a5750",
    ),
    (
        "components/Modal.vx",
        ".confirm:hover",
        "background",
        "light",
        "accent-hover-light",
        "#0a5750",
    ),
    (
        "components/Modal.vx",
        ".dark .overlay",
        "background",
        "dark",
        "scrim-dark",
        "rgba(4, 7, 9, 0.72)",
    ),
    (
        "components/Modal.vx",
        ".dark .panel",
        "border",
        "dark",
        "border-decorative-dark",
        "#313B42",
    ),
    (
        "components/Modal.vx",
        ".dark .panel",
        "background",
        "dark",
        "surface-raised-dark",
        "#1A2126",
    ),
    (
        "components/Modal.vx",
        ".dark .panel",
        "color",
        "dark",
        "text-primary-dark",
        "#EDF2F3",
    ),
    (
        "components/Modal.vx",
        ".dark .title",
        "color",
        "dark",
        "text-primary-dark",
        "#EDF2F3",
    ),
    (
        "components/Modal.vx",
        ".dark .dismiss",
        "border",
        "dark",
        "border-none",
        "transparent",
    ),
    (
        "components/Modal.vx",
        ".dark .dismiss",
        "background",
        "dark",
        "surface-raised-dark",
        "#1A2126",
    ),
    (
        "components/Modal.vx",
        ".dark .dismiss",
        "color",
        "dark",
        "text-muted-dark",
        "#849299",
    ),
    (
        "components/Modal.vx",
        ".dark .dismiss:hover",
        "border",
        "dark",
        "border-control-hover-dark",
        "#7F8C94",
    ),
    (
        "components/Modal.vx",
        ".dark .dismiss:hover",
        "background",
        "dark",
        "surface-hover-dark",
        "#20282D",
    ),
    (
        "components/Modal.vx",
        ".dark .dismiss:hover",
        "color",
        "dark",
        "text-primary-dark",
        "#EDF2F3",
    ),
    (
        "components/Modal.vx",
        ".dark .body-text",
        "color",
        "dark",
        "text-secondary-dark",
        "#A8B4B9",
    ),
    (
        "components/Modal.vx",
        ".dark .cancel",
        "border",
        "dark",
        "border-control-dark",
        "#64727B",
    ),
    (
        "components/Modal.vx",
        ".dark .cancel",
        "background",
        "dark",
        "surface-raised-dark",
        "#1A2126",
    ),
    (
        "components/Modal.vx",
        ".dark .cancel",
        "color",
        "dark",
        "text-secondary-dark",
        "#A8B4B9",
    ),
    (
        "components/Modal.vx",
        ".dark .cancel:hover",
        "border",
        "dark",
        "border-control-hover-dark",
        "#7F8C94",
    ),
    (
        "components/Modal.vx",
        ".dark .cancel:hover",
        "background",
        "dark",
        "surface-hover-dark",
        "#20282D",
    ),
    (
        "components/Modal.vx",
        ".dark .cancel:hover",
        "color",
        "dark",
        "text-primary-dark",
        "#EDF2F3",
    ),
    (
        "components/Modal.vx",
        ".dark .confirm",
        "border",
        "dark",
        "accent-dark",
        "#2DD4BF",
    ),
    (
        "components/Modal.vx",
        ".dark .confirm",
        "background",
        "dark",
        "accent-dark",
        "#2DD4BF",
    ),
    (
        "components/Modal.vx",
        ".dark .confirm",
        "color",
        "dark",
        "on-accent-dark",
        "#04211D",
    ),
    (
        "components/Modal.vx",
        ".dark .confirm:hover",
        "border",
        "dark",
        "accent-hover-dark",
        "#5EEAD4",
    ),
    (
        "components/Modal.vx",
        ".dark .confirm:hover",
        "background",
        "dark",
        "accent-hover-dark",
        "#5EEAD4",
    ),
    (
        "components/TodoInput.vx",
        ".input",
        "background",
        "light",
        "card-light",
        "#ffffff",
    ),
    (
        "components/TodoInput.vx",
        ".input",
        "color",
        "light",
        "text-primary-light",
        "#14181b",
    ),
    (
        "components/TodoInput.vx",
        ".input::placeholder",
        "color",
        "light",
        "text-muted-light",
        "#5F6A70",
    ),
    (
        "components/TodoInput.vx",
        ".dark .input",
        "background",
        "dark",
        "card-dark",
        "#141A1E",
    ),
    (
        "components/TodoInput.vx",
        ".dark .input",
        "color",
        "dark",
        "text-primary-dark",
        "#EDF2F3",
    ),
    (
        "components/TodoInput.vx",
        ".dark .input::placeholder",
        "color",
        "dark",
        "text-muted-dark",
        "#849299",
    ),
    (
        "components/TodoItem.vx",
        ".todo-item",
        "border",
        "light",
        "border-decorative-light",
        "#DFDBD2",
    ),
    (
        "components/TodoItem.vx",
        ".todo-item",
        "background",
        "light",
        "card-light",
        "#ffffff",
    ),
    (
        "components/TodoItem.vx",
        ".check",
        "border",
        "light",
        "border-control-light",
        "#868D92",
    ),
    (
        "components/TodoItem.vx",
        ".check",
        "background",
        "light",
        "card-light",
        "#ffffff",
    ),
    (
        "components/TodoItem.vx",
        ".check",
        "color",
        "light",
        "on-filled-light",
        "#ffffff",
    ),
    (
        "components/TodoItem.vx",
        ".check:hover",
        "border",
        "light",
        "accent-light",
        "#0d6e66",
    ),
    (
        "components/TodoItem.vx",
        ".check-mark",
        "color",
        "light",
        "on-filled-light",
        "#ffffff",
    ),
    (
        "components/TodoItem.vx",
        ".todo-text",
        "color",
        "light",
        "text-primary-light",
        "#14181b",
    ),
    (
        "components/TodoItem.vx",
        ".remove",
        "border",
        "light",
        "border-none",
        "transparent",
    ),
    (
        "components/TodoItem.vx",
        ".remove",
        "background",
        "light",
        "card-light",
        "#ffffff",
    ),
    (
        "components/TodoItem.vx",
        ".remove",
        "color",
        "light",
        "text-muted-light",
        "#5F6A70",
    ),
    (
        "components/TodoItem.vx",
        ".remove:hover",
        "border",
        "light",
        "danger-light",
        "#a3342c",
    ),
    (
        "components/TodoItem.vx",
        ".remove:hover",
        "background",
        "light",
        "danger-light",
        "#a3342c",
    ),
    (
        "components/TodoItem.vx",
        ".remove:hover",
        "color",
        "light",
        "on-filled-light",
        "#ffffff",
    ),
    (
        "components/TodoItem.vx",
        ".completed .check",
        "border",
        "light",
        "accent-light",
        "#0d6e66",
    ),
    (
        "components/TodoItem.vx",
        ".completed .check",
        "background",
        "light",
        "accent-light",
        "#0d6e66",
    ),
    (
        "components/TodoItem.vx",
        ".completed .check:hover",
        "border",
        "light",
        "accent-hover-light",
        "#0a5750",
    ),
    (
        "components/TodoItem.vx",
        ".completed .check:hover",
        "background",
        "light",
        "accent-hover-light",
        "#0a5750",
    ),
    (
        "components/TodoItem.vx",
        ".completed .todo-text",
        "color",
        "light",
        "text-muted-light",
        "#5F6A70",
    ),
    (
        "components/TodoItem.vx",
        ".dark .todo-item",
        "border",
        "dark",
        "border-decorative-dark",
        "#313B42",
    ),
    (
        "components/TodoItem.vx",
        ".dark .todo-item",
        "background",
        "dark",
        "card-dark",
        "#141A1E",
    ),
    (
        "components/TodoItem.vx",
        ".dark .check",
        "border",
        "dark",
        "border-control-dark",
        "#64727B",
    ),
    (
        "components/TodoItem.vx",
        ".dark .check",
        "background",
        "dark",
        "card-dark",
        "#141A1E",
    ),
    (
        "components/TodoItem.vx",
        ".dark .check",
        "color",
        "dark",
        "on-accent-dark",
        "#04211D",
    ),
    (
        "components/TodoItem.vx",
        ".dark .check:hover",
        "border",
        "dark",
        "accent-dark",
        "#2DD4BF",
    ),
    (
        "components/TodoItem.vx",
        ".dark .check-mark",
        "color",
        "dark",
        "on-accent-dark",
        "#04211D",
    ),
    (
        "components/TodoItem.vx",
        ".dark .todo-text",
        "color",
        "dark",
        "text-primary-dark",
        "#EDF2F3",
    ),
    (
        "components/TodoItem.vx",
        ".dark .remove",
        "border",
        "dark",
        "border-none",
        "transparent",
    ),
    (
        "components/TodoItem.vx",
        ".dark .remove",
        "background",
        "dark",
        "card-dark",
        "#141A1E",
    ),
    (
        "components/TodoItem.vx",
        ".dark .remove",
        "color",
        "dark",
        "text-muted-dark",
        "#849299",
    ),
    (
        "components/TodoItem.vx",
        ".dark .remove:hover",
        "border",
        "dark",
        "danger-dark",
        "#FF8A80",
    ),
    (
        "components/TodoItem.vx",
        ".dark .remove:hover",
        "background",
        "dark",
        "danger-dark",
        "#FF8A80",
    ),
    (
        "components/TodoItem.vx",
        ".dark .remove:hover",
        "color",
        "dark",
        "on-danger-dark",
        "#2B0704",
    ),
    (
        "components/TodoItem.vx",
        ".dark .completed .check",
        "border",
        "dark",
        "accent-dark",
        "#2DD4BF",
    ),
    (
        "components/TodoItem.vx",
        ".dark .completed .check",
        "background",
        "dark",
        "accent-dark",
        "#2DD4BF",
    ),
    (
        "components/TodoItem.vx",
        ".dark .completed .check:hover",
        "border",
        "dark",
        "accent-hover-dark",
        "#5EEAD4",
    ),
    (
        "components/TodoItem.vx",
        ".dark .completed .check:hover",
        "background",
        "dark",
        "accent-hover-dark",
        "#5EEAD4",
    ),
    (
        "components/TodoItem.vx",
        ".dark .completed .todo-text",
        "color",
        "dark",
        "text-muted-dark",
        "#849299",
    ),
    (
        "components/Todos.vx",
        ".todos",
        "color",
        "light",
        "text-primary-light",
        "#14181b",
    ),
    (
        "components/Todos.vx",
        ".composer",
        "border",
        "light",
        "border-control-light",
        "#868D92",
    ),
    (
        "components/Todos.vx",
        ".composer",
        "background",
        "light",
        "card-light",
        "#ffffff",
    ),
    (
        "components/Todos.vx",
        ".add",
        "border",
        "light",
        "accent-light",
        "#0d6e66",
    ),
    (
        "components/Todos.vx",
        ".add",
        "background",
        "light",
        "accent-light",
        "#0d6e66",
    ),
    (
        "components/Todos.vx",
        ".add",
        "color",
        "light",
        "on-filled-light",
        "#ffffff",
    ),
    (
        "components/Todos.vx",
        ".add:hover",
        "border",
        "light",
        "accent-hover-light",
        "#0a5750",
    ),
    (
        "components/Todos.vx",
        ".add:hover",
        "background",
        "light",
        "accent-hover-light",
        "#0a5750",
    ),
    (
        "components/Todos.vx",
        ".empty",
        "border",
        "light",
        "border-decorative-light",
        "#DFDBD2",
    ),
    (
        "components/Todos.vx",
        ".empty",
        "background",
        "light",
        "surface-hover-light",
        "#EFEDE8",
    ),
    (
        "components/Todos.vx",
        ".empty",
        "color",
        "light",
        "text-secondary-light",
        "#4A555B",
    ),
    (
        "components/Todos.vx",
        ".dark .composer",
        "border",
        "dark",
        "border-control-dark",
        "#64727B",
    ),
    (
        "components/Todos.vx",
        ".dark .composer",
        "background",
        "dark",
        "card-dark",
        "#141A1E",
    ),
    (
        "components/Todos.vx",
        ".dark .add",
        "border",
        "dark",
        "accent-dark",
        "#2DD4BF",
    ),
    (
        "components/Todos.vx",
        ".dark .add",
        "background",
        "dark",
        "accent-dark",
        "#2DD4BF",
    ),
    (
        "components/Todos.vx",
        ".dark .add",
        "color",
        "dark",
        "on-accent-dark",
        "#04211D",
    ),
    (
        "components/Todos.vx",
        ".dark .add:hover",
        "border",
        "dark",
        "accent-hover-dark",
        "#5EEAD4",
    ),
    (
        "components/Todos.vx",
        ".dark .add:hover",
        "background",
        "dark",
        "accent-hover-dark",
        "#5EEAD4",
    ),
    (
        "components/Todos.vx",
        ".dark .empty",
        "border",
        "dark",
        "border-decorative-dark",
        "#313B42",
    ),
    (
        "components/Todos.vx",
        ".dark .empty",
        "background",
        "dark",
        "surface-hover-dark",
        "#20282D",
    ),
    (
        "components/Todos.vx",
        ".dark .empty",
        "color",
        "dark",
        "text-secondary-dark",
        "#A8B4B9",
    ),
];

/// SC 1.4.3 at 4.5:1.
///
/// Named by selector, and the value on each side is resolved out of the sheet, so
/// these rows cannot go stale against an edited palette. Where a `:hover` rule
/// only changes the fill, the fg column names the rule that DECLARES the colour,
/// because that is the one the cascade actually inherits.
///
/// (fg file, fg selector, fg property, bg file, bg selector, bg property, min)
const TEXT_PAIRS: &[(&str, &str, &str, &str, &str, &str, f32)] = &[
    (
        "App.vx",
        ".app",
        "color",
        "App.vx",
        ".app",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".title",
        "color",
        "App.vx",
        ".app",
        "background",
        4.5,
    ),
    (
        "components/Todos.vx",
        ".todos",
        "color",
        "App.vx",
        ".app",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".tagline",
        "color",
        "App.vx",
        ".app",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".meta-count",
        "color",
        "App.vx",
        ".app",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".eyebrow",
        "color",
        "App.vx",
        ".app",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".ghost",
        "color",
        "App.vx",
        ".ghost",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".toggle",
        "color",
        "App.vx",
        ".toggle",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".ghost:hover",
        "color",
        "App.vx",
        ".ghost:hover",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".toggle:hover",
        "color",
        "App.vx",
        ".toggle:hover",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".footnote",
        "color",
        "App.vx",
        ".app",
        "background",
        4.5,
    ),
    (
        "components/Confirm.vx",
        ".message",
        "color",
        "components/Confirm.vx",
        ".panel",
        "background",
        4.5,
    ),
    (
        "components/Confirm.vx",
        ".title",
        "color",
        "components/Confirm.vx",
        ".panel",
        "background",
        4.5,
    ),
    (
        "components/Confirm.vx",
        ".badge",
        "color",
        "components/Confirm.vx",
        ".badge",
        "background",
        4.5,
    ),
    (
        "components/Confirm.vx",
        ".accept",
        "color",
        "components/Confirm.vx",
        ".accept",
        "background",
        4.5,
    ),
    (
        "components/Confirm.vx",
        ".accept",
        "color",
        "components/Confirm.vx",
        ".accept:hover",
        "background",
        4.5,
    ),
    (
        "components/Confirm.vx",
        ".cancel",
        "color",
        "components/Confirm.vx",
        ".cancel",
        "background",
        4.5,
    ),
    (
        "components/Confirm.vx",
        ".cancel:hover",
        "color",
        "components/Confirm.vx",
        ".cancel:hover",
        "background",
        4.5,
    ),
    (
        "components/Modal.vx",
        ".body-text",
        "color",
        "components/Modal.vx",
        ".panel",
        "background",
        4.5,
    ),
    (
        "components/Modal.vx",
        ".dismiss",
        "color",
        "components/Modal.vx",
        ".dismiss",
        "background",
        4.5,
    ),
    (
        "components/Modal.vx",
        ".confirm",
        "color",
        "components/Modal.vx",
        ".confirm",
        "background",
        4.5,
    ),
    (
        "components/Modal.vx",
        ".confirm",
        "color",
        "components/Modal.vx",
        ".confirm:hover",
        "background",
        4.5,
    ),
    (
        "components/TodoInput.vx",
        ".input",
        "color",
        "components/TodoInput.vx",
        ".input",
        "background",
        4.5,
    ),
    (
        "components/TodoInput.vx",
        ".input::placeholder",
        "color",
        "components/TodoInput.vx",
        ".input",
        "background",
        4.5,
    ),
    (
        "components/TodoItem.vx",
        ".todo-text",
        "color",
        "components/TodoItem.vx",
        ".todo-item",
        "background",
        4.5,
    ),
    (
        "components/TodoItem.vx",
        ".check-mark",
        "color",
        "components/TodoItem.vx",
        ".completed .check",
        "background",
        4.5,
    ),
    (
        "components/TodoItem.vx",
        ".remove",
        "color",
        "components/TodoItem.vx",
        ".remove",
        "background",
        4.5,
    ),
    (
        "components/TodoItem.vx",
        ".remove:hover",
        "color",
        "components/TodoItem.vx",
        ".remove:hover",
        "background",
        4.5,
    ),
    (
        "components/TodoItem.vx",
        ".completed .todo-text",
        "color",
        "components/TodoItem.vx",
        ".todo-item",
        "background",
        4.5,
    ),
    (
        "components/Todos.vx",
        ".add",
        "color",
        "components/Todos.vx",
        ".add",
        "background",
        4.5,
    ),
    (
        "components/Todos.vx",
        ".empty",
        "color",
        "components/Todos.vx",
        ".empty",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".dark .app",
        "color",
        "App.vx",
        ".dark .app",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".dark .title",
        "color",
        "App.vx",
        ".dark .app",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".dark .tagline",
        "color",
        "App.vx",
        ".dark .app",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".dark .meta-count",
        "color",
        "App.vx",
        ".dark .app",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".dark .eyebrow",
        "color",
        "App.vx",
        ".dark .app",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".dark .footnote",
        "color",
        "App.vx",
        ".dark .app",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".dark .ghost",
        "color",
        "App.vx",
        ".dark .ghost",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".dark .toggle",
        "color",
        "App.vx",
        ".dark .toggle",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".dark .ghost:hover",
        "color",
        "App.vx",
        ".dark .ghost:hover",
        "background",
        4.5,
    ),
    (
        "App.vx",
        ".dark .toggle:hover",
        "color",
        "App.vx",
        ".dark .toggle:hover",
        "background",
        4.5,
    ),
    (
        "components/Confirm.vx",
        ".dark .panel",
        "color",
        "components/Confirm.vx",
        ".dark .panel",
        "background",
        4.5,
    ),
    (
        "components/Confirm.vx",
        ".dark .message",
        "color",
        "components/Confirm.vx",
        ".dark .panel",
        "background",
        4.5,
    ),
    (
        "components/Confirm.vx",
        ".dark .badge",
        "color",
        "components/Confirm.vx",
        ".dark .badge",
        "background",
        4.5,
    ),
    (
        "components/Confirm.vx",
        ".dark .accept",
        "color",
        "components/Confirm.vx",
        ".dark .accept",
        "background",
        4.5,
    ),
    (
        "components/Confirm.vx",
        ".dark .accept",
        "color",
        "components/Confirm.vx",
        ".dark .accept:hover",
        "background",
        4.5,
    ),
    (
        "components/Confirm.vx",
        ".dark .cancel",
        "color",
        "components/Confirm.vx",
        ".dark .cancel",
        "background",
        4.5,
    ),
    (
        "components/Confirm.vx",
        ".dark .cancel:hover",
        "color",
        "components/Confirm.vx",
        ".dark .cancel:hover",
        "background",
        4.5,
    ),
    (
        "components/Modal.vx",
        ".dark .title",
        "color",
        "components/Modal.vx",
        ".dark .panel",
        "background",
        4.5,
    ),
    (
        "components/Modal.vx",
        ".dark .body-text",
        "color",
        "components/Modal.vx",
        ".dark .panel",
        "background",
        4.5,
    ),
    (
        "components/Modal.vx",
        ".dark .dismiss",
        "color",
        "components/Modal.vx",
        ".dark .dismiss",
        "background",
        4.5,
    ),
    (
        "components/Modal.vx",
        ".dark .confirm",
        "color",
        "components/Modal.vx",
        ".dark .confirm",
        "background",
        4.5,
    ),
    (
        "components/Modal.vx",
        ".dark .confirm",
        "color",
        "components/Modal.vx",
        ".dark .confirm:hover",
        "background",
        4.5,
    ),
    (
        "components/TodoInput.vx",
        ".dark .input",
        "color",
        "components/TodoInput.vx",
        ".dark .input",
        "background",
        4.5,
    ),
    (
        "components/TodoInput.vx",
        ".dark .input::placeholder",
        "color",
        "components/TodoInput.vx",
        ".dark .input",
        "background",
        4.5,
    ),
    (
        "components/TodoItem.vx",
        ".dark .todo-text",
        "color",
        "components/TodoItem.vx",
        ".dark .todo-item",
        "background",
        4.5,
    ),
    (
        "components/TodoItem.vx",
        ".dark .check-mark",
        "color",
        "components/TodoItem.vx",
        ".dark .completed .check",
        "background",
        4.5,
    ),
    (
        "components/TodoItem.vx",
        ".dark .remove",
        "color",
        "components/TodoItem.vx",
        ".dark .remove",
        "background",
        4.5,
    ),
    (
        "components/TodoItem.vx",
        ".dark .remove:hover",
        "color",
        "components/TodoItem.vx",
        ".dark .remove:hover",
        "background",
        4.5,
    ),
    (
        "components/TodoItem.vx",
        ".dark .completed .todo-text",
        "color",
        "components/TodoItem.vx",
        ".dark .todo-item",
        "background",
        4.5,
    ),
    (
        "components/Todos.vx",
        ".dark .add",
        "color",
        "components/Todos.vx",
        ".dark .add",
        "background",
        4.5,
    ),
    (
        "components/Todos.vx",
        ".dark .add",
        "color",
        "components/Todos.vx",
        ".dark .add:hover",
        "background",
        4.5,
    ),
    (
        "components/Todos.vx",
        ".dark .empty",
        "color",
        "components/Todos.vx",
        ".dark .empty",
        "background",
        4.5,
    ),
];

/// SC 1.4.11 at 3:1 — the boundary of something you can operate.
///
/// For a FILLED control the boundary is its fill against what it sits on, not its
/// border against itself: `.accept` paints its border and its background the same
/// value, so pairing those two would score 1.0:1 and prove nothing.
const BOUNDARY_PAIRS: &[(&str, &str, &str, &str, &str, &str, f32)] = &[
    // The composer's edge IS the text field's edge now: the field declares no
    // border of its own, so this row replaces the `.input`/`border` pair it was
    // moved out of. It is a boundary rather than a decoration because this is the
    // one place on the page where an edge is a hit target.
    (
        "components/Todos.vx",
        ".composer",
        "border",
        "components/Todos.vx",
        ".composer",
        "background",
        3.0,
    ),
    (
        "App.vx",
        ".toggle",
        "border",
        "App.vx",
        ".app",
        "background",
        3.0,
    ),
    (
        "App.vx",
        ".ghost",
        "border",
        "App.vx",
        ".ghost",
        "background",
        3.0,
    ),
    (
        "App.vx",
        ".toggle:hover",
        "border",
        "App.vx",
        ".toggle:hover",
        "background",
        3.0,
    ),
    (
        "App.vx",
        ".ghost:hover",
        "border",
        "App.vx",
        ".ghost:hover",
        "background",
        3.0,
    ),
    (
        "components/Confirm.vx",
        ".cancel",
        "border",
        "components/Confirm.vx",
        ".cancel",
        "background",
        3.0,
    ),
    (
        "components/Confirm.vx",
        ".cancel:hover",
        "border",
        "components/Confirm.vx",
        ".cancel:hover",
        "background",
        3.0,
    ),
    (
        "components/Confirm.vx",
        ".accept",
        "background",
        "App.vx",
        ".app",
        "background",
        3.0,
    ),
    (
        "components/Confirm.vx",
        ".accept:hover",
        "background",
        "components/Confirm.vx",
        ".panel",
        "background",
        3.0,
    ),
    (
        "components/Modal.vx",
        ".confirm",
        "background",
        "App.vx",
        ".app",
        "background",
        3.0,
    ),
    (
        "components/Modal.vx",
        ".confirm:hover",
        "background",
        "App.vx",
        ".app",
        "background",
        3.0,
    ),
    (
        "components/Modal.vx",
        ".dismiss:hover",
        "border",
        "components/Modal.vx",
        ".dismiss:hover",
        "background",
        3.0,
    ),
    (
        "components/TodoItem.vx",
        ".check",
        "border",
        "components/TodoItem.vx",
        ".todo-item",
        "background",
        3.0,
    ),
    (
        "components/TodoItem.vx",
        ".check:hover",
        "border",
        "components/TodoItem.vx",
        ".todo-item",
        "background",
        3.0,
    ),
    (
        "components/TodoItem.vx",
        ".completed .check",
        "background",
        "components/TodoItem.vx",
        ".todo-item",
        "background",
        3.0,
    ),
    (
        "components/TodoItem.vx",
        ".remove:hover",
        "background",
        "components/TodoItem.vx",
        ".todo-item",
        "background",
        3.0,
    ),
    (
        "components/Todos.vx",
        ".add",
        "background",
        "App.vx",
        ".app",
        "background",
        3.0,
    ),
    (
        "components/Todos.vx",
        ".add:hover",
        "background",
        "App.vx",
        ".app",
        "background",
        3.0,
    ),
    (
        "App.vx",
        ".dark .toggle",
        "border",
        "App.vx",
        ".dark .app",
        "background",
        3.0,
    ),
    (
        "App.vx",
        ".dark .ghost",
        "border",
        "App.vx",
        ".dark .app",
        "background",
        3.0,
    ),
    (
        "App.vx",
        ".dark .toggle:hover",
        "border",
        "App.vx",
        ".dark .toggle:hover",
        "background",
        3.0,
    ),
    (
        "App.vx",
        ".dark .ghost:hover",
        "border",
        "App.vx",
        ".dark .ghost:hover",
        "background",
        3.0,
    ),
    (
        "components/Confirm.vx",
        ".dark .cancel",
        "border",
        "components/Confirm.vx",
        ".dark .panel",
        "background",
        3.0,
    ),
    (
        "components/Confirm.vx",
        ".dark .cancel:hover",
        "border",
        "components/Confirm.vx",
        ".dark .cancel:hover",
        "background",
        3.0,
    ),
    (
        "components/Confirm.vx",
        ".dark .accept",
        "background",
        "components/Confirm.vx",
        ".dark .panel",
        "background",
        3.0,
    ),
    (
        "components/Confirm.vx",
        ".dark .accept:hover",
        "background",
        "components/Confirm.vx",
        ".dark .panel",
        "background",
        3.0,
    ),
    (
        "components/Modal.vx",
        ".dark .confirm",
        "background",
        "components/Modal.vx",
        ".dark .panel",
        "background",
        3.0,
    ),
    (
        "components/Modal.vx",
        ".dark .dismiss:hover",
        "border",
        "components/Modal.vx",
        ".dark .dismiss:hover",
        "background",
        3.0,
    ),
    (
        "components/Todos.vx",
        ".dark .composer",
        "border",
        "components/Todos.vx",
        ".dark .composer",
        "background",
        3.0,
    ),
    (
        "components/TodoItem.vx",
        ".dark .check",
        "border",
        "components/TodoItem.vx",
        ".dark .todo-item",
        "background",
        3.0,
    ),
    (
        "components/TodoItem.vx",
        ".dark .check:hover",
        "border",
        "components/TodoItem.vx",
        ".dark .todo-item",
        "background",
        3.0,
    ),
    (
        "components/TodoItem.vx",
        ".dark .completed .check",
        "background",
        "components/TodoItem.vx",
        ".dark .todo-item",
        "background",
        3.0,
    ),
    (
        "components/TodoItem.vx",
        ".dark .remove:hover",
        "background",
        "components/TodoItem.vx",
        ".dark .todo-item",
        "background",
        3.0,
    ),
    (
        "components/Todos.vx",
        ".dark .add",
        "background",
        "components/Todos.vx",
        ".dark .composer",
        "background",
        3.0,
    ),
    (
        "components/Todos.vx",
        ".dark .add:hover",
        "background",
        "components/Todos.vx",
        ".dark .composer",
        "background",
        3.0,
    ),
];

/// The quiet edges, held only to `DECORATIVE_FLOOR`.
const DECORATIVE_PAIRS: &[(&str, &str, &str, &str, &str, &str, f32)] = &[
    (
        "App.vx",
        ".rule",
        "background",
        "App.vx",
        ".app",
        "background",
        DECORATIVE_FLOOR,
    ),
    (
        "components/Confirm.vx",
        ".panel",
        "border",
        "components/Confirm.vx",
        ".panel",
        "background",
        DECORATIVE_FLOOR,
    ),
    (
        "components/Modal.vx",
        ".panel",
        "border",
        "components/Modal.vx",
        ".panel",
        "background",
        DECORATIVE_FLOOR,
    ),
    (
        "components/TodoItem.vx",
        ".todo-item",
        "border",
        "components/TodoItem.vx",
        ".todo-item",
        "background",
        DECORATIVE_FLOOR,
    ),
    (
        "components/Todos.vx",
        ".empty",
        "border",
        "components/Todos.vx",
        ".empty",
        "background",
        DECORATIVE_FLOOR,
    ),
    (
        "components/Confirm.vx",
        ".badge",
        "border",
        "components/Confirm.vx",
        ".badge",
        "background",
        DECORATIVE_FLOOR,
    ),
    (
        "App.vx",
        ".dark .rule",
        "background",
        "App.vx",
        ".dark .app",
        "background",
        DECORATIVE_FLOOR,
    ),
    (
        "components/Confirm.vx",
        ".dark .panel",
        "border",
        "components/Confirm.vx",
        ".dark .panel",
        "background",
        DECORATIVE_FLOOR,
    ),
    (
        "components/Modal.vx",
        ".dark .panel",
        "border",
        "components/Modal.vx",
        ".dark .panel",
        "background",
        DECORATIVE_FLOOR,
    ),
    (
        "components/TodoItem.vx",
        ".dark .todo-item",
        "border",
        "components/TodoItem.vx",
        ".dark .todo-item",
        "background",
        DECORATIVE_FLOOR,
    ),
    (
        "components/Todos.vx",
        ".dark .empty",
        "border",
        "components/Todos.vx",
        ".dark .empty",
        "background",
        DECORATIVE_FLOOR,
    ),
    (
        "components/Confirm.vx",
        ".dark .badge",
        "border",
        "components/Confirm.vx",
        ".dark .badge",
        "background",
        DECORATIVE_FLOOR,
    ),
];

/// The six places one value deliberately serves two roles. Any other overlap is a
/// coincidence that will drift apart, and fails.
const ALLOWED_SHARINGS: &[(&str, &str, &[&str])] = &[
    ("light", "#0d6e66", &["accent-light", "text-accent-light"]),
    (
        "light",
        "#8a2a23",
        &["danger-hover-light", "text-danger-light"],
    ),
    ("light", "#ffffff", &["card-light", "on-filled-light"]),
    ("dark", "#2DD4BF", &["accent-dark", "text-accent-dark"]),
    (
        "dark",
        "#313B42",
        &["border-decorative-dark", "divider-dark"],
    ),
    ("dark", "#FF8A80", &["danger-dark", "text-danger-dark"]),
];

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Decl {
    file: String,
    selector: String,
    prop: String,
    value: String,
    dark: bool,
}

impl Decl {
    fn key(&self) -> (&str, &str, &str, &str) {
        (
            self.file.as_str(),
            self.selector.as_str(),
            self.prop.as_str(),
            if self.dark { "dark" } else { "light" },
        )
    }
}

fn template_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/project/src")
}

fn strip_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        match rest[i + 2..].find("*/") {
            Some(j) => rest = &rest[i + 2 + j + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// The `<style scoped>` block of a `.vx` file, comments removed. The sheets
/// document themselves heavily, and a commented-out rule must not read as a live
/// one.
fn style_block(src: &str) -> String {
    let open = src.find("<style").expect("a <style> block");
    let after = open + src[open..].find('>').expect("a closed <style> tag") + 1;
    let close = after + src[after..].find("</style>").expect("a closed </style>");
    strip_comments(&src[after..close])
}

/// The colour out of a `border` shorthand (`1px solid #868D92` -> `#868D92`, a
/// shorthand with no colour -> `transparent`), with inner spacing collapsed so an
/// `rgba()` compares the way it is written.
fn normalise(prop: &str, value: &str) -> String {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if !(prop.starts_with("border") || prop.starts_with("outline")) {
        return value;
    }
    let colour: Vec<&str> = value
        .split(' ')
        .filter(|t| {
            !t.starts_with(|c: char| c.is_ascii_digit())
                && !matches!(*t, "solid" | "dashed" | "none" | "inset")
        })
        .collect();
    if colour.is_empty() {
        "transparent".to_string()
    } else {
        colour.join(" ")
    }
}

/// Every colour-bearing declaration in the six sheets.
fn sheets() -> Vec<Decl> {
    let dir = template_dir();
    let mut out = Vec::new();
    for rel in SHEETS {
        let src = fs::read_to_string(dir.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
        for rule in style_block(&src).split('}') {
            let Some((selector, decls)) = rule.split_once('{') else {
                continue;
            };
            let selector = selector.split_whitespace().collect::<Vec<_>>().join(" ");
            for decl in decls.split(';') {
                let Some((prop, value)) = decl.split_once(':') else {
                    continue;
                };
                let (prop, value) = (prop.trim().to_string(), value.trim().to_string());
                if prop.is_empty() || !COLOUR_PROPS.contains(&prop.as_str()) {
                    continue;
                }
                out.push(Decl {
                    file: (*rel).to_string(),
                    dark: selector.starts_with(".dark "),
                    selector: selector.clone(),
                    prop: prop.clone(),
                    value: normalise(&prop, &value),
                });
            }
        }
    }
    out
}

/// The declared value of one property on one selector in one sheet.
///
/// Panics when it is missing, and panics when the same property is declared
/// twice on the same selector. That is what stops the pair tables above from
/// rotting into checking something that is not there, or checking an ambiguous
/// one.
fn value_of(sheets: &[Decl], file: &str, selector: &str, prop: &str) -> String {
    let hits: Vec<&Decl> = sheets
        .iter()
        .filter(|d| d.file == file && d.selector == selector && d.prop == prop)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one `{prop}` on `{selector}` in {file}, found {}",
        hits.len()
    );
    hits[0].value.clone()
}

fn srgb_to_linear(c: f32) -> f32 {
    let c = c / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn channel(hex: &str, at: usize) -> f32 {
    u8::from_str_radix(&hex[at..at + 2], 16)
        .unwrap_or_else(|e| panic!("{hex:?} is not 6-digit hex at offset {at}: {e}")) as f32
}

/// WCAG 2.1 relative luminance.
///
/// Panics on anything that is not an opaque `#rrggbb`, so a translucent scrim can
/// never be scored as though it were solid.
fn relative_luminance(hex: &str) -> f32 {
    assert!(
        hex.len() == 7 && hex.starts_with('#'),
        "{hex:?} is not an opaque #rrggbb colour; a translucent value has no single \\
         luminance and must not be compared as one"
    );
    0.2126 * srgb_to_linear(channel(hex, 1))
        + 0.7152 * srgb_to_linear(channel(hex, 3))
        + 0.0722 * srgb_to_linear(channel(hex, 5))
}

fn contrast(a: &str, b: &str) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

fn assert_pairs(sheets: &[Decl], pairs: &[(&str, &str, &str, &str, &str, &str, f32)], sc: &str) {
    let mut failures = Vec::new();
    for &(ff, fs, fp, bf, bs, bp, min) in pairs {
        let fg = value_of(sheets, ff, fs, fp);
        let bg = value_of(sheets, bf, bs, bp);
        let got = contrast(&fg, &bg);
        // 0.005 of slack: the ratios here are all well clear of their floor, and
        // this only stops a last-bit rounding difference failing a build.
        if got + 0.005 < min {
            failures.push(format!(
                "{sc}: `{fp}` of `{fs}` ({fg}) on `{bp}` of `{bs}` ({bg}) is {got:.2}:1, \
                 needs {min:.2}:1"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} pairs fail {sc}:\n  {}",
        failures.len(),
        pairs.len(),
        failures.join("\n  ")
    );
}

/// Every colour in the template has a role, and every role row still exists.
///
/// Both directions matter. Without the forward one a newly added colour would go
/// unchecked; without the reverse one a renamed selector would leave a role row
/// pointing at nothing while `one_value_per_role` went on passing it.
#[test]
fn every_colour_declaration_has_a_role() {
    let sheets = sheets();
    let declared: BTreeSet<(&str, &str, &str, &str)> = sheets.iter().map(Decl::key).collect();
    let classified: BTreeSet<(&str, &str, &str, &str)> = ROLES
        .iter()
        .map(|&(f, s, p, mode, _, _)| (f, s, p, mode))
        .collect();

    let unclassified: Vec<_> = declared.difference(&classified).collect();
    assert!(
        unclassified.is_empty(),
        "these colour declarations have no role, so no test constrains them:\n  {:#?}",
        unclassified
    );
    let stale: Vec<_> = classified.difference(&declared).collect();
    assert!(
        stale.is_empty(),
        "these role rows match no declaration — a selector or property was \
         renamed or dropped:\n  {:#?}",
        stale
    );
    assert_eq!(
        ROLES.len(),
        declared.len(),
        "ROLES has {} rows for {} declarations, so a duplicate row is standing in \
         for a missing one",
        ROLES.len(),
        declared.len()
    );
    for &(file, selector, prop, _mode, role, value) in ROLES {
        let live = value_of(&sheets, file, selector, prop);
        assert_eq!(
            live, value,
            "{role}: `{prop}` of `{selector}` in {file} is {live}, not the {value} this \
             table recorded"
        );
    }
}

/// No colour token may hide in a property none of these tests read.
#[test]
fn no_colour_hides_in_an_unread_property() {
    let dir = template_dir();
    for rel in SHEETS {
        let src = fs::read_to_string(dir.join(rel)).expect("read a .vx file");
        for rule in style_block(&src).split('}') {
            let Some((selector, decls)) = rule.split_once('{') else {
                continue;
            };
            let selector = selector.split_whitespace().collect::<Vec<_>>().join(" ");
            for decl in decls.split(';') {
                let Some((prop, value)) = decl.split_once(':') else {
                    continue;
                };
                let (prop, value) = (prop.trim(), value.trim());
                let is_colour =
                    value.contains('#') || value.starts_with("rgb") || value == "transparent";
                assert!(
                    !is_colour || COLOUR_PROPS.contains(&prop),
                    "{rel}: `{selector} {{ {prop}: {value} }}` puts a colour in a property \
                     none of the contrast tests read — add it to COLOUR_PROPS and classify \
                     it, or drop the declaration"
                );
            }
        }
    }
}

/// The rot T6 cleaned up: one role, one value per mode.
#[test]
fn one_value_per_role() {
    let mut by_role: BTreeMap<(&str, &str), BTreeSet<&str>> = BTreeMap::new();
    let mut by_value: BTreeMap<(&str, &str), BTreeSet<&str>> = BTreeMap::new();
    for &(_f, _s, _p, mode, role, value) in ROLES {
        by_role.entry((role, mode)).or_default().insert(value);
        by_value.entry((mode, value)).or_default().insert(role);
    }

    let split: Vec<String> = by_role
        .iter()
        .filter(|(_, values)| values.len() != 1)
        .map(|(k, values)| format!("{k:?} is spelled {values:?}"))
        .collect();
    assert!(
        split.is_empty(),
        "a role with more than one value is the drift this task exists to stop — two \
         files disagreeing about one interaction:\n  {}",
        split.join("\n  ")
    );

    let unexpected: Vec<String> = by_value
        .iter()
        .filter(|(key, roles)| {
            roles.len() > 1
                && !ALLOWED_SHARINGS
                    .iter()
                    .any(|&(m, v, _)| (key.0, key.1) == (m, v))
        })
        .map(|(k, roles)| format!("{k:?} is used by {roles:?}"))
        .collect();
    assert!(
        unexpected.is_empty(),
        "these values serve more than one role without appearing in ALLOWED_SHARINGS:\n  {}",
        unexpected.join("\n  ")
    );
}

/// WCAG 2.1 SC 1.4.3 — body text at 4.5:1. Large-text relief is not claimed.
#[test]
fn every_text_pair_clears_four_five_to_one() {
    assert_pairs(&sheets(), TEXT_PAIRS, "SC 1.4.3 (4.5:1 body text)");
}

/// WCAG 2.1 SC 1.4.11 — the boundary of a control at 3:1.
#[test]
fn every_control_boundary_clears_three_to_one() {
    assert_pairs(
        &sheets(),
        BOUNDARY_PAIRS,
        "SC 1.4.11 (3:1 non-text contrast)",
    );
}

/// The quiet edges, held only to "you can see it".
#[test]
fn decorative_edges_clear_their_documented_floor() {
    assert_pairs(&sheets(), DECORATIVE_PAIRS, "the decorative floor");
}

/// A scrim carries no text, so it has no ratio — but it must actually dim, and the
/// two overlays must dim by the same amount or one dialog will read differently
/// from the other for no reason.
#[test]
fn the_scrims_are_not_transparent() {
    let sheets = sheets();
    let alpha_of = |v: &str| -> f32 {
        let inner = v
            .strip_prefix("rgba(")
            .and_then(|s| s.strip_suffix(')'))
            .unwrap_or_else(|| panic!("{v:?} is not an rgba() colour"));
        inner
            .split(',')
            .next_back()
            .expect("an alpha channel")
            .trim()
            .parse()
            .unwrap_or_else(|e| panic!("{v:?} has an unparseable alpha: {e}"))
    };

    let mut alphas = Vec::new();
    for rel in ["components/Modal.vx", "components/Confirm.vx"] {
        let dark = value_of(&sheets, rel, ".dark .overlay", "background");
        assert!(
            alpha_of(&dark) >= 0.7,
            "{rel} `.dark .overlay` is {dark:?}; dark mode raises the scrim on purpose \
             so a bright page cannot glare through it"
        );
        let light = value_of(&sheets, rel, ".overlay", "background");
        assert!(
            alpha_of(&light) >= 0.3,
            "{rel} `.overlay` is {light:?}, too transparent to separate a dialog from \
             the page"
        );
        alphas.push(alpha_of(&dark) - alpha_of(&light));
    }
    assert_eq!(
        alphas[0], alphas[1],
        "the two overlays differ in how much they dim: {alphas:?}"
    );
}

/// Every dark rule must come AFTER the light rule it beats.
///
/// Velox's cascade has no specificity and no `!important` — last match wins
/// (`velox-style/src/lib.rs` `apply_styles_with_hover`, cascade loop at :840) — so
/// a `.dark .x` rule written above `.x` is dead code, and the app silently renders
/// light. A screenshot would show it; this names it.
#[test]
fn every_dark_rule_comes_after_the_light_rule_it_beats() {
    let dir = template_dir();
    let mut checked = 0usize;
    for rel in SHEETS {
        let src = fs::read_to_string(dir.join(rel)).expect("read a .vx file");
        // Selectors in source order, comments removed so a commented-out rule does
        // not count as a live one.
        let mut order: Vec<String> = Vec::new();
        for rule in style_block(&src).split('}') {
            let Some((selector, _)) = rule.split_once('{') else {
                continue;
            };
            order.push(selector.split_whitespace().collect::<Vec<_>>().join(" "));
        }
        for (i, dark) in order.iter().enumerate() {
            let Some(light) = dark.strip_prefix(".dark ") else {
                continue;
            };
            if let Some(j) = order.iter().position(|c| c == light) {
                assert!(
                    j < i,
                    "{rel}: `{dark}` is at position {i} but the `{light}` it beats is \
                     at {j} — with no specificity the light rule wins and the dark one \
                     never paints"
                );
            }
            checked += 1;
        }
    }
    assert!(
        checked >= 40,
        "only {checked} dark rules were checked; the six sheets carry far more, so \
         this test is not looking at the sheets it thinks it is"
    );
}
