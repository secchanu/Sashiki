//! Color theme definitions (Yukidama UI Dark Mode)
//!
//! Based on yukidama-ui design system for visual consistency.
//! Usage: `rgb(theme::BG_BASE)` or `rgba(theme::OVERLAY)`

// Monospace font for terminal and code display
#[cfg(target_os = "macos")]
pub const MONOSPACE_FONT: &str = "Menlo";
#[cfg(target_os = "windows")]
pub const MONOSPACE_FONT: &str = "Consolas";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const MONOSPACE_FONT: &str = "DejaVu Sans Mono";

// Background colors (RGB format: 0xRRGGBB, use with rgb())
// From yukidama-ui neutral palette (dark mode)
pub const BG_BASE: u32 = 0x07090c; // neutral[950] - darkest base
pub const BG_MANTLE: u32 = 0x13161a; // neutral[900] - surface
pub const BG_SURFACE0: u32 = 0x22272c; // neutral[800] - elevated
pub const BG_SURFACE1: u32 = 0x32393f; // neutral[700] - subtle
pub const BG_SURFACE2: u32 = 0x414951; // neutral[600] - strong

// Overlay color (RGBA format: 0xRRGGBBAA, use with rgba())
pub const OVERLAY: u32 = 0x000000B3; // rgba(0, 0, 0, 0.7)

// Text colors (from yukidama-ui semantic dark mode)
pub const TEXT: u32 = 0xeef2f7; // neutral[50] - primary text
pub const TEXT_SECONDARY: u32 = 0x9da6af; // neutral[400] - secondary text
pub const TEXT_MUTED: u32 = 0x8590a0; // neutral[450] - tertiary/muted (WCAG AA: 4.66:1 on SURFACE0)

// Accent colors (using yukidama-ui palette, 400 level for dark mode visibility)
pub const BLUE: u32 = 0x74a9e4; // primary[400] - main theme color
pub const GREEN: u32 = 0x63bb78; // success[400]
pub const RED: u32 = 0xee8073; // error[400]
pub const YELLOW: u32 = 0xd8953e; // warning[400]
pub const MAUVE: u32 = 0xb494d9; // secondary[400] - purple
pub const TEAL: u32 = 0x52b8b2; // accent[400] - cyan
pub const PEACH: u32 = 0xd69e5b; // warning[300] - lighter amber
pub const PINK: u32 = 0xb79dd7; // secondary[300] - lighter purple
pub const MAROON: u32 = 0xeb8e82; // error[300] - soft red
pub const SAPPHIRE: u32 = 0x83afe0; // primary[300] - light blue
pub const SKY: u32 = 0x6bbbb6; // accent[300] - light teal

// Diff colors (行レベル背景: GitHub dark default 準拠)
pub const DIFF_ADDED_BG: u32 = 0x122d1f; // 緑背景 (GitHub: #122117 よりやや明るく)
pub const DIFF_REMOVED_BG: u32 = 0x3b1620; // 赤背景 (GitHub: #2d1117 よりやや明るく)
// 文字レベル diff ハイライト背景 (行背景比3:1以上のコントラスト比を確保)
pub const DIFF_ADDED_WORD_BG: u32 = 0x1e7b37ff; // RGBA: 文字レベル追加強調 (DIFF_ADDED_BGと3:1コントラスト)
pub const DIFF_REMOVED_WORD_BG: u32 = 0xbf2626ff; // RGBA: 文字レベル削除強調 (DIFF_REMOVED_BGと3:1コントラスト)
// Hunk ヘッダー行
pub const DIFF_HUNK_HEADER_BG: u32 = 0x0d1f38; // 暗い青背景 (primary[900]相当)
pub const DIFF_HUNK_HEADER_FG: u32 = 0x83afe0; // 明るい青テキスト (SAPPHIRE と同値)
// 行番号ガター背景 (VS Code/GitHub準拠: 行背景より暗いtint)
pub const DIFF_ADDED_GUTTER_BG: u32 = 0x0e2318; // DIFF_ADDED_BG より暗い緑
pub const DIFF_REMOVED_GUTTER_BG: u32 = 0x30111a; // DIFF_REMOVED_BG より暗い赤
// フィラー行 (split view: 対面パネルの追加/削除に対応する空行)
pub const DIFF_FILLER_BG: u32 = 0x171b20; // BG_MANTLE より微かに明るいグレー (VS Code準拠)
// 省略行
pub const DIFF_COLLAPSE_BG: u32 = 0x1a1e24; // BG_SURFACE0 よりやや暗め

// Terminal ANSI colors (aligned with yukidama-ui palette)
// Normal colors use [400] level, bright colors use [300] level for dark mode
pub mod ansi {
    pub const BLACK: u32 = 0x32393f; // neutral[700]
    pub const RED: u32 = 0xee8073; // error[400]
    pub const GREEN: u32 = 0x63bb78; // success[400]
    pub const YELLOW: u32 = 0xd8953e; // warning[400]
    pub const BLUE: u32 = 0x74a9e4; // primary[400]
    pub const MAGENTA: u32 = 0xb494d9; // secondary[400]
    pub const CYAN: u32 = 0x52b8b2; // accent[400]
    pub const WHITE: u32 = 0xa4acb4; // neutral[300]
    pub const BRIGHT_BLACK: u32 = 0x414951; // neutral[600]
    pub const BRIGHT_RED: u32 = 0xeb8e82; // error[300]
    pub const BRIGHT_GREEN: u32 = 0x77be86; // success[300]
    pub const BRIGHT_YELLOW: u32 = 0xd69e5b; // warning[300]
    pub const BRIGHT_BLUE: u32 = 0x83afe0; // primary[300]
    pub const BRIGHT_MAGENTA: u32 = 0xb79dd7; // secondary[300]
    pub const BRIGHT_CYAN: u32 = 0x6bbbb6; // accent[300]
    pub const BRIGHT_WHITE: u32 = 0xeef2f7; // neutral[50]
    pub const FOREGROUND: u32 = 0xeef2f7; // neutral[50]
    pub const BACKGROUND: u32 = 0x07090c; // neutral[950]
    pub const CURSOR: u32 = 0x74a9e4; // primary[400] - theme color
}
