use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CtrlCProvenance {
    Unknown,
    Hardware,
    Injected,
}

pub(crate) fn init_native_input_probe() {
    imp::init_native_input_probe();
}

pub(crate) fn recent_ctrl_c_provenance(max_age: Duration) -> CtrlCProvenance {
    imp::recent_ctrl_c_provenance(max_age)
}

pub(crate) fn recent_injected_paste_text(max_age: Duration) -> Option<String> {
    imp::recent_injected_paste_text(max_age)
}

pub(crate) fn ensure_window_uia_bridge(window: &mut gpui::Window) {
    imp::ensure_window_uia_bridge(window);
}

pub(crate) fn notify_accessibility_text_committed(text: &str) {
    imp::notify_accessibility_text_committed(text);
}

pub(crate) fn begin_automation_input_session() {
    imp::begin_automation_input_session();
}

pub(crate) fn end_automation_input_session() {
    imp::end_automation_input_session();
}

pub(crate) fn synthesize_automation_paste_probe() {
    imp::synthesize_automation_paste_probe();
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::{CtrlCProvenance, Duration};
    use gpui::Window;

    pub(super) fn init_native_input_probe() {}

    pub(super) fn recent_ctrl_c_provenance(_max_age: Duration) -> CtrlCProvenance {
        CtrlCProvenance::Unknown
    }

    pub(super) fn recent_injected_paste_text(_max_age: Duration) -> Option<String> {
        None
    }

    pub(super) fn ensure_window_uia_bridge(_window: &mut Window) {}

    pub(super) fn notify_accessibility_text_committed(_text: &str) {}

    pub(super) fn begin_automation_input_session() {}

    pub(super) fn end_automation_input_session() {}

    pub(super) fn synthesize_automation_paste_probe() {}
}

#[cfg(target_os = "windows")]
mod imp {
    use super::{CtrlCProvenance, Duration};
    use gpui::Window;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::cell::Cell;
    use std::collections::{HashMap, HashSet};
    use std::sync::{Arc, LazyLock, Mutex, Once, OnceLock};
    use std::time::Instant;
    use windows_sys::Win32::Foundation::{
        GetLastError, HANDLE, HWND, LPARAM, LRESULT, SetLastError, WPARAM,
    };
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows_sys::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
    use windows_sys::Win32::UI::Accessibility::{
        AccessibleObjectFromWindow, LresultFromObject, NotifyWinEvent, UiaHostProviderFromHwnd,
        UiaReturnRawElementProvider, UiaRootObjectId,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SetFocus, VK_CONTROL, VK_LCONTROL, VK_RCONTROL,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CWPSTRUCT, CallNextHookEx, CallWindowProcW, CreateWindowExW, DefWindowProcW, DestroyWindow,
        DispatchMessageW, ES_AUTOHSCROLL, ES_LEFT, EVENT_OBJECT_FOCUS, EVENT_OBJECT_LOCATIONCHANGE,
        EVENT_OBJECT_TEXTSELECTIONCHANGED, EVENT_OBJECT_VALUECHANGE, GWLP_WNDPROC,
        GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, HC_ACTION, KBDLLHOOKSTRUCT,
        LLKHF_INJECTED, MSG, OBJID_CARET, OBJID_CLIENT, PostMessageW, SendMessageW,
        SetWindowLongPtrW, SetWindowTextW, SetWindowsHookExW, TranslateMessage,
        UnhookWindowsHookEx, WH_CALLWNDPROC, WH_KEYBOARD_LL, WM_COMMAND, WM_GETOBJECT, WM_KEYDOWN,
        WM_KEYUP, WM_NCDESTROY, WM_PASTE, WM_SYSKEYDOWN, WM_SYSKEYUP, WNDPROC, WS_CHILD,
        WS_VISIBLE,
    };

    #[derive(Clone, Copy, Debug)]
    struct CtrlComboEvent {
        at: Instant,
        injected: bool,
    }

    #[derive(Clone, Debug)]
    struct PasteTextSnapshot {
        at: Instant,
        text: String,
    }

    #[derive(Debug, Default)]
    struct KeyboardProbeInner {
        ctrl_down: bool,
        ctrl_down_injected: bool,
        last_ctrl_c: Option<CtrlComboEvent>,
        last_injected_paste_text: Option<PasteTextSnapshot>,
    }

    #[derive(Debug)]
    struct KeyboardProbeState {
        process_id: u32,
        ui_thread_id: u32,
        inner: Mutex<KeyboardProbeInner>,
    }

    #[derive(Default)]
    struct UiaBridgeState {
        old_wndproc_by_hwnd: HashMap<isize, isize>,
        provider_by_hwnd: HashMap<isize, isize>,
        mirror_edit_by_hwnd: HashMap<isize, isize>,
        automation_session_by_hwnd: HashMap<isize, Instant>,
    }

    static INIT: Once = Once::new();
    static STATE: OnceLock<Arc<KeyboardProbeState>> = OnceLock::new();
    static UIA_BRIDGE_STATE: OnceLock<Mutex<UiaBridgeState>> = OnceLock::new();
    static NATIVE_TRACE_ONCE: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

    thread_local! {
        static IN_UIA_GETOBJECT: Cell<bool> = const { Cell::new(false) };
    }
    const EM_SETSEL_MSG: u32 = 0x00B1;
    const MIRROR_EDIT_CHILD_ID: isize = 1;
    const EN_CHANGE: usize = 0x0300;
    const WM_SASHIKI_RESTORE_HOST_FOCUS_FOR_PASTE: u32 = 0x8000 + 0x535;
    const AUTOMATION_SESSION_BEGIN_HOLD: Duration = Duration::from_secs(8);
    const AUTOMATION_SESSION_END_GRACE: Duration = Duration::from_secs(2);
    const AUTOMATION_SESSION_COMMIT_GRACE: Duration = Duration::from_secs(3);
    const IID_IACCESSIBLE: windows_sys::core::GUID =
        windows_sys::core::GUID::from_u128(0x618736E0_3C3D_11CF_810C_00AA00389B71);

