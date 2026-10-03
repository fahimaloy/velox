//! One shared content-box geometry for `<input>`.
//!
//! # Why this module exists
//!
//! It used to be that four places each answered "where does the first glyph of
//! an `<input>` go?" and all four disagreed:
//!
//! | lane | answer it produced |
//! |------|--------------------|
//! | paint, text origin | `rect.x + 5.0` (hardcoded `TEXT_PAD = 4.0` + a hardcoded 1px border inset) |
//! | paint, selection/caret x | the same two constants, recomputed at the call site |
//! | hit test, class-based `padding` | `rect.x + 0` — the cascade stores the authored `padding` shorthand under the key `padding`, and the lookup read the literal key `padding-left`, which is never present |
//! | hit test, inline `padding-left: 12px` | `rect.x + 12` |
//!
//! and the font size was worse: the paint lane inherited the root default of
//! `14.0` while the hit-test lane carried its own `DEFAULT_INPUT_FONT_SIZE =
//! 16.0`.
//!
//! The root cause of the padding half was that every lane **re-parsed the
//! authored style string**. The cascade does not expand shorthands —
//! `merge_styles` in `velox-style` writes the author's own property names — so
//! `padding: 10px 12px` survives to paint as the four-part shorthand and a
//! lookup for `padding-left` misses it. Reading **computed** values instead
//! makes `padding`, `padding-left` and `12px` the same declaration, which is
//! the only way they can be made to behave identically.
//!
//! Every one of those four answers is now a call to [`input_text_metrics`]
//! below. The hit-test lane stopped last and for the same reason: it kept its
//! own `padding-left` lookup long after paint had moved here, so a field
//! styled with the `padding` shorthand put its caret a whole left-padding left
//! of the glyph it claimed to be on. It now calls this function with the same
//! border box, the same logical viewport and the same inherited font size
//! paint uses, so the two cannot diverge again.
//!
//! # Contract
//!
//! [`input_text_metrics`] is the single authority. Both the paint lane
//! (`skia_render.rs`) and the hit-test lane (`lib.rs::resolve_text_origin_x`)
//! call it; neither computes an inset of its own. Anything that needs a glyph
//! origin, a caret x, a selection edge or a content width must read it from
//! here.
//!
//! # Coordinate space
//!
//! Everything is **logical px**, matching `velox_dom::layout::Rect` and the
//! logical rects the paint lane draws with. `border_box` is the element's
//! border box as `compute_layout` produced it — the very rect the renderer
//! paints — not a re-derived box.

use velox_dom::layout::{DEFAULT_ROOT_FONT_SIZE, Rect};
use velox_dom::style::ComputedStyle;

/// Horizontal padding the painter applies when **no** padding is specified at
/// all — neither by the author nor by the UA sheet.
///
/// This is a Velox fallback, not a CSS claim. `ua.css` gives every cascaded
/// `<input>` a real padding, so this only survives for an input that reached
/// the painter without going through the cascade (a bare `VNode` handed
/// straight to `render_vnode_to_rgba` with no `style` attribute). It is the
/// value the old hardcoded `TEXT_PAD = 4.0` used, kept so a bare field does
/// not collapse its first glyph onto the border.
pub const DEFAULT_INPUT_TEXT_PADDING: f32 = 4.0;

