//! Pseudo terminal process management.
//!
//! libghostty-vt only emulates the terminal state machine, so the pty itself
//! is provided by `portable-pty` (ConPTY on Windows, `openpty` elsewhere).

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::path::PathBuf;

/// Terminfo entry reported to child processes. `xterm-256color` is chosen
/// because it is present on every supported platform, unlike terminal-specific
/// entries that would have to be installed alongside the app.
const TERM: &str = "xterm-256color";

pub struct Pty {
    pub master: Box<dyn MasterPty + Send>,
    pub writer: Box<dyn Write + Send>,
    pub reader: Option<Box<dyn Read + Send>>,
    pub killer: Box<dyn ChildKiller + Send + Sync>,
}

/// The shell to start. On Windows the default program would be `cmd.exe`,
/// so PowerShell is requested explicitly instead.
fn default_command() -> CommandBuilder {
    #[cfg(windows)]
    {
        CommandBuilder::new("powershell")
    }
    #[cfg(not(windows))]
    {
        CommandBuilder::new_default_prog()
    }
}

pub fn spawn(working_directory: Option<PathBuf>, size: PtySize) -> anyhow::Result<Pty> {
    let pair = native_pty_system().openpty(size)?;

    let mut cmd = default_command();
    if let Some(dir) = working_directory {
        cmd.cwd(dir);
    }
    cmd.env("TERM", TERM);
    cmd.env("COLORTERM", "truecolor");

    let mut child = pair.slave.spawn_command(cmd)?;
    // The slave must be closed here, otherwise the reader never sees EOF
    // after the child exits.
    drop(pair.slave);

    let killer = child.clone_killer();
    let reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;

    // Reap the child so it does not linger as a zombie. Exit is reported to
    // the VT thread through the reader reaching EOF.
    std::thread::Builder::new()
        .name("sashiki-pty-wait".into())
        .spawn(move || {
            let _ = child.wait();
        })?;

    Ok(Pty {
        master: pair.master,
        writer,
        reader: Some(reader),
        killer,
    })
}
