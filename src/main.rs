//! Sashiki - A lightweight cockpit for AI agents

mod action;
mod app;
mod buffer;
mod config;
mod diff;
mod font;
mod git;
mod session;
mod terminal;
mod ui;

use app::{App, ViewState};
use config::Config;
use eframe::egui;
use session::SessionStatus;
use ui::{render_diff_stats, FileListMode};

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("Starting Sashiki");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("Sashiki"),
        ..Default::default()
    };

    eframe::run_native(
        "Sashiki",
        options,
        Box::new(|cc| Ok(Box::new(SashikiApp::new(cc)))),
    )
}

struct SashikiApp {
    app: App,
}

impl SashikiApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::setup_fonts(&cc.egui_ctx);

        let config = Config::load_or_default();
        let mut app = App::new(config);

        if let Ok(cwd) = std::env::current_dir() {
            let _ = app.open_repository(&cwd);
        }

        Self { app }
    }

    fn setup_fonts(ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();

        // Load preferred monospace font (e.g., JetBrains Mono, Fira Code)
        if let Some((name, data)) = font::find_monospace_font() {
            fonts
                .font_data
                .insert("mono_font".to_owned(), egui::FontData::from_owned(data));

            // Insert at the beginning of Monospace family for priority
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "mono_font".to_owned());

            tracing::info!("Loaded monospace font: {}", name);
        }

        // Load Japanese font as fallback for CJK characters
        if let Some((name, data)) = font::find_japanese_font() {
            fonts
                .font_data
                .insert("jp_font".to_owned(), egui::FontData::from_owned(data));

            // Add to both families as fallback (at the end)
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("jp_font".to_owned());

            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("jp_font".to_owned());

            tracing::info!("Loaded Japanese font: {}", name);
        }

        ctx.set_fonts(fonts);
    }
}

impl eframe::App for SashikiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.handle_keyboard(ctx);
        self.app.update_sessions();
        self.app.process_pending_actions();

        self.render_top_panel(ctx);
        self.render_status_bar(ctx);
        self.render_terminal_panel(ctx);
        self.render_sidebar(ctx);
        self.render_main_content(ctx);

        if self.app.show_open_dialog {
            self.render_open_dialog(ctx);
        }

        let any_running = self.app.sessions.sessions().iter().any(|s| s.is_terminal_running());
        if any_running {
            ctx.request_repaint();
        }
    }
}