    fn native_trace_enabled() -> bool {
        static ENABLED: LazyLock<bool> = LazyLock::new(|| {
            std::env::var("SASHIKI_TERMINAL_NATIVE_TRACE")
                .map(|v| v != "0")
                .unwrap_or(false)
        });
        *ENABLED
    }

    fn native_trace_verbose_enabled() -> bool {
        static ENABLED: LazyLock<bool> = LazyLock::new(|| {
            std::env::var("SASHIKI_TERMINAL_NATIVE_TRACE_VERBOSE")
                .map(|v| v != "0")
                .unwrap_or(false)
        });
        *ENABLED
    }

    fn should_emit_native_trace(message: &str) -> bool {
        if native_trace_verbose_enabled() {
            return true;
        }
        message.starts_with("native keyboard probe")
            || message.starts_with("llkbd combo=ctrl-c")
            || message.starts_with("llkbd combo=ctrl-v")
            || message.starts_with("llkbd captured injected ctrl-v clipboard")
            || message.starts_with("llkbd injected ctrl-v clipboard capture missed")
            || message.starts_with("uia bridge installed")
            || message.contains("failed")
    }

    fn trace_native_input(message: impl AsRef<str>) {
        let message = message.as_ref();
        let always_emit = message.contains("failed");
        if (always_emit || native_trace_enabled()) && should_emit_native_trace(message) {
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            eprintln!("[terminal-native-input {ms}] {}", message);
        }
    }

    fn trace_native_input_once(key: impl Into<String>, message: impl AsRef<str>) {
        if !native_trace_enabled() {
            return;
        }
        let set = NATIVE_TRACE_ONCE.get_or_init(|| Mutex::new(HashSet::new()));
        let Ok(mut set) = set.lock() else {
            return;
        };
        if set.insert(key.into()) {
            trace_native_input(message);
        }
    }

    fn uia_bridge_enabled() -> bool {
        static ENABLED: LazyLock<bool> = LazyLock::new(|| {
            std::env::var("SASHIKI_TERMINAL_UIA_BRIDGE")
                .map(|v| v != "0")
                .unwrap_or(true)
        });
        *ENABLED
    }

