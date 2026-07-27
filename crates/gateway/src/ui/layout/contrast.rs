//! WCAG AA contrast-ratio verification for the design tokens.
//!
//! Implements the WCAG 2.1 relative-luminance and contrast-ratio formulas
//! (https://www.w3.org/TR/WCAG21/#dfn-contrast-ratio) and pins the
//! foreground/background pairs that Noye actually renders. If a future
//! token edit drops a pair below the threshold, the unit test in this
//! module fails before deploy — accidental contrast regressions cannot
//! ship.
//!
//! ## What's pinned
//!
//! The pairs in `tests::critical_pairs_meet_aa` cover every place where
//! body-sized text or UI-sized indicators sit on a colored background.
//! When in doubt about whether a pair needs pinning, ask: "would a user
//! ever read this without being able to control the colors?" — if yes,
//! it should be in the test.
//!
//! ## Why pure Rust and not a CSS preprocessor
//!
//! The tokens live in plain CSS (no preprocessor) so the deployment is
//! one Rust crate, not a JS/CSS toolchain. Verifying contrast at compile
//! time would require running CSS through a parser; verifying at unit-
//! test time, with the hex codes hard-coded in this module, is simpler
//! and catches the same regressions.
//!
//! ## Limitations
//!
//! - Does not verify text-over-image or alpha-blended pairs. We don't
//!   use either today.
//! - Pins specific hex codes, so editing the token CSS without updating
//!   `tests::critical_pairs_meet_aa` will leave the test green even if
//!   the new value fails AA. Mitigation: code review + the explicit
//!   list of pairs makes drift easy to catch.

#![allow(dead_code)]

/// Parse a `#RRGGBB` string into `(R, G, B)` 0..=255.
///
/// Pure helper used only by the unit tests; not part of the runtime
/// path. We keep it hand-rolled (no dependency on a color crate) to
/// keep the runtime crate lean — `palette` or `csscolorparser` would
/// pull in noticeable amounts of code for what is, here, a single
/// hex parse.
pub fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some((r, g, b))
}

