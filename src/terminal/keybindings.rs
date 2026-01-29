//! Terminal key bindings and action handlers
//!
//! This module defines all keyboard actions for the terminal and their handlers.

use super::TerminalView;
use gpui::{App, ClipboardItem, Context, KeyBinding, Window, actions};

// Define actions for special keys
actions!(
    terminal,
    [
        Enter,
        Backspace,
        Tab,
        Escape,
        Up,
        Down,
        Left,
        Right,
        Home,
        End,
        Delete,
        PageUp,
        PageDown,
        Insert,
        // Function keys
        F1,
        F2,
        F3,
        F4,
        F5,
        F6,
        F7,
        F8,
        F9,
        F10,
        F11,
        F12,
        // Control keys (Ctrl+letter)
        CtrlA,
        CtrlB,
        CtrlC,
        CtrlD,
        CtrlE,
        CtrlF,
        CtrlG,
        CtrlH,
        CtrlI,
        CtrlJ,
        CtrlK,
        CtrlL,
        CtrlM,
        CtrlN,
        CtrlO,
        CtrlP,
        CtrlQ,
        CtrlR,
        CtrlS,
        CtrlT,
        CtrlU,
        CtrlV,
        CtrlW,
        CtrlX,
        CtrlY,
        CtrlZ,
        // Control keys (Ctrl+symbol)
        CtrlBackslash,
        CtrlBracketRight,
        CtrlCaret,
        CtrlUnderscore,
        // Alt keys
        AltB,
        AltD,
        AltF,
        AltBackspace,
        // Modified arrow keys
        AltUp,
        AltDown,
        AltLeft,
        AltRight,
        ShiftUp,
        ShiftDown,
        ShiftLeft,
        ShiftRight,
        ShiftHome,
        ShiftEnd,
        ShiftInsert,
        ShiftPageUp,
        ShiftPageDown,
        CtrlUp,
        CtrlDown,
        CtrlLeft,
        CtrlRight,
        CtrlShiftUp,
        CtrlShiftDown,
        CtrlShiftLeft,
        CtrlShiftRight,
        CtrlShiftC,
        CtrlShiftV,
        CtrlAltUp,
        CtrlAltDown,
        CtrlAltLeft,
        CtrlAltRight,
    ]
);

