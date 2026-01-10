//! Sidebar dimensions management

pub struct Sidebar {
    width: f32,
    min_width: f32,
    max_width: f32,
}

impl Sidebar {
    pub fn new(width: f32) -> Self {
        Self {
            width,
            min_width: 100.0,
            max_width: 400.0,
        }
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn set_width(&mut self, width: f32) {
        self.width = width.clamp(self.min_width, self.max_width);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sidebar_creation() {
        let sidebar = Sidebar::new(200.0);
        assert_eq!(sidebar.width(), 200.0);
    }

    #[test]
    fn test_sidebar_width_clamping() {
        let mut sidebar = Sidebar::new(200.0);

        sidebar.set_width(50.0);
        assert_eq!(sidebar.width(), 100.0);

        sidebar.set_width(500.0);
        assert_eq!(sidebar.width(), 400.0);
    }
}
