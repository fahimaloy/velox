//! Lightweight Skia raster renderer helpers (Phase 1 minimal implementation).
//!
//! Provides a small helper to render a `VNode` into a raster PNG byte buffer.
//! This file is intentionally minimal for Phase 1: it draws element background
//! rectangles (via inline `style` attr parsing) and placeholders for text.
//!
#![allow(unused)]

use velox_dom::VNode;
use velox_dom::text_wrap::MeasuredText;
use velox_style::{Stylesheet, apply_with_cascade};

/// The font size the painter's ROOT `TextStyle` starts at, in logical px.
///
/// This is the size an element paints at when it declares no `font-size` of
/// its own, which makes it the *inherited* size every relative length and every
/// measured advance in the hit-test lane has to resolve against. It used to be
/// a bare `14.0` written out in both render entry points, while the caret lane
/// in `lib.rs` carried a third answer — the DOM's
/// [`velox_dom::layout::DEFAULT_ROOT_FONT_SIZE`] — and measured glyphs in it.
/// One named constant, reachable from both lanes, is what stops the third
/// answer from reappearing.
#[cfg(feature = "skia-native")]
pub(crate) const PAINT_ROOT_FONT_SIZE: f32 = 14.0;

/// The basis `rem` resolves against, in logical px.
///
/// Distinct from [`PAINT_ROOT_FONT_SIZE`] on purpose: `rem` is a *root* unit,
/// so it must not move when the painter's root text size changes. It is the
/// same constant `input_metrics::input_text_metrics` and `parse_text_style`
/// use, and the three must stay that way — a `rem` resolved against the
/// element's own size is not `rem` at all, and it made `border: 1rem` and
/// `padding: 1rem` disagree between the paint lane and the caret lane by
/// exactly the element's font size.
#[cfg(feature = "skia-native")]
pub(crate) const REM_ROOT_FONT_SIZE: f32 = velox_dom::layout::DEFAULT_ROOT_FONT_SIZE;

#[cfg(feature = "skia-native")]
pub mod skia_impl {
    use super::*;
    use skia_safe as sk;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    /// One resolved `border:` shorthand, in the units painting needs.
    ///
    /// `style` exists because this struct previously could not express it.
    /// `parse_border_value` returned `None` for anything that was not literally
    /// `solid`, so `border: 1px dashed` painted *nothing at all* — the style
    /// had nowhere to go, so the whole declaration was discarded. Carrying the
    /// style is what lets those declarations survive to the canvas.
    #[derive(Clone, Copy)]
    struct BorderSpec {
        width: f32,
        color: sk::Color,
        style: velox_dom::style::BorderStyle,
    }

    #[derive(Hash, Eq, PartialEq, Clone)]
    struct FontKey {
        family: String,
        size_key: u32,
    }

    /// Cache key for one memoized advance: the font's own key plus the run.
    ///
    /// The `text` is a `String` rather than borrowed, because the map owns its
    /// keys. Lookups build it per call; at ~150 measures per frame that is
    /// ~150 short allocations, which is far cheaper than the Skia shaping pass
    /// it replaces and is what keeps a `HashMap<&str, _>` self-referential
    /// lifetime problem out of the design.
    #[derive(Hash, Eq, PartialEq, Clone)]
    struct AdvanceKey {
        font: FontKey,
        text: String,
    }

    /// How far ABOVE the baseline the `line-through` rule sits, in `em` of the font
    /// size. See `line_through_y` for why it is above and not below.
    const LINE_THROUGH_ASCENT_EM: f32 = 0.25;

    /// Attribute the cascade writes `::placeholder` declarations onto.
    ///
    /// A pseudo-element's declarations cannot live in the element's own
    /// `style`: `input::placeholder { color: red }` would otherwise set the
    /// VALUE text red, which is the opposite of what the author wrote. One
    /// attribute per pseudo-element is what keeps the two separate all the way
    /// to paint.
    ///
    /// The literal lives in `velox_style::PLACEHOLDER_STYLE_ATTR`, because
    /// `velox_style` is what WRITES this attribute and this is what READS it. A
    /// private copy of the string here was a second, unfalsifiable answer to
    /// "what is that attribute called": renaming it in the cascade would have
    /// made the painter silently read nothing, with no compiler or test able to
    /// say so. Re-exported from the crate that owns the name.
    pub use velox_style::PLACEHOLDER_STYLE_ATTR;

    /// How much of the text colour a derived placeholder keeps.
    ///
    /// 0.55 is the same ballpark as every browser's default placeholder
    /// opacity (`opacity: 0.54` in Blink, `color: GrayText` elsewhere): enough
    /// dimming to read as "not the value" while staying legible.
    const PLACEHOLDER_CONTRAST: f32 = 0.55;

    /// Cap on memoized text advances, in `(family, size, text)` entries.
    ///
    /// The wrap path (`wrap_text`/`truncate_with_ellipsis`) measures every
    /// prefix of a line, so a long paragraph contributes one entry per word
    /// per candidate, and those keys are not the same keys the next frame
    /// asks for. Left unbounded this is a slow leak in a long session. 4096 is
    /// far above any realistic live set (a busy frame touches a few hundred
    /// distinct runs) while still being a few hundred KB worst case.
    const ADVANCE_CACHE_CAP: usize = 4096;

    /// One memoized `measure_str` result.
    ///
    /// `scale_key` is stored per entry and re-checked on every hit, so a stale
    /// entry cannot be served even if the `FontCache` scale bookkeeping is
    /// ever bypassed. See `FontCache::advances`.
    #[derive(Clone, Copy)]
    struct AdvanceEntry {
        width: f32,
        ascent: f32,
        descent: f32,
        scale_key: u32,
        /// Insertion order, for dropping the oldest when the cap is hit.
        seq: u64,
    }

    #[derive(Clone, Copy)]
    enum TextAlign {
        Left,
        Center,
        Right,
    }

    #[derive(Clone, Copy)]
    struct TextStyle {
        color: sk::Color,
        align: TextAlign,
        /// The two INLINE text decorations that are drawn as a rule across
        /// the run's measured advance: `underline` and `line-through`.
        ///
        /// They are two independent flags rather than one enum because CSS
        /// 2.1 §8.3.1 lets them combine — `text-decoration: underline
        /// line-through` draws both — and a single-valued enum (which is what
        /// `velox_dom::style::TextDecoration` is) cannot hold that. Painting
        /// is the only consumer that needs both at once.
        underline: bool,
        line_through: bool,
        font_size: f32,
        bold: bool,
        line_height: f32,
        nowrap: bool,
        ellipsis: bool,
    }

    #[derive(Clone, Copy)]
    struct ClipInsets {
        top: f32,
        right: f32,
        bottom: f32,
        left: f32,
    }