/// The resolved geometry of an `<input>`'s padding box and text area.
///
/// All fields are logical px. See [`input_text_metrics`] for how each is
/// derived.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputTextMetrics {
    /// Border box deflated by the resolved border widths — the **padding box**.
    /// This is what a fill and a clip should use, NOT the border box: clipping
    /// to the border box lets a long value's caret and selection paint over
    /// the border.
    pub pad_left: f32,
    /// `<see cref="Self::pad_left"/>` for the top edge.
    pub pad_top: f32,
    /// `<see cref="Self::pad_left"/>` for the right edge.
    pub pad_right: f32,
    /// `<see cref="Self::pad_left"/>` for the bottom edge.
    pub pad_bottom: f32,

    /// **x of the first glyph**: `pad_left + padding_left`. Also the x the
    /// caret sits at for character index 0, and the left edge of a selection
    /// that starts at index 0.
    pub text_left: f32,
    /// **Rightmost x a glyph may occupy**: `pad_right - padding_right`, never
    /// less than [`Self::text_left`]. A caret whose measured advance runs past
    /// this clamps here, landing it flat on the content edge rather than
    /// scrolling the text.
    pub content_right: f32,

    /// Resolved border widths, read from the computed `border-width` sides (so
    /// `border: 2px solid red` yields 2 on all four). A side whose
    /// `border-style` is `none`/`hidden` contributes **0**, per CSS 2.1
    /// §8.5.2: a `none` border is not drawn and occupies no space.
    pub border_left: f32,
    /// `<see cref="Self::border_left"/>` for the top edge.
    pub border_top: f32,
    /// `<see cref="Self::border_left"/>` for the right edge.
    pub border_right: f32,
    /// `<see cref="Self::border_left"/>` for the bottom edge.
    pub border_bottom: f32,

    /// Resolved padding, left. `padding: 10px 12px` and `padding-left: 12px`
    /// both land here as `12.0`.
    pub padding_left: f32,
    /// Resolved padding, right.
    pub padding_right: f32,
    /// Resolved padding, top.
    pub padding_top: f32,
    /// Resolved padding, bottom.
    pub padding_bottom: f32,

    /// The element's **own** computed `font-size` in px: its declaration
    /// resolved against `inherited_font_size`, or `inherited_font_size` itself
    /// when it declares none. Callers must pass the same inherited size to
    /// paint and to hit-test, or the two lanes measure different glyphs again.
    pub font_size: f32,
}

impl InputTextMetrics {
    /// Width available to the value text, px. Never negative.
    pub fn text_width(&self) -> f32 {
        (self.content_right - self.text_left).max(0.0)
    }

    /// Height of the padding box, px. Never negative.
    pub fn content_height(&self) -> f32 {
        (self.pad_bottom - self.pad_top).max(0.0)
    }

    /// The padding box as a tuple `(left, top, right, bottom)`, for a caller
    /// that wants to build its own rect type from it.
    pub fn padding_box(&self) -> (f32, f32, f32, f32) {
        (self.pad_left, self.pad_top, self.pad_right, self.pad_bottom)
    }
}

