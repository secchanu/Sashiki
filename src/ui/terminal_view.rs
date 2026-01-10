//! Terminal view component with ANSI color support

use crate::terminal::{Terminal, TerminalKey, TerminalSize};
use crate::ui::Theme;
use egui::{self, Color32, FontId, Pos2, Rect, Sense, Ui, Vec2};

const CHAR_WIDTH: f32 = 8.0;
const CHAR_HEIGHT: f32 = 16.0;

pub struct TerminalView {
    height: f32,
    min_height: f32,
    max_height: f32,
    resizing: bool,
    focused: bool,
    cursor_visible: bool,
    last_blink: std::time::Instant,
}

impl TerminalView {
    pub fn new(height: f32) -> Self {
        Self {
            height,
            min_height: 100.0,
            max_height: 600.0,
            resizing: false,
            focused: false,
            cursor_visible: true,
            last_blink: std::time::Instant::now(),
        }
    }

    pub fn height(&self) -> f32 {
        self.height
    }

    pub fn set_height(&mut self, height: f32) {
        self.height = height.clamp(self.min_height, self.max_height);
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        terminal: &mut Terminal,
    ) -> TerminalViewResponse {
        let mut response = TerminalViewResponse::default();

        // Cursor blink
        let now = std::time::Instant::now();
        if now.duration_since(self.last_blink).as_millis() > 500 {
            self.cursor_visible = !self.cursor_visible;
            self.last_blink = now;
        }

        // Resize handle
        let resize_rect = Rect::from_min_size(
            ui.min_rect().left_top(),
            Vec2::new(ui.available_width(), 6.0),
        );

        let resize_response = ui.allocate_rect(resize_rect, Sense::drag());

        if resize_response.dragged() {
            self.resizing = true;
            let delta = -resize_response.drag_delta().y;
            self.set_height(self.height + delta);
            response.height_changed = true;
        }

        if resize_response.drag_stopped() {
            self.resizing = false;
        }

        if resize_response.hovered() || self.resizing {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
            ui.painter().rect_filled(resize_rect, 0.0, theme.accent);
        } else {
            ui.painter().rect_filled(resize_rect, 0.0, theme.border);
        }

        // Terminal content
        let content_rect = Rect::from_min_size(
            resize_rect.left_bottom(),
            Vec2::new(ui.available_width(), self.height - 6.0),
        );

        let term_response = ui.allocate_rect(content_rect, Sense::click_and_drag());

        if term_response.clicked() {
            self.focused = true;
        }

        ui.painter().rect_filled(content_rect, 0.0, theme.bg_primary);

        // Calculate terminal size
        let cols = ((content_rect.width() - 16.0) / CHAR_WIDTH) as u16;
        let rows = ((content_rect.height() - 8.0) / CHAR_HEIGHT) as u16;

        let new_size = TerminalSize {
            rows: rows.max(1),
            cols: cols.max(1),
        };
        let _ = terminal.resize(new_size);

        terminal.process_output();

        self.render_grid(ui, theme, terminal, content_rect);

        if self.focused {
            self.handle_keyboard_input(ui, terminal);
        }

        // Scroll handling
        if term_response.hovered() {
            ui.input(|i| {
                let scroll = i.raw_scroll_delta.y;
                if scroll != 0.0 {
                    if let Ok(mut state) = terminal.state.lock() {
                        let delta = if scroll > 0.0 { 3 } else { -3 };
                        state.scroll_view(delta);
                    }
                }
            });
        }

        response
    }

    fn render_grid(&self, ui: &mut Ui, theme: &Theme, terminal: &Terminal, rect: Rect) {
        let state = match terminal.state.lock() {
            Ok(s) => s,
            Err(_) => return,
        };

        let painter = ui.painter();
        let font_id = FontId::monospace(14.0);

        let start_x = rect.left() + 8.0;
        let start_y = rect.top() + 4.0;

        for (row_idx, row) in state.visible_rows().enumerate() {
            let y = start_y + row_idx as f32 * CHAR_HEIGHT;

            if y > rect.bottom() {
                break;
            }

            for (col_idx, cell) in row.iter().enumerate() {
                let x = start_x + col_idx as f32 * CHAR_WIDTH;

                if x > rect.right() {
                    break;
                }

                let cell_rect =
                    Rect::from_min_size(Pos2::new(x, y), Vec2::new(CHAR_WIDTH, CHAR_HEIGHT));

                let (fg, bg) = if cell.attrs.inverse {
                    (cell.attrs.bg, cell.attrs.fg)
                } else {
                    (cell.attrs.fg, cell.attrs.bg)
                };

                if bg != Color32::TRANSPARENT {
                    painter.rect_filled(cell_rect, 0.0, bg);
                }

                if cell.c != ' ' && cell.c != '\0' {
                    let text_pos = Pos2::new(x, y);

                    let fg_color = if cell.attrs.bold {
                        brighten_color(fg)
                    } else {
                        fg
                    };

                    painter.text(
                        text_pos,
                        egui::Align2::LEFT_TOP,
                        cell.c,
                        font_id.clone(),
                        fg_color,
                    );

                    if cell.attrs.underline {
                        painter.line_segment(
                            [
                                Pos2::new(x, y + CHAR_HEIGHT - 2.0),
                                Pos2::new(x + CHAR_WIDTH, y + CHAR_HEIGHT - 2.0),
                            ],
                            egui::Stroke::new(1.0, fg_color),
                        );
                    }
                }
            }
        }

        // Cursor
        if self.focused && self.cursor_visible && state.cursor.visible {
            let cursor_x = start_x + state.cursor.col as f32 * CHAR_WIDTH;
            let cursor_y = start_y + state.cursor.row as f32 * CHAR_HEIGHT;

            let cursor_rect = Rect::from_min_size(
                Pos2::new(cursor_x, cursor_y),
                Vec2::new(CHAR_WIDTH, CHAR_HEIGHT),
            );

            painter.rect_filled(cursor_rect, 0.0, theme.text_primary);

            if state.cursor.row < state.rows && state.cursor.col < state.cols {
                if let Some(row) = state.visible_rows().nth(state.cursor.row) {
                    if let Some(cell) = row.get(state.cursor.col) {
                        if cell.c != ' ' && cell.c != '\0' {
                            painter.text(
                                Pos2::new(cursor_x, cursor_y),
                                egui::Align2::LEFT_TOP,
                                cell.c,
                                font_id,
                                theme.bg_primary,
                            );
                        }
                    }
                }
            }
        }
    }

