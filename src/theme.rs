//! Color theme definitions (Yukidama UI Dark Mode)
//!
//! Based on yukidama-ui design system for visual consistency.
//! Usage: `rgb(theme::BG_BASE)` or `rgba(theme::OVERLAY)`

// Monospace font for terminal and code display
pub const MONOSPACE_FONT: &str = "Consolas";

// Background colors (RGB format: 0xRRGGBB, use with rgb())
// From yukidama-ui neutral palette (dark mode)
pub const BG_BASE: u32 = 0x020617; // neutral[950] - darkest base
pub const BG_MANTLE: u32 = 0x0f172a; // neutral[900] - surface
pub const BG_SURFACE0: u32 = 0x1e293b; // neutral[800] - elevated
pub const BG_SURFACE1: u32 = 0x334155; // neutral[700] - subtle
pub const BG_SURFACE2: u32 = 0x475569; // neutral[600] - strong

// Overlay color (RGBA format: 0xRRGGBBAA, use with rgba())
pub const OVERLAY: u32 = 0x000000B3; // rgba(0, 0, 0, 0.7)

// Text colors (from yukidama-ui semantic dark mode)
pub const TEXT: u32 = 0xf7f9fb; // neutral[50] - primary text
pub const TEXT_SECONDARY: u32 = 0x94a3b8; // neutral[400] - secondary text
pub const TEXT_MUTED: u32 = 0x64748b; // neutral[500] - tertiary/muted

// Accent colors (using yukidama-ui palette, 400 level for dark mode visibility)
pub const BLUE: u32 = 0x5a92d6; // primary[400] - main theme color
pub const GREEN: u32 = 0x34d399; // success[400]
pub const RED: u32 = 0xf87171; // error[400]
pub const YELLOW: u32 = 0xfbbf24; // warning[400]
pub const MAUVE: u32 = 0x9c70d1; // secondary[400] - purple
pub const TEAL: u32 = 0x2ecece; // accent[400] - cyan
pub const PEACH: u32 = 0xfcd34d; // warning[300] - lighter amber
pub const PINK: u32 = 0xb493de; // secondary[300] - lighter purple
pub const ROSEWATER: u32 = 0xc4ced9; // neutral[300] - soft highlight
pub const MAROON: u32 = 0xfca5a5; // error[300] - soft red

// Diff colors (based on success/error 950 tints)
pub const DIFF_ADDED_BG: u32 = 0x052e16; // success[950]
pub const DIFF_REMOVED_BG: u32 = 0x450a0a; // error[950]

// Terminal ANSI colors (aligned with yukidama-ui palette)
// Normal colors use [400] level, bright colors use [300] level for dark mode
pub mod ansi {
    pub const BLACK: u32 = 0x334155; // neutral[700]
    pub const RED: u32 = 0xf87171; // error[400]
    pub const GREEN: u32 = 0x34d399; // success[400]
    pub const YELLOW: u32 = 0xfbbf24; // warning[400]
    pub const BLUE: u32 = 0x5a92d6; // primary[400]
    pub const MAGENTA: u32 = 0x9c70d1; // secondary[400]
    pub const CYAN: u32 = 0x2ecece; // accent[400]
    pub const WHITE: u32 = 0xc4ced9; // neutral[300]
    pub const BRIGHT_BLACK: u32 = 0x475569; // neutral[600]
    pub const BRIGHT_RED: u32 = 0xfca5a5; // error[300]
    pub const BRIGHT_GREEN: u32 = 0x6ee7b7; // success[300]
    pub const BRIGHT_YELLOW: u32 = 0xfde68a; // warning[300]
    pub const BRIGHT_BLUE: u32 = 0x93c5fd; // primary[300]
    pub const BRIGHT_MAGENTA: u32 = 0xb493de; // secondary[300]
    pub const BRIGHT_CYAN: u32 = 0x67e8f9; // accent[300]
    pub const BRIGHT_WHITE: u32 = 0xf7f9fb; // neutral[50]
    pub const FOREGROUND: u32 = 0xf7f9fb; // neutral[50]
    pub const BACKGROUND: u32 = 0x020617; // neutral[950]
    pub const CURSOR: u32 = 0x5a92d6; // primary[400] - theme color
}
