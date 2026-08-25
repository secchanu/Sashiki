//! Terminal key bindings and action handlers
//!
//! Only keys the application itself acts on are bound here. Everything else is
//! translated in `input` and encoded by libghostty-vt, which keeps the
//! encoding (including the Kitty keyboard protocol) consistent with Ghostty.

use super::TerminalView;
use super::input_probe::{self, CtrlCProvenance};
use gpui::{
    App, Context, KeyBinding, KeyDownEvent, KeyUpEvent, KeybindingKeystroke, Keystroke,
    ModifiersChangedEvent, Window, actions,
};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

// Typeless-like synthetic Ctrl+C tends to press+release within a few ms.
// Keep this tight so normal human Ctrl+C still interrupts quickly.
const CTRL_C_RAPID_TAP_THRESHOLD: Duration = Duration::from_millis(30);
const CTRL_C_PROVENANCE_MAX_AGE: Duration = Duration::from_millis(250);
const INJECTED_PASTE_SNAPSHOT_MAX_AGE: Duration = Duration::from_millis(1500);

fn keybinding(keystroke: &str) -> KeybindingKeystroke {
    KeybindingKeystroke::from_keystroke(
        Keystroke::parse(keystroke).expect("terminal keybinding must be valid"),
    )
}

static CTRL_C_KEYBINDING: LazyLock<KeybindingKeystroke> = LazyLock::new(|| keybinding("ctrl-c"));
static CTRL_V_KEYBINDING: LazyLock<KeybindingKeystroke> = LazyLock::new(|| keybinding("ctrl-v"));
static SHIFT_INSERT_KEYBINDING: LazyLock<KeybindingKeystroke> =
    LazyLock::new(|| keybinding("shift-insert"));
static CTRL_SHIFT_C_KEYBINDING: LazyLock<KeybindingKeystroke> =
    LazyLock::new(|| keybinding("ctrl-shift-c"));
static CTRL_SHIFT_V_KEYBINDING: LazyLock<KeybindingKeystroke> =
    LazyLock::new(|| keybinding("ctrl-shift-v"));
static SHIFT_PAGE_UP_KEYBINDING: LazyLock<KeybindingKeystroke> =
    LazyLock::new(|| keybinding("shift-pageup"));
static SHIFT_PAGE_DOWN_KEYBINDING: LazyLock<KeybindingKeystroke> =
    LazyLock::new(|| keybinding("shift-pagedown"));

actions!(
    terminal,
    [
        CtrlC,
        CtrlV,
        CtrlShiftC,
        CtrlShiftV,
        ShiftInsert,
        ShiftPageUp,
        ShiftPageDown,
    ]
);

impl TerminalView {
    fn is_ctrl_c_keystroke(keystroke: &Keystroke) -> bool {
        keystroke.should_match(&CTRL_C_KEYBINDING)
    }

    fn is_paste_keystroke(keystroke: &Keystroke) -> bool {
        keystroke.should_match(&CTRL_V_KEYBINDING)
            || keystroke.should_match(&SHIFT_INSERT_KEYBINDING)
            || keystroke.should_match(&CTRL_SHIFT_V_KEYBINDING)
    }

    /// Keystrokes handled by an action, which must not also be encoded and
    /// sent to the terminal.
    fn is_reserved_keystroke(keystroke: &Keystroke) -> bool {
        Self::is_ctrl_c_keystroke(keystroke)
            || Self::is_paste_keystroke(keystroke)
            || keystroke.should_match(&CTRL_SHIFT_C_KEYBINDING)
            || keystroke.should_match(&SHIFT_PAGE_UP_KEYBINDING)
            || keystroke.should_match(&SHIFT_PAGE_DOWN_KEYBINDING)
    }

    pub(super) fn cancel_pending_ctrl_c(&mut self) {
        if let Some(pending) = self.pending_ctrl_c {
            Self::trace_input_event(format!(
                "cancel pending_ctrl_c age_ms={}",
                pending.armed_at.elapsed().as_millis()
            ));
        }
        self.pending_ctrl_c = None;
        input_probe::end_automation_input_session();
    }

    fn flush_pending_ctrl_c(&mut self, reason: &'static str) {
        if let Some(pending) = self.pending_ctrl_c.take() {
            Self::trace_input_event(format!(
                "flush pending_ctrl_c reason={} age_ms={}",
                reason,
                pending.armed_at.elapsed().as_millis()
            ));
            self.write_to_terminal(b"\x03");
        }
        input_probe::end_automation_input_session();
    }

    fn arm_pending_ctrl_c(&mut self) {
        self.pending_ctrl_c = Some(super::view::PendingCtrlCState {
            armed_at: Instant::now(),
            rapid_tap_detected: false,
            ctrl_c_released: false,
        });
    }

