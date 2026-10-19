// SPDX-License-Identifier: Apache-2.0

//! The color tokens from docs/ux.md, light and dark, as CSS.
//!
//! The rules name colors; the color sheet is swapped when libadwaita turns dark.

use gtk::gdk;

/// (name, light, dark). Each tint pairs a foreground with its background.
const TOKENS: &[(&str, &str, &str)] = &[
    ("accent_bg_color", "#2F5D8C", "#8DB4E2"),
    ("accent_color", "#2F5D8C", "#8DB4E2"),
    ("accent_fg_color", "#FFFFFF", "#10243A"),
    ("ow_candidate_fg", "#8F5600", "#FFC266"),
    ("ow_candidate_bg", "#FFF4E0", "#3A2A10"),
    ("ow_incomplete_fg", "#B34700", "#FFA066"),
    ("ow_incomplete_bg", "#FFEDE0", "#3A2210"),
    ("ow_neutral_fg", "#4A5560", "#C3CCD5"),
    ("ow_neutral_bg", "#EEF1F4", "#232A31"),
    ("ow_error_fg", "#B3261E", "#F2B8B5"),
    ("ow_error_bg", "#FCE8E6", "#3B1D1B"),
    ("ow_demo_fg", "#6B4FA0", "#C9B6F0"),
    ("ow_demo_bg", "#F1ECFA", "#2B2338"),
];

const RULES: &str = r#"
.ow-card { padding: 18px; border-radius: 12px; }
.ow-title { font-size: 2.4em; font-weight: 800; }
.ow-tagline { font-size: 1.15em; }
.ow-tone-candidate { background-color: @ow_candidate_bg; color: @ow_candidate_fg; }
.ow-tone-incomplete { background-color: @ow_incomplete_bg; color: @ow_incomplete_fg; }
.ow-tone-neutral { background-color: @ow_neutral_bg; color: @ow_neutral_fg; }
.ow-tone-error { background-color: @ow_error_bg; color: @ow_error_fg; }
.ow-demo { background-color: @ow_demo_bg; color: @ow_demo_fg; }
.ow-headline { padding: 20px 22px; border-radius: 12px; }
.ow-headline-title { font-size: 1.4em; font-weight: 800; }
.ow-chip { padding: 3px 10px; border-radius: 999px; font-size: 0.85em; font-weight: 700; }
.ow-badge { padding: 2px 8px; border-radius: 6px; font-size: 0.85em; font-weight: 700; }
.ow-trust { padding: 6px 14px; border-radius: 999px; background-color: alpha(currentColor, 0.07); font-weight: 600; }
.ow-drop { padding: 36px 24px; border-radius: 12px; border: 2px dashed alpha(currentColor, 0.22); }
.ow-drop.ow-drop-hover { border-color: @accent_color; background-color: alpha(@accent_bg_color, 0.10); }
.ow-candidate-card { box-shadow: inset 0 0 0 2px @ow_candidate_fg; }
.ow-picture { border-radius: 8px; background-color: alpha(currentColor, 0.06); border: 1px solid alpha(currentColor, 0.12); }
.ow-estimate-time { font-size: 2em; font-weight: 800; }
.ow-toggle-chip { border-radius: 999px; padding: 6px 18px; font-weight: 600; }
.ow-toggle-chip:checked { background-color: @accent_bg_color; color: @accent_fg_color; }
.ow-inline { padding: 8px 12px; border-radius: 8px; }
.ow-face { padding: 8px; }
.ow-line { padding: 2px 0; }
"#;

fn colors(dark: bool) -> String {
    TOKENS.iter().map(|(name, light, night)| format!("@define-color {name} {};\n", if dark { night } else { light })).collect()
}

/// Load the rules and the color sheet, and follow the light or dark setting.
pub fn install() {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let rules = gtk::CssProvider::new();
    rules.load_from_string(RULES);
    let palette = gtk::CssProvider::new();
    let manager = adw::StyleManager::default();
    palette.load_from_string(&colors(manager.is_dark()));
    gtk::style_context_add_provider_for_display(&display, &palette, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    gtk::style_context_add_provider_for_display(&display, &rules, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    manager.connect_dark_notify(move |manager| palette.load_from_string(&colors(manager.is_dark())));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance of "#RRGGBB".
    fn luminance(hex: &str) -> f64 {
        let channel = |i: usize| {
            let c = u8::from_str_radix(&hex[i..i + 2], 16).unwrap() as f64 / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
    }

    #[test]
    fn every_tint_reads_at_aa_contrast() {
        let find = |name: &str| TOKENS.iter().find(|t| t.0 == name).unwrap();
        for tone in ["candidate", "incomplete", "neutral", "error", "demo"] {
            let (fg, bg) = (find(&format!("ow_{tone}_fg")), find(&format!("ow_{tone}_bg")));
            for (f, b) in [(fg.1, bg.1), (fg.2, bg.2)] {
                let (l1, l2) = (luminance(f).max(luminance(b)), luminance(f).min(luminance(b)));
                assert!((l1 + 0.05) / (l2 + 0.05) >= 4.5, "{tone}: {f} on {b}");
            }
        }
    }
}