    fn uia_bridge_state() -> &'static Mutex<UiaBridgeState> {
        UIA_BRIDGE_STATE.get_or_init(|| Mutex::new(UiaBridgeState::default()))
    }

    pub(super) fn init_native_input_probe() {
        INIT.call_once(|| {
            let state = Arc::new(KeyboardProbeState {
                process_id: unsafe { GetCurrentProcessId() },
                ui_thread_id: unsafe { GetCurrentThreadId() },
                inner: Mutex::new(KeyboardProbeInner::default()),
            });

            if STATE.set(state).is_err() {
                trace_native_input("probe state was already initialized");
                return;
            }

            let result = std::thread::Builder::new()
                .name("sashiki-native-input-probe".to_string())
                .spawn(move || {
                    run_keyboard_probe_loop();
                });

            if let Err(error) = result {
                trace_native_input(format!(
                    "failed to start native keyboard probe thread: {error}"
                ));
            } else {
                trace_native_input("native keyboard probe thread started");
            }
        });
    }

    pub(super) fn recent_ctrl_c_provenance(max_age: Duration) -> CtrlCProvenance {
        let Some(state) = STATE.get() else {
            return CtrlCProvenance::Unknown;
        };
        let Ok(inner) = state.inner.lock() else {
            return CtrlCProvenance::Unknown;
        };
        let Some(last_ctrl_c) = inner.last_ctrl_c else {
            return CtrlCProvenance::Unknown;
        };
        if last_ctrl_c.at.elapsed() > max_age {
            return CtrlCProvenance::Unknown;
        }
        if last_ctrl_c.injected {
            CtrlCProvenance::Injected
        } else {
            CtrlCProvenance::Hardware
        }
    }

    pub(super) fn recent_injected_paste_text(max_age: Duration) -> Option<String> {
        let state = STATE.get()?;
        let inner = state.inner.lock().ok()?;
        let snapshot = inner.last_injected_paste_text.as_ref()?;
        if snapshot.at.elapsed() > max_age {
            return None;
        }
        Some(snapshot.text.clone())
    }

    pub(super) fn ensure_window_uia_bridge(window: &mut Window) {
        if !uia_bridge_enabled() {
            return;
        }
        let Some(hwnd) = window_hwnd(window) else {
            return;
        };
        let hwnd_key = hwnd as isize;

        let Ok(mut bridge) = uia_bridge_state().lock() else {
            return;
        };
        if bridge.old_wndproc_by_hwnd.contains_key(&hwnd_key) {
            return;
        }

        if ensure_mirror_edit_hwnd(&mut bridge, hwnd).is_none() {
            trace_native_input(format!(
                "uia bridge warning: mirror edit creation failed hwnd=0x{:X}",
                hwnd as usize
            ));
        }

        let provider = match bridge.provider_by_hwnd.get(&hwnd_key).copied() {
            Some(provider) if provider != 0 => provider as *mut core::ffi::c_void,
            _ => {
                let mut provider = std::ptr::null_mut();
                let hr = unsafe { UiaHostProviderFromHwnd(hwnd, &mut provider) };
                if hr < 0 || provider.is_null() {
                    trace_native_input(format!(
                        "uia bridge skipped hwnd=0x{:X} (UiaHostProviderFromHwnd failed hr=0x{:X})",
                        hwnd as usize, hr as u32
                    ));
                    return;
                }
                bridge.provider_by_hwnd.insert(hwnd_key, provider as isize);
                provider
            }
        };

        unsafe { SetLastError(0) };
        let old_proc = unsafe {
            SetWindowLongPtrW(
                hwnd,
                GWLP_WNDPROC,
                terminal_uia_wnd_proc as *const () as usize as isize,
            )
        };
        let last_error = unsafe { GetLastError() };
        if old_proc == 0 && last_error != 0 {
            trace_native_input(format!(
                "uia bridge failed hwnd=0x{:X} err={}",
                hwnd as usize, last_error
            ));
            return;
        }

        bridge.old_wndproc_by_hwnd.insert(hwnd_key, old_proc);
        trace_native_input(format!(
            "uia bridge installed hwnd=0x{:X} provider=0x{:X}",
            hwnd as usize, provider as usize
        ));
    }

    pub(super) fn notify_accessibility_text_committed(text: &str) {
        let Some(state) = STATE.get() else {
            return;
        };
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return;
        }
        let mut pid = 0_u32;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid != state.process_id {
            return;
        }
        // Only extend and rely on the automation session when it was already
        // active via begin_automation_input_session. For ordinary character-by-
        // character typing we still update the mirror edit and fire WinEvents,
        // but must NOT call SetFocus — that steals keyboard focus for
        // COMMIT_FOCUS_RESTORE_FALLBACK_MS, blocking all subsequent input.
        let automation_active = automation_session_active(hwnd);
        if automation_active {
            extend_automation_session(hwnd, AUTOMATION_SESSION_COMMIT_GRACE, "commit");
        }

        if let Some(edit_hwnd) = mirror_edit_for(hwnd) {
            let sanitized: String = text.chars().filter(|c| *c != '\0').collect();
            let mut utf16: Vec<u16> = sanitized.encode_utf16().collect();
            let utf16_len = utf16.len();
            if utf16.is_empty() {
                utf16.push(' ' as u16);
            }
            utf16.push(0);
            unsafe {
                // Update mirror edit content and fire accessibility events so
                // UIA/MSAA clients can read the committed text. SetFocus is
                // intentionally omitted — modern clients can query without focus,
                // and stealing focus blocks all physical keyboard input.
                SetWindowTextW(edit_hwnd, utf16.as_ptr());
                SendMessageW(edit_hwnd, EM_SETSEL_MSG, 0, utf16_len as isize);
                NotifyWinEvent(EVENT_OBJECT_VALUECHANGE, edit_hwnd, OBJID_CLIENT, 0);
                NotifyWinEvent(
                    EVENT_OBJECT_TEXTSELECTIONCHANGED,
                    edit_hwnd,
                    OBJID_CLIENT,
                    0,
                );
                NotifyWinEvent(EVENT_OBJECT_LOCATIONCHANGE, edit_hwnd, OBJID_CARET, 0);
                // Notify host after mirror state has been committed.
                NotifyWinEvent(EVENT_OBJECT_VALUECHANGE, hwnd, OBJID_CLIENT, 0);
                NotifyWinEvent(EVENT_OBJECT_TEXTSELECTIONCHANGED, hwnd, OBJID_CLIENT, 0);
                NotifyWinEvent(EVENT_OBJECT_LOCATIONCHANGE, hwnd, OBJID_CARET, 0);
                // Some automation clients subscribe to child-id specific updates.
                NotifyWinEvent(
                    EVENT_OBJECT_VALUECHANGE,
                    hwnd,
                    OBJID_CLIENT,
                    MIRROR_EDIT_CHILD_ID as i32,
                );
                NotifyWinEvent(
                    EVENT_OBJECT_TEXTSELECTIONCHANGED,
                    hwnd,
                    OBJID_CLIENT,
                    MIRROR_EDIT_CHILD_ID as i32,
                );
                let wparam =
                    ((EN_CHANGE as usize) << 16) | ((MIRROR_EDIT_CHILD_ID as usize) & 0xFFFF);
                SendMessageW(hwnd, WM_COMMAND, wparam, edit_hwnd as isize);
            }
            trace_native_input(format!(
                "notify accessibility text committed hwnd=0x{:X} mirror_edit=0x{:X} len={} focus_stole={}",
                hwnd as usize,
                edit_hwnd as usize,
                sanitized.chars().count(),
                automation_active,
            ));
            return;
        }

        unsafe {
            NotifyWinEvent(EVENT_OBJECT_VALUECHANGE, hwnd, OBJID_CLIENT, 0);
            NotifyWinEvent(EVENT_OBJECT_TEXTSELECTIONCHANGED, hwnd, OBJID_CLIENT, 0);
            NotifyWinEvent(EVENT_OBJECT_LOCATIONCHANGE, hwnd, OBJID_CARET, 0);
            NotifyWinEvent(EVENT_OBJECT_FOCUS, hwnd, OBJID_CLIENT, 0);
        }
        trace_native_input(format!(
            "notify accessibility text committed hwnd=0x{:X}",
            hwnd as usize
        ));
    }

    pub(super) fn begin_automation_input_session() {
        let Some(state) = STATE.get() else {
            return;
        };
        let host_hwnd = unsafe { GetForegroundWindow() };
        if host_hwnd.is_null() {
            return;
        }
        let mut pid = 0_u32;
        unsafe { GetWindowThreadProcessId(host_hwnd, &mut pid) };
        if pid != state.process_id {
            return;
        }
        let Some(edit_hwnd) = mirror_edit_for(host_hwnd) else {
            return;
        };
        extend_automation_session(host_hwnd, AUTOMATION_SESSION_BEGIN_HOLD, "begin");

        // Do NOT call SetFocus here. Stealing keyboard focus to the mirror edit
        // blocks all physical keyboard input to GPUI for the session duration.
        // Modern UIA clients can query the mirror edit without it having focus.
        unsafe {
            NotifyWinEvent(EVENT_OBJECT_FOCUS, edit_hwnd, OBJID_CLIENT, 0);
        }
        trace_native_input_once(
            "automation-focus-begin",
            format!(
                "uia bridge automation focus hold host=0x{:X} mirror=0x{:X}",
                host_hwnd as usize, edit_hwnd as usize
            ),
        );
    }

    pub(super) fn end_automation_input_session() {
        let Some(state) = STATE.get() else {
            return;
        };
        let host_hwnd = unsafe { GetForegroundWindow() };
        if host_hwnd.is_null() {
            return;
        }
        let mut pid = 0_u32;
        unsafe { GetWindowThreadProcessId(host_hwnd, &mut pid) };
        if pid != state.process_id {
            return;
        }
        // Keep bridge behavior alive briefly after Ctrl+V/commit so automation
        // clients can complete their post-paste verification.
        extend_automation_session(host_hwnd, AUTOMATION_SESSION_END_GRACE, "end");
    }

    pub(super) fn synthesize_automation_paste_probe() {
        let Some(state) = STATE.get() else {
            return;
        };
        let host_hwnd = unsafe { GetForegroundWindow() };
        if host_hwnd.is_null() {
            return;
        }
        let mut pid = 0_u32;
        unsafe { GetWindowThreadProcessId(host_hwnd, &mut pid) };
        if pid != state.process_id {
            return;
        }
        if !automation_session_active(host_hwnd) {
            return;
        }
        let Some(edit_hwnd) = mirror_edit_for(host_hwnd) else {
            return;
        };

        extend_automation_session(host_hwnd, AUTOMATION_SESSION_COMMIT_GRACE, "wm_paste_probe");
        unsafe {
            let prev_focus = SetFocus(edit_hwnd);
            let _ = SendMessageW(edit_hwnd, WM_PASTE, 0, 0);
            let _ = SendMessageW(edit_hwnd, EM_SETSEL_MSG, 0, isize::MAX);
            NotifyWinEvent(EVENT_OBJECT_VALUECHANGE, edit_hwnd, OBJID_CLIENT, 0);
            NotifyWinEvent(
                EVENT_OBJECT_TEXTSELECTIONCHANGED,
                edit_hwnd,
                OBJID_CLIENT,
                0,
            );
            NotifyWinEvent(EVENT_OBJECT_FOCUS, edit_hwnd, OBJID_CLIENT, 0);
            NotifyWinEvent(
                EVENT_OBJECT_VALUECHANGE,
                host_hwnd,
                OBJID_CLIENT,
                MIRROR_EDIT_CHILD_ID as i32,
            );
            let wparam = ((EN_CHANGE as usize) << 16) | ((MIRROR_EDIT_CHILD_ID as usize) & 0xFFFF);
            SendMessageW(host_hwnd, WM_COMMAND, wparam, edit_hwnd as isize);
            if !prev_focus.is_null() {
                let _ = SetFocus(prev_focus);
            }
        }
        trace_native_input_once(
            "automation-wm-paste-probe",
            format!(
                "uia bridge synthesized WM_PASTE host=0x{:X} mirror=0x{:X}",
                host_hwnd as usize, edit_hwnd as usize
            ),
        );
    }

    fn extend_automation_session(hwnd: HWND, hold: Duration, reason: &'static str) {
        let hwnd_key = hwnd as isize;
        let deadline = Instant::now() + hold;
        let Ok(mut bridge) = uia_bridge_state().lock() else {
            return;
        };
        let should_log = match bridge.automation_session_by_hwnd.get(&hwnd_key).copied() {
            Some(prev) => prev < deadline,
            None => true,
        };
        bridge.automation_session_by_hwnd.insert(hwnd_key, deadline);
        drop(bridge);

        if should_log {
            trace_native_input_once(
                format!("automation-session-{reason}"),
                format!(
                    "uia bridge automation session extended host=0x{:X} hold_ms={} reason={}",
                    hwnd as usize,
                    hold.as_millis(),
                    reason
                ),
            );
        }
    }

    fn automation_session_active(hwnd: HWND) -> bool {
        let hwnd_key = hwnd as isize;
        let Ok(mut bridge) = uia_bridge_state().lock() else {
            return false;
        };
        let now = Instant::now();
        bridge
            .automation_session_by_hwnd
            .retain(|_, deadline| *deadline > now);
        bridge
            .automation_session_by_hwnd
            .get(&hwnd_key)
            .is_some_and(|deadline| *deadline > now)
    }

    fn request_restore_host_focus_for_injected_paste(process_id: u32) {
        let host_hwnd = unsafe { GetForegroundWindow() };
        if host_hwnd.is_null() {
            return;
        }
        let mut pid = 0_u32;
        unsafe { GetWindowThreadProcessId(host_hwnd, &mut pid) };
        if pid != process_id {
            return;
        }
        if !automation_session_active(host_hwnd) {
            return;
        }
        let Some(edit_hwnd) = mirror_edit_for(host_hwnd) else {
            return;
        };
        let _ = unsafe { PostMessageW(host_hwnd, WM_SASHIKI_RESTORE_HOST_FOCUS_FOR_PASTE, 0, 0) };
        trace_native_input_once(
            "request-host-focus-before-paste",
            format!(
                "uia bridge requested host focus restore before injected ctrl-v host=0x{:X} mirror=0x{:X}",
                host_hwnd as usize, edit_hwnd as usize
            ),
        );
    }

    fn run_keyboard_probe_loop() {
        let keyboard_hook = unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(low_level_keyboard_proc),
                std::ptr::null_mut(),
                0,
            )
        };
        if keyboard_hook.is_null() {
            trace_native_input("SetWindowsHookExW(WH_KEYBOARD_LL) failed");
            return;
        }

        let callwnd_hook = STATE.get().and_then(|state| unsafe {
            let hook = SetWindowsHookExW(
                WH_CALLWNDPROC,
                Some(call_wnd_proc),
                std::ptr::null_mut(),
                state.ui_thread_id,
            );
            if hook.is_null() {
                trace_native_input("SetWindowsHookExW(WH_CALLWNDPROC) failed");
                None
            } else {
                trace_native_input(format!(
                    "native keyboard probe callwnd hook installed thread_id={}",
                    state.ui_thread_id
                ));
                Some(hook)
            }
        });

        trace_native_input("native keyboard probe hook installed");

        let mut msg: MSG = unsafe { std::mem::zeroed() };
        loop {
            let status = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
            if status > 0 {
                unsafe {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                continue;
            }
            if status == 0 {
                trace_native_input("native keyboard probe loop exited");
            } else {
                trace_native_input("native keyboard probe loop failed (GetMessageW < 0)");
            }
            break;
        }

        let _ = unsafe { UnhookWindowsHookEx(keyboard_hook) };
        if let Some(callwnd_hook) = callwnd_hook {
            let _ = unsafe { UnhookWindowsHookEx(callwnd_hook) };
        }
    }

    unsafe extern "system" fn low_level_keyboard_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code == HC_ACTION as i32 && lparam != 0 {
            if let Some(state) = STATE.get() {
                if foreground_process_matches(state.process_id) {
                    let keyboard = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
                    process_keyboard_event(state, wparam, keyboard);
                }
            }
        }

        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
    }

    unsafe extern "system" fn call_wnd_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 && lparam != 0 {
            let cwp = unsafe { &*(lparam as *const CWPSTRUCT) };
            if cwp.message == WM_GETOBJECT
                && (cwp.lParam as i32 == UiaRootObjectId || cwp.lParam as i32 == OBJID_CLIENT)
            {
                let object_id = cwp.lParam as i32;
                trace_native_input_once(
                    format!("callwnd-getobject:{object_id}"),
                    format!(
                        "callwnd WM_GETOBJECT first-seen object_id={} wparam={} hwnd=0x{:X}",
                        object_id, cwp.wParam, cwp.hwnd as usize
                    ),
                );
            }
        }
        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
    }

    unsafe extern "system" fn terminal_uia_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_SASHIKI_RESTORE_HOST_FOCUS_FOR_PASTE {
            // Restore keyboard focus to the host window before an automation
            // paste so the paste lands in the terminal, not the mirror edit.
            unsafe {
                let _ = SetFocus(hwnd);
                NotifyWinEvent(EVENT_OBJECT_FOCUS, hwnd, OBJID_CLIENT, 0);
            }
            trace_native_input_once(
                "host-focus-restored-before-paste",
                format!(
                    "uia bridge restored host focus before paste host=0x{:X}",
                    hwnd as usize
                ),
            );
            return 0;
        }

        if msg == WM_GETOBJECT {
            let object_id = lparam as i32;
            let automation_active = automation_session_active(hwnd);
            let edit_hwnd_for_host = mirror_edit_for(hwnd);
            // Keep host-native behavior for positive child object ids.
            // Automation tools often resolve OBJID_CLIENT children (e.g. child id=1)
            // and expect host-specific object identity.
            if object_id > 0 {
                trace_native_input_once(
                    format!("getobject-child-passthrough:{object_id}"),
                    format!(
                        "uia bridge passthrough WM_GETOBJECT child object_id={} host=0x{:X}",
                        object_id, hwnd as usize
                    ),
                );
                return call_original_wnd_proc(hwnd, msg, wparam, lparam);
            }
            // Keep host-native handling for query-class probes. Some automation
            // clients use this to detect editable controls, and mirror answers can
            // cause false negatives.
            if object_id == -12 {
                if automation_active && let Some(edit_hwnd) = edit_hwnd_for_host {
                    let same = unsafe { SendMessageW(edit_hwnd, WM_GETOBJECT, wparam, lparam) };
                    if same != 0 {
                        trace_native_input_once(
                            "getobject-queryclassname-remap",
                            format!(
                                "uia bridge remapped WM_GETOBJECT object_id=-12 route=edit-hwnd session=active host=0x{:X} mirror=0x{:X} result=0x{:X}",
                                hwnd as usize, edit_hwnd as usize, same as usize
                            ),
                        );
                        return same;
                    }
                    if wparam != 0 {
                        let zero_wparam =
                            unsafe { SendMessageW(edit_hwnd, WM_GETOBJECT, 0, lparam) };
                        if zero_wparam != 0 {
                            trace_native_input_once(
                                "getobject-queryclassname-remap-zero-wparam",
                                format!(
                                    "uia bridge remapped WM_GETOBJECT object_id=-12 route=edit-hwnd-zero-wparam session=active host=0x{:X} mirror=0x{:X} result=0x{:X}",
                                    hwnd as usize, edit_hwnd as usize, zero_wparam as usize
                                ),
                            );
                            return zero_wparam;
                        }
                    }
                }
                trace_native_input_once(
                    "getobject-queryclassname-passthrough",
                    format!(
                        "uia bridge passthrough WM_GETOBJECT object_id=-12 host=0x{:X}",
                        hwnd as usize
                    ),
                );
                return call_original_wnd_proc(hwnd, msg, wparam, lparam);
            }

            if let Some(edit_hwnd) = edit_hwnd_for_host {
                if let Some((result, route)) =
                    forward_getobject_to_mirror_edit(edit_hwnd, object_id, wparam, lparam)
                {
                    trace_native_input_once(
                        format!("getobject-forwarded:{object_id}:{route}"),
                        format!(
                            "uia bridge forwarded WM_GETOBJECT object_id={} route={} host=0x{:X} mirror=0x{:X} result=0x{:X}",
                            object_id, route, hwnd as usize, edit_hwnd as usize, result as usize
                        ),
                    );
                    return result;
                }
                trace_native_input_once(
                    format!("getobject-forward-miss:{object_id}"),
                    format!(
                        "uia bridge forward miss object_id={} wparam={} lparam={} host=0x{:X} mirror=0x{:X}",
                        object_id, wparam, lparam, hwnd as usize, edit_hwnd as usize
                    ),
                );

                if object_id != UiaRootObjectId
                    && object_id <= 0
                    && let Some(result) =
                        msaa_lresult_from_hwnd(edit_hwnd, object_id, wparam, "mirror")
                {
                    trace_native_input_once(
                        format!("getobject-msaa-remap-mirror:{object_id}"),
                        format!(
                            "uia bridge remapped WM_GETOBJECT msaa source=mirror object_id={} host=0x{:X} mirror=0x{:X} result=0x{:X}",
                            object_id, hwnd as usize, edit_hwnd as usize, result as usize
                        ),
                    );
                    return result;
                }

                if object_id == UiaRootObjectId {
                    let try_edit_first = automation_active;
                    if try_edit_first {
                        if let Some(provider) = uia_host_provider_from_hwnd(edit_hwnd) {
                            if let Some(result) =
                                uia_return_raw_provider(edit_hwnd, wparam, lparam, provider)
                            {
                                trace_native_input_once(
                                    "getobject-root-remap:edit-hwnd-session",
                                    format!(
                                        "uia bridge remapped WM_GETOBJECT root route=edit-hwnd session=active host=0x{:X} mirror=0x{:X} result=0x{:X}",
                                        hwnd as usize, edit_hwnd as usize, result as usize
                                    ),
                                );
                                return result;
                            }
                            if let Some(result) =
                                uia_return_raw_provider(hwnd, wparam, lparam, provider)
                            {
                                trace_native_input_once(
                                    "getobject-root-remap:host-hwnd-session",
                                    format!(
                                        "uia bridge remapped WM_GETOBJECT root route=host-hwnd session=active host=0x{:X} mirror=0x{:X} result=0x{:X}",
                                        hwnd as usize, edit_hwnd as usize, result as usize
                                    ),
                                );
                                return result;
                            }
                        }
                    }
                    if let Some(provider) = uia_provider_for(hwnd)
                        && let Some(result) =
                            uia_return_raw_provider(hwnd, wparam, lparam, provider)
                    {
                        trace_native_input_once(
                            if try_edit_first {
                                "getobject-root-remap:host-provider-fallback"
                            } else {
                                "getobject-root-remap:host-provider-first"
                            },
                            format!(
                                "uia bridge remapped WM_GETOBJECT root route=host-provider session_active={} host=0x{:X} result=0x{:X}",
                                automation_active, hwnd as usize, result as usize
                            ),
                        );
                        return result;
                    }
                    if !try_edit_first
                        && let Some(provider) = uia_host_provider_from_hwnd(edit_hwnd)
                    {
                        if let Some(result) =
                            uia_return_raw_provider(edit_hwnd, wparam, lparam, provider)
                        {
                            trace_native_input_once(
                                "getobject-root-remap:edit-hwnd",
                                format!(
                                    "uia bridge remapped WM_GETOBJECT root route=edit-hwnd host=0x{:X} mirror=0x{:X} result=0x{:X}",
                                    hwnd as usize, edit_hwnd as usize, result as usize
                                ),
                            );
                            return result;
                        }
                        if let Some(result) =
                            uia_return_raw_provider(hwnd, wparam, lparam, provider)
                        {
                            trace_native_input_once(
                                "getobject-root-remap:host-hwnd",
                                format!(
                                    "uia bridge remapped WM_GETOBJECT root route=host-hwnd host=0x{:X} mirror=0x{:X} result=0x{:X}",
                                    hwnd as usize, edit_hwnd as usize, result as usize
                                ),
                            );
                            return result;
                        }
                    }
                }
            }

            if object_id != UiaRootObjectId
                && object_id <= 0
                && let Some(result) = msaa_lresult_from_hwnd(hwnd, object_id, wparam, "host")
            {
                trace_native_input_once(
                    format!("getobject-msaa-host:{object_id}"),
                    format!(
                        "uia bridge handled WM_GETOBJECT msaa source=host object_id={} host=0x{:X} result=0x{:X}",
                        object_id, hwnd as usize, result as usize
                    ),
                );
                return result;
            }
            if object_id > 0
                && let Some(result) =
                    msaa_lresult_from_hwnd_with_positive(hwnd, object_id, wparam, "host-child")
            {
                trace_native_input_once(
                    format!("getobject-msaa-host-child:{object_id}"),
                    format!(
                        "uia bridge handled WM_GETOBJECT msaa source=host-child object_id={} host=0x{:X} result=0x{:X}",
                        object_id, hwnd as usize, result as usize
                    ),
                );
                return result;
            }

            if object_id == UiaRootObjectId {
                if let Some(provider) = uia_provider_for(hwnd) {
                    if let Some(result) = uia_return_raw_provider(hwnd, wparam, lparam, provider) {
                        trace_native_input_once(
                            "getobject-root-host-provider",
                            format!(
                                "uia bridge handled WM_GETOBJECT root with host-provider hwnd=0x{:X} result=0x{:X}",
                                hwnd as usize, result as usize
                            ),
                        );
                        return result;
                    }
                }
            } else if object_id == OBJID_CLIENT {
                trace_native_input_once(
                    "getobject-client-passthrough",
                    format!(
                        "uia bridge passthrough WM_GETOBJECT client hwnd=0x{:X}",
                        hwnd as usize
                    ),
                );
            }
        }

        call_original_wnd_proc(hwnd, msg, wparam, lparam)
    }

    fn forward_getobject_to_mirror_edit(
        edit_hwnd: HWND,
        object_id: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<(LRESULT, &'static str)> {
        // UIA root requests must be handled by UiaReturnRawElementProvider path,
        // not by legacy/MSAA fallback.
        if object_id == UiaRootObjectId || object_id == -12 {
            return None;
        }

        let same = unsafe { SendMessageW(edit_hwnd, WM_GETOBJECT, wparam, lparam) };
        if same != 0 {
            return Some((same, "same"));
        }

        if wparam != 0 {
            let zero_wparam = unsafe { SendMessageW(edit_hwnd, WM_GETOBJECT, 0, lparam) };
            if zero_wparam != 0 {
                return Some((zero_wparam, "zero-wparam"));
            }
        }

        if object_id == 0 {
            let window_obj = unsafe { SendMessageW(edit_hwnd, WM_GETOBJECT, 0, 0) };
            if window_obj != 0 {
                return Some((window_obj, "obj-window"));
            }
            let client_obj =
                unsafe { SendMessageW(edit_hwnd, WM_GETOBJECT, 0, OBJID_CLIENT as LPARAM) };
            if client_obj != 0 {
                return Some((client_obj, "obj-window->client"));
            }
        } else if object_id > 0 {
            // Prefer exact child-id mapping first.
            if let Some(result) =
                msaa_lresult_from_hwnd_with_positive(edit_hwnd, object_id, wparam, "mirror-child")
            {
                return Some((result, "obj-child->msaa-exact"));
            }
            // As a fallback, degrade to the mirror CLIENT object.
            let client_obj =
                unsafe { SendMessageW(edit_hwnd, WM_GETOBJECT, 0, OBJID_CLIENT as LPARAM) };
            if client_obj != 0 {
                return Some((client_obj, "obj-child->client"));
            }
            if let Some(result) = msaa_lresult_from_hwnd_with_positive(
                edit_hwnd,
                OBJID_CLIENT,
                wparam,
                "mirror-child-client",
            ) {
                return Some((result, "obj-child->msaa-client"));
            }
        } else if object_id == OBJID_CLIENT {
            let client_obj =
                unsafe { SendMessageW(edit_hwnd, WM_GETOBJECT, 0, OBJID_CLIENT as LPARAM) };
            if client_obj != 0 {
                return Some((client_obj, "obj-client"));
            }
        }

        None
    }

    fn msaa_lresult_from_hwnd(
        target_hwnd: HWND,
        object_id: i32,
        wparam: WPARAM,
        source: &'static str,
    ) -> Option<LRESULT> {
        if object_id > 0 {
            return None;
        }
        msaa_lresult_from_hwnd_inner(target_hwnd, object_id, wparam, source)
    }

    fn msaa_lresult_from_hwnd_with_positive(
        target_hwnd: HWND,
        object_id: i32,
        wparam: WPARAM,
        source: &'static str,
    ) -> Option<LRESULT> {
        msaa_lresult_from_hwnd_inner(target_hwnd, object_id, wparam, source)
    }

    fn msaa_lresult_from_hwnd_inner(
        target_hwnd: HWND,
        object_id: i32,
        wparam: WPARAM,
        source: &'static str,
    ) -> Option<LRESULT> {
        let mut object = std::ptr::null_mut();
        let hr = unsafe {
            AccessibleObjectFromWindow(
                target_hwnd,
                object_id as u32,
                &IID_IACCESSIBLE as *const _,
                &mut object,
            )
        };
        if hr < 0 || object.is_null() {
            trace_native_input_once(
                format!("msaa-miss:{source}:{object_id}:{hr}"),
                format!(
                    "uia bridge msaa miss source={} hwnd=0x{:X} object_id={} hr=0x{:X}",
                    source, target_hwnd as usize, object_id, hr as u32
                ),
            );
            return None;
        }

        let result = unsafe { LresultFromObject(&IID_IACCESSIBLE as *const _, wparam, object) };
        release_com_unknown(object);
        if result == 0 {
            trace_native_input_once(
                format!("msaa-lresult-zero:{source}:{object_id}"),
                format!(
                    "uia bridge msaa lresult=0 source={} hwnd=0x{:X} object_id={}",
                    source, target_hwnd as usize, object_id
                ),
            );
            None
        } else {
            Some(result)
        }
    }

    fn release_com_unknown(object: *mut core::ffi::c_void) {
        if object.is_null() {
            return;
        }
        #[repr(C)]
        struct IUnknownVTable {
            query_interface: unsafe extern "system" fn(
                *mut core::ffi::c_void,
                *const windows_sys::core::GUID,
                *mut *mut core::ffi::c_void,
            ) -> windows_sys::core::HRESULT,
            add_ref: unsafe extern "system" fn(*mut core::ffi::c_void) -> u32,
            release: unsafe extern "system" fn(*mut core::ffi::c_void) -> u32,
        }
        #[repr(C)]
        struct IUnknownRaw {
            vtable: *const IUnknownVTable,
        }
        let raw = object as *mut IUnknownRaw;
        unsafe {
            if !raw.is_null() && !(*raw).vtable.is_null() {
                ((*(*raw).vtable).release)(object);
            }
        }
    }

    fn uia_return_raw_provider(
        hwnd: HWND,
        wparam: WPARAM,
        lparam: LPARAM,
        provider: *mut core::ffi::c_void,
    ) -> Option<LRESULT> {
        IN_UIA_GETOBJECT.with(|guard| {
            if guard.get() {
                return None;
            }
            guard.set(true);
            let result = unsafe { UiaReturnRawElementProvider(hwnd, wparam, lparam, provider) };
            guard.set(false);
            Some(result)
        })
    }

    fn uia_host_provider_from_hwnd(hwnd: HWND) -> Option<*mut core::ffi::c_void> {
        let mut provider = std::ptr::null_mut();
        let hr = unsafe { UiaHostProviderFromHwnd(hwnd, &mut provider) };
        if hr < 0 || provider.is_null() {
            trace_native_input(format!(
                "uia bridge provider query failed hwnd=0x{:X} hr=0x{:X}",
                hwnd as usize, hr as u32
            ));
            None
        } else {
            Some(provider)
        }
    }

    fn call_original_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let hwnd_key = hwnd as isize;
        let old_proc = {
            let Ok(mut bridge) = uia_bridge_state().lock() else {
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            };
            let old = bridge.old_wndproc_by_hwnd.get(&hwnd_key).copied();
            if msg == WM_NCDESTROY {
                bridge.old_wndproc_by_hwnd.remove(&hwnd_key);
                bridge.provider_by_hwnd.remove(&hwnd_key);
                bridge.automation_session_by_hwnd.remove(&hwnd_key);
                if let Some(edit_hwnd) = bridge.mirror_edit_by_hwnd.remove(&hwnd_key) {
                    let _ = unsafe { DestroyWindow(edit_hwnd as HWND) };
                }
            }
            old
        };

        if let Some(old_proc) = old_proc {
            let proc: WNDPROC = Some(unsafe { std::mem::transmute(old_proc) });
            unsafe { CallWindowProcW(proc, hwnd, msg, wparam, lparam) }
        } else {
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }

    fn uia_provider_for(hwnd: HWND) -> Option<*mut core::ffi::c_void> {
        let hwnd_key = hwnd as isize;
        let Ok(bridge) = uia_bridge_state().lock() else {
            return None;
        };
        bridge
            .provider_by_hwnd
            .get(&hwnd_key)
            .copied()
            .and_then(|provider| (provider != 0).then_some(provider as *mut core::ffi::c_void))
    }

    fn mirror_edit_for(hwnd: HWND) -> Option<HWND> {
        let hwnd_key = hwnd as isize;
        let Ok(bridge) = uia_bridge_state().lock() else {
            return None;
        };
        bridge
            .mirror_edit_by_hwnd
            .get(&hwnd_key)
            .copied()
            .and_then(|edit_hwnd| (edit_hwnd != 0).then_some(edit_hwnd as HWND))
    }

    fn ensure_mirror_edit_hwnd(bridge: &mut UiaBridgeState, parent_hwnd: HWND) -> Option<HWND> {
        let parent_key = parent_hwnd as isize;
        if let Some(existing) = bridge.mirror_edit_by_hwnd.get(&parent_key).copied()
            && existing != 0
        {
            return Some(existing as HWND);
        }

        const EDIT_CLASS: [u16; 5] = [69, 68, 73, 84, 0]; // "EDIT\0"
        const EMPTY_TEXT: [u16; 1] = [0];

        let style = WS_CHILD | WS_VISIBLE | ES_LEFT as u32 | ES_AUTOHSCROLL as u32;
        let edit_hwnd = unsafe {
            CreateWindowExW(
                0,
                EDIT_CLASS.as_ptr(),
                EMPTY_TEXT.as_ptr(),
                style,
                1,
                1,
                1,
                1,
                parent_hwnd,
                MIRROR_EDIT_CHILD_ID as usize as *mut core::ffi::c_void,
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if edit_hwnd.is_null() {
            return None;
        }

        bridge
            .mirror_edit_by_hwnd
            .insert(parent_key, edit_hwnd as isize);
        Some(edit_hwnd)
    }

    fn window_hwnd(window: &Window) -> Option<HWND> {
        let handle = <Window as HasWindowHandle>::window_handle(window).ok()?;
        match handle.as_raw() {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as HWND),
            _ => None,
        }
    }

    fn foreground_process_matches(process_id: u32) -> bool {
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.is_null() {
            return false;
        }
        let mut foreground_process_id = 0_u32;
        unsafe { GetWindowThreadProcessId(foreground, &mut foreground_process_id) };
        foreground_process_id == process_id
    }

    fn process_keyboard_event(state: &KeyboardProbeState, wparam: WPARAM, kb: &KBDLLHOOKSTRUCT) {
        let message = wparam as u32;
        let key_down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
        let key_up = message == WM_KEYUP || message == WM_SYSKEYUP;
        let injected = (kb.flags & LLKHF_INJECTED) != 0;
        let vk_code = kb.vkCode;

        let Ok(mut inner) = state.inner.lock() else {
            return;
        };

        if is_ctrl_vk(vk_code) {
            if key_down {
                inner.ctrl_down = true;
                inner.ctrl_down_injected = injected;
            } else if key_up {
                inner.ctrl_down = false;
                inner.ctrl_down_injected = false;
            }
            trace_native_input(format!(
                "llkbd key=ctrl down={} up={} injected={}",
                key_down, key_up, injected
            ));
            return;
        }

        if key_down && vk_code == b'C' as u32 && inner.ctrl_down {
            let combo_injected = injected || inner.ctrl_down_injected;
            inner.last_ctrl_c = Some(CtrlComboEvent {
                at: Instant::now(),
                injected: combo_injected,
            });
            trace_native_input(format!(
                "llkbd combo=ctrl-c injected={} (c_injected={} ctrl_injected={})",
                combo_injected, injected, inner.ctrl_down_injected
            ));
            return;
        }

        if key_down && vk_code == b'V' as u32 && inner.ctrl_down {
            let combo_injected = injected || inner.ctrl_down_injected;
            trace_native_input(format!(
                "llkbd combo=ctrl-v injected={} (v_injected={} ctrl_injected={})",
                combo_injected, injected, inner.ctrl_down_injected
            ));
            if combo_injected {
                let snapshot = snapshot_clipboard_text();
                if let Some(text) = snapshot {
                    trace_native_input(format!(
                        "llkbd captured injected ctrl-v clipboard len={}",
                        text.len()
                    ));
                    inner.last_injected_paste_text = Some(PasteTextSnapshot {
                        at: Instant::now(),
                        text,
                    });
                } else {
                    trace_native_input("llkbd injected ctrl-v clipboard capture missed");
                    inner.last_injected_paste_text = None;
                }
                request_restore_host_focus_for_injected_paste(state.process_id);
            }
        }
    }

    fn snapshot_clipboard_text() -> Option<String> {
        const CF_UNICODETEXT_FORMAT: u32 = 13;
        const MAX_UNITS: usize = 1_048_576;

        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return None;
            }

            let result = (|| {
                let handle: HANDLE = GetClipboardData(CF_UNICODETEXT_FORMAT);
                if handle.is_null() {
                    return None;
                }

                let locked = GlobalLock(handle) as *const u16;
                if locked.is_null() {
                    return None;
                }

                let mut len = 0usize;
                while len < MAX_UNITS && *locked.add(len) != 0 {
                    len += 1;
                }

                let text = if len >= MAX_UNITS {
                    None
                } else {
                    Some(String::from_utf16_lossy(std::slice::from_raw_parts(
                        locked, len,
                    )))
                };
                let _ = GlobalUnlock(handle);
                text
            })();

            let _ = CloseClipboard();
            result
        }
    }

    fn is_ctrl_vk(vk_code: u32) -> bool {
        vk_code == VK_CONTROL as u32
            || vk_code == VK_LCONTROL as u32
            || vk_code == VK_RCONTROL as u32
    }
}
