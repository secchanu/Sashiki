//! SVG icon helpers for accessible, consistent icon rendering.
//!
//! Icons are 16×16 SVG files. They are embedded in the binary at compile time
//! via `include_bytes!` and extracted to a temp directory at startup if the
//! project `assets/icons/` directory is not available (e.g. release builds).
//! GPUI's `svg().external_path()` renders them, colored by `text_color()`.

use gpui::{SharedString, Svg, svg};
use std::path::PathBuf;
use std::sync::OnceLock;

static ICONS_DIR: OnceLock<PathBuf> = OnceLock::new();

const EMBEDDED_ICONS: &[(&str, &[u8])] = &[
    (
        "arrow_down.svg",
        include_bytes!("../assets/icons/arrow_down.svg"),
    ),
    (
        "arrow_up.svg",
        include_bytes!("../assets/icons/arrow_up.svg"),
    ),
    (
        "check_square.svg",
        include_bytes!("../assets/icons/check_square.svg"),
    ),
    (
        "chevron_down.svg",
        include_bytes!("../assets/icons/chevron_down.svg"),
    ),
    (
        "chevron_right.svg",
        include_bytes!("../assets/icons/chevron_right.svg"),
    ),
    ("close.svg", include_bytes!("../assets/icons/close.svg")),
    (
        "git_branch.svg",
        include_bytes!("../assets/icons/git_branch.svg"),
    ),
    (
        "git_commit.svg",
        include_bytes!("../assets/icons/git_commit.svg"),
    ),
    (
        "layout_grid.svg",
        include_bytes!("../assets/icons/layout_grid.svg"),
    ),
    (
        "layout_single.svg",
        include_bytes!("../assets/icons/layout_single.svg"),
    ),
    ("list.svg", include_bytes!("../assets/icons/list.svg")),
    ("plus.svg", include_bytes!("../assets/icons/plus.svg")),
    (
        "settings.svg",
        include_bytes!("../assets/icons/settings.svg"),
    ),
    ("square.svg", include_bytes!("../assets/icons/square.svg")),
    ("stash.svg", include_bytes!("../assets/icons/stash.svg")),
    (
        "tree_view.svg",
        include_bytes!("../assets/icons/tree_view.svg"),
    ),
    (
        "x_circle.svg",
        include_bytes!("../assets/icons/x_circle.svg"),
    ),
    ("sync.svg", include_bytes!("../assets/icons/sync.svg")),
    (
        "cloud_upload.svg",
        include_bytes!("../assets/icons/cloud_upload.svg"),
    ),
];

/// Initialize the icon directory path. Must be called once at app startup.
///
/// Checks for SVG files in order: CWD `assets/icons/`, exe-relative `assets/icons/`.
/// If neither exists, extracts embedded SVGs to a temp directory.
pub fn init() {
    let candidates = [
        PathBuf::from("assets").join("icons"),
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.join("assets").join("icons")))
            .unwrap_or_default(),
    ];

    for dir in &candidates {
        if dir.join("close.svg").exists() {
            ICONS_DIR.set(dir.clone()).ok();
            return;
        }
    }

    // Extract embedded SVGs to temp directory
    let temp_dir = std::env::temp_dir().join("sashiki-icons");
    if let Err(e) = std::fs::create_dir_all(&temp_dir) {
        eprintln!("Failed to create icon temp dir: {e}");
        ICONS_DIR.set(temp_dir).ok();
        return;
    }
    for (name, data) in EMBEDDED_ICONS {
        let path = temp_dir.join(name);
        // Always overwrite to ensure latest version after updates
        if let Ok(mut f) = std::fs::File::create(&path) {
            let _ = std::io::Write::write_all(&mut f, data);
        }
    }
    ICONS_DIR.set(temp_dir).ok();
}

fn icon(name: &str) -> Svg {
    let dir = ICONS_DIR.get().expect("call icon::init() at startup");
    let path: SharedString = dir.join(name).to_string_lossy().to_string().into();
    svg().external_path(path)
}

// === Navigation ===

pub fn chevron_down() -> Svg {
    icon("chevron_down.svg")
}

pub fn chevron_right() -> Svg {
    icon("chevron_right.svg")
}

// === Actions ===

pub fn close() -> Svg {
    icon("close.svg")
}

pub fn plus() -> Svg {
    icon("plus.svg")
}

pub fn settings() -> Svg {
    icon("settings.svg")
}

// === Checkbox ===

pub fn check_square() -> Svg {
    icon("check_square.svg")
}

pub fn square() -> Svg {
    icon("square.svg")
}

// === Git ===

pub fn git_branch() -> Svg {
    icon("git_branch.svg")
}

pub fn git_commit() -> Svg {
    icon("git_commit.svg")
}

pub fn stash() -> Svg {
    icon("stash.svg")
}

// === View modes ===

pub fn list() -> Svg {
    icon("list.svg")
}

pub fn tree_view() -> Svg {
    icon("tree_view.svg")
}

pub fn layout_grid() -> Svg {
    icon("layout_grid.svg")
}

pub fn layout_single() -> Svg {
    icon("layout_single.svg")
}

// === Stage / unstage ===

pub fn arrow_up() -> Svg {
    icon("arrow_up.svg")
}

pub fn arrow_down() -> Svg {
    icon("arrow_down.svg")
}

pub fn x_circle() -> Svg {
    icon("x_circle.svg")
}

pub fn sync() -> Svg {
    icon("sync.svg")
}

pub fn cloud_upload() -> Svg {
    icon("cloud_upload.svg")
}