    fn mark_pending_ctrl_c_as_automation(&mut self) {
        let Some(mut pending) = self.pending_ctrl_c else {
            return;
        };
        pending.rapid_tap_detected = true;
        pending.ctrl_c_released = true;
        self.pending_ctrl_c = Some(pending);
        Self::trace_input_event(format!(
            "mark pending_ctrl_c as automation age_ms={}",
            pending.armed_at.elapsed().as_millis()
        ));
    }

    pub(super) fn on_terminal_key_down(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        Self::trace_input_event(format!(
            "keydown key={} key_char={:?} mods(c={} a={} s={} p={} f={}) pending={}",
            event.keystroke.key,
            event.keystroke.key_char,
            event.keystroke.modifiers.control,
            event.keystroke.modifiers.alt,
            event.keystroke.modifiers.shift,
            event.keystroke.modifiers.platform,
            event.keystroke.modifiers.function,
            self.pending_ctrl_c.is_some()
        ));

        let reserved = Self::is_reserved_keystroke(&event.keystroke);

        if self.pending_ctrl_c.is_some()
            && !Self::is_ctrl_c_keystroke(&event.keystroke)
            && !Self::is_paste_keystroke(&event.keystroke)
        {
            if self
                .pending_ctrl_c
                .as_ref()
                .is_some_and(|p| p.rapid_tap_detected)
            {
                self.cancel_pending_ctrl_c();
            } else {
                self.flush_pending_ctrl_c("other_keydown");
            }
        }

        if !reserved {
            self.send_key(&event.keystroke);
        }
    }

    pub(super) fn on_terminal_key_up(
        &mut self,
        event: &KeyUpEvent,
        _: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        Self::trace_input_event(format!(
            "keyup key={} key_char={:?} mods(c={} a={} s={} p={} f={}) pending={}",
            event.keystroke.key,
            event.keystroke.key_char,
            event.keystroke.modifiers.control,
            event.keystroke.modifiers.alt,
            event.keystroke.modifiers.shift,
            event.keystroke.modifiers.platform,
            event.keystroke.modifiers.function,
            self.pending_ctrl_c.is_some()
        ));

        if let Some(pending) = self.pending_ctrl_c {
            if Self::is_ctrl_c_keystroke(&event.keystroke) {
                if pending.rapid_tap_detected {
                    // Native probe or rapid-tap heuristic already classified this Ctrl+C
                    // as automation. Keep pending until paste/commit cancels it.
                    Self::trace_input_event("ctrl-c keyup while automation pending -> keep armed");
                    return;
                }

                let age = pending.armed_at.elapsed();
                if age <= CTRL_C_RAPID_TAP_THRESHOLD {
                    Self::trace_input_event(format!(
                        "ctrl-c rapid_tap detected age_ms={} threshold_ms={}",
                        age.as_millis(),
                        CTRL_C_RAPID_TAP_THRESHOLD.as_millis()
                    ));
                    self.mark_pending_ctrl_c_as_automation();
                } else {
                    self.flush_pending_ctrl_c("ctrl_c_keyup");
                }
            } else if !pending.rapid_tap_detected {
                self.flush_pending_ctrl_c("other_keyup");
            }
        }
    }

    pub(super) fn on_terminal_modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        Self::trace_input_event(format!(
            "modifiers c={} a={} s={} p={} f={} pending={:?}",
            event.modifiers.control,
            event.modifiers.alt,
            event.modifiers.shift,
            event.modifiers.platform,
            event.modifiers.function,
            self.pending_ctrl_c
        ));
        if !event.modifiers.control
            && self
                .pending_ctrl_c
                .is_some_and(|pending| !pending.rapid_tap_detected && !pending.ctrl_c_released)
        {
            self.flush_pending_ctrl_c("ctrl_released");
        }

