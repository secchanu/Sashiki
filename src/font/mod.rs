//! Font discovery module using font-kit
//!
//! Provides system font discovery for Japanese/CJK text support.

use font_kit::family_name::FamilyName;
use font_kit::properties::{Properties, Weight};
use font_kit::source::SystemSource;

/// Preferred Japanese font families in order of preference
const JAPANESE_FONT_FAMILIES: &[&str] = &[
    // Windows
    "MS Gothic",
    "Meiryo",
    "Yu Gothic",
    "BIZ UDGothic",
    // macOS
    "Hiragino Sans",
    "Hiragino Kaku Gothic ProN",
    // Linux / Cross-platform
    "Noto Sans CJK JP",
    "Noto Sans JP",
    "IPAGothic",
    "VL Gothic",
    "Takao Gothic",
];

/// Preferred monospace font families
const MONOSPACE_FONT_FAMILIES: &[&str] = &[
    // Cross-platform
    "JetBrains Mono",
    "Fira Code",
    "Source Code Pro",
    // Windows
    "Consolas",
    "Cascadia Code",
    // macOS
    "SF Mono",
    "Menlo",
    "Monaco",
    // Linux
    "DejaVu Sans Mono",
    "Ubuntu Mono",
    "Liberation Mono",
];

/// Load font data by family name from system fonts
pub fn load_font_by_name(family_name: &str) -> Option<Vec<u8>> {
    let source = SystemSource::new();

    let handle = source
        .select_best_match(
            &[FamilyName::Title(family_name.to_string())],
            &Properties::new().weight(Weight::NORMAL),
        )
        .ok()?;

    let font = handle.load().ok()?;
    Some(font.copy_font_data()?.to_vec())
}

/// Find and load the first available Japanese font from system
pub fn find_japanese_font() -> Option<(String, Vec<u8>)> {
    for family in JAPANESE_FONT_FAMILIES {
        if let Some(data) = load_font_by_name(family) {
            tracing::info!("Found Japanese font: {}", family);
            return Some((family.to_string(), data));
        }
    }

    // Fallback: try to find any CJK font
    let source = SystemSource::new();
    if let Ok(families) = source.all_families() {
        for family in families {
            let lower = family.to_lowercase();
            if lower.contains("cjk")
                || lower.contains("japanese")
                || lower.contains("gothic")
                || lower.contains("mincho")
            {
                if let Some(data) = load_font_by_name(&family) {
                    tracing::info!("Found CJK font via search: {}", family);
                    return Some((family, data));
                }
            }
        }
    }

    tracing::warn!("No Japanese font found on system");
    None
}

/// Find and load a monospace font from system
pub fn find_monospace_font() -> Option<(String, Vec<u8>)> {
    for family in MONOSPACE_FONT_FAMILIES {
        if let Some(data) = load_font_by_name(family) {
            tracing::info!("Found monospace font: {}", family);
            return Some((family.to_string(), data));
        }
    }

    tracing::warn!("No preferred monospace font found, using system default");
    None
}

/// List all available font families on the system (for debugging)
#[allow(dead_code)]
pub fn list_all_fonts() -> Vec<String> {
    let source = SystemSource::new();
    source.all_families().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_japanese_font() {
        // This test may pass or fail depending on system fonts
        let result = find_japanese_font();
        if let Some((name, data)) = result {
            assert!(!name.is_empty());
            assert!(!data.is_empty());
        }
    }

    #[test]
    fn test_find_monospace_font() {
        let result = find_monospace_font();
        if let Some((name, data)) = result {
            assert!(!name.is_empty());
            assert!(!data.is_empty());
        }
    }

    #[test]
    fn test_list_fonts() {
        let fonts = list_all_fonts();
        // Should have at least some fonts on any system
        println!("Found {} font families", fonts.len());
    }
}
