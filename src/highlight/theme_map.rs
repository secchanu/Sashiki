use gpui::{FontStyle, FontWeight, HighlightStyle};

use crate::theme;

fn style(color: u32) -> HighlightStyle {
    HighlightStyle {
        color: Some(gpui::rgb(color).into()),
        ..Default::default()
    }
}

fn style_bold(color: u32) -> HighlightStyle {
    HighlightStyle {
        color: Some(gpui::rgb(color).into()),
        font_weight: Some(FontWeight::BOLD),
        ..Default::default()
    }
}

fn style_italic(color: u32) -> HighlightStyle {
    HighlightStyle {
        color: Some(gpui::rgb(color).into()),
        font_style: Some(FontStyle::Italic),
        ..Default::default()
    }
}

pub fn highlight_style_for_capture(index: usize, capture_names: &[String]) -> HighlightStyle {
    match capture_names.get(index).map(String::as_str) {
        Some("comment") => style_italic(theme::TEXT_MUTED),
        Some("comment.documentation") => style_italic(theme::TEXT_SECONDARY),

        Some("keyword") => style_bold(theme::BLUE),

        Some("string") => style(theme::GREEN),
        Some("string.special") => style(theme::TEAL),
        Some("string.special.key") => style(theme::SAPPHIRE),

        Some("number") => style(theme::PEACH),
        Some("boolean") => style_bold(theme::BLUE),

        Some("escape") => style(theme::PEACH),

        Some("type") => style(theme::TEAL),
        Some("type.builtin") => style_bold(theme::TEAL),

        Some("function") => style(theme::YELLOW),
        Some("function.builtin") => style_bold(theme::YELLOW),
        Some("function.method") => style(theme::YELLOW),
        Some("function.macro") => style_bold(theme::TEAL),

        Some("variable") => style(theme::SAPPHIRE),
        Some("variable.builtin") => style_italic(theme::RED),
        Some("variable.parameter") => style(theme::MAROON),

        Some("operator") => style(theme::PINK),

        Some("punctuation") | Some("punctuation.bracket") | Some("punctuation.delimiter") => {
            style(theme::TEXT_SECONDARY)
        }
        Some("punctuation.special") => style(theme::TEAL),

        Some("property") => style(theme::SKY),

        Some("constant") => style_bold(theme::PEACH),
        Some("constant.builtin") => style_bold(theme::PEACH),

        Some("constructor") => style_bold(theme::TEAL),

        Some("attribute") => style_italic(theme::YELLOW),

        Some("tag") => style(theme::RED),
        Some("tag.error") => style(theme::RED),

        Some("label") => style_italic(theme::SAPPHIRE),

        Some("namespace") => style(theme::PINK),

        Some("embedded") => style(theme::TEXT),

        _ => style(theme::TEXT),
    }
}