/// Resolve the padding box and text area of an `<input>`, in logical px.
///
/// # Arguments
///
/// * `style` — the element's **cascaded** `style` attribute, i.e. the merged
///   UA + author + inline declaration string `velox_style`'s cascade wrote onto
///   the VNode. `None` or `""` is fine and yields the fallback padding.
///   Shorthands are expanded *by the DOM's own `ComputedStyle::set_property`*,
///   which is what makes `padding`, `padding-left` and `12px` agree.
/// * `border_box` — the element's border box, straight out of
///   `compute_layout`. Do not deflate it first; this function owns the insets.
/// * `viewport` — the **logical** viewport `(w, h)`, for `vw`/`vh` units. The
///   same pair the paint lane already threads through `parse_style_attr` for
///   `border-width`.
/// * `inherited_font_size` — the px font size this element inherits from its
///   parent, for resolving an `em`/`%` `padding`/`font-size`. The paint lane
///   has it (`TextStyle::font_size`); a hit-test lane with no inheritance
///   context passes [`DEFAULT_ROOT_FONT_SIZE`].
///
/// # Why not re-parse the authored string
///
/// See the module docs. Reading computed values is the entire fix for the
/// shorthand hole; a second hand-rolled parser here would reopen it.
///
/// # Stability
///
/// This is the cross-lane contract. The paint lane and the hit-test lane must
/// both call this function rather than adding an inset of their own, or the
/// "caret is not where the text is" defect returns in a new shape.
pub fn input_text_metrics(
    style: Option<&str>,
    border_box: Rect,
    viewport: (f32, f32),
    inherited_font_size: f32,
) -> InputTextMetrics {
    let mut computed = ComputedStyle::new();
    if let Some(s) = style
        && !s.trim().is_empty()
    {
        computed.apply_inline_style(s);
    }

    // Relative lengths resolve against the element's OWN font size (CSS 2.1
    // §6.7), which is itself the declaration resolved against the parent's.
    let font_size = resolve_font_size(style, &computed, inherited_font_size, viewport);

    let px = |len: &velox_dom::style::Length| -> f32 {
        let v = len.to_px(font_size, DEFAULT_ROOT_FONT_SIZE, viewport);
        // A non-finite or negative length would poison every downstream rect
        // (and `Length::Auto` is meaningless as a padding); 0 is the only
        // safe answer.
        if v.is_finite() && v > 0.0 { v } else { 0.0 }
    };

    let mut padding_left = px(&computed.padding.left);
    let mut padding_right = px(&computed.padding.right);
    let padding_top = px(&computed.padding.top);
    let padding_bottom = px(&computed.padding.bottom);

    // Fallback for an input that specified NO padding on any side — shorthand
    // or longhand. Gated on the DECLARATION being absent rather than on the
    // resolved values being zero, so `padding: 0` (an explicit request for a
    // glyph flush against the border) is honoured instead of being overridden
    // by an inset the author did not ask for.
    if !declares_padding(style) {
        padding_left = DEFAULT_INPUT_TEXT_PADDING;
        padding_right = DEFAULT_INPUT_TEXT_PADDING;
    }

    let bw = |len: &velox_dom::style::Length, style_side: velox_dom::style::BorderStyle| -> f32 {
        // CSS 2.1 §8.5.2: a `none`/`hidden` border is not drawn and occupies
        // no space. This also covers `border-width: 1px` alone, which leaves
        // the style at its `BorderStyle::None` initial value.
        if matches!(
            style_side,
            velox_dom::style::BorderStyle::None | velox_dom::style::BorderStyle::Hidden
        ) {
            return 0.0;
        }
        px(len)
    };
    let border_left = bw(&computed.border.width.left, computed.border.style.left);
    let border_right = bw(&computed.border.width.right, computed.border.style.right);
    let border_top = bw(&computed.border.width.top, computed.border.style.top);
    let border_bottom = bw(&computed.border.width.bottom, computed.border.style.bottom);

    let left = border_box.x as f32;
    let top = border_box.y as f32;
    let right = left + border_box.w.max(0) as f32;
    let bottom = top + border_box.h.max(0) as f32;

    let pad_left = (left + border_left).min(right);
    let pad_right = (right - border_right).max(pad_left);
    let pad_top = (top + border_top).min(bottom);
    let pad_bottom = (bottom - border_bottom).max(pad_top);

    let text_left = (pad_left + padding_left).min(pad_right);
    let text_right = (pad_right - padding_right).max(text_left);

    InputTextMetrics {
        pad_left,
        pad_top,
        pad_right,
        pad_bottom,
        text_left,
        content_right: text_right,
        border_left,
        border_top,
        border_right,
        border_bottom,
        padding_left,
        padding_right,
        padding_top,
        padding_bottom,
        font_size,
    }
}

/// The element's own `font-size` in px, resolved against the inherited size.
///
/// Mirrors `parse_text_style`'s `font-size` arm (including its rejection of
/// `auto`, non-positive and non-finite results) so the paint lane's inherited
/// `TextStyle::font_size` and this function's answer are the same number for
/// the same declaration.
fn resolve_font_size(
    style: Option<&str>,
    computed: &ComputedStyle,
    inherited_font_size: f32,
    viewport: (f32, f32),
) -> f32 {
    // `ComputedStyle::default()` seeds `font_size` with a hardcoded
    // `Length::Px(16.0)`, NOT with "unset". So reading `computed.font_size`
    // unconditionally makes an element that declared no `font-size` at all
    // report 16px — which is exactly the `DEFAULT_INPUT_FONT_SIZE = 16.0` vs
    // inherited-`14.0` disagreement this module exists to end, re-created one
    // level down. Presence therefore has to be checked, and the computed value
    // is only trusted once the declaration is really there.
    if !declares_font_size(style) {
        return inherited_font_size;
    }
    let own = computed
        .font_size
        .to_px(inherited_font_size, DEFAULT_ROOT_FONT_SIZE, viewport);
    if own.is_finite() && own > 0.0 {
        own.max(1.0)
    } else {
        inherited_font_size
    }
}

