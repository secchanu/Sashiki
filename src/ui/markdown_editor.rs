//! Markdown editor component for editing AI instructions
//!
//! A minimal editor specifically for markdown files (.md).
//! Used for editing CLAUDE.md, rules, and other instruction files.

use crate::ui::Theme;
use egui::{self, Ui};
use std::path::{Path, PathBuf};

/// Markdown editor state
pub struct MarkdownEditor {
    /// Current file being edited
    file_path: Option<PathBuf>,
    /// Content being edited
    content: String,
    /// Original content (for dirty checking)
    original_content: String,
    /// Whether editor is visible
    visible: bool,
    /// Whether content has been modified
    modified: bool,
    /// Error message to display
    error_message: Option<String>,
    /// Success message to display
    success_message: Option<String>,
}

impl MarkdownEditor {
    pub fn new() -> Self {
        Self {
            file_path: None,
            content: String::new(),
            original_content: String::new(),
            visible: false,
            modified: false,
            error_message: None,
            success_message: None,
        }
    }

    /// Open a markdown file for editing
    pub fn open(&mut self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();

        // Only allow markdown files
        if !Self::is_markdown_file(path) {
            return Err("Only markdown files (.md) can be edited".to_string());
        }

        self.open_any(path)
    }

    /// Open any text file for editing
    pub fn open_any(&mut self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();

        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        self.file_path = Some(path.to_path_buf());
        self.content = content.clone();
        self.original_content = content;
        self.modified = false;
        self.visible = true;
        self.error_message = None;
        self.success_message = None;

        Ok(())
    }

    /// Create a new markdown file
    pub fn create_new(&mut self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();

        if !Self::is_markdown_file(path) {
            return Err("Only markdown files (.md) can be created".to_string());
        }

        self.file_path = Some(path.to_path_buf());
        self.content = String::new();
        self.original_content = String::new();
        self.modified = false;
        self.visible = true;
        self.error_message = None;
        self.success_message = None;

        Ok(())
    }

    /// Save the current content
    pub fn save(&mut self) -> Result<(), String> {
        let path = self.file_path.as_ref()
            .ok_or_else(|| "No file open".to_string())?;

        std::fs::write(path, &self.content)
            .map_err(|e| format!("Failed to save file: {}", e))?;

        self.original_content = self.content.clone();
        self.modified = false;
        self.success_message = Some("Saved".to_string());
        self.error_message = None;

        Ok(())
    }

    /// Close the editor
    pub fn close(&mut self) {
        self.file_path = None;
        self.content.clear();
        self.original_content.clear();
        self.visible = false;
        self.modified = false;
        self.error_message = None;
        self.success_message = None;
    }

    /// Check if content has been modified
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Check if editor is visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Get current file path
    pub fn file_path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    /// Check if a path is a markdown file
    fn is_markdown_file(path: &Path) -> bool {
        path.extension()
            .map(|ext| ext.eq_ignore_ascii_case("md"))
            .unwrap_or(false)
    }

    /// Show the editor UI
    pub fn show(&mut self, ui: &mut Ui, theme: &Theme) -> MarkdownEditorResponse {
        let mut response = MarkdownEditorResponse::default();

        if !self.visible {
            return response;
        }

        egui::Frame::none()
            .fill(theme.bg_primary)
            .show(ui, |ui| {
                // Header with file name and controls
                ui.horizontal(|ui| {
                    ui.add_space(8.0);

                    // File name
                    if let Some(ref path) = self.file_path {
                        let filename = path.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Untitled.md".to_string());

                        let display = if self.modified {
                            format!("{}*", filename)
                        } else {
                            filename
                        };

                        ui.label(
                            egui::RichText::new(display)
                                .size(12.0)
                                .color(theme.text_primary),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Close button
                        if ui.button("Close").clicked() {
                            if self.modified {
                                response.wants_close_confirmation = true;
                            } else {
                                self.close();
                                response.closed = true;
                            }
                        }

                        // Save button
                        if ui.add_enabled(self.modified, egui::Button::new("Save (Ctrl+S)")).clicked() {
                            if let Err(e) = self.save() {
                                self.error_message = Some(e);
                            } else {
                                response.saved = true;
                            }
                        }
                    });
                });

                // Status messages
                if let Some(ref msg) = self.error_message {
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(msg)
                                .size(11.0)
                                .color(theme.diff_delete_fg),
                        );
                    });
                }