impl TerminalView {
    /// Bind terminal key actions to the application
    pub fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            // Basic keys
            KeyBinding::new("enter", Enter, Some("Terminal")),
            KeyBinding::new("backspace", Backspace, Some("Terminal")),
            KeyBinding::new("tab", Tab, Some("Terminal")),
            KeyBinding::new("escape", Escape, Some("Terminal")),
            KeyBinding::new("up", Up, Some("Terminal")),
            KeyBinding::new("down", Down, Some("Terminal")),
            KeyBinding::new("left", Left, Some("Terminal")),
            KeyBinding::new("right", Right, Some("Terminal")),
            KeyBinding::new("home", Home, Some("Terminal")),
            KeyBinding::new("end", End, Some("Terminal")),
            KeyBinding::new("delete", Delete, Some("Terminal")),
            KeyBinding::new("pageup", PageUp, Some("Terminal")),
            KeyBinding::new("pagedown", PageDown, Some("Terminal")),
            KeyBinding::new("insert", Insert, Some("Terminal")),
            // Function keys
            KeyBinding::new("f1", F1, Some("Terminal")),
            KeyBinding::new("f2", F2, Some("Terminal")),
            KeyBinding::new("f3", F3, Some("Terminal")),
            KeyBinding::new("f4", F4, Some("Terminal")),
            KeyBinding::new("f5", F5, Some("Terminal")),
            KeyBinding::new("f6", F6, Some("Terminal")),
            KeyBinding::new("f7", F7, Some("Terminal")),
            KeyBinding::new("f8", F8, Some("Terminal")),
            KeyBinding::new("f9", F9, Some("Terminal")),
            KeyBinding::new("f10", F10, Some("Terminal")),
            KeyBinding::new("f11", F11, Some("Terminal")),
            KeyBinding::new("f12", F12, Some("Terminal")),
            // Control keys (Ctrl+letter)
            KeyBinding::new("ctrl-a", CtrlA, Some("Terminal")),
            KeyBinding::new("ctrl-b", CtrlB, Some("Terminal")),
            KeyBinding::new("ctrl-c", CtrlC, Some("Terminal")),
            KeyBinding::new("ctrl-d", CtrlD, Some("Terminal")),
            KeyBinding::new("ctrl-e", CtrlE, Some("Terminal")),
            KeyBinding::new("ctrl-f", CtrlF, Some("Terminal")),
            KeyBinding::new("ctrl-g", CtrlG, Some("Terminal")),
            KeyBinding::new("ctrl-h", CtrlH, Some("Terminal")),
            KeyBinding::new("ctrl-i", CtrlI, Some("Terminal")),
            KeyBinding::new("ctrl-j", CtrlJ, Some("Terminal")),
            KeyBinding::new("ctrl-k", CtrlK, Some("Terminal")),
            KeyBinding::new("ctrl-l", CtrlL, Some("Terminal")),
            KeyBinding::new("ctrl-m", CtrlM, Some("Terminal")),
            KeyBinding::new("ctrl-n", CtrlN, Some("Terminal")),
            KeyBinding::new("ctrl-o", CtrlO, Some("Terminal")),
            KeyBinding::new("ctrl-p", CtrlP, Some("Terminal")),
            KeyBinding::new("ctrl-q", CtrlQ, Some("Terminal")),
            KeyBinding::new("ctrl-r", CtrlR, Some("Terminal")),
            KeyBinding::new("ctrl-s", CtrlS, Some("Terminal")),
            KeyBinding::new("ctrl-t", CtrlT, Some("Terminal")),
            KeyBinding::new("ctrl-u", CtrlU, Some("Terminal")),
            KeyBinding::new("ctrl-v", CtrlV, Some("Terminal")),
            KeyBinding::new("ctrl-w", CtrlW, Some("Terminal")),
            KeyBinding::new("ctrl-x", CtrlX, Some("Terminal")),
            KeyBinding::new("ctrl-y", CtrlY, Some("Terminal")),
            KeyBinding::new("ctrl-z", CtrlZ, Some("Terminal")),
            // Control keys (Ctrl+symbol)
            KeyBinding::new("ctrl-\\", CtrlBackslash, Some("Terminal")),
            KeyBinding::new("ctrl-]", CtrlBracketRight, Some("Terminal")),
            KeyBinding::new("ctrl-^", CtrlCaret, Some("Terminal")),
            KeyBinding::new("ctrl-_", CtrlUnderscore, Some("Terminal")),
            // Alt keys
            KeyBinding::new("alt-b", AltB, Some("Terminal")),
            KeyBinding::new("alt-d", AltD, Some("Terminal")),
            KeyBinding::new("alt-f", AltF, Some("Terminal")),
            KeyBinding::new("alt-backspace", AltBackspace, Some("Terminal")),
            // Alt+arrow keys
            KeyBinding::new("alt-up", AltUp, Some("Terminal")),
            KeyBinding::new("alt-down", AltDown, Some("Terminal")),
            KeyBinding::new("alt-left", AltLeft, Some("Terminal")),
            KeyBinding::new("alt-right", AltRight, Some("Terminal")),
            // Shift+arrow keys
            KeyBinding::new("shift-up", ShiftUp, Some("Terminal")),
            KeyBinding::new("shift-down", ShiftDown, Some("Terminal")),
            KeyBinding::new("shift-left", ShiftLeft, Some("Terminal")),
            KeyBinding::new("shift-right", ShiftRight, Some("Terminal")),
            KeyBinding::new("shift-home", ShiftHome, Some("Terminal")),
            KeyBinding::new("shift-end", ShiftEnd, Some("Terminal")),
            KeyBinding::new("shift-insert", ShiftInsert, Some("Terminal")),
            KeyBinding::new("shift-pageup", ShiftPageUp, Some("Terminal")),
            KeyBinding::new("shift-pagedown", ShiftPageDown, Some("Terminal")),
            // Ctrl+arrow keys
            KeyBinding::new("ctrl-up", CtrlUp, Some("Terminal")),
            KeyBinding::new("ctrl-down", CtrlDown, Some("Terminal")),
            KeyBinding::new("ctrl-left", CtrlLeft, Some("Terminal")),
            KeyBinding::new("ctrl-right", CtrlRight, Some("Terminal")),
            // Ctrl+Shift keys
            KeyBinding::new("ctrl-shift-up", CtrlShiftUp, Some("Terminal")),
            KeyBinding::new("ctrl-shift-down", CtrlShiftDown, Some("Terminal")),
            KeyBinding::new("ctrl-shift-left", CtrlShiftLeft, Some("Terminal")),
            KeyBinding::new("ctrl-shift-right", CtrlShiftRight, Some("Terminal")),
            KeyBinding::new("ctrl-shift-c", CtrlShiftC, Some("Terminal")),
            KeyBinding::new("ctrl-shift-v", CtrlShiftV, Some("Terminal")),
            // Ctrl+Alt+arrow keys
            KeyBinding::new("ctrl-alt-up", CtrlAltUp, Some("Terminal")),
            KeyBinding::new("ctrl-alt-down", CtrlAltDown, Some("Terminal")),
            KeyBinding::new("ctrl-alt-left", CtrlAltLeft, Some("Terminal")),
            KeyBinding::new("ctrl-alt-right", CtrlAltRight, Some("Terminal")),
        ]);
    }

    // ========================================================================
    // Action handlers - send ANSI escape sequences to terminal
    // ========================================================================

    pub(super) fn on_enter(&mut self, _: &Enter, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\r");
    }

    pub(super) fn on_backspace(&mut self, _: &Backspace, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x7f");
    }

    pub(super) fn on_tab(&mut self, _: &Tab, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\t");
    }

    pub(super) fn on_escape(&mut self, _: &Escape, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b");
    }

    pub(super) fn on_up(&mut self, _: &Up, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[A");
    }

    pub(super) fn on_down(&mut self, _: &Down, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[B");
    }

    pub(super) fn on_left(&mut self, _: &Left, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[D");
    }

    pub(super) fn on_right(&mut self, _: &Right, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[C");
    }

    pub(super) fn on_home(&mut self, _: &Home, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[H");
    }

    pub(super) fn on_end(&mut self, _: &End, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[F");
    }

    pub(super) fn on_delete(&mut self, _: &Delete, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[3~");
    }

    pub(super) fn on_page_up(&mut self, _: &PageUp, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[5~");
    }

    pub(super) fn on_page_down(&mut self, _: &PageDown, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[6~");
    }

    pub(super) fn on_insert(&mut self, _: &Insert, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[2~");
    }

    // Function key handlers
    pub(super) fn on_f1(&mut self, _: &F1, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1bOP");
    }

    pub(super) fn on_f2(&mut self, _: &F2, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1bOQ");
    }

    pub(super) fn on_f3(&mut self, _: &F3, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1bOR");
    }

    pub(super) fn on_f4(&mut self, _: &F4, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1bOS");
    }

    pub(super) fn on_f5(&mut self, _: &F5, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[15~");
    }

    pub(super) fn on_f6(&mut self, _: &F6, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[17~");
    }

    pub(super) fn on_f7(&mut self, _: &F7, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[18~");
    }

    pub(super) fn on_f8(&mut self, _: &F8, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[19~");
    }

    pub(super) fn on_f9(&mut self, _: &F9, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[20~");
    }

    pub(super) fn on_f10(&mut self, _: &F10, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[21~");
    }

    pub(super) fn on_f11(&mut self, _: &F11, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[23~");
    }

    pub(super) fn on_f12(&mut self, _: &F12, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[24~");
    }

    // Control key handlers (Ctrl+letter sends ASCII control codes 0x01-0x1A)
    pub(super) fn on_ctrl_a(&mut self, _: &CtrlA, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x01"); // SOH - beginning of line
    }

    pub(super) fn on_ctrl_b(&mut self, _: &CtrlB, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x02"); // STX - move left
    }

    pub(super) fn on_ctrl_c(&mut self, _: &CtrlC, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x03"); // ETX - SIGINT
    }

    pub(super) fn on_ctrl_d(&mut self, _: &CtrlD, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x04"); // EOT - EOF
    }

    pub(super) fn on_ctrl_e(&mut self, _: &CtrlE, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x05"); // ENQ - end of line
    }

    pub(super) fn on_ctrl_f(&mut self, _: &CtrlF, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x06"); // ACK - move right
    }

    pub(super) fn on_ctrl_g(&mut self, _: &CtrlG, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x07"); // BEL - bell/cancel
    }

    pub(super) fn on_ctrl_h(&mut self, _: &CtrlH, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x08"); // BS - backspace
    }

    pub(super) fn on_ctrl_i(&mut self, _: &CtrlI, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x09"); // HT - tab
    }

    pub(super) fn on_ctrl_j(&mut self, _: &CtrlJ, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x0a"); // LF - newline
    }

    pub(super) fn on_ctrl_k(&mut self, _: &CtrlK, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x0b"); // VT - kill to end of line
    }

    pub(super) fn on_ctrl_l(&mut self, _: &CtrlL, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x0c"); // FF - clear screen
    }

    pub(super) fn on_ctrl_m(&mut self, _: &CtrlM, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x0d"); // CR - carriage return
    }

    pub(super) fn on_ctrl_n(&mut self, _: &CtrlN, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x0e"); // SO - next history
    }

    pub(super) fn on_ctrl_o(&mut self, _: &CtrlO, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x0f"); // SI - flush output
    }

    pub(super) fn on_ctrl_p(&mut self, _: &CtrlP, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x10"); // DLE - previous history
    }

    pub(super) fn on_ctrl_q(&mut self, _: &CtrlQ, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x11"); // DC1 - XON (resume)
    }

    pub(super) fn on_ctrl_r(&mut self, _: &CtrlR, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x12"); // DC2 - reverse search
    }

    pub(super) fn on_ctrl_s(&mut self, _: &CtrlS, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x13"); // DC3 - XOFF (stop)
    }

    pub(super) fn on_ctrl_t(&mut self, _: &CtrlT, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x14"); // DC4 - transpose chars
    }

    pub(super) fn on_ctrl_u(&mut self, _: &CtrlU, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x15"); // NAK - kill line
    }

    pub(super) fn on_ctrl_v(&mut self, _: &CtrlV, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x16"); // SYN - literal input
    }

    pub(super) fn on_ctrl_w(&mut self, _: &CtrlW, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x17"); // ETB - kill word
    }

    pub(super) fn on_ctrl_x(&mut self, _: &CtrlX, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x18"); // CAN - prefix
    }

    pub(super) fn on_ctrl_y(&mut self, _: &CtrlY, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x19"); // EM - yank
    }

    pub(super) fn on_ctrl_z(&mut self, _: &CtrlZ, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1a"); // SUB - SIGTSTP (suspend)
    }

    // Control+symbol handlers
    pub(super) fn on_ctrl_backslash(
        &mut self,
        _: &CtrlBackslash,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.write_to_terminal(b"\x1c"); // FS - SIGQUIT
    }

    pub(super) fn on_ctrl_bracket_right(
        &mut self,
        _: &CtrlBracketRight,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.write_to_terminal(b"\x1d"); // GS - ESC
    }

    pub(super) fn on_ctrl_caret(&mut self, _: &CtrlCaret, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1e"); // RS - control char
    }

    pub(super) fn on_ctrl_underscore(
        &mut self,
        _: &CtrlUnderscore,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.write_to_terminal(b"\x1f"); // US - undo
    }

    // Alt key handlers (send ESC + character)
    pub(super) fn on_alt_b(&mut self, _: &AltB, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1bb"); // previous word
    }

    pub(super) fn on_alt_d(&mut self, _: &AltD, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1bd"); // delete next word
    }

    pub(super) fn on_alt_f(&mut self, _: &AltF, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1bf"); // next word
    }

    pub(super) fn on_alt_backspace(
        &mut self,
        _: &AltBackspace,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.write_to_terminal(b"\x1b\x7f"); // delete previous word
    }

    // Alt+arrow handlers (xterm sequences with modifier 3)
    pub(super) fn on_alt_up(&mut self, _: &AltUp, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[1;3A");
    }

    pub(super) fn on_alt_down(&mut self, _: &AltDown, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[1;3B");
    }

    pub(super) fn on_alt_left(&mut self, _: &AltLeft, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[1;3D");
    }

    pub(super) fn on_alt_right(&mut self, _: &AltRight, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[1;3C");
    }

    // Shift+arrow handlers (xterm sequences with modifier 2)
    pub(super) fn on_shift_up(&mut self, _: &ShiftUp, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[1;2A");
    }

    pub(super) fn on_shift_down(&mut self, _: &ShiftDown, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[1;2B");
    }

    pub(super) fn on_shift_left(&mut self, _: &ShiftLeft, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[1;2D");
    }

    pub(super) fn on_shift_right(&mut self, _: &ShiftRight, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[1;2C");
    }

    pub(super) fn on_shift_home(&mut self, _: &ShiftHome, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[1;2H");
    }

    pub(super) fn on_shift_end(&mut self, _: &ShiftEnd, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[1;2F");
    }

    pub(super) fn on_shift_insert(
        &mut self,
        _: &ShiftInsert,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Paste from clipboard
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.write_to_terminal(text.as_bytes());
        }
    }

    pub(super) fn on_shift_page_up(
        &mut self,
        _: &ShiftPageUp,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Scroll display up (not sent to PTY, handled locally)
        let page_lines = self.page_scroll_lines();
        if let Some(ref terminal) = self.terminal {
            terminal.scroll(alacritty_terminal::grid::Scroll::Delta(page_lines));
        } else {
            return;
        }
        self.update_content_cache();
        cx.notify();
    }

    pub(super) fn on_shift_page_down(
        &mut self,
        _: &ShiftPageDown,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Scroll display down (not sent to PTY, handled locally)
        let page_lines = self.page_scroll_lines();
        if let Some(ref terminal) = self.terminal {
            terminal.scroll(alacritty_terminal::grid::Scroll::Delta(-page_lines));
        } else {
            return;
        }
        self.update_content_cache();
        cx.notify();
    }

    // Ctrl+arrow handlers (xterm sequences with modifier 5)
    pub(super) fn on_ctrl_up(&mut self, _: &CtrlUp, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[1;5A");
    }

    pub(super) fn on_ctrl_down(&mut self, _: &CtrlDown, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[1;5B");
    }

    pub(super) fn on_ctrl_left(&mut self, _: &CtrlLeft, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[1;5D");
    }

    pub(super) fn on_ctrl_right(&mut self, _: &CtrlRight, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[1;5C");
    }

    // Ctrl+Shift+arrow handlers (xterm sequences with modifier 6)
    pub(super) fn on_ctrl_shift_up(
        &mut self,
        _: &CtrlShiftUp,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.write_to_terminal(b"\x1b[1;6A");
    }

    pub(super) fn on_ctrl_shift_down(
        &mut self,
        _: &CtrlShiftDown,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.write_to_terminal(b"\x1b[1;6B");
    }

    pub(super) fn on_ctrl_shift_left(
        &mut self,
        _: &CtrlShiftLeft,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.write_to_terminal(b"\x1b[1;6D");
    }

    pub(super) fn on_ctrl_shift_right(
        &mut self,
        _: &CtrlShiftRight,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.write_to_terminal(b"\x1b[1;6C");
    }

    pub(super) fn on_ctrl_shift_c(
        &mut self,
        _: &CtrlShiftC,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Copy selection to clipboard
        if let Some(text) = self.get_selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    pub(super) fn on_ctrl_shift_v(
        &mut self,
        _: &CtrlShiftV,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Paste from clipboard
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.write_to_terminal(text.as_bytes());
        }
    }

    // Ctrl+Alt+arrow handlers (xterm sequences with modifier 7)
    pub(super) fn on_ctrl_alt_up(&mut self, _: &CtrlAltUp, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[1;7A");
    }

    pub(super) fn on_ctrl_alt_down(
        &mut self,
        _: &CtrlAltDown,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.write_to_terminal(b"\x1b[1;7B");
    }

    pub(super) fn on_ctrl_alt_left(
        &mut self,
        _: &CtrlAltLeft,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.write_to_terminal(b"\x1b[1;7D");
    }

    pub(super) fn on_ctrl_alt_right(
        &mut self,
        _: &CtrlAltRight,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.write_to_terminal(b"\x1b[1;7C");
    }
}