/// Whether the cascaded declaration string contains any `font-size` declaration.
fn declares_font_size(style: Option<&str>) -> bool {
    declared_keys(style).any(|k| k.eq_ignore_ascii_case("font-size"))
}

/// Whether the cascaded declaration string contains any padding declaration —
/// the `padding` shorthand OR any of its four longhands.
///
/// This distinction is load-bearing: `padding: 0` is an explicit request for a
/// glyph flush against the border, and must not trigger the
/// [`DEFAULT_INPUT_TEXT_PADDING`] fallback. Only an input that declared NO
/// padding on any side gets the fallback.
fn declares_padding(style: Option<&str>) -> bool {
    declared_keys(style).any(|k| {
        k.eq_ignore_ascii_case("padding")
            || k.eq_ignore_ascii_case("padding-top")
            || k.eq_ignore_ascii_case("padding-right")
            || k.eq_ignore_ascii_case("padding-bottom")
            || k.eq_ignore_ascii_case("padding-left")
    })
}

/// The property names in a cascaded declaration string, lowercased.
///
/// Used ONLY to distinguish **declared** from **unset**; every value still
/// comes from `ComputedStyle`, which is what expands shorthands. The scan is
/// deliberately naive (first `:` splits, exactly as every other reader in this
/// workspace does) because a false negative here costs a fallback, and a false
/// positive costs only the fallback's absence.
fn declared_keys(style: Option<&str>) -> impl Iterator<Item = String> + '_ {
    style
        .into_iter()
        .flat_map(|s| s.split(';'))
        .filter_map(|d| {
            d.trim()
                .split_once(':')
                .map(|(k, _)| k.trim().to_ascii_lowercase())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: Rect = Rect {
        x: 20,
        y: 30,
        w: 200,
        h: 40,
    };

    const VIEW: (f32, f32) = (800.0, 600.0);

    fn m(style: Option<&str>) -> InputTextMetrics {
        input_text_metrics(style, BOX, VIEW, 14.0)
    }

    #[test]
    fn no_style_falls_back_to_the_documented_padding() {
        let r = m(None);
        assert_eq!(r.text_left, 20.0 + DEFAULT_INPUT_TEXT_PADDING);
        assert_eq!(r.content_right, 220.0 - DEFAULT_INPUT_TEXT_PADDING);
        assert_eq!(r.text_width(), 200.0 - 2.0 * DEFAULT_INPUT_TEXT_PADDING);
        assert_eq!(r.font_size, 14.0);
    }

    #[test]
    fn border_width_deflates_the_padding_box() {
        let r = m(Some("border: 2px solid red"));
        assert_eq!(r.border_left, 2.0);
        assert_eq!(r.pad_left, 22.0);
        assert_eq!(r.pad_right, 218.0);
        assert_eq!(r.text_left, 26.0);
        assert_eq!(r.pad_bottom, 68.0);
    }

    #[test]
    fn none_border_style_occupies_no_space() {
        // CSS 2.1 8.5.2: a `none` border is not drawn and takes no space.
        let r = m(Some("border: 3px none red"));
        assert_eq!(r.border_left, 0.0);
        assert_eq!(r.pad_left, 20.0);
    }

    #[test]
    fn shorthand_and_longhand_padding_agree() {
        let shorthand = m(Some("border: 1px solid #333; padding: 10px 12px"));
        let longhand = m(Some(
            "border: 1px solid #333; padding-left: 12px; padding-right: 12px",
        ));
        // The defect being fixed: the hit-test lane looked up this literal key
        // and got nothing, so `padding-left: 12px` placed the caret at
        // `rect.x + 0` while `padding: 10px 12px` placed it at `rect.x + 12`.
        let inline_left_only = m(Some("border: 1px solid #333; padding-left: 12px"));
        assert_eq!(shorthand.text_left, longhand.text_left);
        assert_eq!(shorthand.text_left, inline_left_only.text_left);
        assert_eq!(shorthand.content_right, longhand.content_right);
        // `padding-left` alone leaves `padding-right` at the initial 0, so the
        // CONTENT WIDTH legitimately differs from the shorthand. Only the text
        // ORIGIN — which is what the click-to-caret lane needs — has to agree.
        assert_eq!(shorthand.padding_left, inline_left_only.padding_left);
        assert_eq!(inline_left_only.padding_right, 0.0);
        assert_eq!(shorthand.padding_right, 12.0);
    }

    #[test]
    fn the_shorthand_is_what_the_old_lookup_missed() {
        // `padding: 10px 12px` -> left/right 12, top/bottom 10.
        let r = m(Some("border: 1px solid #333; padding: 10px 12px"));
        assert_eq!(r.padding_left, 12.0);
        assert_eq!(r.padding_right, 12.0);
        assert_eq!(r.padding_top, 10.0);
        assert_eq!(r.padding_bottom, 10.0);
        assert_eq!(r.text_left, 21.0 + 12.0);
    }

    #[test]
    fn four_value_shorthand() {
        let r = m(Some("padding: 1px 2px 3px 4px"));
        assert_eq!(r.padding_top, 1.0);
        assert_eq!(r.padding_right, 2.0);
        assert_eq!(r.padding_bottom, 3.0);
        assert_eq!(r.padding_left, 4.0);
        // CSS 2.1 §8.3.3 four-value form is top / right / bottom / LEFT.
        assert_eq!(r.text_left, BOX.x as f32 + 4.0);
        assert_eq!(r.content_right, BOX.x as f32 + BOX.w as f32 - 2.0);
    }

    #[test]
    fn explicit_zero_padding_is_honoured() {
        // `padding: 0` is a real declaration; the fallback must not fire.
        let r = m(Some("padding: 0"));
        assert_eq!(r.text_left, 20.0);
        assert_eq!(r.content_right, 220.0);
    }

    #[test]
    fn em_padding_resolves_against_the_elements_own_font_size() {
        let r = input_text_metrics(Some("font-size: 20px; padding: 0 1em"), BOX, VIEW, 14.0);
        assert_eq!(r.font_size, 20.0);
        assert_eq!(r.padding_left, 20.0);
        assert_eq!(r.text_left, 40.0);
    }

    #[test]
    fn font_size_declared_in_em_resolves_against_the_parent() {
        let r = input_text_metrics(Some("font-size: 2em"), BOX, VIEW, 10.0);
        assert_eq!(r.font_size, 20.0);
    }

    #[test]
    fn degenerate_font_size_falls_back_to_the_inherited_one() {
        for bad in ["font-size: 0", "font-size: -4px", "font-size: auto"] {
            let r = m(Some(bad));
            assert_eq!(r.font_size, 14.0, "{bad} must not change the font size");
        }
    }

    #[test]
    fn content_right_never_falls_left_of_text_left() {
        // Padding larger than the box must not produce a negative width or an
        // inverted range, which would make the caret x arithmetic nonsense.
        // 200px of box, 120px of padding per side: the content area collapses.
        let r = m(Some("padding: 40px 120px"));
        // The clamping makes the two edges MEET rather than cross, so the text
        // area collapses to zero width instead of inverting and feeding the
        // caret arithmetic a negative span.
        assert!(r.content_right >= r.text_left, "{r:?}");
        assert_eq!(r.text_width(), 0.0);
        assert_eq!(r.text_left, BOX.x as f32 + 120.0);
        assert_eq!(r.content_right, r.text_left);
        // A merely generous padding is still honoured rather than clamped away.
        let roomy = m(Some("padding: 40px 60px"));
        assert_eq!(roomy.text_width(), 200.0 - 120.0);
    }

    #[test]
    fn degenerate_box_does_not_produce_nan() {
        let zero = Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
        let r = input_text_metrics(
            Some("border: 2px solid red; padding: 4px"),
            zero,
            VIEW,
            14.0,
        );
        assert!(r.text_left.is_finite());
        assert!(r.content_right.is_finite());
        assert_eq!(r.text_left, r.content_right);
    }
}
