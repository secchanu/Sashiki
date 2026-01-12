//! Theme module for Sashiki
//!
//! Defines colors, styling, and button styles for the application.

use iced::widget::button;
use iced::{Color, Theme};

/// Application color palette
#[derive(Debug, Clone)]
pub struct Palette {
    pub bg_primary: Color,
    pub bg_secondary: Color,
    pub bg_tertiary: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub accent: Color,
    pub border: Color,
    pub diff_add_bg: Color,
    pub diff_add_fg: Color,
    pub diff_delete_bg: Color,
    pub diff_delete_fg: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self::dark()
    }
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            bg_primary: Color::from_rgb8(0x1e, 0x1e, 0x2e),
            bg_secondary: Color::from_rgb8(0x28, 0x28, 0x3c),
            bg_tertiary: Color::from_rgb8(0x32, 0x32, 0x4a),
            text_primary: Color::from_rgb8(0xcd, 0xd6, 0xf4),
            text_secondary: Color::from_rgb8(0xa6, 0xad, 0xc8),
            text_muted: Color::from_rgb8(0x6c, 0x70, 0x86),
            accent: Color::from_rgb8(0x89, 0xb4, 0xfa),
            border: Color::from_rgb8(0x45, 0x47, 0x5a),
            diff_add_bg: Color::from_rgba8(0xa6, 0xe3, 0xa1, 0.2),
            diff_add_fg: Color::from_rgb8(0xa6, 0xe3, 0xa1),
            diff_delete_bg: Color::from_rgba8(0xf3, 0x8b, 0xa8, 0.2),
            diff_delete_fg: Color::from_rgb8(0xf3, 0x8b, 0xa8),
        }
    }

    pub fn light() -> Self {
        Self {
            bg_primary: Color::from_rgb8(0xef, 0xf1, 0xf5),
            bg_secondary: Color::from_rgb8(0xe6, 0xe9, 0xef),
            bg_tertiary: Color::from_rgb8(0xdc, 0xe0, 0xe8),
            text_primary: Color::from_rgb8(0x4c, 0x4f, 0x69),
            text_secondary: Color::from_rgb8(0x6c, 0x6f, 0x85),
            text_muted: Color::from_rgb8(0x9c, 0xa0, 0xb0),
            accent: Color::from_rgb8(0x1e, 0x66, 0xf5),
            border: Color::from_rgb8(0xcc, 0xd0, 0xda),
            diff_add_bg: Color::from_rgba8(0x40, 0xa0, 0x2b, 0.15),
            diff_add_fg: Color::from_rgb8(0x40, 0xa0, 0x2b),
            diff_delete_bg: Color::from_rgba8(0xd2, 0x00, 0x36, 0.15),
            diff_delete_fg: Color::from_rgb8(0xd2, 0x00, 0x36),
        }
    }
}

/// Standard button style with good contrast
pub fn button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Active => Color::from_rgb(0.25, 0.25, 0.27),
        button::Status::Hovered => Color::from_rgb(0.35, 0.35, 0.38),
        button::Status::Pressed => Color::from_rgb(0.20, 0.20, 0.22),
        button::Status::Disabled => Color::from_rgb(0.15, 0.15, 0.15),
    };
    button::Style {
        background: Some(bg.into()),
        text_color: Color::from_rgb(0.9, 0.9, 0.9),
        border: iced::Border {
            color: Color::from_rgb(0.4, 0.4, 0.42),
            width: 1.0,
            radius: 3.0.into(),
        },
        ..Default::default()
    }
}

/// Accent button style for primary actions
pub fn accent_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Active => Color::from_rgb(0.0, 0.47, 0.8),
        button::Status::Hovered => Color::from_rgb(0.0, 0.55, 0.9),
        button::Status::Pressed => Color::from_rgb(0.0, 0.40, 0.7),
        button::Status::Disabled => Color::from_rgb(0.2, 0.3, 0.4),
    };
    button::Style {
        background: Some(bg.into()),
        text_color: Color::WHITE,
        border: iced::Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 3.0.into(),
        },
        ..Default::default()
    }
}