impl SashikiApp {
    fn render_top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open Repository (Ctrl+O)").clicked() {
                        self.app.show_open_dialog = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit (Ctrl+Q)").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.button("Toggle Terminal (Ctrl+`)").clicked() {
                        self.app.toggle_terminal();
                        ui.close_menu();
                    }
                    if ui.button("Toggle Split Direction (Ctrl+\\)").clicked() {
                        self.app.toggle_split_direction();
                        ui.close_menu();
                    }
                });
            });
        });
    }

    fn render_status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(28.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(8.0);

                    let statuses = self.app.get_session_statuses();
                    let active_idx = self.app.sessions.active_index();

                    for (idx, (name, status)) in statuses.iter().enumerate() {
                        let is_active = idx == active_idx;
                        let color = if is_active {
                            self.app.theme.accent
                        } else {
                            self.app.theme.text_muted
                        };

                        let text = format!("{} {}", status.symbol(), name);
                        let label = egui::RichText::new(&text).size(11.0).color(color);

                        if ui
                            .add(egui::Label::new(label).sense(egui::Sense::click()))
                            .clicked()
                        {
                            self.app.select_session(idx);
                        }

                        if idx < statuses.len() - 1 {
                            ui.separator();
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        if let Some(ref diff) = self.app.diff_result {
                            render_diff_stats(ui, &self.app.theme, &diff.stats);
                        }
                    });
                });
            });
    }

    fn render_terminal_panel(&mut self, ctx: &egui::Context) {
        if !self.app.terminal_visible {
            return;
        }

        if let Some(session) = self.app.sessions.active_mut() {
            egui::TopBottomPanel::bottom("terminal_panel")
                .exact_height(self.app.terminal_view.height())
                .resizable(false)
                .show(ctx, |ui| {
                    self.app.terminal_view.show(ui, &self.app.theme, &mut session.terminal);
                });
        }
    }

    fn render_sidebar(&mut self, ctx: &egui::Context) {
        let theme = self.app.theme.clone();

        egui::SidePanel::left("sidebar")
            .exact_width(self.app.sidebar.width())
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Sessions")
                            .size(12.0)
                            .color(theme.text_secondary),
                    );
                });
                ui.separator();

                let active_idx = self.app.sessions.active_index();
                let sessions: Vec<_> = self.app.sessions.sessions()
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (i, s.display_name().to_string(), s.status, s.worktree.branch.clone()))
                    .collect();

                let mut session_to_select: Option<usize> = None;

                for (idx, name, status, branch) in sessions {
                    let is_active = idx == active_idx;
                    let bg = if is_active { theme.bg_tertiary } else { theme.bg_secondary };

                    egui::Frame::none()
                        .fill(bg)
                        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let status_color = match status {
                                    SessionStatus::Idle => theme.text_muted,
                                    SessionStatus::Running => theme.accent,
                                    SessionStatus::Completed => theme.diff_add_fg,
                                    SessionStatus::Error => theme.diff_delete_fg,
                                };
                                ui.label(
                                    egui::RichText::new(status.symbol())
                                        .size(12.0)
                                        .color(status_color),
                                );

                                let name_color = if is_active { theme.text_primary } else { theme.text_secondary };
                                if ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&name).size(12.0).color(name_color),
                                    ).sense(egui::Sense::click()),
                                ).clicked() {
                                    session_to_select = Some(idx);
                                }

                                if idx < 9 {
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(
                                            egui::RichText::new(format!("^{}", idx + 1))
                                                .size(10.0)
                                                .color(theme.text_muted),
                                        );
                                    });
                                }
                            });

                            if let Some(ref branch) = branch {
                                if branch != &name {
                                    ui.horizontal(|ui| {
                                        ui.add_space(16.0);
                                        ui.label(
                                            egui::RichText::new(format!("⎇ {}", branch))
                                                .size(10.0)
                                                .color(theme.text_muted),
                                        );
                                    });
                                }
                            }
                        });
                }

                if let Some(idx) = session_to_select {
                    self.app.select_session(idx);
                }

                ui.separator();

                ui.horizontal(|ui| {
                    ui.add_space(8.0);

                    let source_label = self.app.file_tree.source.label();
                    if ui.add(
                        egui::Button::new(
                            egui::RichText::new(source_label)
                                .size(12.0)
                                .color(theme.text_secondary),
                        ).frame(false),
                    ).on_hover_text("Toggle between Git changes and all files").clicked() {
                        self.app.file_tree.source.toggle();
                        self.app.file_tree.invalidate_cache();
                    }

                    if matches!(self.app.file_tree.source, ui::FileSource::Git) {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(8.0);
                            let mode = &self.app.file_tree.mode;
                            if ui.add(
                                egui::Button::new(
                                    egui::RichText::new(mode.icon())
                                        .size(12.0)
                                        .color(theme.text_muted),
                                ).frame(false),
                            ).on_hover_text(format!(
                                "Switch to {} view",
                                if *mode == FileListMode::Flat { "tree" } else { "list" }
                            )).clicked() {
                                self.app.file_tree.mode.toggle();
                            }
                        });
                    }
                });

                let changed_files = self.app.get_changed_files();

                if let Some(worktree) = self.app.current_worktree() {
                    let wt_path = worktree.path.clone();
                    self.app.file_tree.load_files(&wt_path, &changed_files);
                }

                let entries = self.app.file_tree.get_entries(&changed_files);

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let response = self.app.file_tree.show(ui, &theme, &entries);

                    if let Some(path) = response.show_diff {
                        let _ = self.app.show_diff(&path);
                    }
                    if let Some(path) = response.open_file {
                        let _ = self.app.open_file(&path);
                    }
                    if let Some(path) = response.insert_to_terminal {
                        let action = self.app.create_path_action(&path, None);
                        self.app.process_action(action);
                    }
                });
            });
    }

    fn render_main_content(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(self.app.theme.bg_primary))
            .show(ctx, |ui| {
                let panel_rect = ui.max_rect();

                let current_path = match &self.app.current_view {
                    ViewState::File { path, .. } => Some(path.clone()),
                    ViewState::Diff { path } => Some(path.clone()),
                    _ => None,
                };

                if let Some(path) = &current_path {
                    let theme = self.app.theme.clone();
                    let path_clone = path.clone();

                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(
                                egui::RichText::new("Edit").color(theme.accent),
                            )).on_hover_text("Edit this file").clicked() {
                                let _ = self.app.edit_file(&path_clone);
                            }
                            ui.add_space(8.0);
                        });
                    });
                    ui.separator();
                }

                match &self.app.current_view {
                    ViewState::Empty => self.render_welcome(ui),
                    ViewState::File { path, buffer } => {
                        let path_str = path.display().to_string();
                        self.app.text_view.show(ui, &self.app.theme, buffer, Some(&path_str));
                    }
                    ViewState::Diff { path } => {
                        self.app.diff_view.show(
                            ui,
                            &self.app.theme,
                            &self.app.side_by_side,
                            Some("HEAD"),
                            Some(&path.display().to_string()),
                            panel_rect,
                        );
                    }
                    ViewState::ChangedFiles => self.render_changed_files_view(ui),
                    ViewState::Edit { .. } => self.render_editor(ui),
                }
            });
    }

    fn render_editor(&mut self, ui: &mut egui::Ui) {
        let theme = self.app.theme.clone();
        self.app.markdown_editor.handle_keyboard(ui.ctx());

        let response = self.app.markdown_editor.show(ui, &theme);

        if response.closed {
            self.app.current_view = ViewState::ChangedFiles;
        }
    }

    fn render_welcome(&mut self, ui: &mut egui::Ui) {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 3.0);

                ui.label(
                    egui::RichText::new("Sashiki")
                        .size(28.0)
                        .color(self.app.theme.text_muted),
                );

                ui.add_space(8.0);

                ui.label(
                    egui::RichText::new("Lightweight cockpit for AI agents")
                        .size(14.0)
                        .color(self.app.theme.text_muted),
                );

                ui.add_space(24.0);

                if self.app.git_manager.is_none() {
                    ui.label(
                        egui::RichText::new("Press Ctrl+O to open a repository")
                            .size(12.0)
                            .color(self.app.theme.text_secondary),
                    );
                }
            });
        });
    }

    fn render_changed_files_view(&mut self, ui: &mut egui::Ui) {
        let theme = self.app.theme.clone();
        let changed_files = self.app.get_changed_files();

        if changed_files.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new("No changes detected")
                        .size(16.0)
                        .color(theme.text_muted),
                );
            });
            return;
        }

        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.label(
                egui::RichText::new(format!("{} changed files", changed_files.len()))
                    .size(14.0)
                    .color(theme.text_secondary),
            );
        });

        ui.add_space(8.0);
        ui.separator();

        let mut diff_to_show: Option<std::path::PathBuf> = None;
        let mut path_to_insert: Option<std::path::PathBuf> = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for file in &changed_files {
                let color = match file.status {
                    git::FileStatusType::New => theme.diff_add_fg,
                    git::FileStatusType::Modified => theme.accent,
                    git::FileStatusType::Deleted => theme.diff_delete_fg,
                    _ => theme.text_secondary,
                };

                let status_char = match file.status {
                    git::FileStatusType::New => "+",
                    git::FileStatusType::Modified => "~",
                    git::FileStatusType::Deleted => "-",
                    git::FileStatusType::Renamed => "R",
                    git::FileStatusType::Untracked => "?",
                };

                let path_str = file.path.display().to_string();

                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(16.0, 4.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(status_char)
                                    .size(12.0)
                                    .color(color)
                                    .monospace(),
                            );

                            ui.add_space(8.0);

                            let response = ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&path_str)
                                        .size(12.0)
                                        .color(theme.text_primary)
                                        .monospace(),
                                ).sense(egui::Sense::click()),
                            );

                            if response.clicked() {
                                diff_to_show = Some(file.path.clone());
                            }

                            if response.secondary_clicked() {
                                path_to_insert = Some(file.path.clone());
                            }

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add(
                                    egui::Button::new(
                                        egui::RichText::new("→")
                                            .size(12.0)
                                            .color(theme.text_muted),
                                    ).frame(false),
                                ).on_hover_text("Insert path to terminal").clicked() {
                                    path_to_insert = Some(file.path.clone());
                                }
                            });
                        });
                    });
            }
        });

        if let Some(path) = diff_to_show {
            let _ = self.app.show_diff(&path);
        }
        if let Some(path) = path_to_insert {
            let action = self.app.create_path_action(&path, None);
            self.app.process_action(action);
        }
    }

    fn render_open_dialog(&mut self, ctx: &egui::Context) {
        egui::Window::new("Open Repository")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Path:");
                    ui.add(egui::TextEdit::singleline(&mut self.app.dialog_path).desired_width(300.0));
                });

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    if ui.button("Open").clicked() {
                        let path = self.app.dialog_path.clone();
                        if !path.is_empty() {
                            let _ = self.app.open_repository(&path);
                        }
                        self.app.show_open_dialog = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.app.show_open_dialog = false;
                    }
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_loading() {
        let config = Config::load_or_default();
        assert!(matches!(
            config.theme,
            crate::config::Theme::Dark | crate::config::Theme::Light
        ));
    }

    #[test]
    fn test_app_default() {
        let app = App::default();
        assert!(app.sessions.is_empty());
    }
}
