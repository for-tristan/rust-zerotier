//! Embedded FontAwesome 6 "Solid" font and icon codepoints.
//!
//! The TTF is embedded with [`include_bytes!`] so the binary stays
//! self-contained. [`install`] registers it as a fallback in egui's default
//! font families, after which any icon codepoint below renders as the matching
//! FontAwesome glyph. No emoji are used — only these font icons.

use eframe::egui;

/// The FontAwesome 6 "Solid" TTF, embedded at compile time.
pub const FONT_BYTES: &[u8] = include_bytes!("../assets/fa-solid-900.ttf");

/// Register FontAwesome as a fallback in the Proportional and Monospace font
/// families, so icon codepoints render anywhere.
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "fa-solid".to_owned(),
        egui::FontData::from_static(FONT_BYTES),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("fa-solid".to_owned());
    }
    ctx.set_fonts(fonts);
}

// --- FontAwesome 6 Solid icon codepoints (BMP Private Use Area) -------------

pub const POWER_OFF: &str = "\u{f011}";
pub const COG: &str = "\u{f013}";
pub const PLAY: &str = "\u{f04b}";
pub const STOP: &str = "\u{f04d}";
pub const ARROW_RIGHT: &str = "\u{f061}";
pub const LIST: &str = "\u{f03a}";
pub const TABLE: &str = "\u{f0ce}";
pub const TRASH: &str = "\u{f1f8}";
pub const SERVER: &str = "\u{f233}";
pub const NETWORK_WIRED: &str = "\u{f6ff}";
pub const BROADCAST_TOWER: &str = "\u{f519}";
pub const CHART_BAR: &str = "\u{f080}";
pub const CIRCLE: &str = "\u{f111}";
pub const PLUG: &str = "\u{f1e6}";
pub const CIRCLE_INFO: &str = "\u{f05a}";
pub const CIRCLE_CHECK: &str = "\u{f058}";
pub const CIRCLE_XMARK: &str = "\u{f057}";
pub const CIRCLE_DOT: &str = "\u{f192}";
pub const GAUGE: &str = "\u{f624}";
pub const TERMINAL: &str = "\u{f120}";
pub const ROUTE: &str = "\u{f4d7}";
pub const ETHERNET: &str = "\u{f796}";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_bytes_are_valid_truetype() {
        // TrueType files start with the 4-byte magic 0x00010000.
        assert!(FONT_BYTES.len() > 100_000, "embedded font unexpectedly small");
        assert_eq!(&FONT_BYTES[0..4], &[0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn icons_are_single_private_use_codepoints() {
        let all = [
            POWER_OFF, COG, PLAY, STOP, ARROW_RIGHT, LIST, TABLE, TRASH, SERVER,
            NETWORK_WIRED, BROADCAST_TOWER, CHART_BAR, CIRCLE, PLUG, CIRCLE_INFO,
            CIRCLE_CHECK, CIRCLE_XMARK, CIRCLE_DOT, GAUGE, TERMINAL, ROUTE, ETHERNET,
        ];
        for icon in all {
            assert_eq!(icon.chars().count(), 1, "{icon} is not a single codepoint");
            let c = icon.chars().next().unwrap() as u32;
            assert!(
                (0xE000..=0xF8FF).contains(&c),
                "icon U+{c:04X} is outside the BMP Private Use Area"
            );
        }
    }
}