/// Convert an sRGB channel (0..=1.0) to its linear-light value, per
/// WCAG 2.1's relative-luminance formula.
fn channel_to_linear(c: f64) -> f64 {
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Relative luminance of an RGB color, per WCAG 2.1.
///
/// Result is in `0.0..=1.0`; black is 0, white is 1.
pub fn relative_luminance(rgb: (u8, u8, u8)) -> f64 {
    let r = channel_to_linear(rgb.0 as f64 / 255.0);
    let g = channel_to_linear(rgb.1 as f64 / 255.0);
    let b = channel_to_linear(rgb.2 as f64 / 255.0);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Contrast ratio between two colors, per WCAG 2.1 §1.4.3.
///
/// Result is in `1.0..=21.0`. Higher is better. AA requires 4.5 for
/// body text and 3.0 for large text or UI components.
pub fn contrast_ratio(fg: (u8, u8, u8), bg: (u8, u8, u8)) -> f64 {
    let l1 = relative_luminance(fg);
    let l2 = relative_luminance(bg);
    let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

/// Check that a fg/bg pair meets WCAG AA for body text (4.5:1) when
/// `body = true`, or AA for large/UI elements (3:1) when `body = false`.
pub fn meets_aa(fg_hex: &str, bg_hex: &str, body: bool) -> bool {
    let fg = match parse_hex(fg_hex) {
        Some(rgb) => rgb,
        None => return false,
    };
    let bg = match parse_hex(bg_hex) {
        Some(rgb) => rgb,
        None => return false,
    };
    let threshold = if body { 4.5 } else { 3.0 };
    contrast_ratio(fg, bg) >= threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_accepts_canonical_form() {
        assert_eq!(parse_hex("#ffffff"), Some((255, 255, 255)));
        assert_eq!(parse_hex("#000000"), Some((0, 0, 0)));
        assert_eq!(parse_hex("#0F1117"), Some((0x0F, 0x11, 0x17)));
    }

    #[test]
    fn parse_hex_rejects_malformed() {
        assert_eq!(parse_hex(""), None);
        assert_eq!(parse_hex("ffffff"), None); // missing leading '#'
        assert_eq!(parse_hex("#fff"), None); // 3-char shorthand not supported
        assert_eq!(parse_hex("#ggg000"), None);
        assert_eq!(parse_hex("#1234567"), None);
    }

    #[test]
    fn contrast_white_on_black_is_max() {
        let r = contrast_ratio((255, 255, 255), (0, 0, 0));
        // WCAG formula yields exactly 21.0 for pure black ↔ white.
        assert!((r - 21.0).abs() < 1e-6, "expected 21.0, got {r}");
    }

    #[test]
    fn contrast_identical_is_one() {
        let r = contrast_ratio((128, 128, 128), (128, 128, 128));
        assert!((r - 1.0).abs() < 1e-9);
    }

    #[test]
    fn contrast_is_symmetric() {
        let a = contrast_ratio((100, 200, 50), (30, 40, 80));
        let b = contrast_ratio((30, 40, 80), (100, 200, 50));
        assert!((a - b).abs() < 1e-9);
    }

    /// Critical pairs: any place where body-size text or a UI indicator
    /// sits on a colored background. All pairs MUST meet WCAG AA.
    ///
    /// If you edit a token in `style.rs`, update the corresponding hex
    /// here. Failing this test is a deploy-blocker — see module docs.
    #[test]
    fn critical_pairs_meet_aa() {
        let pairs: &[(&str, &str, &str, bool)] = &[
            // (description, fg, bg, body_text?)
            // Dark theme — body text on each surface
            ("dark: --c-text on --c-bg", "#e4e6ef", "#0f1117", true),
            ("dark: --c-text on --c-surface", "#e4e6ef", "#1a1d27", true),
            ("dark: --c-text on --c-surface-2", "#e4e6ef", "#232734", true),
            ("dark: --c-text-muted on --c-bg", "#a1a5b8", "#0f1117", true),
            ("dark: --c-text-muted on --c-surface", "#a1a5b8", "#1a1d27", true),
            ("dark: --c-text-quiet on --c-bg", "#71758a", "#0f1117", false),
            // Dark theme — status badge fg on its bg
            ("dark: badge-up", "#4ade80", "#052e1a", true),
            ("dark: badge-down", "#f87171", "#4a1313", true),
            ("dark: badge-degraded", "#fbbf24", "#4a2e08", true),
            ("dark: badge-maint", "#c4b5fd", "#2a1f5c", true),
            ("dark: badge-unknown", "#94a3b8", "#1f2937", true),
            ("dark: badge-info", "#7dd3fc", "#1e3a52", true),
            // Dark theme — primary button text on primary bg
            ("dark: primary-text on primary", "#0f1117", "#7a98ff", true),
            // Light theme — body text on each surface
            ("light: --c-text on --c-bg", "#1a1d27", "#f5f6fa", true),
            ("light: --c-text on --c-surface", "#1a1d27", "#ffffff", true),
            ("light: --c-text on --c-surface-2", "#1a1d27", "#eef0f7", true),
            ("light: --c-text-muted on --c-bg", "#4b5163", "#f5f6fa", true),
            ("light: --c-text-muted on --c-surface", "#4b5163", "#ffffff", true),
            ("light: --c-text-quiet on --c-bg", "#6b7280", "#f5f6fa", false),
            // Light theme — status badges
            ("light: badge-up", "#166534", "#d1fae5", true),
            ("light: badge-down", "#991b1b", "#fee2e2", true),
            ("light: badge-degraded", "#92400e", "#fef3c7", true),
            ("light: badge-maint", "#5b21b6", "#ede9fe", true),
            ("light: badge-unknown", "#4b5563", "#f3f4f6", true),
            ("light: badge-info", "#075985", "#e0f2fe", true),
            // Light theme — primary button
            ("light: primary-text on primary", "#ffffff", "#3b5bdb", true),
        ];

        let mut failures = Vec::new();
        for (label, fg, bg, body) in pairs {
            let fg_rgb = parse_hex(fg).expect(label);
            let bg_rgb = parse_hex(bg).expect(label);
            let ratio = contrast_ratio(fg_rgb, bg_rgb);
            let threshold = if *body { 4.5 } else { 3.0 };
            if ratio < threshold {
                failures.push(format!(
                    "{label}: {fg} on {bg} = {ratio:.2}:1 (< {threshold})"
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "WCAG AA contrast regressions:\n  {}",
            failures.join("\n  ")
        );
    }

    #[test]
    fn meets_aa_threshold_distinction() {
        // Specific pair that meets AA for UI but not for body: a slightly
        // muted fg on bg that yields ~3.5:1.
        let fg = "#888888";
        let bg = "#ffffff";
        let ratio = contrast_ratio(parse_hex(fg).unwrap(), parse_hex(bg).unwrap());
        // Sanity: 0x88 on white is in the 3-4 range (~3.54:1).
        assert!(ratio >= 3.0 && ratio < 4.5, "ratio was {ratio}");
        assert!(meets_aa(fg, bg, false));
        assert!(!meets_aa(fg, bg, true));
    }

    #[test]
    fn meets_aa_handles_malformed_hex() {
        assert!(!meets_aa("not-a-color", "#000000", true));
        assert!(!meets_aa("#000000", "not-a-color", true));
    }
}