    fn handle_keyboard_input(&mut self, ui: &mut Ui, terminal: &mut Terminal) {
        ui.input(|i| {
            for event in &i.events {
                match event {
                    egui::Event::Text(text) => {
                        for c in text.chars() {
                            let _ = terminal.send_key(TerminalKey::Char(c));
                        }
                    }
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        if let Some(tk) = self.map_key(*key, modifiers) {
                            let _ = terminal.send_key(tk);
                        }
                    }
                    _ => {}
                }
            }
        });
    }

    fn map_key(&self, key: egui::Key, modifiers: &egui::Modifiers) -> Option<TerminalKey> {
        if modifiers.ctrl {
            return match key {
                egui::Key::A => Some(TerminalKey::CtrlA),
                egui::Key::C => Some(TerminalKey::CtrlC),
                egui::Key::D => Some(TerminalKey::CtrlD),
                egui::Key::E => Some(TerminalKey::CtrlE),
                egui::Key::K => Some(TerminalKey::CtrlK),
                egui::Key::L => Some(TerminalKey::CtrlL),
                egui::Key::R => Some(TerminalKey::CtrlR),
                egui::Key::U => Some(TerminalKey::CtrlU),
                egui::Key::W => Some(TerminalKey::CtrlW),
                egui::Key::Z => Some(TerminalKey::CtrlZ),
                _ => None,
            };
        }

        match key {
            egui::Key::Enter => Some(TerminalKey::Enter),
            egui::Key::Tab => Some(TerminalKey::Tab),
            egui::Key::Backspace => Some(TerminalKey::Backspace),
            egui::Key::Escape => Some(TerminalKey::Escape),
            egui::Key::ArrowUp => Some(TerminalKey::Up),
            egui::Key::ArrowDown => Some(TerminalKey::Down),
            egui::Key::ArrowLeft => Some(TerminalKey::Left),
            egui::Key::ArrowRight => Some(TerminalKey::Right),
            egui::Key::Home => Some(TerminalKey::Home),
            egui::Key::End => Some(TerminalKey::End),
            egui::Key::PageUp => Some(TerminalKey::PageUp),
            egui::Key::PageDown => Some(TerminalKey::PageDown),
            egui::Key::Delete => Some(TerminalKey::Delete),
            egui::Key::Insert => Some(TerminalKey::Insert),
            egui::Key::F1 => Some(TerminalKey::F(1)),
            egui::Key::F2 => Some(TerminalKey::F(2)),
            egui::Key::F3 => Some(TerminalKey::F(3)),
            egui::Key::F4 => Some(TerminalKey::F(4)),
            egui::Key::F5 => Some(TerminalKey::F(5)),
            egui::Key::F6 => Some(TerminalKey::F(6)),
            egui::Key::F7 => Some(TerminalKey::F(7)),
            egui::Key::F8 => Some(TerminalKey::F(8)),
            egui::Key::F9 => Some(TerminalKey::F(9)),
            egui::Key::F10 => Some(TerminalKey::F(10)),
            egui::Key::F11 => Some(TerminalKey::F(11)),
            egui::Key::F12 => Some(TerminalKey::F(12)),
            _ => None,
        }
    }
}

fn brighten_color(color: Color32) -> Color32 {
    let [r, g, b, a] = color.to_array();
    Color32::from_rgba_unmultiplied(
        (r as u16 * 5 / 4).min(255) as u8,
        (g as u16 * 5 / 4).min(255) as u8,
        (b as u16 * 5 / 4).min(255) as u8,
        a,
    )
}

impl Default for TerminalView {
    fn default() -> Self {
        Self::new(200.0)
    }
}

#[derive(Default)]
pub struct TerminalViewResponse {
    pub height_changed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_view_creation() {
        let view = TerminalView::new(200.0);
        assert_eq!(view.height(), 200.0);
    }

    #[test]
    fn test_height_clamping() {
        let mut view = TerminalView::new(200.0);

        view.set_height(50.0);
        assert_eq!(view.height(), 100.0);

        view.set_height(1000.0);
        assert_eq!(view.height(), 600.0);
    }

    #[test]
    fn test_brighten_color() {
        let color = Color32::from_rgb(100, 100, 100);
        let bright = brighten_color(color);
        assert!(bright.r() > color.r());
        assert!(bright.g() > color.g());
        assert!(bright.b() > color.b());
    }
}
