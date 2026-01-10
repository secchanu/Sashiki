//! Side-by-side diff view with resizable splitter

use crate::diff::{DiffLineType, SideBySideLine};
use crate::ui::Theme;
use egui::{self, Pos2, Rect, Ui, Vec2};

const SPLITTER_WIDTH: f32 = 6.0;

pub struct DiffView {
    line_height: f32,
    gutter_width: f32,
    split_ratio: f32,
    dragging: bool,
}

impl DiffView {
    pub fn new() -> Self {
        Self {
            line_height: 18.0,
            gutter_width: 50.0,
            split_ratio: 0.5,
            dragging: false,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        side_by_side: &[SideBySideLine],
        old_path: Option<&str>,
        new_path: Option<&str>,
        panel_rect: Rect,
    ) -> DiffViewResponse {
        let response = DiffViewResponse::default();
        let total_width = ui.available_width();
        let left_width = (total_width - SPLITTER_WIDTH) * self.split_ratio;
        let right_width = (total_width - SPLITTER_WIDTH) * (1.0 - self.split_ratio);

        // Splitter spans full panel height (passed from parent CentralPanel)
        let available_rect = ui.available_rect_before_wrap();
        let splitter_x = available_rect.min.x + left_width;
        let splitter_rect = Rect::from_min_size(
            Pos2::new(splitter_x, panel_rect.min.y),
            Vec2::new(SPLITTER_WIDTH, panel_rect.height()),
        );

        // Handle splitter drag using ctx.input() to avoid clipping issues
        let ctx = ui.ctx();
        let pointer_pos = ctx.input(|i| i.pointer.hover_pos());
        let is_hovered = pointer_pos.map(|p| splitter_rect.contains(p)).unwrap_or(false);
        let primary_down = ctx.input(|i| i.pointer.primary_down());
        let primary_released = ctx.input(|i| i.pointer.primary_released());
        let pointer_delta = ctx.input(|i| i.pointer.delta());

        // Start dragging when clicked on splitter
        if is_hovered && primary_down && !self.dragging {
            self.dragging = true;
        }

        // Stop dragging when mouse released
        if primary_released {
            self.dragging = false;
        }

        // Apply drag delta
        if self.dragging {
            let delta = pointer_delta.x;
            let new_ratio = self.split_ratio + delta / total_width;
            self.split_ratio = new_ratio.clamp(0.2, 0.8);
        }

        let is_active = is_hovered || self.dragging;
        if is_active {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }

        let ctx = ui.ctx().clone();
        let splitter_layer_id = egui::LayerId::new(
            egui::Order::Foreground,
            ui.id().with("splitter_layer"),
        );

        egui::Frame::none()
            .fill(theme.bg_primary)
            .show(ui, |ui| {
                // Header
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(left_width, 24.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(old_path.unwrap_or("Original"))
                                    .size(11.0)
                                    .color(theme.diff_delete_fg),
                            );
                        },
                    );

                    ui.add_space(SPLITTER_WIDTH);

                    ui.allocate_ui_with_layout(
                        egui::vec2(right_width, 24.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(new_path.unwrap_or("Modified"))
                                    .size(11.0)
                                    .color(theme.diff_add_fg),
                            );
                        },
                    );
                });

                ui.separator();

                // Content
                egui::ScrollArea::both()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        let content_height = side_by_side.len() as f32 * self.line_height;
                        ui.set_min_height(content_height);

                        let content_start = ui.min_rect().min;

                        self.draw_background_blocks(
                            ui.painter(),
                            theme,
                            side_by_side,
                            content_start,
                            left_width,
                            right_width,
                        );

                        for (idx, line) in side_by_side.iter().enumerate() {
                            let y = content_start.y + idx as f32 * self.line_height;

                            self.draw_line_content(
                                ui.painter(),
                                theme,
                                content_start.x,
                                y,
                                line.left_line_num,
                                line.left_content.as_deref(),
                                &line.left_type,
                            );

                            self.draw_line_content(
                                ui.painter(),
                                theme,
                                content_start.x + left_width + SPLITTER_WIDTH,
                                y,
                                line.right_line_num,
                                line.right_content.as_deref(),
                                &line.right_type,
                            );
                        }

                        ui.allocate_space(egui::vec2(
                            left_width + SPLITTER_WIDTH + right_width,
                            content_height,
                        ));
                    });
            });

        // Draw splitter on foreground layer
        let painter = ctx.layer_painter(splitter_layer_id);
        let center_x = splitter_rect.center().x;

        if is_active {
            painter.rect_filled(
                splitter_rect,
                0.0,
                theme.bg_secondary.gamma_multiply(0.8),
            );
        }

        painter.vline(
            center_x,
            splitter_rect.y_range(),
            egui::Stroke::new(2.0, theme.border),
        );

        if is_active {
            let grip_color = theme.text_muted;
            let dot_radius = 2.0;
            let dot_spacing = 8.0;
            let start_y = splitter_rect.center().y - 2.0 * dot_spacing;

            for i in 0..5 {
                let y = start_y + i as f32 * dot_spacing;
                painter.circle_filled(Pos2::new(center_x, y), dot_radius, grip_color);
            }
        }

        response
    }

    fn draw_background_blocks(
        &self,
        painter: &egui::Painter,
        theme: &Theme,
        lines: &[SideBySideLine],
        start: Pos2,
        left_width: f32,
        right_width: f32,
    ) {
        if lines.is_empty() {
            return;
        }

        let mut left_block: Option<(usize, DiffLineType)> = None;
        let mut right_block: Option<(usize, DiffLineType)> = None;

        for (idx, line) in lines.iter().enumerate() {
            // Left pane
            match &left_block {
                Some((_, block_type)) if *block_type == line.left_type => {}
                Some((block_idx, block_type)) => {
                    self.draw_block(
                        painter,
                        theme,
                        start.x,
                        start.y + *block_idx as f32 * self.line_height,
                        left_width,
                        (idx - block_idx) as f32 * self.line_height,
                        block_type,
                    );
                    left_block = Some((idx, line.left_type.clone()));
                }
                None => {
                    left_block = Some((idx, line.left_type.clone()));
                }
            }

            // Right pane
            match &right_block {
                Some((_, block_type)) if *block_type == line.right_type => {}
                Some((block_idx, block_type)) => {
                    self.draw_block(
                        painter,
                        theme,
                        start.x + left_width + SPLITTER_WIDTH,
                        start.y + *block_idx as f32 * self.line_height,
                        right_width,
                        (idx - block_idx) as f32 * self.line_height,
                        block_type,
                    );
                    right_block = Some((idx, line.right_type.clone()));
                }
                None => {
                    right_block = Some((idx, line.right_type.clone()));
                }
            }
        }

        // Final blocks
        if let Some((block_idx, block_type)) = left_block {
            self.draw_block(
                painter,
                theme,
                start.x,
                start.y + block_idx as f32 * self.line_height,
                left_width,
                (lines.len() - block_idx) as f32 * self.line_height,
                &block_type,
            );
        }

        if let Some((block_idx, block_type)) = right_block {
            self.draw_block(
                painter,
                theme,
                start.x + left_width + SPLITTER_WIDTH,
                start.y + block_idx as f32 * self.line_height,
                right_width,
                (lines.len() - block_idx) as f32 * self.line_height,
                &block_type,
            );
        }
    }

    fn draw_block(
        &self,
        painter: &egui::Painter,
        theme: &Theme,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        line_type: &DiffLineType,
    ) {
        let bg_color = match line_type {
            DiffLineType::Insert => theme.diff_add_bg,
            DiffLineType::Delete => theme.diff_delete_bg,
            DiffLineType::Equal => return,
        };

        painter.rect_filled(
            Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, height)),
            0.0,
            bg_color,
        );
    }

    fn draw_line_content(
        &self,
        painter: &egui::Painter,
        theme: &Theme,
        x: f32,
        y: f32,
        line_num: Option<usize>,
        content: Option<&str>,
        line_type: &DiffLineType,
    ) {
        // Gutter
        let gutter_rect = Rect::from_min_size(
            Pos2::new(x, y),
            Vec2::new(self.gutter_width, self.line_height),
        );
        painter.rect_filled(gutter_rect, 0.0, theme.bg_secondary.gamma_multiply(0.5));

        if let Some(num) = line_num {
            painter.text(
                gutter_rect.right_center() - egui::vec2(4.0, 0.0),
                egui::Align2::RIGHT_CENTER,
                format!("{:>3}", num),
                egui::FontId::monospace(11.0),
                theme.line_number,
            );
        }

        // Content
        if let Some(text) = content {
            let text_color = match line_type {
                DiffLineType::Insert => theme.diff_add_fg,
                DiffLineType::Delete => theme.diff_delete_fg,
                DiffLineType::Equal => theme.text_primary,
            };

            painter.text(
                Pos2::new(x + self.gutter_width + 8.0, y + self.line_height / 2.0),
                egui::Align2::LEFT_CENTER,
                text.trim_end_matches('\n'),
                egui::FontId::monospace(12.0),
                text_color,
            );
        }
    }

    pub fn reset_split(&mut self) {
        self.split_ratio = 0.5;
    }

    pub fn split_ratio(&self) -> f32 {
        self.split_ratio
    }
}

impl Default for DiffView {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
pub struct DiffViewResponse {}

pub fn render_diff_stats(ui: &mut Ui, theme: &Theme, stats: &crate::diff::DiffStats) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("+{}", stats.additions))
                .color(theme.diff_add_fg)
                .size(12.0),
        );
        ui.label(
            egui::RichText::new(format!("-{}", stats.deletions))
                .color(theme.diff_delete_fg)
                .size(12.0),
        );
        ui.label(
            egui::RichText::new(format!("~{}", stats.unchanged))
                .color(theme.text_muted)
                .size(12.0),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_view_creation() {
        let view = DiffView::new();
        assert_eq!(view.split_ratio(), 0.5);
        assert_eq!(view.line_height, 18.0);
    }

    #[test]
    fn test_split_ratio_reset() {
        let mut view = DiffView::new();
        view.split_ratio = 0.7;
        view.reset_split();
        assert_eq!(view.split_ratio(), 0.5);
    }
}
