//! Color theme definitions (Yukidama UI Dark Mode)
//!
//! Based on yukidama-ui design system for visual consistency.
//! Usage: `rgb(theme::BG_BASE)` or `rgba(theme::OVERLAY)`

// Monospace font for terminal and code display
pub const MONOSPACE_FONT: &str = "Consolas";

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
pub const TEXT_MUTED: u32 = 0x6c7680; // neutral[500] - tertiary/muted

// Accent colors (using yukidama-ui palette, 400 level for dark mode visibility)
pub const BLUE: u32 = 0x74a9e4; // primary[400] - main theme color
pub const GREEN: u32 = 0x63bb78; // success[400]
pub const RED: u32 = 0xee8073; // error[400]
pub const YELLOW: u32 = 0xd8953e; // warning[400]
pub const MAUVE: u32 = 0xb494d9; // secondary[400] - purple
pub const TEAL: u32 = 0x52b8b2; // accent[400] - cyan
pub const PEACH: u32 = 0xd69e5b; // warning[300] - lighter amber
pub const PINK: u32 = 0xb79dd7; // secondary[300] - lighter purple
pub const ROSEWATER: u32 = 0xa4acb4; // neutral[300] - soft highlight
pub const MAROON: u32 = 0xeb8e82; // error[300] - soft red
pub const SAPPHIRE: u32 = 0x83afe0; // primary[300] - light blue
pub const SKY: u32 = 0x6bbbb6; // accent[300] - light teal

// Diff colors (based on success/error 950 tints)
pub const DIFF_ADDED_BG: u32 = 0x000d03; // success[950]
pub const DIFF_REMOVED_BG: u32 = 0x1b0000; // error[950]

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
