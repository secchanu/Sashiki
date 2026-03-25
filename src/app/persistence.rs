use crate::session::LayoutMode;
use crate::ui::FileListMode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const STATE_VERSION: u32 = 1;
const STATE_FILE_NAME: &str = "app-state.json";
const APP_DIR_NAME: &str = "sashiki";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PersistedAppState {
    pub version: u32,
    pub active_group_path: Option<PathBuf>,
    pub ui: PersistedUiState,
    pub groups: Vec<PersistedGroupState>,
}

impl Default for PersistedAppState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            active_group_path: None,
            ui: PersistedUiState::default(),
            groups: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PersistedUiState {
    pub show_sidebar: bool,
    pub show_file_list: bool,
    pub sidebar_width: f32,
    pub file_view_height: f32,
    pub terminal_split_ratio: f32,
    pub file_list_width: f32,
    pub file_list_mode: PersistedFileListMode,
    pub changes_view_is_tree: bool,
    pub staged_section_collapsed: bool,
    pub unstaged_section_collapsed: bool,
}

impl Default for PersistedUiState {
    fn default() -> Self {
        Self {
            show_sidebar: true,
            show_file_list: true,
            sidebar_width: 224.0,
            file_view_height: 384.0,
            terminal_split_ratio: 0.5,
            file_list_width: 308.0,
            file_list_mode: PersistedFileListMode::default(),
            changes_view_is_tree: true,
            staged_section_collapsed: false,
            unstaged_section_collapsed: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub(crate) struct PersistedGroupState {
    pub project_path: PathBuf,
    pub expanded: bool,
    pub active_session_path: Option<PathBuf>,
    pub layout_mode: PersistedLayoutMode,
    pub parallel_visible_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PersistedLayoutMode {
    #[default]
    Single,
    Parallel,
}

impl From<LayoutMode> for PersistedLayoutMode {
    fn from(value: LayoutMode) -> Self {
        match value {
            LayoutMode::Single => PersistedLayoutMode::Single,
            LayoutMode::Parallel => PersistedLayoutMode::Parallel,
        }
    }
}

impl From<PersistedLayoutMode> for LayoutMode {
    fn from(value: PersistedLayoutMode) -> Self {
        match value {
            PersistedLayoutMode::Single => LayoutMode::Single,
            PersistedLayoutMode::Parallel => LayoutMode::Parallel,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PersistedFileListMode {
    #[default]
    Changes,
    AllFiles,
}

impl From<FileListMode> for PersistedFileListMode {
    fn from(value: FileListMode) -> Self {
        match value {
            FileListMode::Changes => PersistedFileListMode::Changes,
            FileListMode::AllFiles => PersistedFileListMode::AllFiles,
        }
    }
}

impl From<PersistedFileListMode> for FileListMode {
    fn from(value: PersistedFileListMode) -> Self {
        match value {
            PersistedFileListMode::Changes => FileListMode::Changes,
            PersistedFileListMode::AllFiles => FileListMode::AllFiles,
        }
    }
}

fn state_file_path() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)?;
    Some(home.join(format!(".{APP_DIR_NAME}")).join(STATE_FILE_NAME))
}

pub(crate) fn load_app_state() -> Option<PersistedAppState> {
    let path = state_file_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<PersistedAppState>(&raw).ok()
}

pub(crate) fn save_app_state(state: &PersistedAppState) -> std::io::Result<()> {
    let Some(path) = state_file_path() else {
        return Ok(());
    };

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut state = state.clone();
    state.version = STATE_VERSION;
    let body = serde_json::to_vec_pretty(&state)
        .map_err(|err| std::io::Error::other(format!("serialize state: {err}")))?;
    std::fs::write(path, body)
}
