//! File list rendering

use crate::app::SashikiApp;
use crate::git::ChangeType;
use crate::theme::*;
use crate::ui::{FileListMode, FileTreeNode, read_dir_shallow};
use gpui::{
    AnyElement, Context, Div, IntoElement, ParentElement, Styled, div, prelude::*, px, rgb,
};
use std::path::{Path, PathBuf};

/// Render directory expand/collapse arrow and folder icon
fn render_dir_icons(is_expanded: bool) -> (Div, Div) {
    let arrow = div()
        .w_4()
        .text_center()
        .text_color(rgb(BLUE))
        .text_xs()
        .child(if is_expanded { "▼" } else { "▶" });
    let folder = div()
        .w_4()
        .text_center()
        .text_color(rgb(YELLOW))
        .text_sm()
        .child(if is_expanded { "📂" } else { "📁" });
    (arrow, folder)
}

impl SashikiApp {
    pub fn render_file_list(&self, cx: &Context<Self>) -> AnyElement {
        let mode = self.file_list_mode;

        div()
            .w(px(self.file_list_width))
            .h_full()
            .bg(rgb(BG_MANTLE))
            .flex()
            .flex_col()
            .child(self.render_file_list_header(mode, cx))
            .child(match mode {
                FileListMode::Changes => self.render_changes_tree(cx),
                FileListMode::AllFiles => self.render_all_files_tree(cx),
            })
            .into_any_element()
    }

