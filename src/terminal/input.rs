//! Translation from GPUI input events to libghostty-vt input events.
//!
//! Escape sequences are produced by libghostty-vt's encoders, so this module
//! only has to normalize key names and modifiers.

use super::vt::KeyInput;
use gpui::{Keystroke, Modifiers};
use libghostty_vt::key;

/// Map a GPUI key name to its libghostty-vt key.
fn key_from_name(name: &str) -> Option<key::Key> {
    use key::Key;

    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && let Some(key) = key_from_char(c)
    {
        return Some(key);
    }

    Some(match name {
        "enter" => Key::Enter,
        "escape" => Key::Escape,
        "backspace" => Key::Backspace,
        "tab" => Key::Tab,
        "space" => Key::Space,
        "up" => Key::ArrowUp,
        "down" => Key::ArrowDown,
        "left" => Key::ArrowLeft,
        "right" => Key::ArrowRight,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "insert" => Key::Insert,
        "delete" => Key::Delete,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        "f13" => Key::F13,
        "f14" => Key::F14,
        "f15" => Key::F15,
        "f16" => Key::F16,
        "f17" => Key::F17,
        "f18" => Key::F18,
        "f19" => Key::F19,
        _ => return None,
    })
}

fn key_from_char(c: char) -> Option<key::Key> {
    use key::Key;

    Some(match c.to_ascii_lowercase() {
        'a' => Key::A,
        'b' => Key::B,
        'c' => Key::C,
        'd' => Key::D,
        'e' => Key::E,
        'f' => Key::F,
        'g' => Key::G,
        'h' => Key::H,
        'i' => Key::I,
        'j' => Key::J,
        'k' => Key::K,
        'l' => Key::L,
        'm' => Key::M,
        'n' => Key::N,
        'o' => Key::O,
        'p' => Key::P,
        'q' => Key::Q,
        'r' => Key::R,
        's' => Key::S,
        't' => Key::T,
        'u' => Key::U,
        'v' => Key::V,
        'w' => Key::W,
        'x' => Key::X,
        'y' => Key::Y,
        'z' => Key::Z,
        '0' => Key::Digit0,
        '1' => Key::Digit1,
        '2' => Key::Digit2,
        '3' => Key::Digit3,
        '4' => Key::Digit4,
        '5' => Key::Digit5,
        '6' => Key::Digit6,
        '7' => Key::Digit7,
        '8' => Key::Digit8,
        '9' => Key::Digit9,
        '`' => Key::Backquote,
        '\\' => Key::Backslash,
        '[' => Key::BracketLeft,
        ']' => Key::BracketRight,
        ',' => Key::Comma,
        '=' => Key::Equal,
        '-' => Key::Minus,
        '.' => Key::Period,
        '\'' => Key::Quote,
        ';' => Key::Semicolon,
        '/' => Key::Slash,
        ' ' => Key::Space,
        _ => return None,
    })
}

/// Codepoint the key produces with no modifiers applied. libghostty-vt uses it
/// for control codes and for the Kitty keyboard protocol.
fn unshifted_codepoint(name: &str) -> Option<char> {
    if name == "space" {
        return Some(' ');
    }
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii() => Some(c.to_ascii_lowercase()),
        _ => None,
    }
}

pub fn mods_from_gpui(modifiers: &Modifiers) -> key::Mods {
    let mut mods = key::Mods::empty();
    if modifiers.control {
        mods |= key::Mods::CTRL;
    }
    if modifiers.alt {
        mods |= key::Mods::ALT;
    }
    if modifiers.shift {
        mods |= key::Mods::SHIFT;
    }
    if modifiers.platform {
        mods |= key::Mods::SUPER;
    }
    mods
}

/// Whether a key is delivered to the terminal as an escape sequence rather
/// than as committed text.
///
/// Printable characters without Control or Alt reach the terminal through the
/// platform text input path (which is also what drives IME), so encoding them
/// here as well would send them twice.
fn needs_encoding(name: &str, modifiers: &Modifiers) -> bool {
    modifiers.control || modifiers.alt || unshifted_codepoint(name).is_none()
}

/// Translate a keystroke, or return `None` when the terminal should not
/// receive it.
pub fn key_input(keystroke: &Keystroke, composing: bool) -> Option<KeyInput> {
    // Command/Super shortcuts belong to the application, not the terminal.
    if keystroke.modifiers.platform {
        return None;
    }
    if !needs_encoding(&keystroke.key, &keystroke.modifiers) {
        return None;
    }

    let key = key_from_name(&keystroke.key)?;
    Some(KeyInput {
        key,
        mods: mods_from_gpui(&keystroke.modifiers),
        text: keystroke.key_char.clone(),
        unshifted: unshifted_codepoint(&keystroke.key),
        composing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keystroke(value: &str) -> Keystroke {
        Keystroke::parse(value).expect("valid keystroke")
    }

    #[test]
    fn encodes_named_keys() {
        let input = key_input(&keystroke("enter"), false).expect("enter is encoded");
        assert_eq!(input.key, key::Key::Enter);
        assert!(input.mods.is_empty());
    }

    #[test]
    fn encodes_modified_keys() {
        let input = key_input(&keystroke("ctrl-a"), false).expect("ctrl-a is encoded");
        assert_eq!(input.key, key::Key::A);
        assert_eq!(input.mods, key::Mods::CTRL);
        assert_eq!(input.unshifted, Some('a'));
    }

    #[test]
    fn leaves_printable_keys_to_the_text_input_path() {
        assert!(key_input(&keystroke("a"), false).is_none());
        assert!(key_input(&keystroke("space"), false).is_none());
        assert!(key_input(&keystroke("shift-1"), false).is_none());
    }

    #[test]
    fn ignores_platform_shortcuts() {
        assert!(key_input(&keystroke("cmd-c"), false).is_none());
    }
}