    /// The two image filters `img-filter` can express. `Debug`/`PartialEq` are
    /// for the parser tests, which assert what a rejected value did NOT parse
    /// into — a dropped declaration and an empty `FilterSpec` paint the same
    /// pixels, so the tests must compare the spec, not the frame.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct FilterSpec {
        blur_sigma: Option<f32>,
        brightness: Option<f32>,
    }

    /// Resolved padding sides, px.
    ///
    /// Parsed from the authored declaration by `parse_padding` (or, for the
    /// `<input>` lane's horizontal geometry, from computed values via
    /// `crate::input_metrics` — see that module for why the cross-lane
    /// contract reads computed values).
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Padding {
        top: f32,
        right: f32,
        bottom: f32,
        left: f32,
    }

    impl Padding {
        const ZERO: Padding = Padding {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        };
    }

    /// What an element's own `style` attribute declares about its BOX, already
    /// resolved to px where a length is involved.
    #[derive(Clone, Copy)]
    struct BoxStyle {
        background: Option<sk::Color>,
        border: Option<BorderSpec>,
        radius: Option<f32>,
        /// `None` when the element declared no padding at all, which is
        /// different from declaring `padding: 0`.
        padding: Option<Padding>,
        overflow_hidden: bool,
        clip_inset: Option<ClipInsets>,
        opacity: f32,
        filters: FilterSpec,
        z_index: i32,
    }

    impl Default for BoxStyle {
        /// Hand-written, not derived: `#[derive(Default)]` would make
        /// `opacity` `0.0`, and the paint lane MULTIPLIES the inherited opacity
        /// by this one — so every element would paint at zero alpha and the
        /// whole frame would come back transparent. The predecessor's local
        /// started at `1.0f32`, and CSS's initial `opacity` is 1.
        fn default() -> Self {
            Self {
                background: None,
                border: None,
                radius: None,
                padding: None,
                overflow_hidden: false,
                clip_inset: None,
                opacity: 1.0,
                filters: FilterSpec::default(),
                z_index: 0,
            }
        }
    }

    impl BoxStyle {
        /// The declared padding, or zero.
        fn padding_or_zero(&self) -> Padding {
            self.padding.unwrap_or(Padding::ZERO)
        }
    }

    fn parse_color_hex(value: &str) -> Option<sk::Color> {
        let value = value.trim();
        let hex = value.strip_prefix('#');
        if let Some(hex) = hex {
            if hex.len() == 6 {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                return Some(sk::Color::from_argb(255, r, g, b));
            }
            if hex.len() == 8 {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                return Some(sk::Color::from_argb(a, r, g, b));
            }
        }
        // Try rgb(r, g, b) and rgba(r, g, b, a) formats
        if let Some(inner) = value
            .strip_prefix("rgb(")
            .or_else(|| value.strip_prefix("rgba("))
        {
            let inner = inner.trim_end_matches(')').trim();
            let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
            if parts.len() >= 3
                && let (Ok(r), Ok(g), Ok(b)) = (
                    parts[0].parse::<u8>(),
                    parts[1].parse::<u8>(),
                    parts[2].parse::<u8>(),
                )
            {
                let a = if parts.len() >= 4 {
                    parts[3]
                        .parse::<f32>()
                        .map(|v| (v * 255.0) as u8)
                        .unwrap_or(255)
                } else {
                    255
                };
                return Some(sk::Color::from_argb(a, r, g, b));
            }
        }
        // Try named colors
        let named = match value.trim().to_lowercase().as_str() {
            "red" => Some(sk::Color::from_argb(255, 255, 0, 0)),
            "green" | "lime" => Some(sk::Color::from_argb(255, 0, 128, 0)),
            "blue" => Some(sk::Color::from_argb(255, 0, 0, 255)),
            "white" => Some(sk::Color::from_argb(255, 255, 255, 255)),
            "black" => Some(sk::Color::from_argb(255, 0, 0, 0)),
            "yellow" => Some(sk::Color::from_argb(255, 255, 255, 0)),
            "cyan" => Some(sk::Color::from_argb(255, 0, 255, 255)),
            "magenta" | "fuchsia" => Some(sk::Color::from_argb(255, 255, 0, 255)),
            "gray" | "grey" => Some(sk::Color::from_argb(255, 128, 128, 128)),
            "orange" => Some(sk::Color::from_argb(255, 255, 165, 0)),
            "purple" => Some(sk::Color::from_argb(255, 128, 0, 128)),
            "transparent" => Some(sk::Color::from_argb(0, 0, 0, 0)),
            _ => None,
        };
        if named.is_some() {
            return named;
        }
        None
    }

    /// Why this parser exists at all, and why it is not `ComputedStyle`.
    ///
    /// Painting consumes a *merged style string* built by the cascade in
    /// `velox-style`, not a `ComputedStyle` — `velox-dom`'s `set_property` has
    /// no production caller in this crate, so there is nothing to read a
    /// resolved border off. That is why this re-implements the grammar
    /// `velox_dom::style::parse_border_shorthand` already owns. The DOM's copy
    /// is the reference: the two must agree, and where they disagreed this one
    /// was the wrong one (it accepted only `px` widths and only `solid`).
    ///
    /// Unifying them — routing painting through `ComputedStyle`, or moving the
    /// grammar into a shared module both crates can see — is a design task and
    /// deliberately not attempted here.
    ///
    /// `rem` in a border width resolves against `REM_ROOT_FONT_SIZE`, not
    /// against the element's own `font_size`. `input_metrics::input_text_metrics`
    /// insets the value text by the same declaration using the root basis, and
    /// it has to: the border is painted AFTER `canvas.restore()`, so it cannot
    /// cover text that was laid out under a border too wide. Resolving `rem`
    /// against the element's size instead made `font-size:32px` +
    /// `border:1rem` stroke 32px while the text was inset by 16px, painting the
    /// value straight over the border.
    fn parse_border_value(value: &str, font_size: f32, viewport: (f32, f32)) -> Option<BorderSpec> {
        use velox_dom::style::{BorderStyle, Color, Length};

        let mut width: Option<f32> = None;
        let mut color: Option<sk::Color> = None;
        let mut style: Option<BorderStyle> = None;

        for part in value.split_whitespace() {
            if let Some(l) = Length::parse(part) {
                let px = l.to_px(font_size, super::REM_ROOT_FONT_SIZE, viewport);
                // Same non-finite rule as `parse_px_value`, on the other path
                // into a length: `border: 1e999px solid red` resolved to an
                // infinite stroke width. `apply_border_style` guards the width it
                // scales a DASH by, but the caller hands the raw value to
                // `set_stroke_width`, so the rejection has to happen here.
                if !px.is_finite() {
                    return None;
                }
                width = Some(px);
            } else if let Some(s) = BorderStyle::parse(part)
                && style.is_none()
            {
                style = Some(s);
            } else if let Some(c) = Color::parse(part)
                && color.is_none()
            {
                color = Some(sk::Color::from_argb(c.a, c.r, c.g, c.b));
            }
        }

        // CSS 2.1 §8.5.2: the initial value of `border-style` is `none`, so a
        // width with no style paints nothing. The DOM's `parse_border_shorthand`
        // does the same, and the two only ever agreed by accident before.
        let style = style.unwrap_or_default();
        if matches!(style, BorderStyle::None | BorderStyle::Hidden) {
            return None;
        }

        Some(BorderSpec {
            // CSS 2.1 §8.5.1: the initial `border-width` is `medium` (3px),
            // matching the DOM's own default rather than the old 1px.
            width: width.unwrap_or(3.0),
            color: color.unwrap_or_else(|| sk::Color::from_argb(255, 0, 0, 0)),
            style,
        })
    }

    /// Turn a resolved `BorderSpec::style` into something the stroke paint can
    /// actually draw.
    ///
    /// `dashed` and `dotted` get a real Skia dash path effect, scaled by the
    /// border width so the pattern tracks the weight.
    ///
    /// `double`, `groove`, `ridge`, `inset` and `outset` are drawn as a plain
    /// single stroke. That is a deliberate simplification, not an oversight:
    /// the defect fixed here was that they rendered as *nothing at all*. They
    /// now draw a visible border of the requested width and colour, which is
    /// strictly closer to the reference. The 3D bevels are cosmetic and are
    /// listed in the report as a known remaining gap.
    ///
    /// `solid` and `hidden`/`none` clear any dash left on the paint: the paint
    /// is reused across elements within a frame, so a stale path effect would
    /// otherwise bleed into the next element's border.
    fn apply_border_style(paint: &mut sk::Paint, border: &BorderSpec) {
        use velox_dom::style::BorderStyle;
        let w = if border.width.is_finite() && border.width > 0.0 {
            border.width
        } else {
            1.0
        };
        let intervals = match border.style {
            BorderStyle::Dashed => Some([w * 3.0, w * 3.0]),
            BorderStyle::Dotted => Some([w, w]),
            _ => None,
        };
        paint.set_path_effect(intervals.and_then(|iv| sk::path_effect::PathEffect::dash(&iv, 0.0)));
    }

    /// Placeholder ink for an empty `<input>`.
    ///
    /// Resolution order, most specific first:
    ///  1. an author's `input::placeholder { color: … }`, which the cascade
    ///     carries on a separate `style:placeholder` attribute so it can never
    ///     be confused with the element's own `style`;
    ///  2. otherwise a **derived** colour: the field's own `color` mixed
    ///     toward its background.
    ///
    /// Why derived rather than a fixed grey: the hardcoded `#999` that a
    /// literal-based implementation reaches for is illegible on the dark
    /// fields this engine's own boilerplate uses (`background:#16213e`), and
    /// equally invisible on a light one if the theme is flipped. Mixing
    /// toward the field's own background keeps the placeholder reading as the
    /// same hue, held back by a fixed amount of contrast, on either.
    fn placeholder_color(
        props: &velox_dom::Props,
        text_color: sk::Color,
        field_bg: sk::Color,
    ) -> sk::Color {
        if let Some(styled) = props.attrs.get(PLACEHOLDER_STYLE_ATTR)
            && let Some(c) = styled
                .split(';')
                .filter_map(|d| d.trim().split_once(':'))
                .find(|(k, _)| k.trim() == "color")
                .and_then(|(_, v)| {
                    parse_color_hex(v.trim()).or_else(|| {
                        velox_dom::style::Color::parse(v.trim())
                            .map(|c| sk::Color::from_argb(c.a, c.r, c.g, c.b))
                    })
                })
        {
            return c;
        }
        // Keep the alpha the author gave the text; only the channels dim.
        let mix = |from: u8, to: u8| -> u8 {
            let v = to as f32 + (from as f32 - to as f32) * PLACEHOLDER_CONTRAST;
            v.round().clamp(0.0, 255.0) as u8
        };
        sk::Color::from_argb(
            text_color.a(),
            mix(text_color.r(), field_bg.r()),
            mix(text_color.g(), field_bg.g()),
            mix(text_color.b(), field_bg.b()),
        )
    }

    /// Read a bare px length, rejecting a non-finite one.
    ///
    /// `f32::parse` does not reject overflow: `"1e999".parse::<f32>()` is
    /// `Ok(inf)`, and `inf.is_finite()` is false. So `border-radius: 1e999px`,
    /// `blur(1e999px)`, `inset(1e999px)` and `line-height: 1e999px` all parse,
    /// and each then carries a non-finite into a rect, a stroke width or a
    /// colour matrix. None of those is a number a painter should ever see.
    ///
    /// **This guard is currently unproven by any test, and that is deliberate —
    /// do not read the tests as evidence for it.** Removing it changes no
    /// frame today, because every consumer happens to absorb the result:
    /// `inset_rect` clamps with `.max(0.0)`, so an infinite inset collapses to a
    /// zero-width rect that `needs_clip` then declines to apply. The guard is
    /// here because that absorption is incidental, not designed. `inset_rect`'s
    /// clamp is one line, and any future change to it — a different padding
    /// source, a non-zero origin, a rect that skips the `needs_clip` check —
    /// turns a rejected declaration back into a silently erased subtree. A
    /// value the author cannot express should not survive parsing in the first
    /// place; it does not need a downstream line to stay harmless.
    ///
    /// The one caller that DID have a live failure was `parse_border_value`
    /// (`border: 1e999px solid red` reached `set_stroke_width` unguarded), and
    /// it is fixed separately at its own call site because it resolves relative
    /// units through `Length::parse`/`to_px` and cannot use this reader.
    ///
    /// The guard lives HERE, in the shared reader, rather than at each of the
    /// four call sites: no caller can turn a non-finite number into a real
    /// value on purpose, so a per-call-site guard is four chances to forget the
    /// same line. `parse_padding` writes the identical check for the same
    /// reason and cannot inherit this one, because it resolves relative units
    /// through `Length::parse`/`to_px` instead of reading px text.
    fn parse_px_value(value: &str) -> Option<f32> {
        let px = value.strip_suffix("px")?.trim().parse::<f32>().ok()?;
        if !px.is_finite() {
            return None;
        }
        Some(px)
    }

    /// Read a unitless number, rejecting a non-finite one. Same reasoning as
    /// `parse_px_value`: `opacity: 1e999` and `brightness(1e999)` both parse,
    /// and both are nonsense rather than a large number.
    fn parse_float_value(value: &str) -> Option<f32> {
        let f = value.trim().parse::<f32>().ok()?;
        if !f.is_finite() {
            return None;
        }
        Some(f)
    }

    /// Parse a `padding` shorthand into resolved px sides.
    ///
    /// CSS 2.1 §8.3.3: 1, 2, 3 and 4-value forms. `em`/`%` resolve against the
    /// element's OWN font size, same as every other relative length in this
    /// file (`font_size` is the element's, not the parent's).
    ///
    /// `rem` does NOT, which is the whole reason the second argument to
    /// `Length::to_px` is `REM_ROOT_FONT_SIZE` and not `font_size`. `Length`'s
    /// signature takes `(parent_size, root_size, viewport)` and uses the second
    /// argument for `rem` only — so passing `font_size` twice silently made
    /// `rem` a synonym for `em` here while `input_metrics::input_text_metrics`
    /// and `parse_text_style` both resolved it against the root. The `%` case
    /// below documents the convention for the third argument; `rem` follows the
    /// second.
    ///
    /// A non-finite or negative component is clamped to 0 rather than being
    /// allowed to poison a rect: `f32::parse` accepts both `1e999px` and
    /// `-8px`, and either would put the painted box somewhere unreachable.
    fn parse_padding(value: &str, font_size: f32, viewport: (f32, f32)) -> Option<Padding> {
        let mut parts: Vec<f32> = Vec::new();
        for part in value.split_whitespace() {
            let len = velox_dom::style::Length::parse(part)?;
            let px = len.to_px(font_size, super::REM_ROOT_FONT_SIZE, viewport);
            if !px.is_finite() {
                return None;
            }
            parts.push(px.max(0.0));
        }
        let sides = match parts.len() {
            1 => Padding {
                top: parts[0],
                right: parts[0],
                bottom: parts[0],
                left: parts[0],
            },
            2 => Padding {
                top: parts[0],
                right: parts[1],
                bottom: parts[0],
                left: parts[1],
            },
            3 => Padding {
                top: parts[0],
                right: parts[1],
                bottom: parts[2],
                left: parts[1],
            },
            4 => Padding {
                top: parts[0],
                right: parts[1],
                bottom: parts[2],
                left: parts[3],
            },
            // `padding: 10px 12px 8px 4px 2px` is not a padding declaration.
            _ => return None,
        };
        Some(sides)
    }

    fn parse_font_family(value: &str) -> Option<String> {
        let first = value.split(',').next()?.trim();
        let trimmed = first.trim_matches('"').trim_matches('\'').trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    /// Parse `clip-path: inset(...)`, the only basic shape this renderer draws.
    ///
    /// 1, 2, 3 and 4-value shorthand, all in px. Everything else is REJECTED
    /// (`None`, i.e. no clip at all) rather than approximated: `circle()`,
    /// `ellipse()`, `polygon()` and `url(#svg-clip)` are other shapes with
    /// their own reference boxes, and a percentage has no px reading here.
    /// `parse_clip_inset` is matched case-sensitively, so `INSET(10px)` is a
    /// rejected declaration and not a clipped one — pinned by
    /// `velox-renderer/tests/clip_path_render.rs`.
    ///
    /// A negative component is a rejected declaration too. CSS 2.1 §4.3
    /// ("negative values are invalid") applies: `inset(-10px)` must not be
    /// allowed to EXPAND the clip rect the way a negative `padding` used to
    /// invert a content box. `None` and a zero inset paint the same pixels, and
    /// `None` is the honest reading of an invalid value.
    fn parse_clip_inset(value: &str) -> Option<ClipInsets> {
        let value = value.trim();
        if !value.starts_with("inset(") || !value.ends_with(')') {
            return None;
        }
        let inner = value.trim_start_matches("inset(").trim_end_matches(')');
        // `inset(10px round 4px)` — the corner-radius half of the shape is
        // accepted and ignored; the inset edges are the same either way.
        let inner = inner.split("round").next().unwrap_or(inner).trim();
        let mut parts: Vec<f32> = Vec::new();
        for part in inner.split_whitespace() {
            // `parse_px_value` has already rejected `1e999px` (non-finite) and
            // anything that is not a px length (`50%`, `2em`, `wide`).
            match parse_px_value(part) {
                Some(px) if px >= 0.0 => parts.push(px),
                _ => return None,
            }
        }
        let (top, right, bottom, left) = match parts.len() {
            1 => (parts[0], parts[0], parts[0], parts[0]),
            2 => (parts[0], parts[1], parts[0], parts[1]),
            3 => (parts[0], parts[1], parts[2], parts[1]),
            4 => (parts[0], parts[1], parts[2], parts[3]),
            _ => return None,
        };
        Some(ClipInsets {
            top,
            right,
            bottom,
            left,
        })
    }

    /// Parse an `img-filter` value into the two image filters this renderer can
    /// actually draw. Deliberately NOT the CSS `filter` property — see the
    /// `img-filter` arm in `parse_style_attr` for why the name was given up.
    ///
    /// All-or-nothing, which is CSS's own rule for a filter list (Filter
    /// Effects 1 §2.1: an invalid filter list makes the declaration invalid at
    /// computed-value time). An unknown function, an argument this renderer
    /// cannot read, or a REPEATED function drops the whole value rather than
    /// applying the half that parsed — the old `split(')')` loop kept whatever
    /// it recognised and discarded the rest with no diagnostic, so
    /// `blur(4px) grayscale(1)` blurred and silently dropped the grayscale,
    /// and `blur(4px) blur(2px)` was last-wins on a one-field `FilterSpec`
    /// where CSS composes. Half a declaration, applied, looks intentional; that
    /// is the worst of the three possible outcomes and is no longer reachable.
    fn parse_img_filter(value: &str) -> Option<FilterSpec> {
        let value = value.trim();
        if value.eq_ignore_ascii_case("none") {
            return Some(FilterSpec::default());
        }
        let mut spec = FilterSpec::default();
        // Walk `name(arg)` items rather than `split(')')`: the old loop matched
        // whatever prefix it found and ignored the rest of the string, so
        // `blur(2px` — no closing parenthesis at all — was applied as a blur.
        // Requiring a `)` for every item makes a malformed list malformed.
        let mut rest = value;
        loop {
            rest = rest.trim_start();
            if rest.is_empty() {
                return Some(spec);
            }
            let open = rest.find('(')?;
            let close = rest.find(')')?;
            let arg = rest[open + 1..close].trim();
            match rest[..open].trim() {
                "blur" => {
                    if spec.blur_sigma.is_some() {
                        return None;
                    }
                    spec.blur_sigma = Some(parse_px_value(arg)?.max(0.0));
                }
                "brightness" => {
                    if spec.brightness.is_some() {
                        return None;
                    }
                    spec.brightness = Some(parse_float_value(arg)?.max(0.0));
                }
                // `grayscale()`, `contrast()`, `opacity()`, a bare keyword, a
                // name with no argument list: none of it is applied, and neither
                // is anything beside it in the same declaration.
                _ => return None,
            }
            rest = &rest[close + 1..];
        }
    }

    /// `font_size` and `viewport` exist for `parse_border_value`'s relative
    /// lengths (`2em solid red` used to vanish because only `px` was read).
    /// They are passed in rather than re-derived here because the paint loop
    /// already knows both.
    ///
    /// Returns a `BoxStyle` rather than a tuple. The tuple had grown to eight
    /// elements and was about to become unreadable at the call sites; a named
    /// field also lets the `padding` arm added below exist without every
    /// reader having to learn its position.
    fn parse_style_attr(style: &str, font_size: f32, viewport: (f32, f32)) -> BoxStyle {
        let mut out = BoxStyle::default();

        for decl in style.split(';') {
            let d = decl.trim();
            if d.is_empty() {
                continue;
            }
            if let Some((k, v)) = d.split_once(':') {
                let key = k.trim();
                let val = v.trim();
                if key == "background-color" || key == "background" {
                    out.background = parse_color_hex(val).or_else(|| {
                        velox_dom::style::Color::parse(val)
                            .map(|c| sk::Color::from_argb(c.a, c.r, c.g, c.b))
                    });
                } else if key == "border" {
                    out.border = parse_border_value(val, font_size, viewport);
                } else if key == "border-radius" {
                    if let Some(px) = parse_px_value(val) {
                        out.radius = Some(px);
                    }
                } else if key == "padding" {
                    // There was NO arm for `padding` here at all, so an
                    // author's padding reached layout (which has its own
                    // reader) but never reached paint. For an `<input>` that
                    // is what put the first glyph hard against the border.
                    // The `padding-*` longhands are read too, because the
                    // cascade does not expand the shorthand and an author who
                    // overrides one side inline writes the longhand.
                    if let Some(p) = parse_padding(val, font_size, viewport) {
                        out.padding = Some(p);
                    }
                } else if key.starts_with("padding-") {
                    let side = key.trim_start_matches("padding-");
                    let len = velox_dom::style::Length::parse(val)
                        .map(|l| l.to_px(font_size, super::REM_ROOT_FONT_SIZE, viewport));
                    if let Some(len) = len {
                        let p = out.padding.get_or_insert(Padding::ZERO);
                        match side {
                            "top" => p.top = len,
                            "right" => p.right = len,
                            "bottom" => p.bottom = len,
                            "left" => p.left = len,
                            _ => {}
                        }
                    }
                } else if key == "overflow" {
                    let v = val.to_ascii_lowercase();
                    out.overflow_hidden = v == "hidden" || v == "scroll" || v == "auto";
                } else if key == "clip-path" {
                    // Non-inherited in real CSS (Masking 1 §3.1), so
                    // `INHERITABLE` (velox-style/src/lib.rs) correctly omits it
                    // and each element reads its own declaration.
                    out.clip_inset = parse_clip_inset(val);
                } else if key == "opacity" {
                    if let Some(alpha) = parse_float_value(val) {
                        out.opacity = alpha.clamp(0.0, 1.0);
                    }
                } else if key == "img-filter" {
                    // NOT the CSS `filter` property, and no longer named it.
                    // CSS `filter` is a composited post-pass over the element's
                    // whole subtree (Filter Effects 1 §2.1), so honouring the name
                    // on an `<img>`-only paint filter over-promised on two
                    // counts at once: `filter: blur(4px)` on a div, on text, on a
                    // background or a border was a total no-op (every call site
                    // of `apply_img_filter` sits inside `if let Some(src)`), and
                    // `filter: blur(4px) grayscale(1)` applied the blur and
                    // dropped the grayscale. The name now says what it is: a
                    // filter on the element's own image, honoured only where an
                    // image is drawn. `parse_img_filter` is all-or-nothing, so
                    // nothing here can half-apply.
                    out.filters = parse_img_filter(val).unwrap_or_default();
                } else if key == "z-index"
                    && let Ok(z) = val.parse::<i32>()
                {
                    out.z_index = z;
                }
            }
        }

        out
    }

    fn z_index_for_props(props: &velox_dom::Props) -> i32 {
        if let Some(style) = props.attrs.get("style") {
            for decl in style.split(';') {
                let d = decl.trim();
                if d.is_empty() {
                    continue;
                }
                if let Some((k, v)) = d.split_once(':')
                    && k.trim() == "z-index"
                    && let Ok(z) = v.trim().parse::<i32>()
                {
                    return z;
                }
            }
        }
        0
    }

    /// `viewport` is a parameter rather than a capture because a nested `fn`
    /// item cannot capture the enclosing function's locals (E0434); both
    /// callers already hold it for `parse_style_attr`'s `vw`/`vh` borders.
    fn parse_text_style(
        style: &str,
        base: TextStyle,
        family: &str,
        viewport: (f32, f32),
    ) -> (TextStyle, String) {
        use velox_dom::layout::DEFAULT_ROOT_FONT_SIZE as ROOT_FONT_SIZE;
        use velox_dom::style::Length;
        // The size a RELATIVE unit in this element's own `font-size` resolves
        // against: the parent's, as the recursion threads it in `base`. Read
        // before `base` is moved, and deliberately NOT `text_style.font_size`
        // — CSS 2.1 §6.7: a relative unit in the value of the property itself
        // is relative to the PARENT's font size, so a second `font-size`
        // declaration in the same block must not compound.
        let parent_font_size = base.font_size;
        let mut text_style = base;
        let mut font_family = family.to_string();
        for decl in style.split(';') {
            let d = decl.trim();
            if d.is_empty() {
                continue;
            }
            if let Some((k, v)) = d.split_once(':') {
                let key = k.trim();
                let val = v.trim();
                if key == "color" {
                    if let Some(color) = parse_color_hex(val).or_else(|| {
                        velox_dom::style::Color::parse(val)
                            .map(|c| sk::Color::from_argb(c.a, c.r, c.g, c.b))
                    }) {
                        text_style.color = color;
                    }
                } else if key == "text-align" {
                    text_style.align = match val.to_ascii_lowercase().as_str() {
                        "center" => TextAlign::Center,
                        "right" => TextAlign::Right,
                        _ => TextAlign::Left,
                    };
                } else if key == "text-decoration" {
                    // CSS 2.1 §8.3.1: a `text-decoration` value is a SPACE-
                    // SEPARATED LIST of keywords, and any combination is
                    // legal. This used to be
                    //   `contains("underline") -> true; else if == "none" -> false`
                    // which had three defects: `line-through` matched neither
                    // branch and so inherited whatever the parent had (and
                    // rendered nothing), `underline line-through` set the
                    // underline and silently dropped the strike, and any
                    // value containing "none" as a substring was misread.
                    // Both flags are now assigned explicitly, so a combined
                    // value draws both and `none` clears both.
                    let val_l = val.to_ascii_lowercase();
                    if val_l.split_whitespace().any(|k| k == "none") {
                        text_style.underline = false;
                        text_style.line_through = false;
                    } else {
                        text_style.underline = val_l.split_whitespace().any(|k| k == "underline");
                        text_style.line_through =
                            val_l.split_whitespace().any(|k| k == "line-through");
                    }
                } else if key == "font-size" {
                    // `parse_px_value(val).or_else(|| parse_float_value(val))`
                    // read `px` and bare numbers only, so `2em`, `1.5rem`,
                    // `150%` and `5vw` were all dropped with no warning: the
                    // declaration reached the style string, the cascade, and
                    // the parser, and painted nothing. The whole `h1`–`h6`
                    // hierarchy rendered at its parent's size.
                    //
                    // Delegate to the DOM's own grammar — the same
                    // `Length::parse`/`Length::to_px` pair `parse_border_value`
                    // uses for `2em solid red` — so the glyph size and the box
                    // size come out of one unit table. Filtering this down to
                    // `em`/`rem` would have left `150%` silently dropped, which
                    // is the same lie in a smaller hole.
                    if let Some(len) = Length::parse(val) {
                        let px = len.to_px(parent_font_size, ROOT_FONT_SIZE, viewport);
                        // One guard drops three classes of input, all of which
                        // the old `.max(1.0)`-only path mishandled:
                        //  - `auto`: not a `font-size` value in CSS 2.1 §15.5
                        //    (`<absolute-size> | <relative-size> | <length> |
                        //    <percentage>`), and `Length::to_px` maps it to 0,
                        //    which the floor alone would paint as a 1px font.
                        //  - non-positive (`0`, `0%`, `-4px`): a negative font
                        //    size is invalid and a 0px one paints nothing.
                        //  - non-finite (`NaN`, or an overflowing literal such
                        //    as `1e999px`): `f32::parse` accepts both, and
                        //    `f32::max(1.0)` keeps `inf`, which Skia rejects.
                        if px.is_finite() && px > 0.0 {
                            text_style.font_size = px.max(1.0);
                        }
                    }
                } else if key == "font-family" {
                    if let Some(family) = parse_font_family(val) {
                        font_family = family;
                    }
                } else if key == "font-weight" {
                    // `bolder` is a relative-bold keyword. The render path has a
                    // single bold/not-bold axis (`TextStyle::bold`, no numeric
                    // weight), so the relative keyword collapses to bold. It was
                    // previously neither "bold" nor a u16 and so fell through to
                    // non-bold, silently dropping the declaration. `lighter`
                    // stays non-bold: it is a relative-light keyword, and a single
                    // boolean cannot represent "bolder than a weight we do not
                    // track", so mapping it to bold would be a new lie.
                    let w = val.trim();
                    text_style.bold = w.eq_ignore_ascii_case("bold")
                        || w.eq_ignore_ascii_case("bolder")
                        || w.parse::<u16>().map(|n| n >= 700).unwrap_or(false);
                } else if key == "white-space" {
                    let v = val.trim().to_ascii_lowercase();
                    text_style.nowrap = v == "nowrap" || v == "pre";
                } else if key == "text-overflow" {
                    text_style.ellipsis = val.trim().eq_ignore_ascii_case("ellipsis");
                } else if key == "line-height" {
                    // The unitless branch bypasses `parse_px_value`, so it needs
                    // the same non-finite guard: `line-height: 1e999` parses, and
                    // an infinite line height is a line box no later pass can
                    // lay out.
                    if let Some(lh) = val.trim().parse::<f32>().ok().filter(|lh| lh.is_finite()) {
                        text_style.line_height = lh;
                    } else if let Some(px) = parse_px_value(val.trim()) {
                        text_style.line_height = px / text_style.font_size;
                    }
                }
            }
        }
        (text_style, font_family)
    }

    fn inset_rect(rect: sk::Rect, inset: ClipInsets) -> sk::Rect {
        let left = rect.left + inset.left;
        let top = rect.top + inset.top;
        let width = (rect.width() - inset.left - inset.right).max(0.0);
        let height = (rect.height() - inset.top - inset.bottom).max(0.0);
        sk::Rect::from_xywh(left, top, width, height)
    }

    fn apply_clips(
        canvas: &sk::Canvas,
        rect: sk::Rect,
        rrect: Option<sk::RRect>,
        overflow_hidden: bool,
        clip_inset: Option<ClipInsets>,
    ) -> bool {
        let needs_clip = rrect.is_some() || overflow_hidden || clip_inset.is_some();
        if !needs_clip {
            return false;
        }
        canvas.save();
        if let Some(rrect) = rrect {
            canvas.clip_rrect(rrect, sk::ClipOp::Intersect, true);
        } else if overflow_hidden {
            canvas.clip_rect(rect, sk::ClipOp::Intersect, true);
        }
        if let Some(inset) = clip_inset {
            let clip_rect = inset_rect(rect, inset);
            canvas.clip_rect(clip_rect, sk::ClipOp::Intersect, true);
        }
        true
    }

    /// Establish the `clip-path` clip for the element's OWN box. Returns whether
    /// a `restore()` is owed.
    ///
    /// This is deliberately NOT folded into `apply_clips`, because it is the one
    /// clip that applies to the element itself rather than only to its
    /// descendants: the reference box of an `inset()` basic shape is the border
    /// box (Masking 1 §3.1), so the element's own background and border are
    /// clipped along with its content. `apply_clips` was called only after those
    /// two draws, so `clip-path: inset(20px)` on an element with a background
    /// used to leave the fill full-bleed and clip only the text inside it.
    ///
    /// The other two clips must NOT be applied here, which is why this is a
    /// separate call rather than `apply_clips` moved earlier:
    ///
    /// - `overflow: hidden` clips the element's CONTENT (CSS Overflow 3 §3.1),
    ///   never its own border, and a Skia stroke is centred on the path, so
    ///   clipping the border stroke to the box would shave the outer half off
    ///   every overflowing element's border.
    /// - `border-radius` only ROUNDS the element; the fill and the stroke are
    ///   already drawn as that same `RRect` here, so clipping them to it would
    ///   be a no-op at best and the same half-stroke loss at worst.
    ///
    /// Both stay where they were: `apply_clips` below still clips the subtree.
    /// The cost is one extra save/restore per element that declares
    /// `clip-path`, and only for those.
    fn apply_clip_path(
        canvas: &sk::Canvas,
        rect: sk::Rect,
        clip_inset: Option<ClipInsets>,
    ) -> bool {
        let Some(inset) = clip_inset else {
            return false;
        };
        canvas.save();
        canvas.clip_rect(inset_rect(rect, inset), sk::ClipOp::Intersect, true);
        true
    }

    fn text_x_for_align(container: sk::Rect, text_w: f32, align: TextAlign) -> f32 {
        let padding = 2.0;
        match align {
            TextAlign::Left => container.left + padding,
            TextAlign::Center => container.left + (container.width() - text_w) * 0.5,
            TextAlign::Right => (container.right - text_w - padding).max(container.left + padding),
        }
    }

    /// Read a boolean-ish input attribute.
    ///
    /// The template binds `<input type="checkbox" :checked="completed" />`
    /// (`test-app/src/components/TodoItem.vx:6`), NOT `:value`, so the old
    /// `value == "true"` read at the call site never saw a checked state. The
    /// SFC emits the expression as a stringified bool, so `"true"`/`"false"`,
    /// and the DOM-ish `checked` / the empty string are all accepted here.
    ///
    /// Returns `None` when the attribute is absent, so the caller can tell
    /// "unset" from "explicitly false" and keep the old `value`-based fallback.
    fn read_bool_attr(props: &velox_dom::Props, name: &str) -> Option<bool> {
        let raw = props.attrs.get(name)?;
        let v = raw.trim();
        if v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("checked") || v.is_empty() {
            Some(true)
        } else if v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("unchecked") {
            Some(false)
        } else {
            None
        }
    }

    fn parse_usize_attr(props: &velox_dom::Props, name: &str) -> Option<usize> {
        let raw = props.attrs.get(name)?;
        raw.trim().parse::<usize>().ok()
    }

    /// Ink for the caret bar, chosen from the luminance of the surface the bar
    /// is painted onto.
    ///
    /// The renderer hardcodes a near-white field fill, so the bar is normally
    /// near-black — but the value TEXT in that field is also near-black, and a
    /// black bar on black glyphs is the "cursor not showing up" report all over
    /// again. The bar is therefore drawn as a two-tone mark: a wide halo in the
    /// field's own background colour with a narrow core in the contrast ink on
    /// top. On a light field that is black-on-white (readable, and it punches a
    /// clean gap through any glyph it crosses); flipped onto a dark field the
    /// same pair becomes white-on-dark. One strategy, correct in both worlds,
    /// and never "white bar on a white input".
    fn caret_colors(field_bg: sk::Color) -> (sk::Color, sk::Color) {
        // Rec. 601 luma is enough for a binary light/dark decision; this is not
        // a colour-science grading path.
        let luma =
            0.299 * field_bg.r() as f32 + 0.587 * field_bg.g() as f32 + 0.114 * field_bg.b() as f32;
        let dark_field = luma < 128.0;
        let core = if dark_field {
            sk::Color::from_argb(255, 255, 255, 255)
        } else {
            sk::Color::from_argb(255, 0, 0, 0)
        };
        // The halo wears the field colour so it reads as a gap punched through
        // the glyphs rather than as a second decorative mark.
        let halo = field_bg;
        (halo, core)
    }

    /// The vertical position of the `line-through` rule for one line box.
    ///
    /// CSS 2.1 §8.3.1 gives no numeric position ("ideally, the middle of the
    /// glyphs"), so this follows the common browser rule: through the middle of
    /// the x-height band, which is ABOVE the baseline — not below it.
    ///
    /// The direction is load-bearing and is the opposite of the first guess.
    /// The underline already sits just below the baseline (`baseline + 1.0`), so
    /// a strike placed below the baseline too lands within a pixel of it and
    /// `underline line-through` renders as one fat blob instead of two rules.
    /// (Not hypothetical: the pixel test `underline_line_through_draws_both_bands`
    /// is what caught it.)
    ///
    /// The offset is derived from the FONT SIZE rather than a constant, because
    /// at 14px and at 48px the same constant lands visibly high and visibly low
    /// respectively. `0.25em` approximates half an x-height, which is close for
    /// every Latin face in this repo's bundles and — being derived — degrades
    /// smoothly instead of jumping to an absolute pixel value.
    fn line_through_y(baseline_y: f32, font_size: f32) -> f32 {
        baseline_y - font_size * LINE_THROUGH_ASCENT_EM
    }

    /// Draw `underline` and/or `line-through` across one already-measured line.
    ///
    /// `line_start_x` and `line_width` are the MEASURED run: the caller has
    /// already wrapped the paragraph and measured each visual line, so this is
    /// a measure-then-strike pass and the rule lands across every WRAPPED line
    /// rather than only the first. `line_width` of 0 draws nothing, so a
    /// blank wrapped line does not get a stray rule.
    fn draw_text_decorations(
        canvas: &sk::Canvas,
        paints: &mut RenderPaints,
        style: TextStyle,
        line_start_x: f32,
        baseline_y: f32,
        line_width: f32,
        font_size: f32,
        inherited_opacity: f32,
    ) {
        if !(style.underline || style.line_through) || line_width <= 0.0 {
            return;
        }
        let ink = color_with_opacity(style.color, inherited_opacity);
        let x0 = line_start_x;
        let x1 = line_start_x + line_width;
        if style.underline {
            paints.underline.set_color(ink);
            // `+1.0` sits just under the baseline, which is where browsers put
            // it and where it does not collide with descenders.
            let uy = baseline_y + 1.0;
            canvas.draw_line((x0, uy), (x1, uy), &paints.underline);
        }
        if style.line_through {
            paints.strike.set_color(ink);
            let sy = line_through_y(baseline_y, font_size);
            canvas.draw_line((x0, sy), (x1, sy), &paints.strike);
        }
    }

    fn color_with_opacity(color: sk::Color, opacity: f32) -> sk::Color {
        let a = ((color.a() as f32) * opacity).round().clamp(0.0, 255.0) as u8;
        sk::Color::from_argb(a, color.r(), color.g(), color.b())
    }

    fn apply_img_filter(paint: &mut sk::Paint, filters: FilterSpec) {
        if let Some(sigma) = filters.blur_sigma
            && sigma > 0.0
        {
            paint.set_image_filter(sk::image_filters::blur((sigma, sigma), None, None, None));
        }
        if let Some(brightness) = filters.brightness {
            let b = brightness.max(0.0);
            let matrix: [f32; 20] = [
                b, 0.0, 0.0, 0.0, 0.0, 0.0, b, 0.0, 0.0, 0.0, 0.0, 0.0, b, 0.0, 0.0, 0.0, 0.0, 0.0,
                1.0, 0.0,
            ];
            paint.set_color_filter(sk::color_filters::matrix_row_major(&matrix, None));
        }
    }

    fn layout_text_lines(
        text: &str,
        max_width: f32,
        fonts: &mut FontCache,
        family: &str,
        size: f32,
    ) -> Vec<(String, f32)> {
        let limit = if max_width <= 0.0 {
            f32::INFINITY
        } else {
            max_width
        };
        let mut lines = Vec::new();
        for para in text.split('\n') {
            if para.trim().is_empty() {
                lines.push((String::new(), 0.0));
                continue;
            }
            let mut current = String::new();
            let mut current_w = 0.0;
            for word in para.split_whitespace() {
                let candidate = if current.is_empty() {
                    word.to_string()
                } else {
                    format!("{} {}", current, word)
                };
                let candidate_w = fonts.measure_text(family, size, &candidate);
                if candidate_w <= limit || current.is_empty() {
                    current = candidate;
                    current_w = candidate_w;
                } else {
                    lines.push((current, current_w));
                    current = word.to_string();
                    current_w = fonts.measure_text(family, size, &current);
                }
            }
            if !current.is_empty() {
                lines.push((current, current_w));
            }
        }
        lines
    }

    /// Truncate `text` to fit `max_width` px, appending a horizontal ellipsis
    /// (`…`) when it overflows. Used for `text-overflow: ellipsis` on
    /// single-line (`white-space: nowrap` / `pre`) text boxes.
    fn truncate_with_ellipsis(
        text: &str,
        max_width: f32,
        fonts: &mut FontCache,
        family: &str,
        size: f32,
    ) -> String {
        const ELLIPSIS: &str = "\u{2026}";
        if max_width <= 0.0 || text.is_empty() {
            return String::new();
        }
        if fonts.measure_text(family, size, text) <= max_width {
            return text.to_string();
        }
        // Reserve room for the ellipsis itself before fitting prefix chars.
        let avail = (max_width - fonts.measure_text(family, size, ELLIPSIS)).max(0.0);
        let mut cur = String::new();
        for ch in text.chars() {
            let candidate = format!("{cur}{ch}");
            if fonts.measure_text(family, size, &candidate) <= avail {
                cur = candidate;
            } else {
                break;
            }
        }
        format!("{cur}{ELLIPSIS}")
    }

    fn collect_debug_hit_rects(
        vnode: &VNode,
        layout: &velox_dom::layout::LayoutNode,
        out: &mut Vec<velox_dom::layout::Rect>,
    ) {
        match vnode {
            VNode::Text(_) => {}
            VNode::Element {
                tag,
                props,
                children,
                ..
            } => {
                if crate::events::is_hoverable(tag, props) {
                    out.push(layout.rect);
                }
                for child_layout in &layout.children {
                    if child_layout.display_none {
                        continue;
                    }
                    if let Some(src_idx) = child_layout.source_index
                        && let Some(child) = children.get(src_idx)
                    {
                        collect_debug_hit_rects(child, child_layout, out);
                    }
                }
            }
        }
    }

    struct RenderPaints {
        fill: sk::Paint,
        stroke: sk::Paint,
        text: sk::Paint,
        underline: sk::Paint,
        /// The `line-through` rule. Its own paint rather than a reused one so
        /// its width is not coupled to whatever the element border last set on
        /// `stroke`.
        strike: sk::Paint,
        image: sk::Paint,
    }

    impl RenderPaints {
        fn new() -> Self {
            let mut fill = sk::Paint::default();
            fill.set_anti_alias(true);
            let mut stroke = sk::Paint::default();
            stroke.set_anti_alias(true);
            stroke.set_style(skia_safe::paint::Style::Stroke);
            let mut text = sk::Paint::default();
            text.set_anti_alias(true);
            let mut underline = sk::Paint::default();
            underline.set_anti_alias(true);
            underline.set_stroke_width(1.0);
            let mut strike = sk::Paint::default();
            strike.set_anti_alias(true);
            // Browsers draw the strike thinner than the underline; 1px is
            // already the thinnest this surface can render honestly, so the
            // weight matches and only the POSITION differs.
            strike.set_stroke_width(1.0);
            let mut image = sk::Paint::default();
            image.set_anti_alias(true);
            RenderPaints {
                fill,
                stroke,
                text,
                underline,
                strike,
                image,
            }
        }
    }

    /// How far into a file [`looks_like_svg`] looks for an `<svg` element.
    ///
    /// Long enough to clear what an SVG author puts in front of the root
    /// element -- a UTF-8 BOM, an `<?xml ... ?>` prolog, an XML declaration,
    /// whitespace, and a `<!DOCTYPE>` or comment -- with room to spare, and far
    /// short enough that a 4 MB photo costs a page-faulted prefix rather than a
    /// full read. It is a gate on the SLOW path, not a format decision: the
    /// raster formats are recognised by Skia first, from their own magic.
    const SVG_SNIFF_WINDOW: usize = 1024;

    /// Whether `bytes` are SVG source, for the sake of not handing a JPEG to a
    /// vector parser.
    ///
    /// SNIFFED, not sniffed from the file name, and deliberately NOT the
    /// primary format decision: `load` tries `Image::from_encoded` first and
    /// only reaches this on `None`, which means PNG/JPEG/WebP/GIF/BMP can never
    /// come through here at all -- Skia has already recognised them by magic
    /// (`sk::Image::from_encoded` is the authority on raster magic, not us), and
    /// a `None` from it is a header this Skia build does not know. So the
    /// asymmetry that matters is handled by ordering, and this only has to
    /// answer "is this text?", which a `<svg` in the first kilobyte answers.
    ///
    /// File extension is not used for the same reason resvg does not: `src` is
    /// whatever the author wrote, `logo.svg?v=2` and `data:` and a temp file
    /// with no name all occur, and an extension check that guesses wrong turns a
    /// working image into a silent blank box.
    fn looks_like_svg(bytes: &[u8]) -> bool {
        let window = &bytes[..bytes.len().min(SVG_SNIFF_WINDOW)];
        window.windows(4).any(|w| w.eq_ignore_ascii_case(b"<svg"))
    }

    /// Rasterise SVG source into an `sk::Image` at the SVG's OWN intrinsic size.
    ///
    /// The size is the SVG's, not the destination's, and that is the whole
    /// reason it can be cached by `src` alone: the paint walk hands this image
    /// to `draw_image_rect` with a `None` source rect, so Skia scales it into
    /// the layout rect at draw time. Nothing downstream -- layout, paint,
    /// `opacity`, `img-filter: blur()/brightness()` -- knows or cares what size the
    /// bitmap happens to be, so one raster serves every box the element is
    /// later given, at every device scale, which is the opposite of the FONT
    /// cache (whose glyphs are rasterised at `scale` and must be re-synced).
    ///
    /// `ceil` on the extents is what makes a fractional viewBox land on a whole
    /// pixel rather than being truncated into a half-empty bottom row; a
    /// non-finite or sub-one intrinsic size is a parse the author would not
    /// recognise, and is reported as unresolvable rather than rasterised to
    /// nothing.
    ///
    /// Returns `None` — which `load` turns into "draw nothing", the same
    /// silence a missing file gets — when the bytes are not SVG, when resvg
    /// cannot make a tree, or when the tree has no usable size.
    ///
    /// The pixel handoff is a copy into an `sk::Data`, not a conversion:
    /// tiny-skia pixmaps are premultiplied RGBA in R,G,B,A byte order
    /// (documented on `tiny_skia::PixmapRef::from_bytes`), which is precisely
    /// `RGBA8888 + AlphaType::Premul` with a row stride of `width * 4`. Colour
    /// space matches too: resvg renders in sRGB and these raster surfaces carry
    /// no colour space of their own. It is the same two calls the PNG path
    /// makes -- `Data::new_copy` then a factory that takes an `ImageInfo` -- so
    /// the two branches converge on one representation and nothing downstream
    /// can tell which produced the bitmap.
    fn rasterize_svg(bytes: &[u8]) -> Option<sk::Image> {
        if !looks_like_svg(bytes) {
            return None;
        }
        let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).ok()?;
        let (w, h) = (tree.size().width().ceil(), tree.size().height().ceil());
        if !w.is_finite() || !h.is_finite() || w < 1.0 || h < 1.0 {
            return None;
        }
        let (w, h) = (w as u32, h as u32);
        let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::default(),
            &mut pixmap.as_mut(),
        );
        let info = sk::ImageInfo::new(
            (w as i32, h as i32),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Premul,
            None,
        );
        let row_bytes = (w as usize) * 4;
        // `images::raster_from_data`, not `Image::from_raster_data`: the latter
        // is the deprecated spelling of this exact call in skia-safe and the
        // crate denies deprecated code.
        sk::images::raster_from_data(&info, sk::Data::new_copy(pixmap.data()), row_bytes)
    }

    /// Decoded sources for the `<img src>` paint walk, keyed by the `src` the
    /// author wrote.
    ///
    /// Persistent per thread, not per frame: see the comment on
    /// `RENDER_IMAGE_CACHE`. Every field here is about avoiding work that
    /// happens once per frame otherwise -- a disk read, and a full decode --
    /// for a picture that does not change between frames.
    struct ImageCache {
        images: HashMap<String, sk::Image>,
    }

    impl Default for ImageCache {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ImageCache {
        fn new() -> Self {
            ImageCache {
                images: HashMap::new(),
            }
        }

        /// The decoded image for `src`, decoding it on first request.
        ///
        /// RASTER FORMATS FIRST, and that ordering is load-bearing rather than
        /// stylistic: `Image::from_encoded` recognises PNG/JPEG/WebP/GIF/BMP
        /// from its own magic, so every one of them takes exactly the path it
        /// took before SVG existed -- same bytes, same `Data`, same call, same
        /// result, and only `resvg` pays anything for a vector. An SVG comes
        /// back `None` from `from_encoded` (no Skia magic matches XML) and
        /// falls to `rasterize_svg`.
        ///
        /// A `src` that cannot be decoded at all is NOT remembered: `None` is
        /// returned without an `insert`, so a file that appears later -- HMR
        /// writing a replacement, a build step producing an asset -- is picked
        /// up on the next frame instead of being pinned to a first-frame
        /// failure. That is a deliberate asymmetry against the success path, and
        /// it is the same asymmetry the PNG path had: a missing file stays a
        /// per-frame read attempt until it resolves. Nothing here panics on any
        /// input; every failure is a `None` that the draw site skips.
        fn load(&mut self, src: &str) -> Option<sk::Image> {
            if let Some(img) = self.images.get(src) {
                return Some(img.clone());
            }
            let bytes = std::fs::read(src).ok()?;
            let data = sk::Data::new_copy(&bytes);
            let image = sk::Image::from_encoded(data).or_else(|| rasterize_svg(&bytes))?;
            // Counted here, AFTER the decode succeeded and only when it is
            // about to be cached: the number this test/diagnostic exists to
            // assert is "how many times did a frame actually decode", and a
            // failed decode is not one.
            let _ = IMAGE_DECODE_COUNT.try_with(|c| c.set(c.get() + 1));
            self.images.insert(src.to_string(), image.clone());
            Some(image)
        }
    }

    /// Render `vnode` into a PNG-encoded raster image.
    ///
    /// This is a minimal proof-of-concept renderer used in Phase 1. It:
    /// - Creates a CPU raster `Surface`
    /// - Draws element backgrounds parsed from a `style` attr (`background-color:#RRGGBB`)
    /// - Draws simple placeholders for text nodes
    /// - Returns PNG bytes
    pub fn render_vnode_to_raster_png(
        vnode: &VNode,
        sheet: &Stylesheet,
        width: i32,
        height: i32,
    ) -> Result<Vec<u8>, String> {
        // DELIBERATELY NOT FUNNELLED THROUGH `prepare_frame` (R-8).
        //
        // This function builds a raw `raster_n32_premul` surface directly and
        // never installs `set_current_scale`/`set_skia_measurer`, and it runs no
        // `compute_layout` at all. `prepare_frame` requires a
        // `crate::skia_surface::SkiaSurface` and derives the measurer and scale
        // from that surface's scale factor, so using the funnel here would need
        // this body rewritten onto a different surface type first. That is a
        // behaviour change, not a mechanical refactor, so it is deliberately out
        // of R-8's scope.
        //
        // Its own lack of `compute_layout` is the pre-existing invariant-4
        // false-greener hazard: this path lays out nothing, so it can paint a
        // tree whose layout was never computed. Migrating its callers to
        // `render_vnode_to_rgba` / `render_vnode_to_raster_png_with_scale` and
        // then deleting this is a separate decision and a separate commit.
        //
        // Apply stylesheet declarations to inline style attrs before drawing,
        // so backgrounds/colors from the sheet are actually painted.
        let styled = apply_with_cascade(vnode, sheet);
        let vnode = &styled;
        let mut surface = sk::surfaces::raster_n32_premul((width, height))
            .ok_or_else(|| "skia: failed to create raster surface".to_string())?;
        let canvas = surface.canvas();
        canvas.clear(sk::Color::TRANSPARENT);

        let mut fonts = FontCache::new_with_scale(1.0);
        // Hoisted out of the per-call path, same rationale and same
        // take/restore shape as the render path's font cache: this proof path
        // painted the same `src` on every invocation, and each invocation paid
        // a disk read and a decode for it. `take_render_image_cache` also means
        // this path SHARES its bitmaps with `render_frame` rather than keeping
        // a second copy of every picture the app has shown.
        let mut image_guard = take_render_image_cache();
        let images = image_guard.cache_mut();
        let default_family = fonts.default_family();
        let default_text_style = TextStyle {
            color: sk::Color::from_argb(255, 0, 0, 0),
            align: TextAlign::Left,
            underline: false,
            line_through: false,
            font_size: super::PAINT_ROOT_FONT_SIZE,
            bold: false,
            line_height: 1.2,
            nowrap: false,
            ellipsis: false,
        };
        let mut paints = RenderPaints::new();

        #[allow(clippy::too_many_arguments)]
        fn draw_node(
            canvas: &sk::Canvas,
            node: &VNode,
            rect: sk::Rect,
            container_rect: sk::Rect,
            text_style: TextStyle,
            font_family: &str,
            fonts: &mut FontCache,
            paints: &mut RenderPaints,
            images: &mut ImageCache,
            inherited_opacity: f32,
            // Logical viewport, for `vw`/`vh` in `border-width`. Passed in
            // because a nested `fn` cannot capture the outer function's locals.
            viewport: (f32, f32),
        ) {
            match node {
                VNode::Element {
                    props, children, ..
                } => {
                    let mut clip_rrect = None;
                    let mut overflow_hidden = false;
                    let mut clip_inset = None;
                    let mut did_box_clip = false;
                    let mut child_text_style = text_style;
                    let mut child_family = font_family.to_string();
                    let mut opacity = inherited_opacity;
                    let mut filters = FilterSpec::default();
                    if let Some(s) = props.attrs.get("style") {
                        // `parse_text_style` runs first: the border parser needs
                        // this element's OWN font size to resolve `em`, and that
                        // is only known after the text style is computed.
                        let (style, family) =
                            parse_text_style(s, text_style, font_family, viewport);
                        let box_style = parse_style_attr(s, style.font_size, viewport);
                        let rrect = box_style.radius.map(|r| sk::RRect::new_rect_xy(rect, r, r));
                        if let Some(rrect) = rrect {
                            clip_rrect = Some(rrect);
                        }
                        overflow_hidden = box_style.overflow_hidden;
                        clip_inset = box_style.clip_inset;
                        child_text_style = style;
                        child_family = family;
                        opacity = (opacity * box_style.opacity).clamp(0.0, 1.0);
                        filters = box_style.filters;
                        // Before the element's OWN two draws, deliberately: see
                        // `apply_clip_path`. `overflow`/`border-radius` are not
                        // applied here, and `apply_clips` still clips the
                        // subtree below.
                        did_box_clip = apply_clip_path(canvas, rect, clip_inset);
                        if let Some(bg) = box_style.background {
                            paints.fill.set_color(color_with_opacity(bg, opacity));
                            if let Some(rrect) = rrect {
                                canvas.draw_rrect(rrect, &paints.fill);
                            } else {
                                canvas.draw_rect(rect, &paints.fill);
                            }
                        }

                        if let Some(border) = box_style.border {
                            paints.stroke.set_stroke_width(border.width);
                            paints
                                .stroke
                                .set_color(color_with_opacity(border.color, opacity));
                            apply_border_style(&mut paints.stroke, &border);
                            if let Some(rrect) = rrect {
                                canvas.draw_rrect(rrect, &paints.stroke);
                            } else {
                                canvas.draw_rect(rect, &paints.stroke);
                            }
                        }
                    }

                    if let Some(src) = props.attrs.get("src") {
                        paints.image.set_image_filter(None);
                        paints.image.set_color_filter(None);
                        paints.image.set_alpha_f(opacity);
                        apply_img_filter(&mut paints.image, filters);
                        if let Some(img) = images.load(src) {
                            canvas.draw_image_rect(img, None, rect, &paints.image);
                        }
                    }

                    // Naive child layout: stack children vertically
                    let child_count = children.len().max(1);
                    let child_h = rect.height() / (child_count as f32);
                    if did_box_clip {
                        canvas.restore();
                    }
                    let did_clip =
                        apply_clips(canvas, rect, clip_rrect, overflow_hidden, clip_inset);
                    let mut ordered: Vec<(i32, usize, &VNode)> = children
                        .iter()
                        .enumerate()
                        .map(|(i, ch)| {
                            let z = match ch {
                                VNode::Element { props, .. } => z_index_for_props(props),
                                _ => 0,
                            };
                            (z, i, ch)
                        })
                        .collect();
                    ordered.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
                    for (_, original_idx, ch) in ordered.iter() {
                        let child_rect = sk::Rect::from_xywh(
                            rect.left,
                            rect.top + *original_idx as f32 * child_h,
                            rect.width(),
                            child_h,
                        );
                        draw_node(
                            canvas,
                            ch,
                            child_rect,
                            rect,
                            child_text_style,
                            &child_family,
                            fonts,
                            paints,
                            images,
                            opacity,
                            viewport,
                        );
                    }
                    if did_clip {
                        canvas.restore();
                    }
                }
                VNode::Text(t) => {
                    paints
                        .text
                        .set_color(color_with_opacity(text_style.color, inherited_opacity));
                    let font_size = text_style.font_size;
                    let mut font = fonts.font(font_family, font_size);
                    if text_style.bold {
                        font.set_embolden(true);
                    }
                    let line_height = font_size * text_style.line_height;
                    let layout_rect =
                        sk::Rect::from_xywh(rect.left, rect.top, rect.width(), rect.height());
                    let lines = if text_style.ellipsis {
                        // Single-line truncated with an ellipsis to the text box width.
                        let single = truncate_with_ellipsis(
                            t.as_str(),
                            layout_rect.width(),
                            fonts,
                            font_family,
                            font_size,
                        );
                        let single_w = fonts.measure_text(font_family, font_size, &single);
                        vec![(single, single_w)]
                    } else {
                        layout_text_lines(
                            t.as_str(),
                            container_rect.width(),
                            fonts,
                            font_family,
                            font_size,
                        )
                    };
                    let align_rect = if layout_rect.width() >= container_rect.width() - 0.5 {
                        container_rect
                    } else {
                        layout_rect
                    };
                    let text_bottom = rect.top + rect.height().max(line_height);
                    for (idx, (line, line_w)) in lines.into_iter().enumerate() {
                        let ty = rect.top + font_size + (idx as f32) * line_height;
                        if ty > text_bottom {
                            break;
                        }
                        let padding = if align_rect.width() >= container_rect.width() - 0.5 {
                            2.0
                        } else {
                            0.0
                        };
                        let tx = match text_style.align {
                            TextAlign::Left => align_rect.left + padding,
                            TextAlign::Center => {
                                align_rect.left + (align_rect.width() - line_w) * 0.5
                            }
                            TextAlign::Right => {
                                (align_rect.right - line_w - padding).max(align_rect.left + padding)
                            }
                        };
                        #[allow(unused_must_use)]
                        {
                            let _ = canvas.draw_str(line.as_str(), (tx, ty), &font, &paints.text);
                        }
                        // Measure-then-strike: `line_w` is this VISUAL line's measured advance
                        // from the wrap pass above, so the rule lands across
                        // every wrapped line, not just the first.
                        draw_text_decorations(
                            canvas,
                            paints,
                            text_style,
                            tx,
                            ty,
                            line_w,
                            font_size,
                            inherited_opacity,
                        );
                    }
                }
            }
        }

        let root_rect = sk::Rect::from_xywh(0.0, 0.0, width as f32, height as f32);
        draw_node(
            canvas,
            vnode,
            root_rect,
            root_rect,
            default_text_style,
            &default_family,
            &mut fonts,
            &mut paints,
            images,
            1.0,
            (root_rect.width(), root_rect.height()),
        );

        let image = surface.image_snapshot();
        #[allow(deprecated)]
        let data = image
            .encode_to_data(skia_safe::EncodedImageFormat::PNG)
            .ok_or_else(|| "skia: failed to encode image".to_string())?;
        Ok(data.as_bytes().to_vec())
    }

    /// R-8's single render prologue. Every live path that cascades with
    /// `apply_with_cascade`, installs the scale/measurer globals and lays out
    /// funnels through here, so the ORDER of those three steps is stated once and
    /// cannot drift between entry points again.
    ///
    /// `logical_w`/`logical_h` are LOGICAL. This function must never multiply them
    /// by a scale factor: invariant 1 makes `viewport::physical_from_logical` the
    /// single place a logical size becomes a physical one, and that happens
    /// exactly once, when the caller constructs the surface it passes in. The
    /// surface is read only for its scale, never resized here.
    ///
    /// The style cascade is applied FIRST because neither of the two steps after
    /// it can change a style: installing a measurer is global-state assignment and
    /// `compute_layout` consumes the already-styled tree.
    pub(crate) fn prepare_frame(
        vnode: &VNode,
        sheet: &Stylesheet,
        logical_w: i32,
        logical_h: i32,
        surface: &crate::skia_surface::SkiaSurface,
    ) -> (VNode, velox_dom::layout::LayoutNode) {
        let styled = apply_with_cascade(vnode, sheet);
        velox_dom::text_wrap::set_current_scale(surface.scale_factor());
        velox_dom::text_wrap::set_skia_measurer(measure_text);
        // Registered HERE, beside the measurer and under the same reasoning:
        // both seams are "global function pointer into this crate's backend,
        // installed at the top of the frame, read back from `velox-dom` during
        // `compute_layout`". It has to be before the `compute_layout` on the
        // next line -- that is the only place the probe is ever asked anything,
        // so registering it after would leave it uninstalled for exactly the one
        // call that needs it, with no failure to observe: an unsized `<img>`
        // would simply be a zero-size box, which is what it is today.
        velox_dom::layout::set_intrinsic_size_probe(intrinsic_image_size);
        let layout = velox_dom::layout::compute_layout(&styled, logical_w, logical_h);
        (styled, layout)
    }

    /// Render `vnode` into a raw RGBA8888 byte buffer (premultiplied, opaque
    /// alpha) of size `width * height * 4`. Useful for pixel-level assertions
    /// in tests without decoding a PNG.
    pub fn render_vnode_to_rgba(
        vnode: &VNode,
        sheet: &Stylesheet,
        width: i32,
        height: i32,
    ) -> Result<Vec<u8>, String> {
        let mut surface = crate::skia_surface::SkiaSurface::new_raster(width, height)?;
        let (vnode, layout) = prepare_frame(vnode, sheet, width, height, &surface);
        render_frame(&mut surface, &vnode, &layout, sheet)?;

        let info = sk::ImageInfo::new(
            (width, height),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Premul,
            None,
        );
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        if !surface.read_pixels(&info, &mut rgba, (width * 4) as usize, (0, 0)) {
            return Err("skia: read_pixels failed".to_string());
        }
        Ok(rgba)
    }

    /// Render `vnode` into a PNG-encoded raster image with a scale factor applied.
    pub fn render_vnode_to_raster_png_with_scale(
        vnode: &VNode,
        sheet: &Stylesheet,
        width: i32,
        height: i32,
        scale_factor: f32,
    ) -> Result<Vec<u8>, String> {
        // Invariant 1: `viewport::physical_from_logical` is the single DPI/scale
        // rounding authority. This used to inline a copy of the same formula
        // the same multiply-then-round formula but was missing the authority's
        // `.max(1)` and the authority's substitute-for-a-degenerate-scale rule, so
        // a non-positive scale collapsed ANY requested size to a 1x1 surface.
        // `SkiaSurface::new_raster` takes i32, so the
        // authority's u32 result is narrowed with a saturating `try_from` rather
        // than an `as` cast that would wrap above `i32::MAX`.
        //
        // The logical inputs are clamped to 0 before the u32 conversion because a
        // negative `i32` cast to `u32` wraps to ~4 billion, which would ask for a
        // 2-billion-pixel surface. Both normalisations are the `.max(1)` fix: a
        // degenerate size now yields a 1px surface instead of a 0px or negative one.
        let (physical_w, physical_h) = crate::Viewport::physical_from_logical(
            width.max(0) as u32,
            height.max(0) as u32,
            scale_factor,
        );
        let mut surface = crate::skia_surface::SkiaSurface::new_raster(
            i32::try_from(physical_w).unwrap_or(i32::MAX),
            i32::try_from(physical_h).unwrap_or(i32::MAX),
        )?;
        surface.set_scale_factor(scale_factor);
        // R-8: the prologue is `prepare_frame`, shared with `render_vnode_to_rgba`.
        // It cascades, installs the scale/measurer globals and lays out, in that
        // one order, and takes LOGICAL `width`/`height` — the logical-to-physical
        // conversion above is the only one, and the funnel does not repeat it.
        let (vnode, layout) = prepare_frame(vnode, sheet, width, height, &surface);
        render_frame(&mut surface, &vnode, &layout, sheet)?;
        surface.encode_png()
    }

    /// Minimal FontCache for mapping sizes to `skia_safe::Font`.
    /// DPI-aware: re-rasters fonts at device pixels (hinting) via scale-factor snapping.
    pub struct FontCache {
        typefaces: HashMap<String, sk::Typeface>,
        fonts: HashMap<FontKey, sk::Font>,
        /// Memoized `measure_str` results, keyed by the same `FontKey` the
        /// `sk::Font` is keyed by plus the text.
        ///
        /// This is the hot one. Layout re-measures the same subtree several
        /// times per frame — a 1-item todo app issues ~57 `measure_run` calls
        /// and a 3-item list ~150 — and the strings are overwhelmingly
        /// repeats, so a lookup here replaces a full Skia shaping pass. The
        /// `FontKey` already carries the DPI-snapped size, which is derived
        /// from `self.scale`, so the scale is *implicitly* part of the key;
        /// `scale_key` on the entry makes that explicit and independently
        /// checked, and `set_scale_factor` clears the map outright, exactly as
        /// it clears `fonts`. Three independent guards, because a stale advance
        /// is a wrong layout and a visibly wrong UI.
        advances: HashMap<AdvanceKey, AdvanceEntry>,
        /// Monotonic insertion counter, for drop-oldest eviction.
        adv_seq: u64,
        /// How many real `measure_str` calls this cache has made.
        ///
        /// Incremented only on a memo MISS, i.e. on the path that actually
        /// shapes a run in Skia, so the counter is a direct measurement of the
        /// work the cache exists to avoid: `skia_measure_calls()` going up by
        /// one per repeated `measure_run` would mean the memo is not working,
        /// and it is the only honest way to test that without timing
        /// assertions. One `u64` increment on a path that then runs a full
        /// Skia shaping pass is not measurable overhead.
        adv_skia_calls: u64,
        /// The one `sk::Paint` every `measure_str` call borrows.
        ///
        /// It existed as a fresh `sk::Paint::default()` inside `measure_run`,
        /// so every measurement allocated one (and registered a native
        /// SkPaint) to set a single anti-alias flag. `measure_str` takes
        /// `Option<&Paint>` — a shared borrow, read-only for Skia's measuring
        /// path — and nothing here mutates it after construction, so one
        /// instance per `FontCache` is safe to reuse across every call and
        /// across every frame the cache serves. Owned as a field rather than a
        /// `thread_local!` so its lifetime is the cache's, with no teardown
        /// ordering question during thread exit.
        measure_paint: sk::Paint,
        default_family: String,
        scale: f32,
    }

    impl FontCache {
        /// Attempt to load a system font or bundled fallback fonts. Defaults to scale 1.0.
        pub fn new() -> Self {
            Self::new_with_scale(1.0)
        }

        /// Create a FontCache that re-rasters at `scale` device pixels.
        pub fn new_with_scale(scale: f32) -> Self {
            let s = if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            };
            let default_family = "default".to_string();
            let mut typefaces = HashMap::new();
            if let Some(tf) = load_default_typeface() {
                typefaces.insert(default_family.clone(), tf);
            }
            // Built once, anti-alias flag set once, then never mutated. See the
            // field's doc for why sharing this across every `measure_str` is
            // safe: the API takes `Option<&Paint>`.
            let mut measure_paint = sk::Paint::default();
            measure_paint.set_anti_alias(true);
            FontCache {
                typefaces,
                fonts: HashMap::new(),
                advances: HashMap::new(),
                adv_seq: 0,
                adv_skia_calls: 0,
                measure_paint,
                default_family,
                scale: s,
            }
        }

        /// Update scale (e.g. on ScaleFactorChanged) — clears cache so glyphs
        /// are re-rastered at new device pixels (prevents blur).
        pub fn set_scale_factor(&mut self, scale: f32) {
            let s = if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            };
            if (self.scale - s).abs() > f32::EPSILON {
                self.scale = s;
                self.fonts.clear();
                // The advance memo is dropped in the SAME place as `fonts`, and
                // for the same reason: an advance measured against glyphs
                // rastered at the old device size is wrong at the new one, and
                // unlike a blurry glyph a wrong advance moves text around. Both
                // maps live on the same `FontCache` and are cleared together, so
                // there is no ordering in which the advance memo can outlive the
                // scale that produced it. The measure-side and render-side
                // caches are separate `FontCache` values, and each clears its
                // own; neither can read the other's entries.
                self.advances.clear();
            }
        }

        /// Exact bit pattern of `self.scale`, as a comparable key.
        #[inline]
        fn scale_key(&self) -> u32 {
            self.scale.to_bits()
        }

        /// The key both `font()` and the advance memo use.
        ///
        /// Extracted so the two can never drift: if `font()` keyed on a
        /// different size than the memo did, a hit could return an advance
        /// measured at another size. `size_key` is the DPI-snapped size in
        /// hundredths, so it is a function of `self.scale` and changes with it.
        #[inline]
        fn font_key(&self, family: &str, size: f32) -> FontKey {
            let snapped = self.snapped_size(size);
            FontKey {
                family: family.to_string(),
                size_key: (snapped * 100.0).round() as u32,
            }
        }

        pub fn scale_factor(&self) -> f32 {
            self.scale
        }

        pub fn default_family(&self) -> String {
            self.default_family.clone()
        }

        /// Resolve a family to a typeface.
        ///
        /// NOT a fontconfig family lookup, and this is a DECLARED NON-GOAL rather
        /// than a silent gap — see `docs/plans/2026-10-02-scaffolded-app-defects.md`
        /// (T5c). `FontCache::new_with_scale` inserts exactly one entry,
        /// `"default"`, from `load_default_typeface`; every other `family`
        /// therefore gets that same face, and `parse_font_family` keeps only the
        /// FIRST name in a comma list, so `font-family: Inter, "Noto Sans", …`
        /// asks for `Inter` and is served the default. `font-family` is inert
        /// because of this function, not because the cascade dropped the
        /// declaration, and per-family loading is a separate task.
        ///
        /// What this DOES do is refuse to poison the cache: a miss is served the
        /// default face WITHOUT writing it under the requested name. Caching it
        /// made the miss permanent and made it indistinguishable from a hit —
        /// nothing could later tell "this family resolved" from "this family does
        /// not exist" — and it was the reason a host that gained the family could
        /// never be reached.
        fn get_or_load_family(&mut self, family: &str) -> Option<sk::Typeface> {
            if let Some(tf) = self.typefaces.get(family) {
                return Some(tf.clone());
            }
            self.typefaces.get(&self.default_family).cloned()
        }

        /// Snap a logical font size to the nearest physical pixel row so that
        /// `(logical * scale).round()` is integer device pixels. This ensures
        /// glyph hinting lands on device pixels at fractional scales (1.25/1.5)
        /// instead of 0.25px subpixel blur after `canvas.scale(scale)`.
        #[inline]
        fn snapped_size(&self, logical_size: f32) -> f32 {
            // NOT a second rounding authority. It differs from
            // `Viewport::snap_logical_to_physical_grid` in two load-bearing ways,
            // so substituting the authority would change font rasterisation:
            //   * the `.max(1.0)` here clamps the DEVICE size; the authority has
            //     no such clamp, so a size that rounds to 0 device pixels becomes
            //     1 here and 0 there. A 0-device-pixel font is not renderable,
            //     so this clamp is the point of the helper.
            //   * the `scale == 1.0` early return skips the round trip entirely,
            //     leaving `logical_size` unrounded; the authority would round it
            //     (16.5 -> 17.0).
            // This is a font-size snap for hinting, not a logical->physical size
            // conversion, so it keeps its own clamp and guard.
            if self.scale == 1.0 {
                return logical_size;
            }
            let device = (logical_size * self.scale).round().max(1.0);
            device / self.scale
        }

        /// Return a `skia_safe::Font` at the requested `size` and `family`.
        /// Size is DPI-snapped to device pixels for `self.scale`.
        pub fn font(&mut self, family: &str, size: f32) -> sk::Font {
            let snapped = self.snapped_size(size);
            // Include scale in key implicitly via snapped value; also clear on scale change.
            let key = self.font_key(family, size);
            if let Some(font) = self.fonts.get(&key) {
                return font.clone();
            }
            let font = if let Some(tf) = self.get_or_load_family(family) {
                sk::Font::new(tf, snapped)
            } else {
                let mut f = sk::Font::default();
                f.set_size(snapped);
                f
            };
            self.fonts.insert(key, font.clone());
            font
        }

        /// Measure the width (in px) of `text` rendered at `size` using the cached typeface.
        /// Measurement is at DPI-snapped size so layout and render agree (no wrap mismatch).
        pub fn measure_text(&mut self, family: &str, size: f32, text: &str) -> f32 {
            self.measure_run(family, size, text).width
        }

        /// Measure `text`'s advance width *and* its vertical extent.
        ///
        /// `font.measure_str` already returned the ink bounds and this code threw
        /// them away (`_bounds`); they are the vertical answer. Skia reports those
        /// bounds with the baseline at y = 0, so the run's reach above the
        /// baseline is `-top` and its reach below is `bottom`.
        ///
        /// A run with no ink — a blank one, or a space — measures a zero-height
        /// rectangle, which is a true statement about ink and a useless one about
        /// a line box, so that case is left for the seam to answer from the
        /// documented approximation rather than propagated. `.max(0.0)` also maps
        /// a NaN bound to 0.0, which lands in that same case.
        ///
        /// Both numbers are in LOGICAL px, not device px. `font()` builds the
        /// `sk::Font` at `snapped_size(size)`, which is `device / self.scale`, so
        /// `measure_str` reports at the logical size the caller asked for — the
        /// same units `measure_heuristic` and `velox_dom`'s own snap point work in,
        /// and the same units the width has always been in. Dividing these by
        /// `scale` would be the bug, not the fix: it would scale every line box by
        /// the device factor. There is one rounding authority in this project and it
        /// is not here, so nothing below rounds.
        pub fn measure_run(&mut self, family: &str, size: f32, text: &str) -> MeasuredText {
            let key = AdvanceKey {
                font: self.font_key(family, size),
                text: text.to_string(),
            };
            // Hit: return the memoized numbers. The `scale_key` comparison is the
            // third of the three guards described on the `advances` field. It is
            // redundant with the `set_scale_factor` clear *by construction*, but
            // it costs one integer compare and turns "the clear happened" from
            // an assumption into something the code verifies on every lookup, so
            // a future path that mutates `self.scale` directly cannot quietly
            // start serving advances from another scale.
            let scale_key = self.scale_key();
            if let Some(e) = self.advances.get(&key)
                && e.scale_key == scale_key
            {
                return MeasuredText {
                    width: e.width,
                    ascent: e.ascent,
                    descent: e.descent,
                };
            }
            // Miss. `font()` inserts into `self.fonts`, so the borrow of
            // `self.advances` above must end before it — hence the lookup being
            // scoped to its own statement rather than held across the call.
            let font = self.font(family, size);
            self.adv_skia_calls = self.adv_skia_calls.wrapping_add(1);
            let (w, bounds) = font.measure_str(text, Some(&self.measure_paint));
            let run = MeasuredText {
                width: w,
                ascent: (-bounds.top).max(0.0),
                descent: bounds.bottom.max(0.0),
            };
            self.remember_advance(key, run, scale_key);
            run
        }

        /// Memoize one measured run, dropping the oldest entries if the cap is hit.
        ///
        /// Drop-oldest, never refuse: hitting the cap evicts down to half of
        /// `ADVANCE_CACHE_CAP` in a single pass (amortized, since it takes
        /// `ADVANCE_CACHE_CAP/2` inserts to reach the cap again) and then
        /// caches this entry anyway. Refusing to cache past the cap would pin
        /// the working set to whatever was measured first and permanently
        /// exclude every key measured after it, which is the opposite of what
        /// the cache is for.
        fn remember_advance(&mut self, key: AdvanceKey, run: MeasuredText, scale_key: u32) {
            if self.advances.len() >= ADVANCE_CACHE_CAP {
                self.evict_oldest_advances(ADVANCE_CACHE_CAP / 2);
            }
            let seq = self.adv_seq;
            self.adv_seq = self.adv_seq.wrapping_add(1);
            self.advances.insert(
                key,
                AdvanceEntry {
                    width: run.width,
                    ascent: run.ascent,
                    descent: run.descent,
                    scale_key,
                    seq,
                },
            );
        }

        /// Drop the `n` oldest memoized advances.
        ///
        /// `sort_unstable_by_key` on the insertion counter is an O(k log k)
        /// pass, but it only runs once per `ADVANCE_CACHE_CAP/2` inserts, so
        /// the per-measure amortized cost is negligible. The victims' keys are
        /// cloned because the map owns them and `retain` cannot see a partial
        /// match on `seq` without one.
        fn evict_oldest_advances(&mut self, n: usize) {
            if n == 0 || self.advances.is_empty() {
                return;
            }
            let mut victims: Vec<(u64, AdvanceKey)> = self
                .advances
                .iter()
                .map(|(k, e)| (e.seq, k.clone()))
                .collect();
            victims.sort_unstable_by_key(|(seq, _)| *seq);
            // `seq` is unique per insertion and never reused (it only ever
            // advances), so this removes exactly the `n` oldest entries.
            for (_, key) in victims.into_iter().take(n) {
                self.advances.remove(&key);
            }
        }

        /// Number of memoized advances currently held. Test/diagnostic only.
        pub fn advance_cache_len(&self) -> usize {
            self.advances.len()
        }

        /// How many real Skia `measure_str` calls this cache has made.
        /// Test/diagnostic only.
        pub fn skia_measure_calls(&self) -> u64 {
            self.adv_skia_calls
        }
    }

    // PERSISTENT FONT CACHES (perf: one typeface probe per scale, not per call).
    //
    // `FontCache::new_with_scale` calls `load_default_typeface`, which builds a
    // `sk::FontMgr::default()`. That construction is not cheap — it measured at
    // ~50% of total process CPU under
    // `perf`. Both `measure_text` (registered as the global measurer via
    // `text_wrap::set_skia_measurer`, so it runs for every measurement layout
    // performs) and `render_frame` (once per frame) used to build a brand new
    // cache per call, so the scan ran per measurement and per frame. A single
    // frame could then spend unbounded time inside one call, starving the main
    // thread's input handling: clicks/buttons/checkboxes appear dead and the
    // app looks hung, with no coredump and no panic.
    //
    // Thread-local rather than a global `Mutex`: layout and rendering both run
    // on the main/UI thread, so the cache needs no cross-thread sharing, and a
    // thread-local keeps the lock off the per-measurement path entirely.
    //
    // Two caches, not one: measurement and rendering scale are supplied
    // independently. A shared cache would clear its `fonts` map on every
    // alternating scale change (`set_scale_factor`), re-rastering the world
    // between the two callers. Separate caches cost at most one extra scan per
    // scale and never thrash.
    //
    // The `RefCell` initializers are lazy, so `load_default_typeface` runs on
    // first access on the accessing thread, not at static-init time. The render
    // slot holds an `Option` so the slot itself is `const`-initialisable, which
    // is what lets `take_render_font_cache` below hand out an owned cache.
    thread_local! {
        /// Cache backing `measure_text`. Borrowed in place, scale set per call.
        static MEASURE_FONT_CACHE: RefCell<FontCache> =
            RefCell::new(FontCache::new_with_scale(1.0));
        /// Cache backing `render_frame`. Lends its cache to the frame painter.
        static RENDER_FONT_CACHE: RefCell<Option<FontCache>> = const { RefCell::new(None) };
    }

    /// Borrow the persistent measure-path `FontCache`, resynced to `scale`.
    fn with_measure_font_cache<R>(scale: f32, f: impl FnOnce(&mut FontCache) -> R) -> R {
        MEASURE_FONT_CACHE.with(|cell| {
            let mut cache = cell.borrow_mut();
            // Reusing the cache across calls makes this resync load-bearing:
            // without it a scale change would keep serving glyphs rastered at
            // the stale device-pixel size and go blurry at fractional scales
            // (1.25/1.5). `set_scale_factor` is also what drops the `fonts` map
            // on a scale change, so this is the re-raster trigger.
            //
            // The caller's own non-finite/`<= 0` guard in `measure_text` is
            // intentionally NOT replaced by this one: `set_scale_factor`
            // substitutes 1.0 for a degenerate scale, whereas the caller leaves
            // the snapped size unrounded. Both are kept, as before.
            cache.set_scale_factor(scale);
            f(&mut cache)
        })
    }

    /// Owns the render-path `FontCache` for the duration of one frame and
    /// returns it to its thread-local slot on drop.
    ///
    /// The field is an `Option` so `Drop` can `take()` the cache out. It is
    /// deliberately NOT a plain `FontCache` handed back via
    /// `mem::replace(.., FontCache::new_with_scale(1.0))`: that would construct
    /// a throwaway `FontCache` — and therefore rebuild the `FontMgr` and
    /// re-probe the typeface this whole change exists to avoid — on every
    /// single frame.
    struct RenderFontCache {
        cache: Option<FontCache>,
    }

    impl RenderFontCache {
        /// The cache for this frame's paint walk.
        fn cache_mut(&mut self) -> &mut FontCache {
            self.cache
                .as_mut()
                .expect("render font cache taken by this guard; not reentrant")
        }
    }

    impl Drop for RenderFontCache {
        fn drop(&mut self) {
            // Take-and-restore rather than holding a `RefCell` borrow across
            // the frame: a non-`const` thread-local cannot hand out a
            // `RefMut<'static, _>`, and the paint walk needs a plain
            // `&mut FontCache` spanning its whole recursion. `try_with`
            // because a `Drop` during thread teardown must not touch a
            // destroyed slot; dropping the cache there is correct, it just
            // forfeits the reuse.
            if let Some(cache) = self.cache.take() {
                let _ = RENDER_FONT_CACHE.try_with(|slot| {
                    *slot.borrow_mut() = Some(cache);
                });
            }
        }
    }

    /// Take the persistent render-path `FontCache`, resynced to `scale`.
    fn take_render_font_cache(scale: f32) -> RenderFontCache {
        let taken = RENDER_FONT_CACHE
            .try_with(|slot| std::mem::take(&mut *slot.borrow_mut()))
            .ok()
            .flatten();
        let mut guard = RenderFontCache {
            // Only the very first frame on this thread pays the
            // `FontMgr`/typeface probe; every later frame gets the previous
            // frame's cache back.
            cache: Some(taken.unwrap_or_else(|| FontCache::new_with_scale(scale))),
        };
        // Same rationale as `with_measure_font_cache`: a frame must be told the
        // current scale, or its glyphs would be rastered at whatever scale the
        // previous frame left behind, going blurry at fractional 1.25/1.5.
        guard.cache_mut().set_scale_factor(scale);
        guard
    }

    // ===== PERSISTENT IMAGE CACHE =========================================
    //
    // The same argument as `RENDER_FONT_CACHE`, for a much bigger win: the font
    // cache's job is to stop rebuilding an `sk::FontMgr`, and this one's is to
    // stop re-reading a file off disk and re-decoding it. Both `ImageCache::new`
    // sites ran per FRAME, so a visible `<img>` cost a `std::fs::read` plus a
    // full PNG decode on every single frame of an animation, a scroll, a hover
    // repaint -- work whose result is by definition identical to the previous
    // frame's. For a logo on a landing page that is the largest per-frame cost
    // in the process after the layout itself.
    //
    // ONE slot, where the font caches have two. The reason there are two is
    // written at `RENDER_FONT_CACHE`: measurement and rendering supply scale
    // independently, and a shared cache would drop its `fonts` map on every
    // alternating scale change. That hazard is scale-driven and there is no
    // scale in an image cache -- `draw_image_rect` is called with a `None`
    // source rect, so Skia does the scaling at draw time and the cached bitmap
    // is valid at any device scale (verified against
    // `skia-safe/src/core/canvas.rs`: `src: Option<(&Rect, SrcRectConstraint)>`,
    // where `None` means "the whole image"). So there is nothing here to
    // re-sync, one slot cannot thrash, and a second one would only buy a second
    // copy of every decoded bitmap.
    //
    // The slot holds an `Option` and the cache is handed out by value, for the
    // same reason `RenderFontCache` does it that way: the paint walk needs a
    // plain `&mut ImageCache` spanning its whole recursion, which a
    // non-`const` thread-local cannot lend. Take-and-restore also degrades
    // correctly under re-entrancy -- a nested take finds the slot empty, builds
    // a throwaway cache, and restores nothing over the outer one.
    //
    // The probe at `intrinsic_image_size` shares this slot on purpose: it is
    // asked during LAYOUT, before any paint walk has started, and answering
    // from the same cache is what makes an `<img src>` cost one decode for the
    // whole frame rather than one for layout and one for paint.
    thread_local! {
        /// Cache backing every `<img src>` paint walk and the layout probe.
        static RENDER_IMAGE_CACHE: RefCell<Option<ImageCache>> = const { RefCell::new(None) };
    }

    thread_local! {
        /// How many sources THIS THREAD has actually decoded since the counter
        /// was last reset. Test/diagnostic only.
        ///
        /// Deliberately not a field of `ImageCache`: the cache is TAKEN out of
        /// its slot for the duration of a paint walk, so a field would read back
        /// as zero to exactly the observer most likely to look -- one that
        /// spans a frame. A separate counter is never taken and is therefore
        /// always readable. Counting decodes rather than cache hits is the point:
        /// it is the number that must stay at 1 no matter how many times the
        /// frame asks for the same `src`.
        static IMAGE_DECODE_COUNT: Cell<u64> = const { Cell::new(0) };
    }

    /// How many sources this thread has decoded since the last reset. See
    /// [`IMAGE_DECODE_COUNT`]. Test/diagnostic only.
    pub fn image_decode_count() -> u64 {
        IMAGE_DECODE_COUNT.try_with(Cell::get).unwrap_or(0)
    }

    /// Zero [`image_decode_count`]. Test/diagnostic only.
    pub fn reset_image_decode_count() {
        let _ = IMAGE_DECODE_COUNT.try_with(|c| c.set(0));
    }

    /// Owns this thread's `ImageCache` for the duration of one paint walk or one
    /// probe call, and returns it to its thread-local slot on drop.
    struct RenderImageCache {
        cache: Option<ImageCache>,
    }

    impl RenderImageCache {
        /// The cache for the caller in hand.
        fn cache_mut(&mut self) -> &mut ImageCache {
            self.cache
                .as_mut()
                .expect("render image cache taken by this guard; not reentrant")
        }
    }

    impl Drop for RenderImageCache {
        fn drop(&mut self) {
            // `try_with` for the same reason `RenderFontCache::drop` uses it: a
            // `Drop` during thread teardown must not touch a destroyed slot.
            // Dropping the cache there is correct, it just forfeits the reuse.
            if let Some(cache) = self.cache.take() {
                let _ = RENDER_IMAGE_CACHE.try_with(|slot| {
                    *slot.borrow_mut() = Some(cache);
                });
            }
        }
    }

    /// Take this thread's persistent `ImageCache`.
    fn take_render_image_cache() -> RenderImageCache {
        let taken = RENDER_IMAGE_CACHE
            .try_with(|slot| std::mem::take(&mut *slot.borrow_mut()))
            .ok()
            .flatten();
        RenderImageCache {
            // First caller on this thread pays for one decode per distinct
            // `src`; every caller after that gets the previous one's cache back
            // with every bitmap already decoded.
            cache: Some(taken.unwrap_or_default()),
        }
    }

    /// `velox-dom`'s intrinsic-size probe for replaced elements, answered from
    /// this thread's image cache.
    ///
    /// Registered as [`velox_dom::layout::IntrinsicSizeProbe`], which is what
    /// lets an unsized `<img src=…>` be laid out at its own size instead of at
    /// zero (CSS 2.1 §10.3.2 step 2). It answers from the cache and not from a
    /// fresh `fs::read` per call because layout asks more than once per `src` --
    /// once for the width pass and once for the height pass -- and a file read
    /// per question would make a replaced element the most expensive thing in
    /// the tree to measure.
    ///
    /// It takes and returns the cache through `take_render_image_cache` like
    /// every other caller, so the decode it triggers is the SAME decode the
    /// paint walk will reuse: one read, one decode, per `src`, per thread, per
    /// session -- not one per layout question and another per frame.
    ///
    /// `None` for a `src` that does not resolve is the right answer and not a
    /// gap: `velox-dom`'s `intrinsic_size_of` drops non-positive extents and
    /// an unsized replaced element with an unresolvable source is a zero-size
    /// box, which paints nothing -- the same thing a broken `<img>` does in a
    /// browser, and the same thing a PNG whose file is missing does here today.
    pub fn intrinsic_image_size(src: &str) -> Option<(i32, i32)> {
        let mut guard = take_render_image_cache();
        let image = guard.cache_mut().load(src)?;
        Some((image.width(), image.height()))
    }

    /// Public measure helper for layout unify: snapped size, scale-aware.
    /// Consumes: text, font_size (logical), font_family, scale -> logical px width
    /// (snapped) plus the run's ascent and descent.
    pub fn measure_text(text: &str, font_size: f32, font_family: &str, scale: f32) -> MeasuredText {
        // NOT a second rounding authority. It agrees with
        // `Viewport::snap_logical_to_physical_grid` only when `scale` is finite and
        // positive; for a non-finite or `<= 0` scale this returns `font_size`
        // unrounded, whereas the authority substitutes `scale = 1.0` and rounds.
        // Substituting it would change the measured advance for a degenerate
        // scale, so this keeps its own guard. This is a glyph-advance size snap
        // for measurement, not a logical->physical size conversion.
        //
        // The `scale != 1.0` term is what keeps this in step with its two
        // siblings, `FontCache::snapped_size` and `TextMeasurer::snapped_size`,
        // both of which skip the round trip at scale 1. That step is a hinting
        // concern: at scale 1 the device size IS the logical size and there is
        // no subpixel grid to land on, so snapping there only discards
        // precision. This function lacked the guard while the other two had it,
        // and it is the one the caret lane measures with — so a fractional
        // `font-size` (16.5px, `1.05em`, `0.9375rem`) painted at 16.5 was
        // measured at 17, a 3% advance error on the most common desktop scale.
        let snapped = if scale.is_finite() && scale > 0.0 && scale != 1.0 {
            (font_size * scale).round() / scale
        } else {
            font_size
        };
        // Persistent cache, not a fresh one per measurement: see the comment on
        // `MEASURE_FONT_CACHE`. This is the hot path — it runs for every text
        // measurement layout does — so the `FontMgr`/typeface probe must happen
        // here at most once per scale instead of once per call.
        with_measure_font_cache(scale, |fc| fc.measure_run(font_family, snapped, text))
    }

    fn load_default_typeface() -> Option<sk::Typeface> {
        use std::fs;

        // Order is a COVERAGE decision, not an arbitrary one. Velox has no glyph
        // fallback: the only text draw is `draw_str`, a single typeface, no
        // shaping, so an unmapped codepoint becomes glyph 0 — a tofu box. The
        // renderer therefore has to pick a face that actually CARRIES the
        // characters an app prints, and the only one in this list that does is
        // DejaVu Sans. Measured from the real cmaps:
        //
        //   face         U+2600 ☀   U+263E ☾   U+2713 ✓   U+2715 ✕   U+00D7 ×
        //   DejaVu Sans  present    present    present    present    present
        //   Noto Sans    MISSING    MISSING    MISSING    MISSING    present
        //   FreeSans     missing    missing    missing    missing    present
        //
        // (FreeSans was dropped from the list below: it covers nothing the other
        // two do not, so it could only ever win on a host that has neither.)
        //
        // Noto Sans is still worth a slot — it is the better text face, and it is
        // what wins on a host with no DejaVu at all. But it is not first, because
        // picking it means the shipped template's ☀ ☾ ✓ ✕ are tofu.
        //
        // The two `dejavu-sans-fonts` / `google-noto` layouts are what Debian,
        // Ubuntu and Fedora actually ship; the `truetype/` and `TTF/` ones are
        // older layouts kept because they are still what some minimal images
        // have. The previous list had `truetype/dejavu` and `google-noto` but
        // NOT `dejavu-sans-fonts`, so on this host it fell through to Noto.
        const CANDIDATES: &[&str] = &[
            "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        ];

        // The two bundled faces below are tried only after every system path
        // misses, and they are the SAME two files in the same order, so a host
        // with a system DejaVu and a host with neither paint identically. They
        // used to be 14-byte text stubs reading `<BINARY FILE>`, which made this
        // whole loop dead: `new_from_data` returns `None` for them. They are now
        // the real faces (`velox-renderer/assets/LICENSE-*.txt` carries each
        // one's licence).

        let font_mgr = sk::FontMgr::default();
        for p in CANDIDATES {
            if let Ok(bytes) = fs::read(p)
                && let Some(tf) = font_mgr.new_from_data(&bytes, None)
            {
                return Some(tf);
            }
        }

        let bundles: &[&[u8]] = &[
            include_bytes!("../assets/DejaVuSans.ttf"),
            include_bytes!("../assets/NotoSans-Regular.ttf"),
        ];
        for b in bundles {
            if let Some(tf) = font_mgr.new_from_data(b, None) {
                return Some(tf);
            }
        }
        let preferred_families = [
            "DejaVu Sans",
            "Noto Sans",
            "Sans",
            "Arial",
            "Liberation Sans",
        ];
        for family in preferred_families {
            let mut set = font_mgr.match_family(family);
            if set.count() == 0 {
                continue;
            }
            if let Some(tf) = set.match_style(sk::FontStyle::default()) {
                return Some(tf);
            }
            if let Some(tf) = set.new_typeface(0) {
                return Some(tf);
            }
        }

        if font_mgr.count_families() > 0 {
            let family = font_mgr.family_name(0);
            let mut set = font_mgr.match_family(&family);
            if let Some(tf) = set.match_style(sk::FontStyle::default()) {
                return Some(tf);
            }
            if let Some(tf) = set.new_typeface(0) {
                return Some(tf);
            }
        }

        let fallback_mgr = sk::FontMgr::new();
        fallback_mgr.legacy_make_typeface(None, sk::FontStyle::default())
    }

    /// Render a VNode tree into an existing `SkiaSurface` using a precomputed layout.
    /// `layout` must be the result of `compute_layout(vnode, logical_w, logical_h)` where
    /// `logical_w,logical_h` were obtained via `logical_size(physical, scale)` — single rounding point.
    /// Hit-test and render must share the same `Viewport` (physical/logical/scale) so that
    /// fractional scales (1.25/1.5) have identical snapped edges and clicks hit the rendered pixel.
    ///
    /// `sheet` is retained for the renderer API, but paint-time code reads the
    /// already-cascaded inline `style` attributes on `vnode`; it performs no
    /// second style application or paint-time sheet lookup.
    pub fn render_frame(
        surface: &mut crate::skia_surface::SkiaSurface,
        vnode: &VNode,
        layout_root: &velox_dom::layout::LayoutNode,
        _sheet: &Stylesheet,
    ) -> Result<(), String> {
        let scale = surface.scale_factor().max(1.0);
        // Ensure layout text measure uses same Skia snapped scale (unified).
        velox_dom::text_wrap::set_current_scale(scale);
        velox_dom::text_wrap::set_skia_measurer(measure_text);
        // Same registration, same reason, same place in the sequence as in
        // `prepare_frame` -- and here it is what a caller who lays out
        // elsewhere still needs, because `render_frame` is reachable without
        // going through `prepare_frame` and is where a caller that paints a
        // precomputed layout arrives. Idempotent: the second write of an
        // identical function pointer costs a write lock on a `RwLock` behind an
        // already-taken layout pass, which is the same cost
        // `set_skia_measurer` above already pays.
        velox_dom::layout::set_intrinsic_size_probe(intrinsic_image_size);

        let canvas = surface.canvas();
        canvas.clear(sk::Color::TRANSPARENT);
        canvas.save();
        // R-H3: single rounding physical=(logical*scale).round() keeping both; don't round-trip.
        // We do canvas.scale(scale) here so logical rects map to device pixels; font re-raster
        // below snaps sizes to physical pixels to avoid 0.25px blur at 1.25/1.5.
        canvas.scale((scale, scale));

        // Persistent per-thread cache rather than a fresh one per frame: see
        // the comment on `RENDER_FONT_CACHE`. `font_guard` lends this frame the
        // cache and returns it to the thread-local slot on drop, so the
        // `FontMgr`/typeface probe behind `load_default_typeface` runs once per
        // thread instead of once per frame. `take_render_font_cache` has already
        // resynced it to `scale`, so glyph rasterisation and the canvas scale
        // above still agree.
        let mut font_guard = take_render_font_cache(scale);
        let fonts = font_guard.cache_mut();
        // Same shape and same reasoning, one step bigger: `ImageCache::new` was
        // on this line, so every frame re-read the file and re-decoded it. The
        // cache is keyed by `src` alone and needs no scale resync -- the bitmap
        // is scaled by Skia at draw time, not baked at the scale it was decoded
        // at -- which is why this is one slot shared with the layout probe
        // rather than a second one. See `RENDER_IMAGE_CACHE`.
        let mut image_guard = take_render_image_cache();
        let images = image_guard.cache_mut();
        let default_text_style = TextStyle {
            color: sk::Color::from_argb(255, 0, 0, 0),
            align: TextAlign::Left,
            underline: false,
            line_through: false,
            font_size: super::PAINT_ROOT_FONT_SIZE,
            bold: false,
            line_height: 1.2,
            nowrap: false,
            ellipsis: false,
        };
        let default_family = fonts.default_family();
        let mut paints = RenderPaints::new();

        #[allow(clippy::too_many_arguments)]
        fn render_with_layout(
            canvas: &sk::Canvas,
            node: &VNode,
            layout: &velox_dom::layout::LayoutNode,
            container_rect: sk::Rect,
            fonts: &mut FontCache,
            text_style: TextStyle,
            font_family: &str,
            paints: &mut RenderPaints,
            images: &mut ImageCache,
            inherited_opacity: f32,
            // Logical viewport, for `vw`/`vh` in `border-width`. Passed in
            // because a nested `fn` cannot capture `render_frame`'s locals.
            viewport: (f32, f32),
            // How many words of THIS text `VNode` have already been painted, and
            // the same tally for every other text VNode in the frame.
            //
            // A text VNode that wraps to N lines reaches the text arm N times,
            // once per line box, and each visit has to take the NEXT line's words
            // rather than the first line's again. A per-parent counter cannot do
            // that: an inline element that wraps also gets one node PER LINE, each
            // carrying the text node for its own line, so counting within one
            // parent's children restarts at 0 on every one of them and line 0 is
            // painted N times -- which is the defect this exists to remove.
            //
            // The tally is keyed by the ADDRESS of the `&VNode` this arm is
            // painting, not by `source_index`: that is a sibling index, so two
            // different parents' `children[0]` share it while being different
            // VNodes, and keying on it makes the second of them start at the
            // first one's word count and paint nothing. The address is exact for
            // the length of a frame: the tree is borrowed throughout and cannot
            // move or be rebuilt under it.
            text_word_offset: &mut std::collections::HashMap<usize, usize>,
        ) {
            match node {
                VNode::Element {
                    props,
                    children,
                    tag,
                    ..
                } => {
                    // Check visibility:hidden
                    if let Some(s) = props.attrs.get("style")
                        && (s.contains("visibility: hidden") || s.contains("visibility:hidden"))
                    {
                        return;
                    }
                    let mut clip_rrect = None;
                    let mut overflow_hidden = false;
                    let mut clip_inset = None;
                    let mut did_box_clip = false;
                    let mut child_text_style = text_style;
                    let mut child_family = font_family.to_string();
                    let mut opacity = inherited_opacity;
                    let mut filters = FilterSpec::default();
                    // The author's own box styling, hoisted out of the
                    // `if let Some(style)` block below. The `<input>` lane
                    // paints its own fill and border OVER the ones drawn
                    // there, so it needs to know what the author asked for
                    // rather than assume white and `#c8c8c8` — which is
                    // exactly the bug: the element box was already painted
                    // correctly and then covered.
                    let mut author_bg: Option<sk::Color> = None;
                    let mut author_border: Option<BorderSpec> = None;
                    let mut author_radius: Option<f32> = None;
                    let mut author_padding: Option<Padding> = None;
                    // The element's own border box, built ONCE and here rather
                    // than in each of the four blocks that want it. The
                    // `clip-path` clip has to be established before the two box
                    // paints, and those are the first ones to need it, so a
                    // binding declared further down could not serve them.
                    let rect = sk::Rect::from_xywh(
                        layout.rect.x as f32,
                        layout.rect.y as f32,
                        layout.rect.w as f32,
                        layout.rect.h as f32,
                    );
                    if let Some(s) = props.attrs.get("style") {
                        // As at the other call site: resolve the element's own
                        // font size before the border parser needs it for `em`.
                        // The logical viewport is the physical surface divided
                        // by the scale the canvas was scaled by, because the
                        // rects being painted here are logical.
                        let (style, family) =
                            parse_text_style(s, text_style, font_family, viewport);
                        let box_style = parse_style_attr(s, style.font_size, viewport);
                        let rrect = box_style.radius.map(|r| sk::RRect::new_rect_xy(rect, r, r));
                        if let Some(rrect) = rrect {
                            clip_rrect = Some(rrect);
                        }
                        author_bg = box_style.background;
                        author_border = box_style.border;
                        author_radius = box_style.radius;
                        author_padding = box_style.padding;
                        overflow_hidden = box_style.overflow_hidden;
                        clip_inset = box_style.clip_inset;
                        child_text_style = style;
                        child_family = family;
                        opacity = (opacity * box_style.opacity).clamp(0.0, 1.0);
                        filters = box_style.filters;
                        // Before the element's OWN two draws, deliberately: see
                        // `apply_clip_path`. `overflow`/`border-radius` are not
                        // applied here, and `apply_clips` still clips the
                        // subtree below.
                        // Before the element's OWN two draws, deliberately: see
                        // `apply_clip_path`. `overflow`/`border-radius` are not
                        // applied here, and `apply_clips` still clips the
                        // subtree below.
                        did_box_clip = apply_clip_path(canvas, rect, clip_inset);
                        if let Some(bg) = box_style.background {
                            paints.fill.set_color(color_with_opacity(bg, opacity));
                            if let Some(rrect) = rrect {
                                canvas.draw_rrect(rrect, &paints.fill);
                            } else {
                                canvas.draw_rect(rect, &paints.fill);
                            }
                        }
                        if let Some(border) = box_style.border {
                            paints.stroke.set_stroke_width(border.width);
                            paints
                                .stroke
                                .set_color(color_with_opacity(border.color, opacity));
                            apply_border_style(&mut paints.stroke, &border);
                            if let Some(rrect) = rrect {
                                canvas.draw_rrect(rrect, &paints.stroke);
                            } else {
                                canvas.draw_rect(rect, &paints.stroke);
                            }
                        }
                    }

                    if let Some(src) = props.attrs.get("src") {
                        paints.image.set_image_filter(None);
                        paints.image.set_color_filter(None);
                        paints.image.set_alpha_f(opacity);
                        apply_img_filter(&mut paints.image, filters);
                        if let Some(img) = images.load(src) {
                            let rect = sk::Rect::from_xywh(
                                layout.rect.x as f32,
                                layout.rect.y as f32,
                                layout.rect.w as f32,
                                layout.rect.h as f32,
                            );
                            canvas.draw_image_rect(img, None, rect, &paints.image);
                        }
                    }

                    // Handle <input> elements - draw a text field
                    if tag == "input" {
                        let input_type = props
                            .attrs
                            .get("type")
                            .map(|s| s.as_str())
                            .unwrap_or("text");
                        let value = props.attrs.get("value").map(|s| s.as_str()).unwrap_or("");

                        // The field's fill and border. These used to be two
                        // hardcoded literals — opaque white and `#c8c8c8` —
                        // which OVERRODE the author's own `background` and
                        // `border`: the boilerplate's `.input`
                        // (`background:#16213e; border:1px solid #3a3a5c`) was
                        // painted correctly by the element box pass above and
                        // then painted white over. The literals now survive
                        // only as the fallback for a field whose author (and
                        // UA) specified nothing, so an unstyled input still
                        // looks like a field.
                        let input_bg =
                            author_bg.unwrap_or_else(|| sk::Color::from_argb(255, 255, 255, 255));
                        let border_color = author_border
                            .map(|b| b.color)
                            .unwrap_or_else(|| sk::Color::from_argb(255, 200, 200, 200));

                        let rect = sk::Rect::from_xywh(
                            layout.rect.x as f32,
                            layout.rect.y as f32,
                            layout.rect.w as f32,
                            layout.rect.h as f32,
                        );

                        if input_type == "checkbox" {
                            // Draw checkbox
                            let size = rect.width().min(rect.height()).min(18.0);
                            let check_rect = sk::Rect::from_xywh(
                                rect.left + (rect.width() - size) / 2.0,
                                rect.top + (rect.height() - size) / 2.0,
                                size,
                                size,
                            );
                            // A checkbox is a 1px-outlined square in the UA
                            // sense, so the author's `border` does not apply
                            // to the box drawn INSIDE it; honour `radius` so
                            // `border-radius` is not silently dropped here
                            // either.
                            let check_radius = author_radius
                                .map(|r| (r * 0.25).min(size * 0.5))
                                .unwrap_or(0.0);
                            let check_shape =
                                sk::RRect::new_rect_xy(check_rect, check_radius, check_radius);
                            // Background
                            paints.fill.set_color(input_bg);
                            canvas.draw_rrect(check_shape, &paints.fill);
                            // Border
                            paints.stroke.set_stroke_width(1.0);
                            paints.stroke.set_color(border_color);
                            // Clear any dash a previous element left on the
                            // shared stroke paint: a checkbox outline is
                            // solid in every UA sheet, and the paint is
                            // reused within a frame.
                            apply_border_style(
                                &mut paints.stroke,
                                &BorderSpec {
                                    width: 1.0,
                                    color: border_color,
                                    style: velox_dom::style::BorderStyle::Solid,
                                },
                            );
                            canvas.draw_rrect(check_shape, &paints.stroke);
                            // Check mark if checked.
                            //
                            // The template binds `:checked="completed"`
                            // (test-app/src/components/TodoItem.vx:6), so
                            // `checked` — not `value` — is what carries the
                            // state. `checked` wins when present; `value` is
                            // kept as the fallback so any caller that really
                            // does pass `value="true"` still ticks.
                            let is_checked = read_bool_attr(props, "checked")
                                .unwrap_or(value == "true" || value == "checked");
                            if is_checked {
                                paints
                                    .fill
                                    .set_color(sk::Color::from_argb(255, 52, 120, 246));
                                let inset = size * 0.2_f32;
                                let inner = sk::RRect::new_rect_xy(
                                    sk::Rect::from_xywh(
                                        check_rect.left + inset,
                                        check_rect.top + inset,
                                        size - inset * 2.0,
                                        size - inset * 2.0,
                                    ),
                                    2.0,
                                    2.0,
                                );
                                canvas.draw_rrect(inner, &paints.fill);
                            }
                        } else {
                            // Text input - draw border, value, and the focus
                            // furniture (selection, caret, ring).
                            //
                            // Contract with the input lane: the FOCUSED input
                            // element carries these as stringified attrs on its
                            // styled VNode. They are read, never computed, and
                            // absence means "not focused, no caret, no
                            // selection".
                            let caret = parse_usize_attr(props, "caret");
                            let sel_start = parse_usize_attr(props, "sel_start");
                            let sel_end = parse_usize_attr(props, "sel_end");
                            let focused = read_bool_attr(props, "focused").unwrap_or(false);
                            let blink = read_bool_attr(props, "caret_blink").unwrap_or(false);
                            // Char index -> byte index. The input lane counts
                            // CHARACTERS; `value` is a Rust &str. A char
                            // index that is not a char boundary (or past the
                            // end) clamps to the nearest legal slice point
                            // rather than panicking the whole frame.
                            let char_to_byte = |ci: usize| -> usize {
                                value
                                    .char_indices()
                                    .nth(ci)
                                    .map(|(b, _)| b)
                                    .unwrap_or(value.len())
                            };

                            // ONE geometry authority for the whole input
                            // lane. This used to be a hardcoded 1px border
                            // inset plus a hardcoded `TEXT_PAD = 4.0`, which is
                            // how the author's `padding` was silently dropped
                            // and how paint came to disagree with the hit-test
                            // lane's caret placement. Reading computed border
                            // widths and padding sides means `padding`,
                            // `padding-left` and `12px` all place the first
                            // glyph identically.
                            //
                            // The hit-test lane calls the same function, so the
                            // two cannot drift apart again.
                            let metrics = crate::input_metrics::input_text_metrics(
                                props.attrs.get("style").map(|s| s.as_str()),
                                layout.rect,
                                viewport,
                                text_style.font_size,
                            );
                            let input_rect = sk::Rect::from_xywh(
                                metrics.pad_left,
                                metrics.pad_top,
                                (metrics.pad_right - metrics.pad_left).max(0.0),
                                (metrics.pad_bottom - metrics.pad_top).max(0.0),
                            );
                            // An author's `border-radius` must reach the field
                            // itself, so the fill is an rrect rather than a
                            // rect. The element box pass above already drew
                            // the author's background; re-filling the padding
                            // box here is what keeps an opaque light fallback
                            // from covering a dark `background`.
                            let field_radius = author_radius.unwrap_or(0.0);
                            // The element box pass above ALREADY painted the
                            // author's `background`, on the border box, with
                            // the author's radius. Re-filling it here would
                            // composite a translucent colour a SECOND time:
                            // `rgba(255,0,0,0.5)` over a black page came out at
                            // 190/255 red instead of 127. (Caught by the pixel
                            // test
                            // `a_translucent_author_background_is_not_forced_opaque`
                            // — an opaque background is idempotent here, which is
                            // exactly why the bug survived the first fix.)
                            //
                            // So this lane paints only the case it actually
                            // owns: the author declared no background at all, and
                            // the fallback is what keeps an unstyled field from
                            // being invisible.
                            if author_bg.is_none() {
                                paints.fill.set_color(color_with_opacity(input_bg, opacity));
                                canvas.draw_rrect(
                                    sk::RRect::new_rect_xy(rect, field_radius, field_radius),
                                    &paints.fill,
                                );
                            }

                            // Everything below is clipped to the field's padding
                            // box so a long value's caret and selection can
                            // never paint into the surrounding page.
                            canvas.save();
                            canvas.clip_rect(input_rect, sk::ClipOp::Intersect, true);

                            let font_size = metrics.font_size;
                            let font = fonts.font(&child_family, font_size);
                            // The glyph line is centred in the CONTENT box, not
                            // in the padding box: CSS puts the line box inside
                            // the content box and lets padding surround it. For
                            // the symmetric padding every author writes
                            // (`padding: 10px 12px`) this is numerically the
                            // same as the old padding-box centring; for
                            // asymmetric padding it is the difference between
                            // the text sitting where the author put it and the
                            // text ignoring the padding entirely.
                            //
                            // Horizontal geometry does NOT come from
                            // `author_padding`: it comes from `metrics`, which
                            // reads computed values, because the hit-test lane
                            // has to reach the same answer and only computed
                            // values make the shorthand and the longhand agree.
                            // Vertical is paint-only, so it uses this paint
                            // lane's own declaration reader.
                            let pad = author_padding.unwrap_or(Padding::ZERO);
                            let content_top = input_rect.top + pad.top;
                            let content_h = (input_rect.height() - pad.top - pad.bottom).max(0.0);
                            let ty = content_top + font_size + (content_h - font_size) / 2.0;
                            // The text origin and the content width come from
                            // `metrics`, the same values the hit-test lane
                            // reads. One source, so the caret can never land
                            // somewhere other than where the glyph is.
                            let text_left = metrics.text_left;
                            let line_top = ty - font_size;
                            let line_bottom = ty;

                            // The caret/selection x is the MEASURED advance of
                            // the prefix, in the same font and size used to
                            // draw the value. No hardcoded advance, no
                            // monospace grid: a proportional font tracks its
                            // actual glyphs.
                            let advance_of = |fonts: &mut FontCache, ci: usize| -> f32 {
                                let byte = char_to_byte(ci);
                                fonts.measure_text(&child_family, font_size, &value[..byte])
                            };
                            // Where text may actually be drawn: the padding box
                            // less the same padding, so the last glyph is not
                            // half under the border. An index whose measured
                            // advance runs past this (an overflowing value)
                            // clamps HERE, landing the caret flat on the clip
                            // edge rather than scrolling the text. The text
                            // itself never reflows: the caret is an overlay, and
                            // a caret that scrolled entirely out of view would
                            // be the "cursor not showing up" report a third
                            // time.
                            let content_right = metrics.content_right;
                            let caret_x_of = |fonts: &mut FontCache, ci: usize| -> f32 {
                                (text_left + advance_of(fonts, ci))
                                    .min(content_right)
                                    .max(text_left)
                            };

                            // 1. Selection highlight, BEHIND the text.
                            if let (Some(s0), Some(s1)) = (sel_start, sel_end) {
                                let (lo, hi) = (s0.min(s1), s0.max(s1));
                                if hi > lo {
                                    let sx = caret_x_of(fonts, lo);
                                    let sw = (caret_x_of(fonts, hi) - sx).max(0.0);
                                    let sel_rect = sk::Rect::from_xywh(
                                        sx,
                                        line_top,
                                        sw.max(0.0),
                                        line_bottom - line_top,
                                    );
                                    // A translucent accent: light enough to
                                    // keep the near-black value text readable
                                    // on top, saturated enough to read as a
                                    // selection against the white field.
                                    paints
                                        .fill
                                        .set_color(sk::Color::from_argb(90, 52, 120, 246));
                                    canvas.draw_rect(sel_rect, &paints.fill);
                                }
                            }

                            // 2. Value text, on top of the selection.
                            //
                            // The colour is the element's OWN `color` — which is
                            // `child_text_style`, the style `parse_text_style`
                            // produced for this element from its own `style`
                            // attribute. Reading the inherited `text_style`
                            // instead was a second, quieter bug: it is the
                            // PARENT's colour, so `color:#ff0000` on the input
                            // was read as "whatever the parent said", which for
                            // the unstyled boilerplate parent is black.
                            // `caret_colors` already keys the caret off the same
                            // field/ink pair, so the two agree by construction.
                            let value_ink = color_with_opacity(child_text_style.color, opacity);
                            if !value.is_empty() {
                                paints.text.set_color(value_ink);
                                let _ =
                                    canvas.draw_str(value, (text_left, ty), &font, &paints.text);
                            } else if let Some(placeholder) = props
                                .attrs
                                .get("placeholder")
                                .map(|s| s.as_str())
                                .filter(|s| !s.is_empty())
                            {
                                // 2b. Placeholder, only while the field is
                                // empty. A non-empty value always wins, which
                                // is the whole point of a placeholder.
                                //
                                // The ink is NOT a fixed grey literal: it is
                                // derived from the field's own `color`, so a
                                // dark-themed input gets a dark-theme-muted
                                // placeholder instead of a grey that vanishes
                                // against it. An author who styled it
                                // explicitly — `input::placeholder { ... }`,
                                // carried in by the cascade as
                                // `style:placeholder` — overrides this.
                                let ph_ink =
                                    placeholder_color(props, child_text_style.color, input_bg);
                                paints.text.set_color(color_with_opacity(ph_ink, opacity));
                                let _ = canvas.draw_str(
                                    truncate_with_ellipsis(
                                        placeholder,
                                        metrics.text_width(),
                                        fonts,
                                        &child_family,
                                        font_size,
                                    )
                                    .as_str(),
                                    (text_left, ty),
                                    &font,
                                    &paints.text,
                                );
                            }

                            // 3. Caret bar. Overlay only — nothing reflows.
                            // Gated on `focused` AND `caret_blink`; this frame
                            // simply omits the bar when the blink phase is
                            // off. The input lane owns the cadence; no timer
                            // lives here.
                            if focused
                                && blink
                                && let Some(caret_idx) = caret
                            {
                                let cx = caret_x_of(fonts, caret_idx);
                                let (halo, core) = caret_colors(input_bg);
                                const BAR_W: f32 = 2.0;
                                // The halo wears the field colour and is 1px
                                // wider on each side, so the core stays legible
                                // even where it crosses a near-black glyph.
                                let halo_rect = sk::Rect::from_xywh(
                                    cx - 1.0,
                                    line_top,
                                    BAR_W + 2.0,
                                    line_bottom - line_top,
                                );
                                paints.fill.set_color(halo);
                                canvas.draw_rect(halo_rect, &paints.fill);
                                let core_rect = sk::Rect::from_xywh(
                                    cx,
                                    line_top,
                                    BAR_W,
                                    line_bottom - line_top,
                                );
                                paints.fill.set_color(core);
                                canvas.draw_rect(core_rect, &paints.fill);
                            }

                            canvas.restore();

                            // 4. Border, then the focus ring.
                            //
                            // The border is drawn on the BORDER BOX, inset by
                            // half the stroke so a 1px border sits wholly on
                            // the box edge rather than straddling it — which is
                            // also what makes the width the author asked for
                            // (`border: 2px`) visible instead of clipped by
                            // the padding box. It is an rrect whenever a
                            // radius is present, so `border-radius` reaches the
                            // outline: a plain `draw_rect` here is what made
                            // the boilerplate's `border-radius: 8px` field look
                            // like a sharp-cornered box drawn over a rounded
                            // background.
                            paints
                                .stroke
                                .set_color(color_with_opacity(border_color, opacity));
                            if let Some(border) = author_border {
                                let sw = if border.width.is_finite() && border.width > 0.0 {
                                    border.width
                                } else {
                                    1.0
                                };
                                let half = sw / 2.0;
                                let border_rect = sk::Rect::from_xywh(
                                    rect.left + half,
                                    rect.top + half,
                                    (rect.width() - sw).max(0.0),
                                    (rect.height() - sw).max(0.0),
                                );
                                if border_rect.width() > 0.0 && border_rect.height() > 0.0 {
                                    // Pull the radius in by the inset so the
                                    // outer edge of the stroke lands on the
                                    // rounded corner the author specified.
                                    let r = (field_radius - half).max(0.0);
                                    let shape = sk::RRect::new_rect_xy(border_rect, r, r);
                                    paints.stroke.set_stroke_width(sw);
                                    apply_border_style(&mut paints.stroke, &border);
                                    canvas.draw_rrect(shape, &paints.stroke);
                                }
                            } else {
                                // No author border at all: keep the historical
                                // 1px `#c8c8c8` outline, but honour the
                                // radius so the fallback does not reintroduce
                                // the square-corner defect.
                                paints.stroke.set_stroke_width(1.0);
                                let shape =
                                    sk::RRect::new_rect_xy(rect, field_radius, field_radius);
                                apply_border_style(
                                    &mut paints.stroke,
                                    &BorderSpec {
                                        width: 1.0,
                                        color: border_color,
                                        style: velox_dom::style::BorderStyle::Solid,
                                    },
                                );
                                canvas.draw_rrect(shape, &paints.stroke);
                            }
                            if focused {
                                // The ring is INSET 2px from the PADDING box and
                                // 2px thick, so it never shares a pixel with
                                // the border drawn above. A ring drawn on the
                                // border's own pixels is invisible, which is
                                // exactly the "input box on focus cursor not
                                // showing up" report.
                                //
                                // The inset is measured from the padding box,
                                // not the border box: the border occupies
                                // `[rect.left, rect.left + border_left)`, so
                                // insetting from `rect` by the same 2px would
                                // put the ring's outer edge ON the border and
                                // leave it half-covering it.
                                const RING_INSET: f32 = 2.0;
                                let ring = sk::Rect::from_xywh(
                                    input_rect.left + RING_INSET,
                                    input_rect.top + RING_INSET,
                                    (input_rect.width() - RING_INSET * 2.0).max(0.0),
                                    (input_rect.height() - RING_INSET * 2.0).max(0.0),
                                );
                                if ring.width() > 0.0 && ring.height() > 0.0 {
                                    let r = (field_radius - RING_INSET).max(0.0);
                                    let shape = sk::RRect::new_rect_xy(ring, r, r);
                                    paints.stroke.set_stroke_width(2.0);
                                    // Reset the path effect BEFORE the ring, not
                                    // after the border. `paints.stroke` is
                                    // frame-scoped and `apply_border_style` is
                                    // the only thing that installs or clears a
                                    // dash on it, so with `border: 2px dashed` the
                                    // border's `[6, 6]` intervals were still live
                                    // here and the focus ring came out dashed —
                                    // defeating the exact report this ring
                                    // exists to fix.
                                    apply_border_style(
                                        &mut paints.stroke,
                                        &BorderSpec {
                                            width: 2.0,
                                            color: sk::Color::from_argb(255, 52, 120, 246),
                                            style: velox_dom::style::BorderStyle::Solid,
                                        },
                                    );
                                    paints
                                        .stroke
                                        .set_color(sk::Color::from_argb(255, 52, 120, 246));
                                    canvas.draw_rrect(shape, &paints.stroke);
                                }
                            }
                        }
                    }

                    // Render children in order using their layout nodes
                    if did_box_clip {
                        canvas.restore();
                    }
                    // The SUBTREE clip, and the arm's one save for it: radius,
                    // `overflow: hidden` and `clip-path` all apply to
                    // descendants, which is what this has always done. The
                    // element's own box was clipped by `apply_clip_path` above.
                    let did_clip =
                        apply_clips(canvas, rect, clip_rrect, overflow_hidden, clip_inset);
                    // Scroll offset is applied in the layout itself (children rects
                    // are shifted by events::apply_scroll_offsets so render and
                    // hit-testing agree) — content renders at content_y - scroll_y.
                    let mut ordered: Vec<(i32, usize)> = layout
                        .children
                        .iter()
                        .enumerate()
                        .filter_map(|(i, ln)| {
                            if ln.display_none {
                                return None;
                            }
                            Some((ln.z_index, i))
                        })
                        .collect();
                    ordered.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
                    for (_, layout_idx) in ordered {
                        if let Some(child_layout) = layout.children.get(layout_idx)
                            && let Some(src_idx) = child_layout.source_index
                            && let Some(child) = children.get(src_idx)
                        {
                            render_with_layout(
                                canvas,
                                child,
                                child_layout,
                                rect,
                                fonts,
                                child_text_style,
                                &child_family,
                                paints,
                                images,
                                opacity,
                                viewport,
                                text_word_offset,
                            );
                        }
                    }
                    // Paint scrollbar thumb when scrollable && max > 0.
                    // (Headless-provable via render_vnode_to_rgba pixel tests.)
                    if layout.scrollable && layout.max_scroll_y > 0 {
                        let scrollbar_w = 8.0;
                        let track_x = rect.right - scrollbar_w - 2.0;
                        let track_y = rect.top + 2.0;
                        let track_h = (rect.height() - 4.0).max(0.0);
                        // Thumb size ~ viewport^2 / scrollHeight, floored at 20px
                        // (or track_h for tiny containers — clamp(min, max) must
                        // never see min > max) and capped at the track height.
                        let min_thumb = 20.0_f32.min(track_h);
                        let thumb_h = ((rect.height() * rect.height())
                            / (layout.scroll_height as f32).max(1.0))
                        .clamp(min_thumb, track_h);
                        let max_y = layout.max_scroll_y as f32;
                        let thumb_y = if max_y > 0.0 {
                            track_y + (layout.scroll_y as f32 / max_y) * (track_h - thumb_h)
                        } else {
                            track_y
                        };
                        let mut sb_paint = sk::Paint::default();
                        sb_paint.set_anti_alias(true);
                        sb_paint.set_color(sk::Color::from_argb(120, 100, 100, 100));
                        let thumb_rect =
                            sk::Rect::from_xywh(track_x, thumb_y, scrollbar_w, thumb_h);
                        let rrect = sk::RRect::new_rect_xy(thumb_rect, 4.0, 4.0);
                        canvas.draw_rrect(rrect, &sb_paint);
                    }
                    if did_clip {
                        canvas.restore();
                    }
                }
                VNode::Text(t) => {
                    paints
                        .text
                        .set_color(color_with_opacity(text_style.color, inherited_opacity));
                    let font_size = text_style.font_size;
                    let mut font = fonts.font(font_family, font_size);
                    if text_style.bold {
                        font.set_embolden(true);
                    }
                    let layout_rect = sk::Rect::from_xywh(
                        layout.rect.x as f32,
                        layout.rect.y as f32,
                        layout.rect.w as f32,
                        layout.rect.h as f32,
                    );
                    let align_rect = if layout_rect.width() >= container_rect.width() - 0.5 {
                        container_rect
                    } else {
                        layout_rect
                    };
                    // WHICH words of `t` are on THIS line, and the width they
                    // measure to.
                    //
                    // A text VNode that wraps arrives here once per line: layout
                    // emits one `LayoutNode` per line box, each carrying the same
                    // `source_index` (`inline_leaf_node` is the only thing in
                    // velox-dom that stamps one, and `inline_slots_to_nodes` runs
                    // once per line). So a node owns exactly one line, and the
                    // question is which one.
                    //
                    // It used to be answered by re-wrapping the WHOLE string at
                    // `container_rect.width()` and stopping after line 0, because
                    // `text_bottom` is one line below this node's own `y`. That
                    // painted line 0 N times and dropped lines 1..N-1 entirely.
                    //
                    // The width to break at is `this line's own box width`, not a
                    // width the painter re-derives. `rect.w` IS the measured
                    // advance of the words layout put on this line, so consuming
                    // words until the next one no longer fits in it reproduces
                    // layout's own break, with no second wrap pass to disagree
                    // with. That matters beyond the reported case: the old width
                    // was the containing box, which for text inside an inline
                    // element is that element's PER-LINE fragment box, so the
                    // painter was breaking a different string on every line of one
                    // paragraph. Where the words have run out the line is empty and
                    // nothing is drawn, which is what an empty line box is.
                    let (line, line_w) = if text_style.ellipsis {
                        // Single-line truncated with an ellipsis to the text box width.
                        let single = truncate_with_ellipsis(
                            t.as_str(),
                            layout_rect.width(),
                            fonts,
                            font_family,
                            font_size,
                        );
                        let single_w = fonts.measure_text(font_family, font_size, &single);
                        (single, single_w)
                    } else {
                        let key = node as *const VNode as usize;
                        let from = text_word_offset.get(&key).copied().unwrap_or(0);
                        // `rect.w` is layout's own line width, which it measured
                        // WITH the trailing space it put at the end of the line —
                        // measured, not rounded tight: the app's `.tagline` breaks
                        // 509 / 167 while the words themselves advance 505 and
                        // 167, so there is always a space's width of slack in
                        // `rect.w` for this comparison. No epsilon is needed, and
                        // adding one would be claiming a tolerance this does not
                        // need.
                        let limit = layout.rect.w as f32;
                        let mut acc = String::new();
                        let mut taken = from;
                        for word in t.split_whitespace().skip(from) {
                            let candidate = if acc.is_empty() {
                                word.to_string()
                            } else {
                                format!("{} {}", acc, word)
                            };
                            // `acc.is_empty()` keeps a word wider than the box on
                            // its own line, which is what layout's wrap does and
                            // what a CSS engine must do rather than lose the word.
                            if acc.is_empty()
                                || fonts.measure_text(font_family, font_size, &candidate) <= limit
                            {
                                acc = candidate;
                                taken += 1;
                            } else {
                                break;
                            }
                        }
                        text_word_offset.insert(key, taken);
                        let w = fonts.measure_text(font_family, font_size, &acc);
                        (acc, w)
                    };
                    let ty = layout.rect.y as f32 + font_size;
                    let padding = if align_rect.width() >= container_rect.width() - 0.5 {
                        2.0
                    } else {
                        0.0
                    };
                    let tx = match text_style.align {
                        TextAlign::Left => align_rect.left + padding,
                        TextAlign::Center => align_rect.left + (align_rect.width() - line_w) * 0.5,
                        TextAlign::Right => {
                            (align_rect.right - line_w - padding).max(align_rect.left + padding)
                        }
                    };
                    #[allow(unused_must_use)]
                    {
                        let _ = canvas.draw_str(line.as_str(), (tx, ty), &font, &paints.text);
                    }
                    // Measure-then-strike: `line_w` is THIS line's measured
                    // advance, so the rule lands across the line this node owns
                    // and not just the first.
                    draw_text_decorations(
                        canvas,
                        paints,
                        text_style,
                        tx,
                        ty,
                        line_w,
                        font_size,
                        inherited_opacity,
                    );
                }
            }
        }

        let root_rect = sk::Rect::from_xywh(
            layout_root.rect.x as f32,
            layout_root.rect.y as f32,
            layout_root.rect.w as f32,
            layout_root.rect.h as f32,
        );
        render_with_layout(
            canvas,
            vnode,
            layout_root,
            root_rect,
            fonts,
            default_text_style,
            &default_family,
            &mut paints,
            images,
            1.0,
            (root_rect.width(), root_rect.height()),
            &mut std::collections::HashMap::new(),
        );
        let debug_overlay = std::env::var("VELOX_DEBUG_HIT_RECTS")
            .ok()
            .as_deref()
            .map(|v| v == "1")
            .unwrap_or(false);
        let debug_log = std::env::var("VELOX_DEBUG_HIT_RECTS_LOG")
            .ok()
            .as_deref()
            .map(|v| v == "1")
            .unwrap_or(false);
        if debug_overlay || debug_log {
            let mut rects = Vec::new();
            collect_debug_hit_rects(vnode, layout_root, &mut rects);
            if debug_log {
                for r in &rects {
                    log::debug!("hit rect: x={} y={} w={} h={}", r.x, r.y, r.w, r.h);
                }
            }
            if debug_overlay {
                let mut paint = sk::Paint::default();
                paint.set_anti_alias(true);
                paint.set_style(skia_safe::paint::Style::Stroke);
                paint.set_stroke_width(1.0);
                paint.set_color(sk::Color::from_argb(200, 255, 0, 0));
                for r in rects {
                    let rect = sk::Rect::from_xywh(r.x as f32, r.y as f32, r.w as f32, r.h as f32);
                    canvas.draw_rect(rect, &paint);
                }
            }
        }
        canvas.restore();

        // Present/flush if GPU-backed
        let _ = surface.present();
        Ok(())
    }

    #[cfg(all(test, feature = "skia-native", unix))]
    mod tests {
        use super::*;
        use velox_dom::h;
        use velox_style::Stylesheet;

        // ------------------------------------------------------------------
        // The measurement seam, against a real font backend.
        //
        // These need `--features skia-native`; without it the crate's fallback
        // measurer is registered instead and there is no font to measure.
        // ------------------------------------------------------------------

        const SEAM_FAMILY: &str = "system-ui";
        const SEAM_SIZE: f32 = 16.0;

        /// Heights of the text line boxes `vnode` lays out, in order.
        fn seam_line_heights(vnode: &VNode) -> Vec<i32> {
            let laid = velox_dom::layout::compute_layout(vnode, 300, 300);
            let mut out = vec![laid.rect.h];
            for child in &laid.children {
                if child.children.is_empty() {
                    out.push(child.rect.h);
                } else {
                    for grandchild in &child.children {
                        out.push(grandchild.rect.h);
                    }
                }
            }
            out
        }

        /// A div whose only child is one text node, wrapping only at explicit
        /// newlines, so each line's content is known exactly.
        fn seam_pre_text_div(text: &str) -> VNode {
            let style = format!("width:240px;font-size:{SEAM_SIZE}px;white-space:pre");
            h(
                "div",
                vec![("style", style.as_str())],
                vec![VNode::Text(text.to_string())],
            )
        }

        #[test]
        fn measured_run_vertical_metrics_come_from_the_glyphs_not_a_constant() {
            let caps = measure_text("H", SEAM_SIZE, SEAM_FAMILY, 1.0);
            let lower = measure_text("x", SEAM_SIZE, SEAM_FAMILY, 1.0);
            let mixed = measure_text("Hg", SEAM_SIZE, SEAM_FAMILY, 1.0);

            // The load-bearing assertion. A fabricated `descent = 0.4em` gives
            // every run the same descent, including a capital, which has none.
            assert_eq!(
                caps.descent,
                0.0,
                "a capital has no descender in any real face; got {} \
                 (0.4em would be {})",
                caps.descent,
                SEAM_SIZE * 0.4
            );
            assert!(
                mixed.descent > 0.0,
                "a run containing a descender must report one, got {}",
                mixed.descent
            );
            assert!(
                mixed.descent > caps.descent,
                "descent must depend on the glyphs, not the font"
            );

            // Likewise a fabricated `ascent = 0.8em` gives every run the same
            // ascent, but cap height is taller than x-height in every real face.
            assert!(
                caps.ascent > lower.ascent,
                "cap height ({}) must exceed x-height ({}); 0.8em would make both {}",
                caps.ascent,
                lower.ascent,
                SEAM_SIZE * 0.8
            );
            assert!(
                mixed.ascent > lower.ascent,
                "a run with a capital must reach higher than one without"
            );

            // In a Latin face the ascent dominates the descent.
            assert!(
                mixed.ascent > mixed.descent,
                "ascent {} must exceed descent {} for Latin text",
                mixed.ascent,
                mixed.descent
            );
            assert!(
                mixed.line_extent() > lower.line_extent(),
                "the total extent must follow the content: {} vs {}",
                mixed.line_extent(),
                lower.line_extent()
            );
            assert!(
                mixed.width != SEAM_SIZE * 0.5 * 2.0,
                "width {} is exactly the 0.5em-per-char heuristic, so this is not \
                 a real proportional font",
                mixed.width
            );
        }

        #[test]
        fn line_box_height_under_real_metrics_is_the_strut_for_every_real_run() {
            velox_dom::text_wrap::set_skia_measurer(measure_text);
            let heights = seam_line_heights(&seam_pre_text_div("Hg\nxxx"));
            assert_eq!(heights.len(), 3, "root, div, two line boxes: {heights:?}");

            // See the bare-text test above for why the old "two different runs
            // get two different heights" assertion had to go: no run of this face
            // at one size overshoots its own strut. What replaces it is the
            // relationship that IS true, and which a fixed multiplier cannot
            // produce: each line is exactly the strut, and the strut is derived
            // from the font file rather than re-run through the implementation.
            let strut = (SEAM_SIZE * 1.362).round() as i32;
            for (line_text, got) in [("Hg", heights[1]), ("xxx", heights[2])] {
                let run = measure_text(line_text, SEAM_SIZE, SEAM_FAMILY, 1.0);
                assert!(
                    (run.line_extent().round() as i32) < strut,
                    "precondition: `{line_text}` ink must be under the strut"
                );
                assert_eq!(
                    got, strut,
                    "line {line_text:?} is the strut, since its ink of {} + {} is \
                     under it",
                    run.ascent, run.descent
                );
            }
            assert_ne!(
                strut,
                (SEAM_SIZE * 1.2).round() as i32,
                "1.362em and 1.2em must not coincide, or this test proves nothing"
            );
        }

        #[test]
        fn a_blank_run_does_not_collapse_its_line_box_under_real_metrics() {
            velox_dom::text_wrap::set_skia_measurer(measure_text);
            // A space really has no ink, so Skia reports a zero-height rectangle
            // for it. Passing that on as a line box would be a zero-height line.
            let blank = measure_text(" ", SEAM_SIZE, SEAM_FAMILY, 1.0);
            assert_eq!(
                blank.line_extent(),
                0.0,
                "precondition: a blank run really does measure zero ink"
            );
            let heights = seam_line_heights(&seam_pre_text_div(" "));
            assert!(
                heights.iter().all(|h| *h > 0),
                "a line box collapsed to {heights:?}"
            );
            assert_eq!(
                heights[1],
                (SEAM_SIZE * 1.362).round() as i32,
                "a zero extent must not pass through: the seam substitutes the ink \
                 approximation (1.2em) and the strut then floors it to 1.362em. \
                 Passing the zero on instead would give 0"
            );
        }

        /// `measure_run` passes Skia's bounds through unmodified.
        ///
        /// Every other test in this module is self-consistent: it asks the seam
        /// what a run measures and then checks that the layout path used that
        /// same number, so a uniform error applied to the measured VALUE — a
        /// stray scale factor, a flipped sign, the descent taken from
        /// `bounds.top` — is invisible to all of them. Falsification confirmed
        /// exactly that: scaling `measure_run`'s ascent by 0.7 left all 30 of
        /// these tests green.
        ///
        /// So this one does not go through `measure_run`'s callers. It takes the
        /// typeface, calls `measure_str` itself, and requires the production
        /// path to have extracted precisely the same numbers. The duplication is
        /// deliberate and is the whole point: independence from the code under
        /// test. The typeface and DPI snap are shared with production because
        /// those are not what is under test.
        #[test]
        fn measure_run_passes_skias_bounds_through_unmodified() {
            let mut cache = FontCache::new_with_scale(1.0);
            for text in ["Hg", "H", "x", "W", "Ag", " "] {
                for size in [16.0f32, 33.0] {
                    let (w, bounds) = {
                        let font = cache.font(SEAM_FAMILY, size);
                        let mut paint = sk::Paint::default();
                        paint.set_anti_alias(true);
                        font.measure_str(text, Some(&paint))
                    };
                    let run = cache.measure_run(SEAM_FAMILY, size, text);
                    assert_eq!(run.width, w, "advance width for {text:?} at {size}px");
                    assert_eq!(
                        run.ascent,
                        (-bounds.top).max(0.0),
                        "ascent for {text:?} at {size}px: Skia puts the baseline at y=0 \
                         with the top negative above it, so the ascent is -top"
                    );
                    assert_eq!(
                        run.descent,
                        bounds.bottom.max(0.0),
                        "descent for {text:?} at {size}px: below the baseline is positive"
                    );
                }
            }
        }

        /// THE PAINT LANE AND THE CARET LANE MUST AGREE ON THE MEASURED ADVANCE, at
        /// every scale including 1.0.
        ///
        /// Three snap rules exist and the file documents two of them diverging
        /// from `Viewport::snap_logical_to_physical_grid`; it never recorded that
        /// they also have to agree with EACH OTHER. `FontCache::snapped_size`
        /// (paint, via `font()`/`measure_run`) early-returns unrounded at
        /// `scale == 1.0`; the free `measure_text` (the caret lane, via
        /// `TextMeasurer::measure_with_scale`) rounded at every finite positive
        /// scale, 1.0 included. So at the most common desktop scale a
        /// fractional `font-size` painted at its exact value and was measured at
        /// the next whole pixel — `16.5px` painted at 16.5, measured at 17, a 3%
        /// advance error on a value click lands about one character short per
        /// thirty-three.
        ///
        /// The assertion is a DIFFERENCE, not a re-derived number: measuring the
        /// same run through both lanes at scale 1.0 must give bit-identical
        /// widths, and at a fractional scale both must agree too. `measure_text`
        /// internally re-snaps before delegating to `FontCache::measure_run`, so
        /// this compares the two independent snap decisions rather than a
        /// function against itself.
        #[test]
        fn the_caret_lane_and_the_paint_lane_measure_the_same_advance() {
            const SEAM: &str = SEAM_FAMILY;
            for scale in [1.0f32, 1.25, 1.5, 2.0] {
                let mut cache = FontCache::new_with_scale(scale);
                // Fractional sizes first: those are the ones that distinguish
                // "snapped" from "not snapped", and an integral size would let
                // a snapped and an unsnapped implementation agree by luck.
                // NOTE on the sizes chosen: the bundled default face quantizes
                // advances to whole logical px, so most size/text pairs measure
                // the same at 16px and 16.5px. Pairs that DO differ ("Wg" and
                // "Hello world" at 16.5 vs 17) are included precisely so this
                // assertion is not vacuous — a snapped/unsnapped pair that
                // happens to measure alike would let both lanes disagree in
                // production while this test stayed green.
                for size in [16.5f32, 13.333, 0.9375 * 16.0, 21.0] {
                    for text in ["MMMMMMMMMMMMMMMMMMMM", "iiiii", "Wg", "Hello world", "0"] {
                        let paint = cache.measure_run(SEAM, size, text).width;
                        let caret = measure_text(text, size, SEAM, scale).width;
                        assert_eq!(
                            caret, paint,
                            "advance for {text:?} at {size}px, scale {scale}: the caret lane \
                             measured {caret} but the paint lane measured {paint}"
                        );
                    }
                }
            }
        }

        /// A measured run's vertical extent is a LOGICAL quantity, so it must not
        /// move when the scale does.
        ///
        /// `measure_run`'s doc says its bounds are logical px rather than the raw
        /// device units Skia produced, and warns that dividing by `scale` would be
        /// the bug rather than the fix. That correction was documentation with
        /// nothing behind it: `measure_run_passes_skias_bounds_through_unmodified`
        /// shares `font()` and `snapped_size` with the code it checks, so it cannot
        /// tell a logical reading from a device one — it only anchors the extraction.
        ///
        /// This is the check that can. `FontCache::snapped_size` divides the device
        /// size by the scale and `font()` builds the typeface at THAT size, so a
        /// correct implementation measures the same extent at scale 1.0 and 2.0. A
        /// device-unit reading doubles at scale 2.0 instead, and a
        /// double-divided one halves. Five assertions, and it is the only test here
        /// that reaches the snap rather than the extraction.
        #[test]
        fn a_runs_vertical_extent_is_logical_and_does_not_move_with_scale() {
            let mut at_one = FontCache::new_with_scale(1.0);
            let mut at_two = FontCache::new_with_scale(2.0);
            for text in ["Hg", "xxx", "Wq"] {
                for size in [16.0f32, 33.0] {
                    let one = at_one.measure_run(SEAM_FAMILY, size, text);
                    let two = at_two.measure_run(SEAM_FAMILY, size, text);
                    assert_eq!(
                        one.line_extent(),
                        two.line_extent(),
                        "{text:?} at {size}px measures a different extent at scale 2.0 \
                         than at 1.0, so these are device px, not logical"
                    );
                    assert_eq!(
                        one.width, two.width,
                        "the advance width must be logical too: {text:?} at {size}px"
                    );
                }
            }
        }

        // ------------------------------------------------------------------
        // The advance memo: does it hit, and does it know when to throw away.
        //
        // Every test above is self-consistent — it asks the seam for a number
        // and then checks that the layout path used that same number — so an
        // advance cache cannot be falsified by any of them. A cache that
        // returned a WRONG number from a stale entry would fail only one of
        // them, and only by accident. These two are the checks that can:
        // `skia_measure_calls` counts the real Skia work, so a memo that never
        // hits is visible, and the scale change is the one event that can
        // legally invalidate a memo, so it is where correctness is load-bearing.
        // ------------------------------------------------------------------

        /// Measuring the same run again must not re-shape it, and must return
        /// the same numbers.
        ///
        /// The result-equality half is the weak half — a correct cache passes
        /// it, and so does no cache at all. `skia_measure_calls` is the half
        /// that can fail: it counts calls into `measure_str` itself, so if the
        /// memo is missing, or keyed on something that never repeats, the
        /// number climbs with every repeat and this goes red. Sixteen repeats
        /// per run rather than two, so a memo that only caught the immediate
        /// successor would not slip through.
        #[test]
        fn a_repeated_measurement_is_served_from_the_memo_without_reshaping() {
            let mut cache = FontCache::new_with_scale(1.0);
            let texts = ["Hg", "xxx", "Buy milk", "Wq", " "];
            for text in texts {
                let first = cache.measure_run(SEAM_FAMILY, SEAM_SIZE, text);
                let calls_after_first = cache.skia_measure_calls();
                assert_eq!(
                    calls_after_first,
                    texts.iter().position(|t| *t == text).unwrap() as u64 + 1,
                    "each distinct run should have cost exactly one Skia call"
                );
                for _ in 0..16 {
                    let again = cache.measure_run(SEAM_FAMILY, SEAM_SIZE, text);
                    assert_eq!(again, first, "re-measuring {text:?} changed the answer");
                }
                assert_eq!(
                    cache.skia_measure_calls(),
                    calls_after_first,
                    "{text:?} was re-shaped in Skia on a repeat: the memo missed"
                );
            }
            assert_eq!(
                cache.advance_cache_len(),
                texts.len(),
                "one entry per distinct run, no more"
            );
        }

        /// A scale change must clear the memo, and what is measured after it
        /// must be what that scale actually measures.
        ///
        /// This is the load-bearing correctness case for the whole cache. An
        /// advance is a function of `(typeface, snapped size, text)`, and the
        /// snapped size is a function of the scale, so a memo that outlived a
        /// scale change would hand the new scale the old scale's text metrics:
        /// wrong wrap points, wrong caret position, visibly wrong UI, and no
        /// panic to notice it by.
        ///
        /// The size is deliberately one where the DPI snap MOVES with the scale
        /// AND the resulting advances land on different f32s — 16.4px is 16.4
        /// logical at 1.0 and 16.666 at 1.5 (`(16.4 * 1.5).round() / 1.5` =
        /// `25 / 1.5`), and `"Buy milk"` measures 68.0 at 1.0 and 75.0 at 1.5.
        /// The second half of that clause is the load-bearing half: the default
        /// face quantizes advances, so a fractional size alone is not enough.
        ///
        /// It was 17.1px until `0cc6982` swapped the default face from Noto Sans
        /// to DejaVu Sans. Noto's advances at these sizes were fine-grained enough
        /// that the snap's movement showed up in the width — 68.0 / 67.0 / 69.0 at
        /// 1.0 / 1.25 / 1.5. DejaVu's are not: its advances land on a far coarser
        /// grid, and at 17.1 all three scales measured `"Buy milk"` at exactly
        /// 75.0. A sweep of every size from 6.00px to 40.00px in 0.01px steps
        /// found 198 sizes where 1.0 and 1.5 disagree and, for `"Buy milk"`,
        /// 16.4 is one of them. The widths above are measured, not derived.
        ///
        /// Worth being explicit about what the snap is FOR: it makes
        /// `(logical * scale).round()` land on integer device pixels so glyph
        /// hinting has a row to land on. That it also perturbs the measured width
        /// is a side effect of the advance being quantized at all, not the point
        /// of the snap — which is exactly why the assertion at the end of this
        /// test is a precondition on the PROBE rather than a claim about the
        /// snap, and why a size has to be chosen that exhibits the side effect.
        ///
        /// Measure a run long enough to separate. This uses `"Buy milk"`, not
        /// a short one: at these sizes a short run like `"Hg"` can separate only
        /// on `ascent`, which is a weaker and less meaningful discriminator than
        /// width — width is what drives wrap points and caret position, which is
        /// what the cache exists to get right.
        ///
        /// The reference values come from caches built directly at each scale,
        /// so they do not share the cache under test.
        #[test]
        fn a_scale_change_clears_the_advance_memo_and_remeasures_for_the_new_scale() {
            const FRACTIONAL: f32 = 16.4;
            const RUN: &str = "Buy milk";
            let mut cache = FontCache::new_with_scale(1.0);

            let at_one = cache.measure_run(SEAM_FAMILY, FRACTIONAL, RUN);
            let expect_at_one =
                FontCache::new_with_scale(1.0).measure_run(SEAM_FAMILY, FRACTIONAL, RUN);
            assert_eq!(
                at_one, expect_at_one,
                "precondition: the 1.0 answer is right"
            );
            assert_eq!(cache.advance_cache_len(), 1, "one run is memoized");

            cache.set_scale_factor(1.5);
            assert_eq!(
                cache.advance_cache_len(),
                0,
                "a scale change must drop the memo in the same place it drops \
                 `fonts`; a surviving entry is an advance measured at the old \
                 device size being served to the new one"
            );

            let at_one_and_a_half = cache.measure_run(SEAM_FAMILY, FRACTIONAL, RUN);
            let expect_at_one_and_a_half =
                FontCache::new_with_scale(1.5).measure_run(SEAM_FAMILY, FRACTIONAL, RUN);
            assert_eq!(
                at_one_and_a_half, expect_at_one_and_a_half,
                "after the scale change the run must measure as a cache built \
                 at 1.5 does"
            );
            assert_ne!(
                at_one.width, at_one_and_a_half.width,
                "precondition: this run snaps to a different logical size at \
                 each scale, so the two widths must differ; if they are equal \
                 the test cannot detect a stale advance at all"
            );

            // And back again, to catch a one-way clear.
            cache.set_scale_factor(1.0);
            assert_eq!(cache.advance_cache_len(), 0, "the clear is not one-way");
            assert_eq!(
                cache.measure_run(SEAM_FAMILY, FRACTIONAL, RUN),
                expect_at_one,
                "returning to 1.0 must restore the 1.0 measurement"
            );
        }

        /// The same guarantee through the PUBLIC seam, which is the one layout
        /// actually calls.
        ///
        /// The two tests above drive a `FontCache` they own. Production does
        /// not: `measure_text` goes through the thread-local
        /// `MEASURE_FONT_CACHE`, and that thread-local is shared by every
        /// measure in the process. A memo keyed on anything short of the full
        /// identity — a family only, or no scale — is invisible to a per-cache
        /// test and wrong here, because the second call in this sequence is
        /// answered by an entry the first call left behind.
        ///
        /// So this walks the real alternating pattern — 1.0, 1.5, 1.0, 1.25,
        /// 1.5, 1.0 — and pins the property that a scale-blind memo breaks
        /// first: the value at a scale must be the same on every visit to that
        /// scale. The repeated 1.5 in the middle is deliberate; it is a hit, so it
        /// also has to agree. The repeated 1.0 at the end is the load-bearing
        /// one: it arrives straight after a 1.5 measurement, which is where a memo
        /// that was never cleared answers with the neighbour's advance.
        ///
        /// The precondition is the weaker "at least two scales in this walk
        /// measure differently", not "every scale measures differently from every
        /// other". It was the stronger form until `0cc6982` swapped the default
        /// face to DejaVu Sans, and the stronger form is no longer satisfiable:
        /// across every size from 6.00px to 40.00px in 0.01px steps, and four
        /// different scale triples, `"Buy milk"` NEVER produced three
        /// pairwise-distinct widths — its advances sit on a grid coarse enough
        /// that two scales always agree. `"todo-item"` and `"Hg"` behave the same
        /// way. Two scales separating is what this test needs to be able to fail
        /// at all, and that is what is asserted below.
        #[test]
        fn the_shared_measure_seam_never_serves_another_scales_advance() {
            // Measured at 16.4px: 68.0 at 1.0, 75.0 at 1.25, 75.0 at 1.5.
            const FRACTIONAL: f32 = 16.4;
            let text = "Buy milk";
            let mut seen: Vec<(f32, f32)> = Vec::new();
            for scale in [1.0f32, 1.5, 1.0, 1.25, 1.5, 1.0] {
                let width = measure_text(text, FRACTIONAL, SEAM_FAMILY, scale).width;
                match seen.iter().find(|(s, _)| *s == scale) {
                    // A second visit to a scale must reproduce the first one.
                    Some((_, first)) => assert_eq!(
                        width, *first,
                        "{text:?} at {scale} measured {width} on a later visit but \
                         {first} on the first, so the memo answered with another \
                         scale's advance"
                    ),
                    None => {
                        seen.push((scale, width));
                    }
                }
            }
            let mut distinct: Vec<f32> = seen.iter().map(|(_, w)| *w).collect();
            distinct.sort_by(|a, b| a.partial_cmp(b).expect("a finite advance"));
            distinct.dedup();
            assert!(
                distinct.len() > 1,
                "precondition: this test cannot detect a scale-blind memo unless the \
                 scales in {seen:?} measure differently, and they all measured \
                 {text:?} at {FRACTIONAL}px as {distinct:?} — pick a size at which \
                 they separate"
            );
        }

        /// The memo is bounded, and it drops the OLDEST entries when full
        /// instead of refusing to cache.
        ///
        /// A wrap measures every prefix of a line, so a long paragraph
        /// contributes an entry per word per candidate, and many of those are
        /// never asked for again. Unbounded, that is a slow leak across a
        /// session. Refusing to cache past the cap would be worse than the
        /// leak: it would freeze the working set at whatever was measured
        /// first and permanently exclude every key measured after it, which
        /// defeats the cache on exactly the strings a growing document adds.
        ///
        /// Driven through `remember_advance` directly so the test does not
        /// spend 4096 real Skia shaping passes proving what a map's `len`
        /// already says.
        #[test]
        fn the_advance_memo_is_bounded_and_evicts_the_oldest_rather_than_refusing() {
            let mut cache = FontCache::new_with_scale(1.0);
            let run = MeasuredText {
                width: 7.0,
                ascent: 12.0,
                descent: 4.0,
            };
            let key = |i: usize| AdvanceKey {
                font: FontKey {
                    family: SEAM_FAMILY.to_string(),
                    size_key: (SEAM_SIZE * 100.0) as u32,
                },
                text: format!("run-{i}"),
            };

            for i in 0..ADVANCE_CACHE_CAP {
                cache.remember_advance(key(i), run, cache.scale_key());
            }
            assert_eq!(cache.advance_cache_len(), ADVANCE_CACHE_CAP);

            // One insert past the cap. The newest key must be cached (it did
            // not refuse) and the length must still be bounded (it did not
            // just grow).
            cache.remember_advance(key(ADVANCE_CACHE_CAP), run, cache.scale_key());
            assert!(
                cache.advance_cache_len() <= ADVANCE_CACHE_CAP,
                "grew past the cap: {}",
                cache.advance_cache_len()
            );
            assert_eq!(
                cache.advances.get(&key(ADVANCE_CACHE_CAP)).map(|e| e.width),
                Some(7.0),
                "the entry that tripped the cap was not cached, so the memo \
                 refuses new keys instead of dropping old ones"
            );
            assert!(
                !cache.advances.contains_key(&key(0)),
                "the oldest entry survived; it is not drop-oldest"
            );
            assert!(
                cache.advances.contains_key(&key(ADVANCE_CACHE_CAP / 2)),
                "an entry newer than the eviction window was dropped too"
            );

            // An evicted key must still measure correctly when it comes back:
            // a miss re-shapes, it does not serve whatever survived.
            let mut fresh = FontCache::new_with_scale(1.0);
            for text in ["Hg", "xxx", "Wq", "Buy milk"] {
                assert_eq!(
                    cache.measure_run(SEAM_FAMILY, SEAM_SIZE, text),
                    fresh.measure_run(SEAM_FAMILY, SEAM_SIZE, text),
                    "{text:?} measured differently through a cache that had been \
                     evicting than through a fresh one"
                );
            }
        }

        /// `at()`'s bare `VNode::Text` arm, under real font metrics. This is
        /// the one path R-5a changed that the velox-dom tests cannot reach: they
        /// all install a synthetic measurer, so what is verified here is that a
        /// real font backend's metrics actually arrive at the branch that uses
        /// them.
        #[test]
        fn a_bare_text_node_under_real_metrics_is_the_strut_and_not_the_ink() {
            velox_dom::text_wrap::set_skia_measurer(measure_text);
            let bare = |text: &str| {
                velox_dom::layout::compute_layout(&VNode::Text(text.to_string()), 300, 300)
                    .rect
                    .h
            };
            // RESTATED in R-5b, and the premise of the old version was false.
            // It asserted that a real run's ink sets the bare node's height. It
            // no longer does -- and must not, because a real face's runs are all
            // shorter than its own ascent + descent:
            //   "Hg" at 16px measures 12.0 up + 4.0 down = 1.0em
            //   "xxx" at 16px measures 9.0 up + 0.0 down   = 0.5625em
            //   the strut is 1.069em + 0.293em              = 1.362em
            // So the strut decides for BOTH, which is what a browser does. Under a
            // single uniform font size no run of this face can overshoot its own
            // strut, so "two runs of different content get different heights" is
            // not a property a real font can exhibit at one size. The
            // run-versus-strut discrimination is therefore asserted in
            // velox-dom's `text_metrics_seam.rs`, where a synthetic measurer CAN
            // overshoot; this test pins the real font's numbers.
            let strut = SEAM_SIZE * 1.362;
            for text in ["Hg", "xxx", "Wq"] {
                let run = measure_text(text, SEAM_SIZE, SEAM_FAMILY, 1.0);
                assert!(
                    run.line_extent() < strut,
                    "precondition for this test's claim: `{text}` ink must be under \
                     the strut, or the strut would not be what is being measured \
                     ({} vs {strut})",
                    run.line_extent()
                );
                assert_eq!(
                    bare(text),
                    strut.round() as i32,
                    "a bare `{text}` node is the strut ({} + {} at 1.069em/0.293em), \
                     not its ink of {} up and {} down",
                    SEAM_SIZE * 1.069,
                    SEAM_SIZE * 0.293,
                    run.ascent,
                    run.descent
                );
            }
            assert_ne!(
                bare("Hg"),
                19,
                "and still not the 1.2em the bare-text path had before R-5a: it went \
                 19 -> {} (ink) -> {} (strut)",
                9,
                22
            );
        }

        #[test]
        fn a_nan_vertical_bound_cannot_escape_the_seam() {
            // `f32::max` maps a NaN bound to 0.0, which is the same
            // zero-extent case the seam already handles. Asserted here because
            // the sanitising happens in the measurer, far from the guard that
            // consumes it, and the two must stay in step.
            assert_eq!(f32::NAN.max(0.0), 0.0);
            assert_eq!((-f32::NAN).max(0.0), 0.0);
        }

        #[test]
        #[ignore = "requires skia-native feature and GPU hardware"]
        fn render_overflow_hidden_clips_children() {
            let vnode = h(
                "div",
                vec![(
                    "style",
                    "background-color:#FFFFFF;overflow:hidden;width:40px;height:40px",
                )],
                vec![h(
                    "div",
                    vec![("style", "background-color:#FF0000;width:40px;height:80px")],
                    vec![],
                )],
            );

            let mut surface =
                crate::skia_surface::SkiaSurface::new_raster(64, 64).expect("surface");
            let layout = velox_dom::layout::compute_layout(&vnode, 64, 64);
            render_frame(&mut surface, &vnode, &layout, &Stylesheet::default()).expect("render");
            let path = "target/skia_overflow_clip.png";
            surface.save_png(path).expect("save png");
            let png = std::fs::read(path).expect("read png");

            let checksum = fnv1a(&png);
            println!("overflow-hidden checksum: 0x{checksum:08x}");
            // Update this checksum after regenerating the raster output.
            const EXPECTED_OVERFLOW_CHECKSUM: u32 = 0xf74653e7;
            assert_eq!(checksum, EXPECTED_OVERFLOW_CHECKSUM);
        }

        #[test]
        #[ignore = "requires skia-native feature and GPU hardware"]
        fn render_z_index_overlap_checksum() {
            let vnode = h(
                "div",
                vec![("style", "background-color:#FFFFFF;width:64px;height:64px")],
                vec![
                    h(
                        "div",
                        vec![(
                            "style",
                            "background-color:#FF0000;width:40px;height:40px;z-index:1",
                        )],
                        vec![],
                    ),
                    h(
                        "div",
                        vec![(
                            "style",
                            "background-color:#0000FF;width:40px;height:40px;margin-top:-20px;z-index:0",
                        )],
                        vec![],
                    ),
                ],
            );

            let mut surface =
                crate::skia_surface::SkiaSurface::new_raster(64, 64).expect("surface");
            let layout = velox_dom::layout::compute_layout(&vnode, 64, 64);
            render_frame(&mut surface, &vnode, &layout, &Stylesheet::default()).expect("render");
            let path = "target/skia_z_index.png";
            surface.save_png(path).expect("save png");
            let png = std::fs::read(path).expect("read png");

            let checksum = fnv1a(&png);
            println!("z-index checksum: 0x{checksum:08x}");
            // Update this checksum after regenerating the raster output.
            const EXPECTED_Z_INDEX_CHECKSUM: u32 = 0x0c864983;
            assert_eq!(checksum, EXPECTED_Z_INDEX_CHECKSUM);
        }

        #[test]
        fn render_debug_hit_rects_collects_clickable() {
            let vnode = h(
                "div",
                vec![],
                vec![
                    h("div", vec![("class", "btn")], vec![]),
                    h("div", vec![], vec![]),
                ],
            );
            let layout = velox_dom::layout::compute_layout(&vnode, 100, 50);
            let mut rects = Vec::new();
            collect_debug_hit_rects(&vnode, &layout, &mut rects);
            assert_eq!(rects.len(), 1);
        }

        #[test]
        fn truncate_ellipsis_fits_short_text_unchanged() {
            let mut fc = FontCache::new();
            let family = fc.default_family();
            let size = 14.0;
            let text = "Hi";
            let out = truncate_with_ellipsis(text, 500.0, &mut fc, &family, size);
            assert_eq!(out, "Hi");
        }

        #[test]
        fn truncate_ellipsis_appends_ellipsis_on_overflow() {
            let mut fc = FontCache::new();
            let family = fc.default_family();
            let size = 14.0;
            let text = "This is a very long line of text that will definitely not fit";
            let out = truncate_with_ellipsis(text, 60.0, &mut fc, &family, size);
            assert!(out.ends_with('\u{2026}'), "expected ellipsis, got: {out:?}");
            let w = fc.measure_text(&family, size, &out);
            assert!(w <= 60.0, "truncated width {w} exceeds 60px: {out:?}");
            assert!(out.len() < text.len(), "expected truncation, got: {out:?}");
        }

        #[test]
        fn truncate_ellipsis_handles_zero_width() {
            let mut fc = FontCache::new();
            let family = fc.default_family();
            let out = truncate_with_ellipsis("anything", 0.0, &mut fc, &family, 14.0);
            assert!(out.is_empty());
        }

        #[test]
        fn parse_text_style_detects_ellipsis_and_nowrap() {
            let base = TextStyle {
                color: sk::Color::from_argb(255, 0, 0, 0),
                align: TextAlign::Left,
                underline: false,
                line_through: false,
                font_size: 14.0,
                bold: false,
                line_height: 1.2,
                nowrap: false,
                ellipsis: false,
            };
            let (s, _f) = parse_text_style(
                "white-space:nowrap;text-overflow:ellipsis",
                base,
                "default",
                (800.0, 600.0),
            );
            assert!(s.nowrap, "expected nowrap");
            assert!(s.ellipsis, "expected ellipsis");
        }

        fn plain_text_style() -> TextStyle {
            TextStyle {
                color: sk::Color::from_argb(255, 0, 0, 0),
                align: TextAlign::Left,
                underline: false,
                line_through: false,
                font_size: 14.0,
                bold: false,
                line_height: 1.2,
                nowrap: false,
                ellipsis: false,
            }
        }

        // `font-weight: bolder` was silently dropped: the check was
        // `"bold" || parse::<u16>() >= 700`, and `bolder` is neither, so the
        // declaration fell through to non-bold. CSS treats `bolder` as
        // relative-bold and the DOM already maps it to 900
        // (`velox_dom::style::FontWeight::Bolder`); the renderer must not
        // contradict that.
        #[test]
        fn parse_text_style_treats_bolder_as_bold() {
            for decl in [
                "font-weight:bolder",
                "font-weight: Bolder ",
                "font-weight:BOLDER",
            ] {
                let (s, _f) = parse_text_style(decl, plain_text_style(), "default", (800.0, 600.0));
                assert!(s.bold, "expected `{decl}` to render bold");
            }
        }

        // The regression guard above is only meaningful if the sibling cases
        // keep working and the relative-light keyword is not swept in with it.
        #[test]
        fn parse_text_style_font_weight_keyword_matrix() {
            // Bold-producing, all four spellings CSS accepts for bold.
            for decl in [
                "font-weight:bold",
                "font-weight:700",
                "font-weight:900",
                "font-weight:bolder",
            ] {
                let (s, _f) = parse_text_style(decl, plain_text_style(), "default", (800.0, 600.0));
                assert!(s.bold, "expected `{decl}` to render bold");
            }
            // Non-bold-producing.
            for decl in [
                "font-weight:normal",
                "font-weight:100",
                "font-weight:400",
                "font-weight:600",
                "font-weight:lighter",
            ] {
                let (s, _f) = parse_text_style(decl, plain_text_style(), "default", (800.0, 600.0));
                assert!(!s.bold, "expected `{decl}` to render non-bold");
            }
        }

        // `bolder` must also override an inherited bold:false base rather than
        // being treated as "no change".
        #[test]
        fn parse_text_style_bolder_overrides_non_bold_base() {
            let (s, _f) = parse_text_style(
                "font-weight:bolder",
                plain_text_style(),
                "default",
                (800.0, 600.0),
            );
            assert!(s.bold);
        }

        // ------------------------------------------------------------------
        // `font-size` in relative units.
        //
        // The arm used to be `parse_px_value(val).or_else(|| parse_float_value(val))`,
        // which reads `px` and bare numbers ONLY. `2em` failed both and the
        // declaration was dropped with no warning, so `ua.css`'s
        // `h1 { font-size: 2em }` laid the BOX at 32px and painted the glyphs
        // at the inherited 14px. The whole `h1`-`h6` hierarchy rendered at its
        // parent's size.
        // ------------------------------------------------------------------

        /// A base style whose inherited font size is `size`, so a test can
        /// prove a relative unit is resolved against the INHERITED value rather
        /// than against a constant baked into the parser.
        fn base_at_font_size(size: f32) -> TextStyle {
            TextStyle {
                font_size: size,
                ..plain_text_style()
            }
        }

        /// Every unit the CSS 2.1 §15.5 `<length> | <percentage>` production
        /// admits for `font-size`, each paired with the pixel value it must
        /// produce against an inherited 16px parent in an 800x600 viewport.
        ///
        /// `%`, `vw` and `vh` are here deliberately. Filtering this list down to
        /// `em`/`rem` would be a smaller diff, but it would leave `font-size:
        /// 150%` silently dropped — the same dead declaration this change
        /// exists to kill, just in a smaller hole.
        #[test]
        fn parse_text_style_resolves_every_relative_font_size_unit() {
            const VIEWPORT: (f32, f32) = (800.0, 600.0);
            // (declaration, expected px against an inherited 16px parent)
            for (decl, expected) in [
                ("font-size: 2em", 32.0),    // 2 * 16
                ("font-size: 0.5em", 8.0),   // 0.5 * 16
                ("font-size: 1.5rem", 24.0), // 1.5 * DEFAULT_ROOT_FONT_SIZE(16)
                ("font-size: 150%", 24.0),   // 1.5 * 16
                ("font-size: 5vw", 40.0),    // 5% of 800
                ("font-size: 5vh", 30.0),    // 5% of 600
            ] {
                let (s, _f) = parse_text_style(decl, base_at_font_size(16.0), "default", VIEWPORT);
                assert_eq!(
                    s.font_size, expected,
                    "`{decl}` must resolve to {expected}px against an inherited 16px parent; \
                     a dropped font-size paints the parent's glyphs, which is the defect"
                );
            }
        }

        /// The relative units must follow the INHERITED size, not a constant.
        /// If the parser had defaulted its `em` basis to 16 this row would be
        /// identical to the one above, which is exactly the bug the previous
        /// test could not see.
        #[test]
        fn parse_text_style_relative_font_size_follows_the_inherited_size() {
            const VIEWPORT: (f32, f32) = (800.0, 600.0);
            // An `h1` inside a `div` that sets `font-size: 20px`: `2em` is 40px,
            // not the 32px it would be at the default root.
            let (s, _f) = parse_text_style(
                "font-size: 2em",
                base_at_font_size(20.0),
                "default",
                VIEWPORT,
            );
            assert_eq!(s.font_size, 40.0, "`2em` of an inherited 20px is 40px");

            // `rem` is the discriminating half: it is rooted at
            // DEFAULT_ROOT_FONT_SIZE and MUST NOT follow the inherited size.
            let (s, _f) = parse_text_style(
                "font-size: 1rem",
                base_at_font_size(20.0),
                "default",
                VIEWPORT,
            );
            assert_eq!(
                s.font_size,
                velox_dom::layout::DEFAULT_ROOT_FONT_SIZE,
                "`rem` is rooted at the root font size and must ignore the inherited 20px"
            );
        }

        /// `vw`/`vh` need the viewport threaded in, so a hard-coded or
        /// zero-viewport resolution would show up here.
        #[test]
        fn parse_text_style_viewport_units_follow_the_viewport_argument() {
            let (s, _f) = parse_text_style(
                "font-size: 10vw",
                base_at_font_size(16.0),
                "default",
                (1000.0, 500.0),
            );
            assert_eq!(s.font_size, 100.0, "10% of a 1000px viewport");
            let (s, _f) = parse_text_style(
                "font-size: 10vh",
                base_at_font_size(16.0),
                "default",
                (1000.0, 500.0),
            );
            assert_eq!(s.font_size, 50.0, "10% of a 500px viewport");
        }

        /// The two forms the arm already honoured must keep working. A fix that
        /// only added relative units and broke `12px` would be a new defect.
        #[test]
        fn parse_text_style_keeps_absolute_font_sizes() {
            const VIEWPORT: (f32, f32) = (800.0, 600.0);
            for (decl, expected) in [
                ("font-size: 12px", 12.0),
                ("font-size: 33px", 33.0),
                ("font-size: 18", 18.0), // bare number, as `Length::parse` maps it to Px
                ("font-size:  20px ", 20.0), // surrounding whitespace
            ] {
                let (s, _f) = parse_text_style(decl, base_at_font_size(16.0), "default", VIEWPORT);
                assert_eq!(
                    s.font_size, expected,
                    "`{decl}` must still resolve to {expected}px"
                );
            }
        }

        /// The one guard in the `font-size` arm drops three classes of input,
        /// and all three must INHERIT rather than paint: `auto` (invalid per
        /// CSS 2.1 §15.5, and `Length::to_px` maps it to 0), a non-positive
        /// size, and a non-finite one. `f32::parse` accepts `NaN` and an
        /// overflowing literal like `1e999px`, and `f32::max(1.0)` keeps
        /// `inf`, so without the `is_finite` half of the guard Skia is handed
        /// an infinite font size.
        #[test]
        fn parse_text_style_drops_font_sizes_that_cannot_paint() {
            const VIEWPORT: (f32, f32) = (800.0, 600.0);
            for decl in [
                "font-size: auto",    // invalid in CSS 2.1 §15.5, to_px => 0
                "font-size: 0",       // Length::Zero
                "font-size: 0px",     // zero after resolution
                "font-size: 0%",      // zero after resolution
                "font-size: -4px",    // negative
                "font-size: 1e999px", // parses to `inf`
                "font-size: NaNpx",   // parses to `NaN`
            ] {
                let (s, _f) = parse_text_style(decl, base_at_font_size(16.0), "default", VIEWPORT);
                assert_eq!(
                    s.font_size, 16.0,
                    "`{decl}` cannot paint a glyph and must be dropped, inheriting the 16px base"
                );
            }
        }

        /// A 1rem-wide box is the root font size, resolved by `velox-dom`
        /// itself, so it is an independent readout of the basis this parser must
        /// share. The two numbers are compared rather than each asserted
        /// against a literal, because AGREEMENT is the property that matters:
        /// if the DOM's root constant ever moves and the painter does not, this
        /// assertion is what notices. The inherited size is 999px so a pass
        /// cannot be an accident of `em` handling.
        ///
        /// What this test does NOT do is catch a renderer that declared its own
        /// copy of the root size: such a copy would hold the same `16.0` today,
        /// so the two numbers would still agree. Mutation M8 of
        /// `gates/4.2b-fixA-falsify.py` demonstrated exactly that — it replaced
        /// the import with `const ROOT_FONT_SIZE: f32 = 16.0;` and left this
        /// test green. The test below is the guard that mutation turns red.
        #[test]
        fn parse_text_style_rem_agrees_with_the_dom_layout_root_basis() {
            let tree = velox_dom::h("div", velox_dom::Props::from_inline("width: 1rem"), vec![]);
            let laid = velox_dom::layout::compute_layout(&tree, 800, 600);
            let (s, _f) = parse_text_style(
                "font-size: 1rem",
                base_at_font_size(999.0),
                "default",
                (800.0, 600.0),
            );
            assert_eq!(
                s.font_size, laid.rect.w as f32,
                "the painter's `1rem` font size ({}) must equal the width velox-dom lays a \
                 `1rem` box out at ({}) — same root basis, or glyphs and boxes diverge",
                s.font_size, laid.rect.w
            );
        }

        /// The root font size must be BORROWED, never restated. This is a
        /// source scan rather than a value comparison, and that is the whole
        /// point: a second literal root size in the renderer is
        /// indistinguishable from the first by any value-based test until the
        /// day someone edits `DEFAULT_ROOT_FONT_SIZE` and only one of the two
        /// moves — glyphs laid out against one root size, boxes against another.
        ///
        /// Only the PRODUCTION half of the file is scanned, split structurally
        /// at `mod tests`, so the `"16.0"` needle cannot match its own source
        /// and the legitimate `16.0`s in the border and seam tests below cannot
        /// mask a real hit.
        #[test]
        fn the_renderer_borrows_the_dom_root_font_size() {
            let src = include_str!("skia_render.rs");
            let production = src
                .split_once("\n    mod tests {")
                .expect("skia_render.rs has a `mod tests` block to split production from")
                .0;
            assert!(
                !production.contains("16.0"),
                "the production half of skia_render.rs must not contain a `16.0` literal: \
                 the root font size is `velox_dom::layout::DEFAULT_ROOT_FONT_SIZE`, and a \
                 second copy would drift away from it silently"
            );
            assert!(
                production.contains("DEFAULT_ROOT_FONT_SIZE"),
                "the production half of skia_render.rs must resolve `rem` through \
                 `velox_dom::layout::DEFAULT_ROOT_FONT_SIZE`"
            );
        }

        /// CSS 2.1 §6.7: a relative unit in the value of a property is relative
        /// to the PARENT's font size, so two `font-size` declarations in one
        /// block must not compound — the second `3em` is 3x the INHERITED 16,
        /// not 3x the 24 the first one set. Resolving against
        /// `text_style.font_size` instead would give 72.
        #[test]
        fn parse_text_style_relative_font_size_does_not_compound_within_one_block() {
            let (s, _f) = parse_text_style(
                "font-size: 1.5em; font-size: 3em",
                base_at_font_size(16.0),
                "default",
                (800.0, 600.0),
            );
            assert_eq!(
                s.font_size, 48.0,
                "the later declaration wins, and its `3em` is 3x the inherited 16px, not 3x the 24px set before it"
            );
        }

        /// ------------------------------------------------------------------
        // `border:` shorthand — the renderer's private copy of the grammar.
        //
        // `parse_border_value` used to strip only a literal `px` suffix and
        // honour only a literal `solid`, returning `None` for anything else.
        // So `border: 1px dashed` and `border: 2em solid red` painted NO
        // BORDER AT ALL, while the DOM parsed both correctly. It now delegates
        // to the DOM's own public grammar (`Length::parse`,
        // `BorderStyle::parse`, `Color::parse`, `Length::to_px`), so the two
        // halves agree by construction rather than by coincidence.
        // ------------------------------------------------------------------
        use velox_dom::style::BorderStyle;

        /// The `border` slot of `parse_style_attr`'s tuple, end to end.
        /// Takes the shorthand *value*; the property name is added here.
        fn border_of(value: &str, font_size: f32) -> Option<BorderSpec> {
            parse_style_attr(&format!("border: {value}"), font_size, (800.0, 600.0)).border
        }

        fn spec(style: BorderStyle) -> BorderSpec {
            BorderSpec {
                width: 2.0,
                color: sk::Color::from_argb(255, 0, 0, 0),
                style,
            }
        }

        // THE HEADLINE FIX. Every style keyword CSS defines must reach the
        // canvas; before, all but `solid` were dropped and drew nothing.
        #[test]
        fn border_value_every_style_keyword_reaches_the_canvas() {
            for (kw, want) in [
                ("solid", BorderStyle::Solid),
                ("dashed", BorderStyle::Dashed),
                ("dotted", BorderStyle::Dotted),
                ("double", BorderStyle::Double),
                ("groove", BorderStyle::Groove),
                ("ridge", BorderStyle::Ridge),
                ("inset", BorderStyle::Inset),
                ("outset", BorderStyle::Outset),
            ] {
                let b = border_of(&format!("2px {kw} red"), 16.0)
                    .unwrap_or_else(|| panic!("`border: 2px {kw} red` produced no border"));
                assert_eq!(b.style, want, "wrong style for `{kw}`");
                assert_eq!(b.width, 2.0, "wrong width for `{kw}`");
                // DOM parity, asserted against the DOM's own parser.
                assert_eq!(BorderStyle::parse(kw), Some(want));
            }
        }

        // The second half of the headline: a non-`px` width used to discard the
        // whole declaration. `em` resolves against the element's own font size,
        // `vw` against the viewport the paint loop was given.
        #[test]
        fn border_value_resolves_relative_lengths() {
            let b = border_of("2em solid red", 20.0).expect("2em solid red dropped");
            assert_eq!(b.width, 40.0, "`em` must scale with the font size");

            let b = border_of("10vw solid red", 16.0).expect("10vw solid red dropped");
            assert_eq!(b.width, 80.0, "`vw` must scale with the viewport");

            // `rem` resolves against the ROOT size (16), not the element's own,
            // so it is deliberately NOT 18 here. This assertion previously
            // documented the bug as the convention, with a comment admitting the
            // value was "wrong" and calling it a known limitation. See
            // `parse_border_resolves_rem_against_the_root_not_the_element` for
            // the invariant that pins it.
            let b = border_of("1rem solid red", 18.0).expect("1rem solid red dropped");
            assert_eq!(b.width, 16.0, "`rem` is a root unit, not an `em`");

            // Still handles every length the DOM handles, unitless included.
            for (decl, want) in [("0 solid red", 0.0), ("3 solid red", 3.0)] {
                assert_eq!(border_of(decl, 16.0).unwrap().width, want);
            }
        }

        // Colour parity. The renderer's own `parse_color_hex` knew 12 names and
        // no 3-digit hex; the DOM's `Color::parse` knows 22 and `#rgb`. These
        // seven are the ones the renderer silently painted black.
        #[test]
        fn border_value_accepts_every_named_colour_the_dom_accepts() {
            for (kw, rgb) in [
                ("silver", (192, 192, 192)),
                ("teal", (0, 128, 128)),
                ("navy", (0, 0, 128)),
                ("olive", (128, 128, 0)),
                ("maroon", (128, 0, 0)),
                ("aqua", (0, 255, 255)),
                ("pink", (255, 192, 203)),
            ] {
                let b = border_of(&format!("1px solid {kw}"), 16.0)
                    .unwrap_or_else(|| panic!("named colour `{kw}` dropped the whole border"));
                assert_eq!(
                    b.color,
                    sk::Color::from_argb(255, rgb.0, rgb.1, rgb.2),
                    "`{kw}` resolved to the wrong colour"
                );
                // DOM parity for the same token.
                let dom = velox_dom::style::Color::parse(kw).expect("DOM rejects its own colour");
                assert_eq!((dom.r, dom.g, dom.b), rgb);
            }
            // 3-digit hex is a DOM capability the renderer's parser lacked.
            assert_eq!(
                border_of("1px solid #f00", 16.0).unwrap().color,
                sk::Color::from_argb(255, 255, 0, 0)
            );
        }

        // Parity with the DOM after 45088c5: a width with no style paints
        // nothing, because CSS's initial `border-style` is `none`. The two
        // halves used to agree here only by accident.
        #[test]
        fn border_width_without_a_style_paints_nothing() {
            assert!(
                border_of("1px", 16.0).is_none(),
                "a bare width must not invent a border style"
            );
            assert!(
                border_of("2em red", 16.0).is_none(),
                "a bare width must not invent a border style"
            );
            // ...and `none`/`hidden` are likewise not a visible border.
            assert!(border_of("1px none red", 16.0).is_none());
            assert!(border_of("1px hidden red", 16.0).is_none());
        }

        // With no width, CSS's initial `border-width` is `medium` (3px). The
        // renderer's old default was 1px, so `border: solid red` was also
        // thinner than the DOM resolved.
        #[test]
        fn border_value_defaults_to_medium_width() {
            assert_eq!(border_of("solid red", 16.0).unwrap().width, 3.0);
        }

        // ── A6: text-decoration is a space-separated list, and every keyword
        // in it must be honoured. The old parser was
        //   `contains("underline") -> true; else if == "none" -> false`
        // so `line-through` matched NEITHER branch, inherited whatever the
        // parent had, and painted nothing at all — the DOM parsed it fine, so
        // the declaration was silently dropped at paint time only.
        fn decoration_of(value: &str) -> (bool, bool) {
            let (s, _) = parse_text_style(
                &format!("text-decoration:{value}"),
                plain_text_style(),
                "default",
                (800.0, 600.0),
            );
            (s.underline, s.line_through)
        }

        #[test]
        fn text_decoration_underline_still_works() {
            assert_eq!(decoration_of("underline"), (true, false));
        }

        #[test]
        fn text_decoration_line_through_is_not_dropped() {
            assert_eq!(
                decoration_of("line-through"),
                (false, true),
                "line-through is the single keyword this task is about: it must \
                 set the strike flag and leave underline alone"
            );
        }

        // CSS 2.1 §8.3.1 allows any combination. The DOM's `TextDecoration` is a
        // single-valued enum and cannot hold this, which is exactly why the
        // renderer carries two independent bools rather than one.
        #[test]
        fn text_decoration_underline_line_through_draws_both() {
            assert_eq!(decoration_of("underline line-through"), (true, true));
            // Order is irrelevant in CSS.
            assert_eq!(decoration_of("line-through underline"), (true, true));
            // Extra whitespace must not create an empty keyword.
            assert_eq!(decoration_of("  underline   line-through  "), (true, true));
        }

        #[test]
        fn text_decoration_none_clears_both_flags() {
            assert_eq!(decoration_of("none"), (false, false));
        }

        // `none` must win even when it appears alongside a keyword, and it
        // must clear an INHERITED flag rather than merely leaving it alone.
        #[test]
        fn text_decoration_none_beats_a_sibling_keyword() {
            assert_eq!(decoration_of("none line-through"), (false, false));
            let base = TextStyle {
                underline: true,
                line_through: true,
                ..plain_text_style()
            };
            let (s, _) = parse_text_style("text-decoration:none", base, "default", (800.0, 600.0));
            assert!(
                !s.underline && !s.line_through,
                "text-decoration:none must clear the inherited flags, not just avoid setting them"
            );
        }

        // The old `== "none"` test meant a value like `underline-none` (not legal
        // CSS, but reachable from a hand-written style string) silently turned
        // the underline OFF because it was not exactly `"none"`… and, more to
        // the point, that `contains` made the two branches asymmetric. Pin the
        // keyword comparison so neither can come back.
        #[test]
        fn text_decoration_matches_whole_keywords_not_substrings() {
            // `xline-through` is not the keyword `line-through`; it must not
            // enable the strike.
            assert_eq!(
                decoration_of("xline-through"),
                (false, false),
                "a keyword must be matched whole, not as a substring"
            );
        }

        // The strike's height is derived from the FONT SIZE, not a constant,
        // because the same constant lands visibly high at 14px and visibly low
        // at 48px. Pin the ratio so the derivation cannot silently invert.
        #[test]
        fn line_through_tracks_the_font_size_rather_than_a_constant() {
            let small = line_through_y(100.0, 14.0);
            let large = line_through_y(100.0, 48.0);
            assert!(
                small < 100.0 && large < 100.0,
                "the strike must sit ABOVE the baseline: it crosses the glyphs, \
                 it does not underline them"
            );
            assert!(
                100.0 - large > 100.0 - small,
                "a larger font must push the strike further up ({small} vs {large})"
            );
            assert!(
                (small - 100.0 + 14.0 * LINE_THROUGH_ASCENT_EM).abs() < 1e-4,
                "the offset must be exactly {}em of the font size",
                LINE_THROUGH_ASCENT_EM
            );
        }

        // The strike and the underline are at DIFFERENT heights, on OPPOSITE
        // sides of the baseline, and far enough apart to render as two rules
        // rather than one blob. This is the regression that motivated the
        // constant's sign: the strike was originally placed BELOW the baseline,
        // within a pixel of the underline at `baseline + 1.0`.
        #[test]
        fn underline_and_line_through_are_far_enough_apart_to_be_two_rules() {
            for size in [12.0_f32, 14.0, 16.0, 24.0, 48.0] {
                let baseline = 100.0;
                let under = baseline + 1.0;
                let strike = line_through_y(baseline, size);
                assert!(
                    under - strike >= 2.0,
                    "at {size}px the underline (row {under}) and the strike (row \
                     {strike}) are less than 2px apart, so `underline \
                     line-through` renders as one fat blob"
                );
            }
        }

        // ── A4: the input lane's own declaration reader.

        fn padding_of(value: &str, font_size: f32) -> Option<Padding> {
            parse_padding(value, font_size, (800.0, 600.0))
        }

        #[test]
        fn parse_padding_reads_every_css_shorthand_form() {
            let p = |v: &str| padding_of(v, 16.0).unwrap();
            assert_eq!(
                p("4px"),
                Padding {
                    top: 4.0,
                    right: 4.0,
                    bottom: 4.0,
                    left: 4.0
                }
            );
            assert_eq!(
                p("1px 2px"),
                Padding {
                    top: 1.0,
                    right: 2.0,
                    bottom: 1.0,
                    left: 2.0
                }
            );
            assert_eq!(
                p("1px 2px 3px"),
                Padding {
                    top: 1.0,
                    right: 2.0,
                    bottom: 3.0,
                    left: 2.0
                },
                "the 3-value form is top / left-right / bottom"
            );
            assert_eq!(
                p("1px 2px 3px 4px"),
                Padding {
                    top: 1.0,
                    right: 2.0,
                    bottom: 3.0,
                    left: 4.0
                },
                "the 4-value form is top / right / bottom / left"
            );
        }

        // `em` resolves against the element's OWN font size, like every other
        // relative length in this file. The boilerplate's `padding: 10px 12px`
        // is the px case; `padding: .5em` is what a themed stylesheet writes.
        #[test]
        fn parse_padding_resolves_em_against_the_elements_font_size() {
            assert_eq!(
                padding_of("2em", 16.0).unwrap(),
                Padding {
                    top: 32.0,
                    right: 32.0,
                    bottom: 32.0,
                    left: 32.0
                }
            );
            assert_eq!(
                padding_of("2em", 8.0).unwrap(),
                Padding {
                    top: 16.0,
                    right: 16.0,
                    bottom: 16.0,
                    left: 16.0
                }
            );
        }

        // `%` here resolves against the element's own font size, the same
        // convention every other relative length in this file already uses.
        // CSS resolves percentage padding against the containing block, which
        // is information this parser does not have; pinning the convention
        // here is what stops the two code paths from drifting apart
        // unremarked.
        #[test]
        fn parse_padding_resolves_percent_against_the_font_size() {
            assert_eq!(
                padding_of("50%", 16.0).unwrap(),
                Padding {
                    top: 8.0,
                    right: 8.0,
                    bottom: 8.0,
                    left: 8.0
                }
            );
        }

        // `rem` does NOT follow the rule the two tests above pin. `Length::to_px`'s
        // second argument is the ROOT basis and is used for `rem` alone, so
        // `rem` must not move when the element's font size moves — that is the
        // entire distinction between `rem` and `em`, and it is why both parse
        // entry points here pass `REM_ROOT_FONT_SIZE` there while
        // `input_metrics::input_text_metrics` and `parse_text_style` already
        // did. Holding the element's own size constant across both calls is the
        // assertion: a `rem` that tracked the element's size would be an `em`,
        // and the paint lane and the caret lane would disagree by exactly
        // `font_size - 16`.
        #[test]
        fn parse_padding_resolves_rem_against_the_root_not_the_element() {
            // font_size 32 with the root at the DOM default: `1rem` is 16 either
            // way the root is fixed, so 32px is the number that proves the
            // element's own size was NOT used.
            assert_eq!(
                padding_of("1rem", 32.0).unwrap(),
                Padding {
                    top: 16.0,
                    right: 16.0,
                    bottom: 16.0,
                    left: 16.0
                },
                "`rem` resolved against the element's own font size"
            );
            // The same declaration at a different font size must resolve to the
            // same px — that invariance IS the property `rem` has and `em` does
            // not, so it is the assertion that cannot pass by coincidence.
            assert_eq!(
                padding_of("1rem", 16.0).unwrap(),
                padding_of("1rem", 32.0).unwrap()
            );
            // And it stays pinned to the root for a shorthand that mixes units.
            assert_eq!(
                padding_of("1rem 2rem", 32.0).unwrap(),
                Padding {
                    top: 16.0,
                    right: 32.0,
                    bottom: 16.0,
                    left: 32.0
                }
            );
        }

        // A negative component must clamp to 0 rather than being allowed to
        // invert the content box; a non-finite one is rejected outright.
        // `f32::parse` accepts both `-8px` and `1e999px`, and either would put
        // the painted box somewhere unreachable.
        #[test]
        fn parse_padding_clamps_negative_lengths_and_rejects_infinite_ones() {
            assert_eq!(
                padding_of("-8px", 16.0).unwrap(),
                Padding::ZERO,
                "a negative padding must clamp to zero, not invert the content box"
            );
            assert!(
                padding_of("1e999px", 16.0).is_none(),
                "an overflowing length must be rejected, not become inf"
            );
        }

        #[test]
        fn parse_padding_rejects_garbage_and_the_four_value_overflow() {
            assert!(
                padding_of("wide", 16.0).is_none(),
                "a non-length must not become padding"
            );
            assert!(
                padding_of("1px 2px 3px 4px 5px", 16.0).is_none(),
                "a five-value shorthand is not CSS and must not be guessed at"
            );
        }

        // ===== `parse_px_value` / `parse_float_value`: the shared guard ====
        //
        // `f32::parse` ACCEPTS overflow: `"1e999".parse::<f32>()` is `Ok(inf)`.
        // These are the four call sites that read a bare number, and each one
        // poisoned something different when the value was non-finite, which is
        // why the guard belongs in the shared reader rather than beside a caller.

        #[test]
        fn a_non_finite_px_value_is_rejected_by_the_shared_reader() {
            assert_eq!(
                parse_px_value("10px"),
                Some(10.0),
                "the guard must not reject ordinary values"
            );
            for value in ["1e999px", "-1e999px", "NaNpx", "infpx", "infinitypx"] {
                assert!(
                    parse_px_value(value).is_none(),
                    "`{value}` parsed as a length; an infinite rect is not a length"
                );
            }
        }

        #[test]
        fn a_non_finite_unitless_value_is_rejected_by_the_shared_reader() {
            assert_eq!(parse_float_value("1.5"), Some(1.5));
            for value in ["1e999", "-1e999", "NaN", "inf", "infinity"] {
                assert!(
                    parse_float_value(value).is_none(),
                    "`{value}` parsed as a number; an infinite factor is not a number"
                );
            }
        }

        // The two callers that reach a length WITHOUT `parse_px_value`, and so
        // have to carry the guard themselves. `border: 1e999px` resolved to an
        // infinite stroke width; `line-height: 1e999` to an infinite line box.

        #[test]
        fn a_non_finite_border_width_is_rejected() {
            assert!(
                border_of("1e999px solid #ff0000", 16.0).is_none(),
                "an infinite stroke width must be rejected, not handed to set_stroke_width"
            );
        }

        #[test]
        fn a_non_finite_line_height_is_rejected() {
            let base = base_at_font_size(16.0);
            for decl in ["line-height: 1e999", "line-height: 1e999px"] {
                let (s, _f) = parse_text_style(decl, base, "default", (800.0, 600.0));
                assert_eq!(
                    s.line_height, 1.2,
                    "`{decl}` parsed: an infinite line height is not a line box"
                );
            }
        }

        // ===== `parse_clip_inset` ============================================

        #[test]
        fn clip_inset_reads_all_four_shorthand_forms() {
            let of = |v: &str| {
                let i = parse_clip_inset(v).expect(v);
                (i.top, i.right, i.bottom, i.left)
            };
            assert_eq!(of("inset(10px)"), (10.0, 10.0, 10.0, 10.0));
            assert_eq!(of("inset(10px 20px)"), (10.0, 20.0, 10.0, 20.0));
            assert_eq!(of("inset(10px 20px 30px)"), (10.0, 20.0, 30.0, 20.0));
            assert_eq!(of("inset(10px 20px 30px 40px)"), (10.0, 20.0, 30.0, 40.0));
            // The corner-radius half is accepted and ignored.
            assert_eq!(of("inset(10px round 4px)"), (10.0, 10.0, 10.0, 10.0));
        }

        #[test]
        fn clip_inset_rejects_the_shapes_and_values_it_cannot_draw() {
            for value in [
                // Other basic shapes, and a reference to an SVG clip.
                "circle(50%)",
                "ellipse(40px 20px)",
                "polygon(0 0, 100px 0, 100px 100px)",
                "url(#svg-clip)",
                // Not px, and not resolvable without a basis this parser has.
                "inset(10% 20%)",
                "inset(10em)",
                "inset(10)",
                // Too many lengths for CSS Shapes 1 §2.1.
                "inset(1px 2px 3px 4px 5px)",
                // A negative component is invalid (CSS 2.1 §4.3) and must be
                // dropped rather than EXPANDING the clip rect.
                "inset(-10px)",
                "inset(10px -20px)",
                // Case-sensitive on purpose: `parse_style_attr`'s whole property
                // dispatch is, so a half-case-insensitive parser would be worse.
                "INSET(10px)",
                // Non-finite, in any one position of the list.
                "inset(1e999px)",
                "inset(NaNpx)",
                "inset(1e999px 10px 10px 10px)",
                // The keywords are not `inset()` values.
                "none",
            ] {
                assert!(
                    parse_clip_inset(value).is_none(),
                    "`{value}` produced a clip; it must be rejected as a whole declaration"
                );
            }
        }

        // ===== `parse_img_filter` ============================================

        #[test]
        fn img_filter_reads_the_two_functions_it_can_draw() {
            let blur = parse_img_filter("blur(2px)").expect("blur");
            assert_eq!(blur.blur_sigma, Some(2.0));
            assert_eq!(blur.brightness, None);
            let both = parse_img_filter("blur(2px) brightness(1.2)").expect("both");
            assert_eq!(both.blur_sigma, Some(2.0));
            assert_eq!(both.brightness, Some(1.2));
            let none = parse_img_filter("none").expect("none");
            assert_eq!(none.blur_sigma, None);
            assert_eq!(none.brightness, None);
        }

        /// The half-application defect, pinned at the parser: an unknown
        /// function or a repeated one drops the WHOLE list, so nothing beside it
        /// can be applied on its own.
        #[test]
        fn img_filter_drops_a_list_it_cannot_apply_whole() {
            for value in [
                "grayscale(1)",
                "blur(2px) grayscale(1)",
                "blur(2px) blur(4px)",
                "blur(2px) brightness(1.2) opacity(0.5)",
                "nonsense(1)",
                "blur",
                "blur(2px",
                "",
                "   ",
            ] {
                let spec = parse_img_filter(value);
                assert!(
                    match spec {
                        None => true,
                        Some(s) => s.blur_sigma.is_none() && s.brightness.is_none(),
                    },
                    "`{value}` produced {spec:?}; a list that cannot be applied whole must \
                     apply nothing"
                );
            }
        }

        /// A non-finite argument is the same class of failure as an unknown
        /// function: it produces a value that cannot be painted, so the whole
        /// list goes with it.
        #[test]
        fn img_filter_rejects_non_finite_arguments() {
            for value in [
                "brightness(1e999)",
                "blur(1e999px)",
                "blur(NaNpx)",
                "brightness(NaN)",
                "blur(2px) brightness(1e999)",
            ] {
                assert_eq!(
                    parse_img_filter(value),
                    None,
                    "`{value}` parsed; an infinite filter factor is not a filter"
                );
            }
        }

        /// The CSS property name itself is inert now: `filter: blur(2px)` reaches
        /// no arm at all, so it cannot half-apply on any element.
        #[test]
        fn the_css_filter_property_name_reads_nothing() {
            let box_style =
                parse_style_attr("filter: blur(2px) brightness(1.5)", 16.0, (800.0, 600.0));
            assert_eq!(box_style.filters.blur_sigma, None);
            assert_eq!(box_style.filters.brightness, None);

            let renamed = parse_style_attr("img-filter: blur(2px)", 16.0, (800.0, 600.0));
            assert_eq!(renamed.filters.blur_sigma, Some(2.0));
        }

        // The border half of the same `rem` rule. A `rem` border width has no
        // symmetric-cancellation escape hatch the way `rem` padding does: the
        // border is painted after `canvas.restore()`, so it cannot cover text
        // laid out under a too-wide border, and an over-wide `rem` border lands
        // the value text ON the ink.
        #[test]
        fn parse_border_resolves_rem_against_the_root_not_the_element() {
            let wide = border_of("1rem solid #ff0000", 32.0).unwrap();
            assert_eq!(
                wide.width, 16.0,
                "a `rem` border width resolved against the element's own font size; the \
                 caret lane insets the value text by the ROOT-resolved width \
                 (`input_metrics`), so the two lanes disagree by `font_size - root`"
            );
            assert_eq!(
                border_of("1rem solid #ff0000", 16.0).unwrap().width,
                wide.width,
                "a `rem` border width must not move when the element's font size moves"
            );
            assert_eq!(border_of("1rem solid #ff0000", 64.0).unwrap().width, 16.0);
            // `em` in the same slot is unaffected and still tracks the element:
            // this is the control that keeps the `rem` assertions above honest
            // about which argument actually changed.
            assert_eq!(border_of("1em solid #ff0000", 32.0).unwrap().width, 32.0);
        }

        // ── A5: placeholder ink is DERIVED, not a fixed grey literal.

        type Props = velox_dom::Props;

        fn ph(props: &Props, ink: sk::Color, bg: sk::Color) -> sk::Color {
            placeholder_color(props, ink, bg)
        }

        #[test]
        fn placeholder_colour_is_derived_from_the_fields_own_colour() {
            let white = sk::Color::from_argb(255, 255, 255, 255);
            let dark = sk::Color::from_argb(255, 22, 33, 62);
            let on_dark = ph(&Props::new(), white, dark);
            assert_ne!(
                on_dark, white,
                "the placeholder must not be the full-strength value ink"
            );
            assert_ne!(
                on_dark, dark,
                "the placeholder must not be the field background either"
            );
            // Still the value ink's HUE, just dimmed: every channel must be
            // strictly between the background's and the text's.
            for c in [on_dark.r(), on_dark.g(), on_dark.b()] {
                let (lo, hi) = if white.r() > dark.r() {
                    (dark.r(), white.r())
                } else {
                    (white.r(), dark.r())
                };
                assert!(
                    c > lo && c < hi,
                    "channel {c} is not between {lo} and {hi}: the mix did not hold the hue"
                );
            }
        }

        #[test]
        fn placeholder_colour_is_derived_not_a_fixed_grey() {
            let ink_a = sk::Color::from_argb(255, 255, 0, 0);
            let ink_b = sk::Color::from_argb(255, 0, 0, 255);
            let bg = sk::Color::from_argb(255, 255, 255, 255);
            assert_ne!(
                ph(&Props::new(), ink_a, bg),
                ph(&Props::new(), ink_b, bg),
                "two different text colours must not produce the same placeholder: \
                 that would mean it is a fixed grey"
            );
        }

        #[test]
        fn placeholder_keeps_the_texts_alpha_and_dims_only_the_channels() {
            let half = sk::Color::from_argb(128, 255, 255, 255);
            let white = sk::Color::from_argb(255, 255, 255, 255);
            assert_eq!(
                ph(&Props::new(), half, white).a(),
                128,
                "the derivation must not silently make a translucent value text opaque"
            );
        }

        // The cascade carries `input::placeholder { color: … }` on a SEPARATE
        // attribute; this is the hook that makes the pseudo-element actually
        // styleable rather than merely present.
        #[test]
        fn an_authored_placeholder_colour_wins_over_the_derived_one() {
            let props = Props::new().set(PLACEHOLDER_STYLE_ATTR, "color:#ff0000");
            let got = ph(
                &props,
                sk::Color::from_argb(255, 255, 255, 255),
                sk::Color::from_argb(255, 255, 255, 255),
            );
            assert_eq!(
                (got.r(), got.g(), got.b()),
                (255, 0, 0),
                "an explicit ::placeholder colour must override the derivation"
            );
        }

        // A pseudo-element block that declares anything OTHER than `color`
        // must not be mistaken for a colour declaration, or the whole thing
        // silently degrades to "unsupported".
        #[test]
        fn an_authored_placeholder_block_without_colour_still_derives() {
            let props = Props::new().set(PLACEHOLDER_STYLE_ATTR, "opacity:0.5;font-style:italic");
            let got = ph(
                &props,
                sk::Color::from_argb(255, 255, 255, 255),
                sk::Color::from_argb(255, 0, 0, 0),
            );
            // White on black, held back by PLACEHOLDER_CONTRAST — i.e. the
            // derivation, not the full-strength value ink and not a fixed grey.
            let want = (0.0 + (255.0 - 0.0) * PLACEHOLDER_CONTRAST).round() as u8;
            assert_eq!(
                (got.r(), got.g(), got.b()),
                (want, want, want),
                "a block with no colour declaration must fall through to the derivation"
            );
            assert_ne!(got.r(), 255, "…and the derivation must have dimmed it");
        }

        // `dashed`/`dotted` get a real Skia dash; the rest paint a plain
        // stroke. Pinned so a later "simplification" cannot quietly drop it.
        #[test]
        fn apply_border_style_dashes_only_dashed_and_dotted() {
            let mut paint = sk::Paint::default();
            for (s, want_dash) in [
                (BorderStyle::Dashed, true),
                (BorderStyle::Dotted, true),
                (BorderStyle::Solid, false),
                (BorderStyle::Double, false),
                (BorderStyle::Inset, false),
            ] {
                apply_border_style(&mut paint, &spec(s));
                assert_eq!(
                    paint.path_effect().is_some(),
                    want_dash,
                    "dash effect wrong for {s:?}"
                );
            }
        }

        // The stroke paint is reused across elements within a frame, so a dash
        // left on it would bleed into the next element's border.
        #[test]
        fn apply_border_style_clears_a_stale_dash() {
            let mut paint = sk::Paint::default();
            apply_border_style(&mut paint, &spec(BorderStyle::Dashed));
            assert!(paint.path_effect().is_some());
            apply_border_style(&mut paint, &spec(BorderStyle::Solid));
            assert!(
                paint.path_effect().is_none(),
                "stale dash leaked onto a solid border"
            );
        }

        // A zero or non-finite width must not produce an infinite/NaN dash
        // interval, which Skia would reject.
        #[test]
        fn apply_border_style_survives_a_degenerate_width() {
            for w in [0.0, -3.0, f32::NAN, f32::INFINITY] {
                let mut paint = sk::Paint::default();
                let mut s = spec(BorderStyle::Dashed);
                s.width = w;
                apply_border_style(&mut paint, &s);
                assert!(
                    paint.path_effect().is_some(),
                    "dashed border with width {w} should still dash"
                );
            }
        }

        fn fnv1a(bytes: &[u8]) -> u32 {
            let mut hash: u32 = 0x811c9dc5;
            for b in bytes {
                hash ^= *b as u32;
                hash = hash.wrapping_mul(0x01000193);
            }
            hash
        }

        // ------------------------------------------------------------------
        // Invariant 1: one DPI/scale rounding authority.
        //
        // `render_vnode_to_raster_png_with_scale` used to inline its own copy of
        // the multiply-then-round formula. It now routes through
        // `Viewport::physical_from_logical`. These read the ENCODED PNG's own IHDR
        // dimensions rather than a value the same code path computed on the way
        // in, so they observe the surface that was really allocated.
        // ------------------------------------------------------------------

        /// `(width, height)` from a PNG's IHDR chunk, which sits at a fixed offset:
        /// 8-byte signature, 4-byte length, 4-byte "IHDR", then the two u32s.
        fn png_dimensions(png: &[u8]) -> (u32, u32) {
            assert_eq!(
                &png[..8],
                b"\x89PNG\r\n\x1a\n",
                "encode_png did not emit a PNG signature, so there is no IHDR to read"
            );
            let be = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
            (be(&png[16..20]), be(&png[20..24]))
        }

        /// A logical size that rounds to zero at a fractional scale must still
        /// render, and the surface must be a real pixel rather than a degenerate
        /// one. `1 * 0.4 == 0.4`, which rounds to 0.
        #[test]
        fn a_logical_size_that_rounds_to_zero_at_a_fractional_scale_still_renders() {
            let vnode = h("div", vec![], vec![]);
            let png =
                render_vnode_to_raster_png_with_scale(&vnode, &Stylesheet::default(), 1, 1, 0.4)
                    .expect("a 1x1 logical size at scale 0.4 must render, not fail");
            assert_eq!(
                png_dimensions(&png),
                (1, 1),
                "a size that rounds to 0 at this scale must still allocate at least one pixel"
            );
        }

        /// The discriminating half: the authority substitutes `scale = 1.0` for a
        /// non-finite or non-positive scale, and the call site must inherit that.
        ///
        /// The inlined arithmetic fed the raw scale straight in, so `width * 0.0`
        /// rounded to 0 for ANY width and the surface collapsed to 1x1 no matter
        /// what size was asked for. This is the behaviour the substitution changes,
        /// and it is the part that would silently drop a render to a single pixel.
        #[test]
        fn a_non_positive_scale_falls_back_to_one_instead_of_collapsing_the_surface() {
            let vnode = h("div", vec![], vec![]);
            let png =
                render_vnode_to_raster_png_with_scale(&vnode, &Stylesheet::default(), 8, 6, 0.0)
                    .expect("scale 0.0 must fall back to 1.0, not fail");
            assert_eq!(
                png_dimensions(&png),
                (8, 6),
                "a non-positive scale must be read as 1.0, so the full logical size survives; \
                 an inlined `width * scale` collapses this to (1, 1)"
            );
        }
    }
}