    fn render_file_list_header(&self, mode: FileListMode, cx: &Context<Self>) -> impl IntoElement {
        div()
            .h_8()
            .px_2()
            .flex()
            .items_center()
            .bg(rgb(BG_BASE))
            .border_b_1()
            .border_color(rgb(BG_SURFACE0))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        div()
                            .id("files-changes-tab")
                            .px_2()
                            .py_1()
                            .cursor_pointer()
                            .rounded_sm()
                            .when(mode == FileListMode::Changes, |el| el.bg(rgb(BG_SURFACE1)))
                            .hover(|el| el.bg(rgb(BG_SURFACE1)))
                            .text_xs()
                            .text_color(rgb(YELLOW))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.file_list_mode = FileListMode::Changes;
                                this.expanded_dirs.clear();
                                this.build_file_tree();
                                cx.notify();
                            }))
                            .child("Changes"),
                    )
                    .child(
                        div()
                            .id("files-all-tab")
                            .px_2()
                            .py_1()
                            .cursor_pointer()
                            .rounded_sm()
                            .when(mode == FileListMode::AllFiles, |el| el.bg(rgb(BG_SURFACE1)))
                            .hover(|el| el.bg(rgb(BG_SURFACE1)))
                            .text_xs()
                            .text_color(rgb(BLUE))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.file_list_mode = FileListMode::AllFiles;
                                this.expanded_dirs.clear();
                                cx.notify();
                            }))
                            .child("All"),
                    ),
            )
    }

    fn render_changes_tree(&self, cx: &Context<Self>) -> AnyElement {
        if let Some(ref tree) = self.file_tree {
            div()
                .flex_1()
                .overflow_hidden()
                .children(
                    tree.children
                        .iter()
                        .map(|node| self.render_tree_node(node, 0, cx)),
                )
                .into_any_element()
        } else {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(TEXT_MUTED))
                .text_sm()
                .child("No files")
                .into_any_element()
        }
    }

    fn render_tree_node(
        &self,
        node: &FileTreeNode,
        depth: usize,
        cx: &Context<Self>,
    ) -> AnyElement {
        let indent = depth * 16;
        let is_expanded = self.expanded_dirs.contains(&node.path);
        let node_path = node.path.clone();
        let node_name = node.name.clone();

        let mut result = div().flex().flex_col();

        if node.is_dir {
            let click_path = node_path.clone();
            let node_element = div()
                .id(format!("tree-dir-{}", node.path.to_string_lossy()))
                .pl(px(indent as f32))
                .pr_3()
                .py_1()
                .cursor_pointer()
                .hover(|el| el.bg(rgb(BG_SURFACE0)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_dir_expanded(&click_path);
                    cx.notify();
                }))
                .flex()
                .items_center()
                .gap_2();
            let (arrow, folder) = render_dir_icons(is_expanded);
            let node_element = node_element
                .child(arrow)
                .child(folder)
                .child(div().text_color(rgb(TEXT)).text_sm().child(node_name));

            result = result.child(node_element);

            if is_expanded {
                for child in &node.children {
                    result = result.child(self.render_tree_node(child, depth + 1, cx));
                }
            }
        } else {
            let click_path = node_path.clone();
            let right_click_path = node_path.clone();
            let change_info = node.change_info;
            let (color, symbol) = if let Some(info) = change_info {
                match info.change_type {
                    ChangeType::Added => (GREEN, "+"),
                    ChangeType::Modified => (YELLOW, "~"),
                    ChangeType::Deleted => (RED, "-"),
                    ChangeType::Renamed => (BLUE, "→"),
                    ChangeType::Unknown => (TEXT_MUTED, "?"),
                }
            } else {
                (TEXT_MUTED, "")
            };

            let node_element = div()
                .id(format!("tree-file-{}", node.path.to_string_lossy()))
                .pl(px(indent as f32))
                .pr_3()
                .py_1()
                .cursor_pointer()
                .hover(|el| el.bg(rgb(BG_SURFACE0)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.on_file_selected(
                        click_path.clone(),
                        change_info.map(|i| i.change_type),
                        cx,
                    );
                }))
                .on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(move |this, _, _, cx| {
                        let path_str = format!("`{}`", right_click_path.to_string_lossy());
                        this.send_to_terminal(&path_str, cx);
                    }),
                )
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w_4()
                        .text_center()
                        .text_color(rgb(color))
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(symbol),
                )
                .child(
                    div()
                        .w_4()
                        .text_center()
                        .text_color(rgb(TEXT_MUTED))
                        .text_sm()
                        .child("📄"),
                )
                .child(div().text_color(rgb(TEXT)).text_sm().child(node_name));

            result = result.child(node_element);
        }

        result.into_any_element()
    }

    fn render_all_files_tree(&self, cx: &Context<Self>) -> AnyElement {
        let base_path = if let Some(session) = self.session_manager.active_session() {
            session.worktree_path().to_path_buf()
        } else {
            PathBuf::from(".")
        };

        let entries = read_dir_shallow(&base_path).unwrap_or_default();

        if entries.is_empty() {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(TEXT_MUTED))
                .text_sm()
                .child("No files")
                .into_any_element();
        }

        div()
            .flex_1()
            .overflow_hidden()
            .children(
                entries.iter().map(|(path, is_dir)| {
                    self.render_lazy_tree_node(path, *is_dir, 0, &base_path, cx)
                }),
            )
            .into_any_element()
    }

    fn render_lazy_tree_node(
        &self,
        path: &Path,
        is_dir: bool,
        depth: usize,
        base_path: &Path,
        cx: &Context<Self>,
    ) -> AnyElement {
        let indent = depth * 16;
        let is_expanded = self.expanded_dirs.contains(path);
        let node_path = path.to_path_buf();
        let node_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let mut result = div().flex().flex_col();

        if is_dir {
            let click_path = node_path.clone();
            let node_element = div()
                .id(format!("lazy-dir-{}", path.to_string_lossy()))
                .pl(px(indent as f32))
                .pr_3()
                .py_1()
                .cursor_pointer()
                .hover(|el| el.bg(rgb(BG_SURFACE0)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_dir_expanded(&click_path);
                    cx.notify();
                }))
                .flex()
                .items_center()
                .gap_2();
            let (arrow, folder) = render_dir_icons(is_expanded);
            let node_element = node_element
                .child(arrow)
                .child(folder)
                .child(div().text_color(rgb(TEXT)).text_sm().child(node_name));

            result = result.child(node_element);

            if is_expanded && let Ok(children) = read_dir_shallow(&node_path) {
                for (child_path, child_is_dir) in children {
                    result = result.child(self.render_lazy_tree_node(
                        &child_path,
                        child_is_dir,
                        depth + 1,
                        base_path,
                        cx,
                    ));
                }
            }
        } else {
            let relative_path = path.strip_prefix(base_path).unwrap_or(path).to_path_buf();
            let click_path = relative_path.clone();
            let right_click_path = relative_path.clone();

            let node_element = div()
                .id(format!("lazy-file-{}", path.to_string_lossy()))
                .pl(px(indent as f32))
                .pr_3()
                .py_1()
                .cursor_pointer()
                .hover(|el| el.bg(rgb(BG_SURFACE0)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.on_file_selected(click_path.clone(), None, cx);
                }))
                .on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(move |this, _, _, cx| {
                        let path_str = format!("`{}`", right_click_path.to_string_lossy());
                        this.send_to_terminal(&path_str, cx);
                    }),
                )
                .flex()
                .items_center()
                .gap_2()
                // Spacer for alignment with Changes mode (which shows change symbols here)
                .child(div().w_4())
                .child(
                    div()
                        .w_4()
                        .text_center()
                        .text_color(rgb(TEXT_MUTED))
                        .text_sm()
                        .child("📄"),
                )
                .child(div().text_color(rgb(TEXT)).text_sm().child(node_name));

            result = result.child(node_element);
        }

        result.into_any_element()
    }
}
