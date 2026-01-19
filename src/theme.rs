//! Color theme definitions (Catppuccin Mocha)
//!
//! Usage: `rgb(theme::BG_BASE)` or `rgba(theme::OVERLAY)`

// Background colors
pub const BG_BASE: u32 = 0x1e1e2e;
pub const BG_MANTLE: u32 = 0x181825;
pub const BG_SURFACE0: u32 = 0x313244;
pub const BG_SURFACE1: u32 = 0x45475a;
pub const BG_SURFACE2: u32 = 0x585b70;
pub const OVERLAY: u32 = 0x00000080;

// Text colors
pub const TEXT: u32 = 0xcdd6f4;
pub const TEXT_SECONDARY: u32 = 0xa6adc8;
pub const TEXT_MUTED: u32 = 0x6c7086;

// Accent colors
pub const BLUE: u32 = 0x89b4fa;
pub const GREEN: u32 = 0xa6e3a1;
pub const RED: u32 = 0xf38ba8;
pub const YELLOW: u32 = 0xf9e2af;
pub const MAUVE: u32 = 0xcba6f7;
pub const TEAL: u32 = 0x94e2d5;
pub const PEACH: u32 = 0xfab387;
pub const PINK: u32 = 0xf5c2e7;
pub const ROSEWATER: u32 = 0xf5e0dc;
pub const MAROON: u32 = 0xeba0ac;

// Diff colors
pub const DIFF_ADDED_BG: u32 = 0x1a3d1a;
pub const DIFF_REMOVED_BG: u32 = 0x3d1a1a;

// Terminal ANSI colors
pub mod ansi {
    pub const BLACK: u32 = 0x45475a;
    pub const RED: u32 = 0xf38ba8;
    pub const GREEN: u32 = 0xa6e3a1;
    pub const YELLOW: u32 = 0xf9e2af;
    pub const BLUE: u32 = 0x89b4fa;
    pub const MAGENTA: u32 = 0xf5c2e7;
    pub const CYAN: u32 = 0x94e2d5;
    pub const WHITE: u32 = 0xbac2de;
    pub const BRIGHT_BLACK: u32 = 0x585b70;
    pub const BRIGHT_WHITE: u32 = 0xcdd6f4;
    pub const FOREGROUND: u32 = 0xcdd6f4;
    pub const BACKGROUND: u32 = 0x1e1e2e;
    pub const CURSOR: u32 = 0xf5e0dc;
}