                if let Some(ref msg) = self.success_message {
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(msg)
                                .size(11.0)
                                .color(theme.diff_add_fg),
                        );
                    });
                    // Clear success message after showing
                    self.success_message = None;
                }

                ui.separator();

                // Editor area
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        let text_edit = egui::TextEdit::multiline(&mut self.content)
                            .font(egui::FontId::monospace(13.0))
                            .code_editor()
                            .desired_width(f32::INFINITY)
                            .min_size(ui.available_size());

                        let edit_response = ui.add(text_edit);

                        if edit_response.changed() {
                            self.modified = self.content != self.original_content;
                            self.error_message = None;
                        }
                    });
            });

        response
    }

    /// Handle keyboard shortcuts
    pub fn handle_keyboard(&mut self, ctx: &egui::Context) -> bool {
        if !self.visible {
            return false;
        }

        let mut handled = false;

        ctx.input(|i| {
            // Ctrl+S: Save
            if i.modifiers.ctrl && i.key_pressed(egui::Key::S) {
                if let Err(e) = self.save() {
                    self.error_message = Some(e);
                }
                handled = true;
            }

            // Escape: Close (if not modified)
            if i.key_pressed(egui::Key::Escape) && !self.modified {
                self.close();
                handled = true;
            }
        });

        handled
    }
}

impl Default for MarkdownEditor {
    fn default() -> Self {
        Self::new()
    }
}

/// Response from the markdown editor
#[derive(Default)]
pub struct MarkdownEditorResponse {
    /// Editor was closed
    pub closed: bool,
    /// Content was saved
    pub saved: bool,
    /// Close was requested but there are unsaved changes
    pub wants_close_confirmation: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_markdown_editor_creation() {
        let editor = MarkdownEditor::new();
        assert!(!editor.is_visible());
        assert!(!editor.is_modified());
        assert!(editor.file_path().is_none());
    }

    #[test]
    fn test_is_markdown_file() {
        assert!(MarkdownEditor::is_markdown_file(Path::new("README.md")));
        assert!(MarkdownEditor::is_markdown_file(Path::new("CLAUDE.MD")));
        assert!(MarkdownEditor::is_markdown_file(Path::new("/path/to/file.md")));
        assert!(!MarkdownEditor::is_markdown_file(Path::new("file.txt")));
        assert!(!MarkdownEditor::is_markdown_file(Path::new("file.rs")));
    }

    #[test]
    fn test_open_markdown_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.md");
        std::fs::write(&file_path, "# Test\n\nContent here.").unwrap();

        let mut editor = MarkdownEditor::new();
        let result = editor.open(&file_path);

        assert!(result.is_ok());
        assert!(editor.is_visible());
        assert!(!editor.is_modified());
        assert_eq!(editor.content, "# Test\n\nContent here.");
    }

    #[test]
    fn test_reject_non_markdown() {
        let mut editor = MarkdownEditor::new();
        let result = editor.open(Path::new("/tmp/test.txt"));

        assert!(result.is_err());
        assert!(!editor.is_visible());
    }

    #[test]
    fn test_save_markdown() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.md");
        std::fs::write(&file_path, "Original").unwrap();

        let mut editor = MarkdownEditor::new();
        editor.open(&file_path).unwrap();

        editor.content = "Modified content".to_string();
        editor.modified = true;

        let result = editor.save();
        assert!(result.is_ok());
        assert!(!editor.is_modified());

        let saved_content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(saved_content, "Modified content");
    }

    #[test]
    fn test_close_editor() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.md");
        std::fs::write(&file_path, "Content").unwrap();

        let mut editor = MarkdownEditor::new();
        editor.open(&file_path).unwrap();
        assert!(editor.is_visible());

        editor.close();
        assert!(!editor.is_visible());
        assert!(editor.file_path().is_none());
        assert!(editor.content.is_empty());
    }

    #[test]
    fn test_create_new() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("new.md");

        let mut editor = MarkdownEditor::new();
        let result = editor.create_new(&file_path);

        assert!(result.is_ok());
        assert!(editor.is_visible());
        assert!(editor.content.is_empty());
    }
}
