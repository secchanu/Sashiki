//! Text view component with virtual scrolling

use crate::buffer::{TextBuffer, VirtualViewport};
use crate::ui::Theme;
use egui::{self, Ui};

pub struct TextView {
    viewport: VirtualViewport,
    line_height: f32,
    gutter_width: f32,
}

impl TextView {
    pub fn new() -> Self {
        Self {
            viewport: VirtualViewport::new(0, 50),
            line_height: 18.0,
            gutter_width: 60.0,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        buffer: &TextBuffer,
        file_path: Option<&str>,
    ) -> TextViewResponse {
        let mut response = TextViewResponse::default();

        let available_height = ui.available_height();
        let visible_lines = (available_height / self.line_height).ceil() as usize + 1;
        self.viewport.visible_lines = visible_lines;

        egui::Frame::none()
            .fill(theme.bg_primary)
            .show(ui, |ui| {
                if let Some(path) = file_path {
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(path)
                                .size(12.0)
                                .color(theme.text_secondary),
                        );
                    });
                    ui.separator();
                }

                let content_height = buffer.len_lines() as f32 * self.line_height;

                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show_viewport(ui, |ui, viewport_rect| {
                        let scroll_top = viewport_rect.min.y;
                        let new_start_line = (scroll_top / self.line_height).floor() as usize;
                        self.viewport.start_line = new_start_line;

                        ui.set_min_height(content_height);

                        let visible = self.viewport.get_visible_lines(buffer);

                        for (line_num, line_content) in &visible {
                            let y_offset = *line_num as f32 * self.line_height;
                            self.draw_line(ui, theme, *line_num, line_content, y_offset);
                        }
                    });
            });

        response.visible_start = self.viewport.start_line;
        response.visible_end = self.viewport.end_line(buffer.len_lines());
        response
    }

    fn draw_line(
        &self,
        ui: &mut Ui,
        theme: &Theme,
        line_num: usize,
        content: &str,
        y_offset: f32,
    ) {
        let rect = ui.max_rect();
        let line_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(0.0, y_offset),
            egui::vec2(rect.width(), self.line_height),
        );

        let gutter_rect = egui::Rect::from_min_size(
            line_rect.min,
            egui::vec2(self.gutter_width, self.line_height),
        );

        ui.painter()
            .rect_filled(gutter_rect, 0.0, theme.bg_secondary);

        let line_num_text = format!("{:>4}", line_num + 1);
        ui.painter().text(
            gutter_rect.right_center() - egui::vec2(8.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            &line_num_text,
            egui::FontId::monospace(12.0),
            theme.line_number,
        );

        let content_pos = line_rect.left_center() + egui::vec2(self.gutter_width + 8.0, 0.0);
        let display_content = content.trim_end_matches('\n');

        ui.painter().text(
            content_pos,
            egui::Align2::LEFT_CENTER,
            display_content,
            egui::FontId::monospace(12.0),
            theme.text_primary,
        );
    }

    pub fn scroll_to_line(&mut self, line: usize, total_lines: usize) {
        self.viewport.scroll_to(line, total_lines);
    }

    pub fn scroll_by(&mut self, delta: isize, total_lines: usize) {
        self.viewport.scroll_by(delta, total_lines);
    }

    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    pub fn set_line_height(&mut self, height: f32) {
        self.line_height = height.max(10.0);
    }

    pub fn gutter_width(&self) -> f32 {
        self.gutter_width
    }
}

impl Default for TextView {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
pub struct TextViewResponse {
    pub visible_start: usize,
    pub visible_end: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_view_creation() {
        let view = TextView::new();
        assert_eq!(view.line_height(), 18.0);
        assert_eq!(view.gutter_width(), 60.0);
    }

    #[test]
    fn test_scroll_to_line() {
        let mut view = TextView::new();
        view.scroll_to_line(100, 1000);
        assert_eq!(view.viewport.start_line, 100);
    }

    #[test]
    fn test_scroll_by() {
        let mut view = TextView::new();
        view.viewport.start_line = 50;
        view.scroll_by(10, 1000);
        assert_eq!(view.viewport.start_line, 60);

        view.scroll_by(-20, 1000);
        assert_eq!(view.viewport.start_line, 40);
    }

    #[test]
    fn test_set_line_height() {
        let mut view = TextView::new();
        view.set_line_height(20.0);
        assert_eq!(view.line_height(), 20.0);

        view.set_line_height(5.0);
        assert_eq!(view.line_height(), 10.0);
    }
}
