//! Text buffer module using rope data structure
//!
//! Provides efficient text storage and manipulation for large files.

use ropey::Rope;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BufferError {
    #[error("Failed to read file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Line index out of bounds: {0}")]
    LineOutOfBounds(usize),
}

#[derive(Debug, Clone)]
pub struct TextBuffer {
    rope: Rope,
    modified: bool,
}

impl TextBuffer {
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            modified: false,
        }
    }

    pub fn from_str(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            modified: false,
        }
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, BufferError> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)?;
        Ok(Self {
            rope: Rope::from_str(&content),
            modified: false,
        })
    }

    pub fn is_modified(&self) -> bool {
        self.modified
    }

    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn is_empty(&self) -> bool {
        self.rope.len_chars() == 0
    }

    pub fn line(&self, line_idx: usize) -> Result<&str, BufferError> {
        if line_idx >= self.len_lines() {
            return Err(BufferError::LineOutOfBounds(line_idx));
        }
        Ok(self.rope.line(line_idx).as_str().unwrap_or(""))
    }

    pub fn line_to_string(&self, line_idx: usize) -> Result<String, BufferError> {
        if line_idx >= self.len_lines() {
            return Err(BufferError::LineOutOfBounds(line_idx));
        }
        Ok(self.rope.line(line_idx).to_string())
    }

    pub fn lines_range(&self, start: usize, end: usize) -> Vec<String> {
        let end = end.min(self.len_lines());
        let start = start.min(end);
        (start..end)
            .filter_map(|i| self.line_to_string(i).ok())
            .collect()
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn insert(&mut self, char_idx: usize, text: &str) {
        self.rope.insert(char_idx, text);
        self.modified = true;
    }

    pub fn remove(&mut self, start: usize, end: usize) {
        self.rope.remove(start..end);
        self.modified = true;
    }
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct VirtualViewport {
    pub start_line: usize,
    pub visible_lines: usize,
}

impl VirtualViewport {
    pub fn new(start_line: usize, visible_lines: usize) -> Self {
        Self {
            start_line,
            visible_lines,
        }
    }

    pub fn end_line(&self, total_lines: usize) -> usize {
        (self.start_line + self.visible_lines).min(total_lines)
    }

    pub fn scroll_to(&mut self, line: usize, total_lines: usize) {
        self.start_line = line.min(total_lines.saturating_sub(self.visible_lines));
    }

    pub fn scroll_by(&mut self, delta: isize, total_lines: usize) {
        let new_start = if delta < 0 {
            self.start_line.saturating_sub((-delta) as usize)
        } else {
            self.start_line.saturating_add(delta as usize)
        };
        self.scroll_to(new_start, total_lines);
    }

    pub fn ensure_visible(&mut self, line: usize, total_lines: usize) {
        if line < self.start_line {
            self.start_line = line;
        } else if line >= self.start_line + self.visible_lines {
            self.start_line = line.saturating_sub(self.visible_lines - 1);
        }
        self.start_line = self.start_line.min(total_lines.saturating_sub(self.visible_lines));
    }

    pub fn get_visible_lines(&self, buffer: &TextBuffer) -> Vec<(usize, String)> {
        let end = self.end_line(buffer.len_lines());
        (self.start_line..end)
            .filter_map(|i| buffer.line_to_string(i).ok().map(|s| (i, s)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_buffer() {
        let buffer = TextBuffer::new();
        assert!(buffer.is_empty());
        assert_eq!(buffer.len_lines(), 1);
        assert!(!buffer.is_modified());
    }

    #[test]
    fn test_from_str() {
        let buffer = TextBuffer::from_str("Hello\nWorld");
        assert_eq!(buffer.len_lines(), 2);
        assert_eq!(buffer.line(0).unwrap(), "Hello\n");
        assert_eq!(buffer.line(1).unwrap(), "World");
    }

    #[test]
    fn test_insert() {
        let mut buffer = TextBuffer::from_str("Hello");
        buffer.insert(5, " World");
        assert_eq!(buffer.text(), "Hello World");
        assert!(buffer.is_modified());
    }

    #[test]
    fn test_remove() {
        let mut buffer = TextBuffer::from_str("Hello World");
        buffer.remove(5, 11);
        assert_eq!(buffer.text(), "Hello");
        assert!(buffer.is_modified());
    }

    #[test]
    fn test_lines_range() {
        let buffer = TextBuffer::from_str("Line1\nLine2\nLine3\nLine4\nLine5");
        let lines = buffer.lines_range(1, 4);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "Line2\n");
        assert_eq!(lines[1], "Line3\n");
        assert_eq!(lines[2], "Line4\n");
    }

    #[test]
    fn test_line_out_of_bounds() {
        let buffer = TextBuffer::from_str("Single line");
        assert!(buffer.line(10).is_err());
    }

    #[test]
    fn test_virtual_viewport() {
        let buffer = TextBuffer::from_str("Line1\nLine2\nLine3\nLine4\nLine5\nLine6\nLine7\nLine8\nLine9\nLine10");
        let mut viewport = VirtualViewport::new(0, 3);

        let visible = viewport.get_visible_lines(&buffer);
        assert_eq!(visible.len(), 3);
        assert_eq!(visible[0].0, 0);
        assert_eq!(visible[2].0, 2);

        viewport.scroll_by(2, buffer.len_lines());
        let visible = viewport.get_visible_lines(&buffer);
        assert_eq!(visible[0].0, 2);

        viewport.ensure_visible(8, buffer.len_lines());
        let visible = viewport.get_visible_lines(&buffer);
        assert!(visible.iter().any(|(i, _)| *i == 8));
    }

    #[test]
    fn test_large_file_simulation() {
        // Simulate a 100k line file
        let lines: String = (0..100_000)
            .map(|i| format!("Line {}\n", i))
            .collect();
        let buffer = TextBuffer::from_str(&lines);

        assert_eq!(buffer.len_lines(), 100_001); // 100k lines + empty last line

        // Test virtual scrolling performance
        let viewport = VirtualViewport::new(50_000, 50);
        let visible = viewport.get_visible_lines(&buffer);
        assert_eq!(visible.len(), 50);
        assert_eq!(visible[0].0, 50_000);
    }
}