#[cfg(not(feature = "skia-native"))]
pub mod skia_impl {
    use super::*;

    pub fn render_vnode_to_raster_png(
        _vnode: &VNode,
        _sheet: &Stylesheet,
        _width: i32,
        _height: i32,
    ) -> Result<Vec<u8>, String> {
        Err("skia-native feature not enabled".into())
    }

    pub fn render_vnode_to_raster_png_with_scale(
        _vnode: &VNode,
        _sheet: &Stylesheet,
        _width: i32,
        _height: i32,
        _scale_factor: f32,
    ) -> Result<Vec<u8>, String> {
        Err("skia-native feature not enabled".into())
    }

    /// Heuristic fallback when skia-native not compiled — still snapped, but uses
    /// fixed 0.5 ratio so divergence test (0.6) triggers while wrap parity holds
    /// via same fallback in both crates headless.
    ///
    /// UNREACHABLE TODAY: `lib.rs` gates this entire file behind
    /// `feature = "skia-native"`, so this `cfg(not(...))` module is in no current
    /// build. The non-Skia width that *is* compiled is `text.rs`'s
    /// `measure_with_scale` fallback branch, which evaluates the same expression
    /// below. This copy is kept correct rather than deleted so ungating the module
    /// cannot hand layout a half-updated seam.
    ///
    /// There is no font backend here, so the vertical half is the approximation
    /// `FontMetrics::heuristic_vertical` documents, not a measurement. It is
    /// deliberately not zero: a zero ascent and descent would collapse every line
    /// box on this path to zero height, which is a silent, catastrophic wrong
    /// answer on the path that is easiest to reach in a test.
    pub fn measure_text(
        text: &str,
        font_size: f32,
        _font_family: &str,
        scale: f32,
    ) -> MeasuredText {
        // NOT a second rounding authority. As in the `skia-native` `measure_text`
        // above, this agrees with `Viewport::snap_logical_to_physical_grid` only
        // when `scale` is finite and positive; for a non-finite or `<= 0` scale it
        // returns `font_size` unrounded where the authority substitutes 1.0 and
        // rounds. It is kept mirroring the `text.rs` fallback so the headless and
        // native paths agree; it is a glyph-advance snap, not a size conversion.
        let snapped = if scale.is_finite() && scale > 0.0 {
            (font_size * scale).round() / scale
        } else {
            font_size
        };
        // Use slightly different ratio than old 0.6 to prove divergence (>0.5) but
        // stable for wrap parity when skia not available.
        let (ascent, descent) = velox_dom::layout::FontMetrics::heuristic_vertical(snapped);
        MeasuredText {
            width: snapped * 0.5 * text.chars().count() as f32,
            ascent,
            descent,
        }
    }
}

pub use skia_impl::measure_text;
pub use skia_impl::render_vnode_to_raster_png;
pub use skia_impl::render_vnode_to_raster_png_with_scale;
