//! Keyboard handling for terminal input

use iced::keyboard::{self, Key, Modifiers};

/// Convert iced key events to terminal bytes
pub fn key_to_terminal_bytes(key: &Key, modifiers: &Modifiers) -> Vec<u8> {
    // Handle Ctrl+key combinations
    if modifiers.control() {
        if let Key::Character(c) = key {
            let ch = c.as_str().chars().next().unwrap_or('\0');
            if ch.is_ascii_lowercase() {
                return vec![(ch as u8) - b'a' + 1];
            }
        }
    }

    match key {
        Key::Character(c) => c.as_str().as_bytes().to_vec(),
        Key::Named(named) => match named {
            keyboard::key::Named::Enter => b"\r".to_vec(),
            keyboard::key::Named::Tab => b"\t".to_vec(),
            keyboard::key::Named::Backspace => b"\x7f".to_vec(),
            keyboard::key::Named::ArrowUp => b"\x1b[A".to_vec(),
            keyboard::key::Named::ArrowDown => b"\x1b[B".to_vec(),
            keyboard::key::Named::ArrowRight => b"\x1b[C".to_vec(),
            keyboard::key::Named::ArrowLeft => b"\x1b[D".to_vec(),
            keyboard::key::Named::Home => b"\x1b[H".to_vec(),
            keyboard::key::Named::End => b"\x1b[F".to_vec(),
            keyboard::key::Named::PageUp => b"\x1b[5~".to_vec(),
            keyboard::key::Named::PageDown => b"\x1b[6~".to_vec(),
            keyboard::key::Named::Delete => b"\x1b[3~".to_vec(),
            keyboard::key::Named::Insert => b"\x1b[2~".to_vec(),
            keyboard::key::Named::Space => b" ".to_vec(),
            _ => vec![],
        },
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arrow_keys() {
        let mods = Modifiers::empty();
        assert_eq!(
            key_to_terminal_bytes(&Key::Named(keyboard::key::Named::ArrowUp), &mods),
            b"\x1b[A"
        );
        assert_eq!(
            key_to_terminal_bytes(&Key::Named(keyboard::key::Named::ArrowDown), &mods),
            b"\x1b[B"
        );
    }

    #[test]
    fn test_enter_key() {
        let mods = Modifiers::empty();
        assert_eq!(
            key_to_terminal_bytes(&Key::Named(keyboard::key::Named::Enter), &mods),
            b"\r"
        );
    }

    #[test]
    fn test_ctrl_c() {
        let mods = Modifiers::CTRL;
        let result = key_to_terminal_bytes(&Key::Character("c".into()), &mods);
        assert_eq!(result, vec![3]); // Ctrl+C = 0x03
    }

    #[test]
    fn test_regular_char() {
        let mods = Modifiers::empty();
        assert_eq!(
            key_to_terminal_bytes(&Key::Character("a".into()), &mods),
            b"a"
        );
    }
}
