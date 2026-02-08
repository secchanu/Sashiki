//! Sashiki - Git worktree manager with integrated terminals
//!
//! Each git worktree gets its own terminal session, making it easy to work on
//! multiple branches simultaneously.

mod app;
mod dialog;
mod git;
mod session;
mod template;
mod terminal;
mod theme;
mod ui;

use app::{
    CloseFileView, NextSession, OpenFolder, PrevSession, Quit, RefreshAll, SashikiApp,
    ToggleFileList, ToggleParallelMode, ToggleSidebar, ToggleVerifyTerminal,
};
use gpui::{App, AppContext, Application, Focusable, KeyBinding, Menu, MenuItem, WindowOptions};
use terminal::TerminalView;

fn main() {
    Application::new().run(|app: &mut App| {
        // Global bindings must be registered BEFORE terminal bindings.
        // GPUI resolves ties (same context depth) by LIFO, so terminal-specific
        // bindings registered later will correctly override these when focused.
        app.bind_keys([
            KeyBinding::new("ctrl-o", OpenFolder, None),
            KeyBinding::new("ctrl-p", ToggleParallelMode, None),
            KeyBinding::new("ctrl-tab", NextSession, None),
            KeyBinding::new("ctrl-shift-tab", PrevSession, None),
            KeyBinding::new("ctrl-b", ToggleSidebar, None),
            KeyBinding::new("ctrl-t", ToggleVerifyTerminal, None),
            KeyBinding::new("ctrl-e", ToggleFileList, None),
            KeyBinding::new("ctrl-r", RefreshAll, None),
            KeyBinding::new("escape", CloseFileView, None),
        ]);

        app.on_action(|_: &Quit, cx: &mut App| {
            cx.quit();
        });

        app.set_menus(vec![
            Menu {
                name: "Sashiki".into(),
                items: vec![
                    MenuItem::action("Quit", Quit),
                ],
            },
            Menu {
                name: "File".into(),
                items: vec![
                    MenuItem::action("Open Folder", OpenFolder),
                ],
            },
            Menu {
                name: "View".into(),
                items: vec![
                    MenuItem::action("Toggle Sidebar", ToggleSidebar),
                    MenuItem::action("Toggle File List", ToggleFileList),
                    MenuItem::action("Toggle Parallel", ToggleParallelMode),
                    MenuItem::separator(),
                    MenuItem::action("Refresh All", RefreshAll),
                ],
            },
        ]);

        TerminalView::bind_keys(app);

        app.activate(true);

        let window = app
            .open_window(WindowOptions::default(), |_window, cx| {
                cx.new(SashikiApp::new)
            })
            .unwrap();

        // Focus the active terminal on startup (ignore if window was closed)
        let _ = window.update(app, |view, window, cx| {
            if let Some(terminal) = view.active_terminal() {
                let focus = terminal.read(cx).focus_handle(cx);
                window.focus(&focus, cx);
            }
        });
    });
}