        let was_ctrl = self.ctrl_held;
        self.ctrl_held = event.modifiers.control;
        if !was_ctrl && self.ctrl_held {
            // Ctrl just pressed: scan for URLs so Ctrl+hover works immediately.
            self.detect_urls_from_frame();
        } else if was_ctrl && !self.ctrl_held {
            // Ctrl released: discard URL state to free memory.
            self.detected_urls.clear();
            self.url_cells = Arc::new(HashMap::new());
            if self.hovered_url_index.is_some() {
                self.hovered_url_index = None;
                cx.notify();
            }
        }
    }

    /// Bind terminal key actions to the application
    pub fn bind_keys(cx: &mut App) {
        input_probe::init_native_input_probe();

        cx.bind_keys([
            KeyBinding::new("ctrl-c", CtrlC, Some("Terminal")),
            KeyBinding::new("ctrl-v", CtrlV, Some("Terminal")),
            KeyBinding::new("ctrl-shift-c", CtrlShiftC, Some("Terminal")),
            KeyBinding::new("ctrl-shift-v", CtrlShiftV, Some("Terminal")),
            KeyBinding::new("shift-insert", ShiftInsert, Some("Terminal")),
            KeyBinding::new("shift-pageup", ShiftPageUp, Some("Terminal")),
            KeyBinding::new("shift-pagedown", ShiftPageDown, Some("Terminal")),
        ]);
    }

    // ========================================================================
    // Action handlers
    // ========================================================================

    pub(super) fn on_ctrl_c(&mut self, _: &CtrlC, _: &mut Window, cx: &mut Context<Self>) {
        // Copy selection if present. Without selection, resolve Ctrl+C intent:
        // either SIGINT (normal terminal behavior) or "cancel-before-paste"
        // sequence used by automation/voice input tools.
        if self.has_selection() {
            Self::trace_input_event("action ctrl-c -> copy selection");
            self.cancel_pending_ctrl_c();
            self.copy_selection();
            self.clear_selection();
            cx.notify();
            return;
        }

        match input_probe::recent_ctrl_c_provenance(CTRL_C_PROVENANCE_MAX_AGE) {
            CtrlCProvenance::Hardware => {
                Self::trace_input_event("action ctrl-c -> SIGINT (native-hardware)");
                self.cancel_pending_ctrl_c();
                self.write_to_terminal(b"\x03");
                return;
            }
            CtrlCProvenance::Injected => {
                Self::trace_input_event("action ctrl-c -> arm pending (native-injected)");
                if self.pending_ctrl_c.is_none() {
                    self.arm_pending_ctrl_c();
                }
                input_probe::begin_automation_input_session();
                self.mark_pending_ctrl_c_as_automation();
                return;
            }
            CtrlCProvenance::Unknown => {}
        }

        // Repeated Ctrl+C should interrupt immediately.
        if self.pending_ctrl_c.is_some() {
            Self::trace_input_event("action ctrl-c -> flush pending to SIGINT");
            self.flush_pending_ctrl_c("ctrl_c_repeated");
        } else {
            Self::trace_input_event("action ctrl-c -> arm pending");
            self.arm_pending_ctrl_c();
        }
    }

    pub(super) fn on_ctrl_v(&mut self, _: &CtrlV, window: &mut Window, cx: &mut Context<Self>) {
        // Paste from clipboard (standard behavior for modern terminals on Windows)
        let automation_paste = self
            .pending_ctrl_c
            .as_ref()
            .is_some_and(|pending| pending.rapid_tap_detected);
        if automation_paste {
            // Drive a native WM_PASTE path on the mirror edit so automation
            // tools can validate "paste accepted" via their expected probes.
            input_probe::synthesize_automation_paste_probe();
        }

        let mut source = "clipboard";
        let text = if automation_paste {
            if let Some(snapshot) =
                input_probe::recent_injected_paste_text(INJECTED_PASTE_SNAPSHOT_MAX_AGE)
            {
                source = "native_snapshot";
                Some(snapshot)
            } else {
                cx.read_from_clipboard().and_then(|item| item.text())
            }
        } else {
            cx.read_from_clipboard().and_then(|item| item.text())
        };

        if let Some(text) = text {
            Self::trace_input_event(format!(
                "action ctrl-v -> paste len={} source={}",
                text.len(),
                source
            ));
            self.paste_text(&text);
            window.invalidate_character_coordinates();
        }
    }

    pub(super) fn on_ctrl_shift_c(
        &mut self,
        _: &CtrlShiftC,
        _: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.copy_selection();
    }

    pub(super) fn on_ctrl_shift_v(
        &mut self,
        _: &CtrlShiftV,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            Self::trace_input_event(format!("action ctrl-shift-v -> paste len={}", text.len()));
            self.paste_text(&text);
            window.invalidate_character_coordinates();
        }
    }

    pub(super) fn on_shift_insert(
        &mut self,
        _: &ShiftInsert,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            Self::trace_input_event(format!("action shift-insert -> paste len={}", text.len()));
            self.paste_text(&text);
            window.invalidate_character_coordinates();
        }
    }

    pub(super) fn on_shift_page_up(
        &mut self,
        _: &ShiftPageUp,
        _: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.scroll_lines(self.page_scroll_lines());
    }

    pub(super) fn on_shift_page_down(
        &mut self,
        _: &ShiftPageDown,
        _: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.scroll_lines(-self.page_scroll_lines());
    }
}
