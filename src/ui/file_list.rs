//! File list rendering

use crate::app::SashikiApp;
use crate::git::{ChangeType, ChangedFile};
use crate::icon;
use crate::theme::*;
use crate::ui::{ChangeInfo, ChangeSection, FileListMode, FileTreeNode, read_dir_shallow};
use gpui::{
    AnyElement, Context, Div, IntoElement, ParentElement, Styled, div, prelude::*, px, rgb,
};
use std::path::{Path, PathBuf};

fn section_key(section: ChangeSection) -> &'static str {
    match section {
        ChangeSection::Staged => "staged",
        ChangeSection::Unstaged => "unstaged",
    }
}

fn section_title(section: ChangeSection) -> &'static str {
    match section {
        ChangeSection::Staged => "Staged Changes",
        ChangeSection::Unstaged => "Changes",
    }
}

fn render_dir_icons(is_expanded: bool) -> (Div, Div) {
    let arrow = div()
        .size(px(16.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(
            if is_expanded {
                icon::chevron_down()
            } else {
                icon::chevron_right()
            }
            .size(px(12.0))
            .text_color(rgb(TEXT_MUTED)),
        );
    let spacer = div().w_2();
    (arrow, spacer)
}

fn change_status(change_type: ChangeType) -> (&'static str, u32) {
    match change_type {
        ChangeType::Added => ("A", GREEN),
        ChangeType::Modified => ("M", YELLOW),
        ChangeType::Deleted => ("D", RED),
        ChangeType::Renamed => ("R", BLUE),
        ChangeType::Unknown => ("?", TEXT_MUTED),
    }
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
            .bg(rgb(BG_BASE))
            .border_b_1()
            .border_color(rgb(BG_SURFACE0))
            .child(
                div()
                    .h_8()
                    .px_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child({
                        let is_tree = self.changes_view_is_tree;
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .id("file-mode-toggle")
                                    .w(px(64.))
                                    .min_h(px(24.0))
                                    .cursor_pointer()
                                    .rounded_sm()
                                    .bg(rgb(BG_SURFACE0))
                                    .hover(|el| el.bg(rgb(BG_SURFACE1)))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_sm()
                                    .text_color(rgb(TEXT))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.file_list_mode = match this.file_list_mode {
                                            FileListMode::Changes => FileListMode::AllFiles,
                                            FileListMode::AllFiles => FileListMode::Changes,
                                        };
                                        this.expanded_dirs.clear();
                                        this.hovered_file_path = None;
                                        this.hovered_file_section = None;
                                        cx.notify();
                                    }))
                                    .child(if mode == FileListMode::Changes {
                                        "Changes"
                                    } else {
                                        "Files"
                                    }),
                            )
                            .when(mode == FileListMode::Changes, |el| {
                                el.child(
                                    div()
                                        .id("changes-view-toggle")
                                        .flex_shrink_0()
                                        .size(px(24.0))
                                        .cursor_pointer()
                                        .rounded_sm()
                                        .bg(rgb(BG_SURFACE0))
                                        .hover(|b| b.bg(rgb(BG_SURFACE1)))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.changes_view_is_tree = !this.changes_view_is_tree;
                                            cx.notify();
                                        }))
                                        .child(
                                            if is_tree {
                                                icon::list()
                                            } else {
                                                icon::tree_view()
                                            }
                                            .size(px(14.0))
                                            .text_color(rgb(TEXT_MUTED)),
                                        ),
                                )
                            })
                    })
                    .when(mode == FileListMode::Changes, |el| {
                        el.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    div()
                                        .id("files-stash-button")
                                        .px_2()
                                        .min_h(px(24.0))
                                        .cursor_pointer()
                                        .rounded_sm()
                                        .bg(rgb(BG_SURFACE0))
                                        .hover(|b| b.bg(rgb(BG_SURFACE1)))
                                        .text_sm()
                                        .text_color(rgb(TEXT_MUTED))
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.open_stash_dialog(window, cx);
                                        }))
                                        .child(
                                            icon::stash()
                                                .size(px(14.0))
                                                .text_color(rgb(TEXT_MUTED)),
                                        )
                                        .child("Stash"),
                                )
                                .child(self.render_main_action_button(cx)),
                        )
                    }),
            )
    }

    /// VSCode-style main action button that changes based on repository state:
    /// - Changes exist → Commit (with dropdown: Commit & Push, Commit & Sync)
    /// - No upstream → Publish Branch
    /// - ahead/behind > 0 → Sync Changes N↓ M↑ (with dropdown: Push, Pull)
    /// - Otherwise → Commit (disabled)
    /// - Sync in progress → Syncing... (disabled, with sync icon)
    fn render_main_action_button(&self, cx: &Context<Self>) -> Div {
        let has_changes = !self.changed_files.is_empty();
        let sync_in_progress = self.git_sync_in_progress;
        let has_upstream = self.git_has_upstream;
        let ahead = self.git_ahead;
        let behind = self.git_behind;

        // Sync in progress → spinning state
        if sync_in_progress {
            return self.render_action_split_button(
                "action-syncing",
                icon::sync().size(px(14.0)).text_color(rgb(BLUE)),
                "Syncing...",
                BLUE,
                false, // disabled
                |_, _, _| {},
                cx,
            );
        }

        // Changes exist → Commit
        if has_changes {
            return self.render_action_split_button(
                "action-commit",
                icon::git_commit().size(px(14.0)).text_color(rgb(GREEN)),
                "Commit",
                GREEN,
                true,
                |this, window, cx| {
                    this.commit_dropdown_open = false;
                    this.open_commit_dialog(window, cx);
                },
                cx,
            );
        }

        // No upstream → Publish Branch
        if !has_upstream {
            return self.render_action_split_button(
                "action-publish",
                icon::cloud_upload().size(px(14.0)).text_color(rgb(BLUE)),
                "Publish",
                BLUE,
                true,
                |this, _, cx| {
                    this.commit_dropdown_open = false;
                    if let Some(path) = this
                        .session_manager
                        .active_session()
                        .map(|s| s.worktree_path().to_path_buf())
                    {
                        this.git_push_async(path, cx);
                    }
                },
                cx,
            );
        }

        // Ahead/behind > 0 → Sync Changes
        if ahead > 0 || behind > 0 {
            let mut label = String::from("Sync");
            if behind > 0 {
                label.push_str(&format!(" {}↓", behind));
            }
            if ahead > 0 {
                label.push_str(&format!(" {}↑", ahead));
            }
            return self.render_action_split_button(
                "action-sync",
                icon::sync().size(px(14.0)).text_color(rgb(BLUE)),
                &label,
                BLUE,
                true,
                |this, _, cx| {
                    this.commit_dropdown_open = false;
                    if let Some(path) = this
                        .session_manager
                        .active_session()
                        .map(|s| s.worktree_path().to_path_buf())
                    {
                        this.git_sync_async(path, cx);
                    }
                },
                cx,
            );
        }

        // Default: Commit (disabled)
        self.render_action_split_button(
            "action-commit-disabled",
            icon::git_commit()
                .size(px(14.0))
                .text_color(rgb(TEXT_MUTED)),
            "Commit",
            TEXT_MUTED,
            false,
            |_, _, _| {},
            cx,
        )
    }

    fn render_action_split_button(
        &self,
        id: &'static str,
        icon_el: gpui::Svg,
        label: &str,
        color: u32,
        enabled: bool,
        on_click: impl Fn(&mut SashikiApp, &mut gpui::Window, &mut gpui::Context<SashikiApp>) + 'static,
        cx: &Context<Self>,
    ) -> Div {
        let label_owned = label.to_string();
        div()
            .flex()
            .items_center()
            .rounded_sm()
            .bg(rgb(BG_SURFACE0))
            .child(
                div()
                    .id(id)
                    .px_2()
                    .min_h(px(24.0))
                    .when(enabled, |b| {
                        b.cursor_pointer().hover(|b| b.bg(rgb(BG_SURFACE1)))
                    })
                    .rounded_tl_sm()
                    .rounded_bl_sm()
                    .text_sm()
                    .text_color(rgb(color))
                    .flex()
                    .items_center()
                    .gap_1()
                    .when(enabled, |b| {
                        b.on_click(cx.listener(move |this, _, window, cx| {
                            on_click(this, window, cx);
                        }))
                    })
                    .child(icon_el)
                    .child(label_owned),
            )
            .child(div().w_px().h_4().bg(rgb(BG_SURFACE1)))
            .child(
                div()
                    .id("action-dropdown-btn")
                    .size(px(24.0))
                    .cursor_pointer()
                    .rounded_tr_sm()
                    .rounded_br_sm()
                    .hover(|b| b.bg(rgb(BG_SURFACE1)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.commit_dropdown_open = !this.commit_dropdown_open;
                        cx.notify();
                    }))
                    .child(icon::chevron_down().size(px(12.0)).text_color(rgb(color))),
            )
    }

    fn render_changes_tree(&self, cx: &Context<Self>) -> AnyElement {
        let staged_count = self
            .changed_files
            .iter()
            .filter(|f| f.has_staged_changes())
            .count();
        let unstaged_count = self
            .changed_files
            .iter()
            .filter(|f| f.has_unstaged_changes())
            .count();

        if staged_count == 0 && unstaged_count == 0 {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(TEXT_MUTED))
                .text_sm()
                .child("No changes")
                .into_any_element();
        }

        div()
            .id("changes-tree-scroll")
            .flex_1()
            .overflow_scroll()
            .flex()
            .flex_col()
            .child(self.render_change_section(
                ChangeSection::Staged,
                staged_count,
                self.staged_section_collapsed,
                cx,
            ))
            .child(self.render_change_section(
                ChangeSection::Unstaged,
                unstaged_count,
                self.unstaged_section_collapsed,
                cx,
            ))
            .into_any_element()
    }

    fn render_change_section(
        &self,
        section: ChangeSection,
        count: usize,
        collapsed: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let section_id = section_key(section);
        let title = section_title(section);
        let is_staged = matches!(section, ChangeSection::Staged);

        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .id(format!("changes-section-header-{}", section_id))
                    .px_2()
                    .h_6()
                    .bg(rgb(BG_BASE))
                    .border_b_1()
                    .border_color(rgb(BG_SURFACE0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .on_mouse_move(cx.listener(|this, _: &gpui::MouseMoveEvent, _, cx| {
                        if this.hovered_file_path.is_some() || this.hovered_file_section.is_some() {
                            this.hovered_file_path = None;
                            this.hovered_file_section = None;
                            cx.notify();
                        }
                    }))
                    .child(
                        // 折りたたみトグル部分 (クリックで折りたたみ)
                        div()
                            .id(format!("changes-section-collapse-{}", section_id))
                            .flex_1()
                            .h_full()
                            .cursor_pointer()
                            .hover(|el| el.bg(rgb(BG_SURFACE0)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                match section {
                                    ChangeSection::Staged => {
                                        this.staged_section_collapsed =
                                            !this.staged_section_collapsed;
                                    }
                                    ChangeSection::Unstaged => {
                                        this.unstaged_section_collapsed =
                                            !this.unstaged_section_collapsed;
                                    }
                                }
                                cx.notify();
                            }))
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .size(px(16.0))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        if collapsed {
                                            icon::chevron_right()
                                        } else {
                                            icon::chevron_down()
                                        }
                                        .size(px(12.0))
                                        .text_color(rgb(TEXT_MUTED)),
                                    ),
                            )
                            .child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(title)),
                    )
                    .child(
                        // ファイル数 + Stage All / Unstage All ボタン
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(TEXT_MUTED))
                                    .child(count.to_string()),
                            )
                            .when(count > 0, |el| {
                                // ファイル行と同じ順: discard → stage（左から）
                                let stage_color = if is_staged { rgb(BLUE) } else { rgb(GREEN) };
                                let stage_btn_id = format!("section-stage-all-{}", section_id);
                                // 未ステージセクションのみ: Discard All ボタンを先に配置
                                let el = if !is_staged {
                                    el.child(
                                        div()
                                            .id("section-discard-all")
                                            .flex_shrink_0()
                                            .size(px(24.0))
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .hover(|b| b.bg(rgb(BG_SURFACE1)))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.open_discard_all_confirm(cx);
                                            }))
                                            .child(
                                                icon::x_circle()
                                                    .size(px(14.0))
                                                    .text_color(rgb(RED)),
                                            ),
                                    )
                                } else {
                                    el
                                };
                                el.child(
                                    div()
                                        .id(stage_btn_id)
                                        .flex_shrink_0()
                                        .size(px(24.0))
                                        .cursor_pointer()
                                        .rounded_sm()
                                        .hover(|b| b.bg(rgb(BG_SURFACE1)))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if is_staged {
                                                this.unstage_all_files(cx);
                                            } else {
                                                this.stage_all_files(cx);
                                            }
                                        }))
                                        .child(
                                            if is_staged {
                                                icon::arrow_down()
                                            } else {
                                                icon::arrow_up()
                                            }
                                            .size(px(14.0))
                                            .text_color(stage_color),
                                        ),
                                )
                            }),
                    ),
            )
            .when(!collapsed, |el| {
                if count == 0 {
                    el.child(
                        div()
                            .px_4()
                            .py_2()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child(if section == ChangeSection::Staged {
                                "No staged changes"
                            } else {
                                "No changes"
                            }),
                    )
                } else if self.changes_view_is_tree {
                    // ツリービュー: ディレクトリ構造を展開して表示
                    let tree_files = self.changed_files.iter().filter_map(|f| match section {
                        ChangeSection::Staged => f.staged_change.map(|ct| {
                            (
                                f.path.clone(),
                                Some(ChangeInfo {
                                    change_type: ct,
                                    section,
                                }),
                            )
                        }),
                        ChangeSection::Unstaged => f.unstaged_change.map(|ct| {
                            (
                                f.path.clone(),
                                Some(ChangeInfo {
                                    change_type: ct,
                                    section,
                                }),
                            )
                        }),
                    });
                    let tree = FileTreeNode::from_files(tree_files);
                    el.child(
                        div().children(
                            tree.children
                                .iter()
                                .map(|node| self.render_tree_node(node, 0, section, cx)),
                        ),
                    )
                } else {
                    // フラットリスト: 全ファイルをパス順に表示
                    let rows: Vec<AnyElement> = self
                        .changed_files
                        .iter()
                        .filter(|f| match section {
                            ChangeSection::Staged => f.has_staged_changes(),
                            ChangeSection::Unstaged => f.has_unstaged_changes(),
                        })
                        .map(|f| self.render_flat_file_row(f, section, cx))
                        .collect();
                    el.child(div().children(rows))
                }
            })
            .into_any_element()
    }

    fn render_flat_file_row(
        &self,
        file: &ChangedFile,
        section: ChangeSection,
        cx: &Context<Self>,
    ) -> AnyElement {
        let path = &file.path;
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| path.to_str().unwrap_or(""))
            .to_string();
        let parent = path
            .parent()
            .and_then(|p| p.to_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let change_type = match section {
            ChangeSection::Staged => file.staged_change.unwrap_or(ChangeType::Unknown),
            ChangeSection::Unstaged => file.unstaged_change.unwrap_or(ChangeType::Unknown),
        };
        let (status, status_color) = change_status(change_type);
        let is_staged = matches!(section, ChangeSection::Staged);
        let section_id = section_key(section);

        let is_selected = self.selected_file_path.as_ref() == Some(path)
            && self.selected_file_section == Some(section);
        let is_hovered = self.hovered_file_path.as_ref() == Some(path)
            && self.hovered_file_section == Some(section);

        let hover_path = path.clone();
        let click_path = path.clone();
        let right_click_path = path.clone();
        let stage_path = path.clone();
        let discard_path = path.clone();

        let row = div()
            .id(format!("flat-{}-{}", section_id, path.to_string_lossy()))
            .h_6()
            .px_2()
            .flex()
            .items_center()
            .rounded_sm()
            .when(is_selected, |el| el.bg(rgb(BG_SURFACE1)))
            .when(!is_selected && is_hovered, |el| el.bg(rgb(BG_SURFACE0)))
            .on_mouse_move(cx.listener(move |this, _: &gpui::MouseMoveEvent, _, cx| {
                if this.hovered_file_path.as_ref() != Some(&hover_path)
                    || this.hovered_file_section != Some(section)
                {
                    this.hovered_file_path = Some(hover_path.clone());
                    this.hovered_file_section = Some(section);
                    cx.notify();
                }
            }))
            // クリックでファイルを開く (ファイル名 + 親パス)
            .child(
                div()
                    .id(format!(
                        "flat-open-{}-{}",
                        section_id,
                        path.to_string_lossy()
                    ))
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.on_file_selected(
                            click_path.clone(),
                            Some(section),
                            Some(change_type),
                            None,
                            cx,
                        );
                    }))
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(move |this, _, _, cx| {
                            let s = format!("`{}`", right_click_path.to_string_lossy());
                            this.send_to_terminal(&s, cx);
                        }),
                    )
                    .child(
                        div()
                            .text_color(rgb(TEXT))
                            .text_xs()
                            .truncate()
                            .child(file_name),
                    )
                    .when_some(parent, |el, p| {
                        el.child(
                            div()
                                .text_color(rgb(TEXT_MUTED))
                                .text_xs()
                                .flex_shrink()
                                .truncate()
                                .child(p),
                        )
                    }),
            );

        // 未ステージファイルのみ: Discard (×) ボタン
        // 追跡済み → git restore、未追跡 (Added=??) → git clean -f
        let row = if !is_staged {
            let discard_btn: AnyElement = if is_hovered {
                div()
                    .id(format!(
                        "flat-discard-{}-{}",
                        section_id,
                        discard_path.to_string_lossy()
                    ))
                    .flex_shrink_0()
                    .size(px(20.0))
                    .cursor_pointer()
                    .rounded_sm()
                    .hover(|b| b.bg(rgb(BG_SURFACE1)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_discard_confirm(discard_path.clone(), change_type, cx);
                    }))
                    .child(icon::close().size(px(12.0)).text_color(rgb(RED)))
                    .into_any_element()
            } else {
                div().w(px(20.0)).flex_shrink_0().into_any_element()
            };
            row.child(discard_btn)
        } else {
            row
        };

        let stage_btn: AnyElement = if is_hovered {
            div()
                .id(format!(
                    "flat-stage-{}-{}",
                    section_id,
                    stage_path.to_string_lossy()
                ))
                .flex_shrink_0()
                .size(px(20.0))
                .cursor_pointer()
                .rounded_sm()
                .hover(|b| b.bg(rgb(BG_SURFACE1)))
                .flex()
                .items_center()
                .justify_center()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_file_staging(stage_path.clone(), is_staged, cx);
                }))
                .child(
                    if is_staged {
                        icon::arrow_down()
                    } else {
                        icon::arrow_up()
                    }
                    .size(px(12.0))
                    .text_color(if is_staged { rgb(BLUE) } else { rgb(GREEN) }),
                )
                .into_any_element()
        } else {
            div().w(px(20.0)).flex_shrink_0().into_any_element()
        };
        let row = row.child(stage_btn);

        // ステータス文字 (常に表示)
        row.child(
            div()
                .w_3()
                .text_right()
                .text_xs()
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(rgb(status_color))
                .child(status),
        )
        .into_any_element()
    }

    fn render_tree_node(
        &self,
        node: &FileTreeNode,
        depth: usize,
        section: ChangeSection,
        cx: &Context<Self>,
    ) -> AnyElement {
        // 基本インデント 12px + 深さごとに 14px を加算して
        // セクションヘッダー (8px) より視覚的に内側に配置
        let indent = 12 + depth * 14;
        // セクション別の展開状態を参照 (staged/unstaged を独立して管理)
        let is_expanded = match section {
            ChangeSection::Staged => self.staged_expanded_dirs.contains(&node.path),
            ChangeSection::Unstaged => self.unstaged_expanded_dirs.contains(&node.path),
        };
        let node_path = node.path.clone();
        let node_name = node.name.clone();
        let section_id = section_key(section);

        let mut result = div().flex().flex_col();

        if node.is_dir {
            let is_staged = matches!(section, ChangeSection::Staged);
            let is_hovered = self.hovered_file_path.as_ref() == Some(&node_path)
                && self.hovered_file_section == Some(section);
            let click_path = node_path.clone();
            let hover_path = node_path.clone();
            let stage_path = node_path.clone();

            let (arrow, spacer) = render_dir_icons(is_expanded);
            let dir_row = div()
                .id(format!(
                    "tree-dir-{}-{}",
                    section_id,
                    node.path.to_string_lossy()
                ))
                .pl(px(indent as f32))
                .pr_2()
                .h_6()
                .when(is_hovered, |el| el.bg(rgb(BG_SURFACE0)))
                .on_mouse_move(cx.listener(move |this, _: &gpui::MouseMoveEvent, _, cx| {
                    if this.hovered_file_path.as_ref() != Some(&hover_path)
                        || this.hovered_file_section != Some(section)
                    {
                        this.hovered_file_path = Some(hover_path.clone());
                        this.hovered_file_section = Some(section);
                        cx.notify();
                    }
                }))
                .flex()
                .items_center()
                .gap_1()
                // フォルダ名エリア (クリックで展開/折りたたみ)
                .child(
                    div()
                        .id(format!(
                            "tree-dir-expand-{}-{}",
                            section_id,
                            node.path.to_string_lossy()
                        ))
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_1()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_dir_expanded_section(&click_path, section);
                            cx.notify();
                        }))
                        .child(arrow)
                        .child(spacer)
                        .child(div().text_color(rgb(TEXT)).text_xs().child(node_name)),
                );

            // ステージ/アンステージボタン (ホバー時のみ表示)
            let stage_btn: AnyElement = if is_hovered {
                div()
                    .id(format!(
                        "tree-dir-stage-{}-{}",
                        section_id,
                        stage_path.to_string_lossy()
                    ))
                    .w_4()
                    .text_center()
                    .cursor_pointer()
                    .text_xs()
                    .text_color(if is_staged { rgb(BLUE) } else { rgb(GREEN) })
                    .hover(|b| {
                        b.rounded_sm()
                            .bg(rgb(BG_SURFACE1))
                            .text_color(if is_staged { rgb(BLUE) } else { rgb(GREEN) })
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_dir_staging(stage_path.clone(), is_staged, cx);
                    }))
                    .child(if is_staged { "-" } else { "+" })
                    .into_any_element()
            } else {
                div().w_4().into_any_element()
            };
            let dir_row = dir_row.child(stage_btn);

            // 未ステージのディレクトリにも Discard (×) ボタンを表示
            let discard_path = node_path.clone();
            let dir_row = if !is_staged {
                let discard_btn: AnyElement = if is_hovered {
                    div()
                        .id(format!(
                            "tree-dir-discard-{}-{}",
                            section_id,
                            discard_path.to_string_lossy()
                        ))
                        .flex_shrink_0()
                        .size(px(20.0))
                        .cursor_pointer()
                        .rounded_sm()
                        .hover(|b| b.bg(rgb(BG_SURFACE1)))
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_discard_confirm(
                                discard_path.clone(),
                                crate::git::ChangeType::Added,
                                cx,
                            );
                        }))
                        .child(icon::close().size(px(12.0)).text_color(rgb(RED)))
                        .into_any_element()
                } else {
                    div().w(px(20.0)).flex_shrink_0().into_any_element()
                };
                dir_row.child(discard_btn)
            } else {
                dir_row
            };

            result = result.child(dir_row);

            if is_expanded {
                for child in &node.children {
                    result = result.child(self.render_tree_node(child, depth + 1, section, cx));
                }
            }
        } else {
            let change_type = node
                .change_info
                .map(|i| i.change_type)
                .unwrap_or(ChangeType::Unknown);
            let (status, status_color) = change_status(change_type);
            let is_staged = matches!(section, ChangeSection::Staged);
            let is_selected = self.selected_file_path.as_ref() == Some(&node_path)
                && self.selected_file_section == Some(section);
            let is_hovered = self.hovered_file_path.as_ref() == Some(&node_path)
                && self.hovered_file_section == Some(section);

            let hover_path = node_path.clone();
            let click_path = node_path.clone();
            let right_click_path = node_path.clone();
            let stage_path = node_path.clone();
            let discard_path = node_path.clone();
            let change_info = node.change_info;

            let file_row = div()
                .id(format!(
                    "tree-file-{}-{}",
                    section_id,
                    node_path.to_string_lossy()
                ))
                .pl(px(indent as f32))
                .pr_2()
                .h_6()
                .rounded_sm()
                .when(is_selected, |el| el.bg(rgb(BG_SURFACE1)))
                .when(!is_selected && is_hovered, |el| el.bg(rgb(BG_SURFACE0)))
                .on_mouse_move(cx.listener(move |this, _: &gpui::MouseMoveEvent, _, cx| {
                    if this.hovered_file_path.as_ref() != Some(&hover_path)
                        || this.hovered_file_section != Some(section)
                    {
                        this.hovered_file_path = Some(hover_path.clone());
                        this.hovered_file_section = Some(section);
                        cx.notify();
                    }
                }))
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .id(format!(
                            "tree-open-{}-{}",
                            section_id,
                            node_path.to_string_lossy()
                        ))
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.on_file_selected(
                                click_path.clone(),
                                change_info.map(|i| i.section),
                                change_info.map(|i| i.change_type),
                                None,
                                cx,
                            );
                        }))
                        .on_mouse_down(
                            gpui::MouseButton::Right,
                            cx.listener(move |this, _, _, cx| {
                                let s = format!("`{}`", right_click_path.to_string_lossy());
                                this.send_to_terminal(&s, cx);
                            }),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_color(rgb(TEXT))
                                .text_xs()
                                .truncate()
                                .child(node_name),
                        ),
                );

            // Discardボタン (未ステージのみ)
            // 追跡済み → git restore、未追跡 (Added=??) → git clean -f
            let file_row = if !is_staged {
                let discard_btn: AnyElement = if is_hovered {
                    div()
                        .id(format!(
                            "tree-discard-{}-{}",
                            section_id,
                            discard_path.to_string_lossy()
                        ))
                        .flex_shrink_0()
                        .size(px(20.0))
                        .cursor_pointer()
                        .rounded_sm()
                        .hover(|b| b.bg(rgb(BG_SURFACE1)))
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_discard_confirm(discard_path.clone(), change_type, cx);
                        }))
                        .child(icon::close().size(px(12.0)).text_color(rgb(RED)))
                        .into_any_element()
                } else {
                    div().w(px(20.0)).flex_shrink_0().into_any_element()
                };
                file_row.child(discard_btn)
            } else {
                file_row
            };

            let stage_btn: AnyElement = if is_hovered {
                div()
                    .id(format!(
                        "tree-stage-{}-{}",
                        section_id,
                        stage_path.to_string_lossy()
                    ))
                    .flex_shrink_0()
                    .size(px(20.0))
                    .cursor_pointer()
                    .rounded_sm()
                    .hover(|b| b.bg(rgb(BG_SURFACE1)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_file_staging(stage_path.clone(), is_staged, cx);
                    }))
                    .child(
                        if is_staged {
                            icon::arrow_down()
                        } else {
                            icon::arrow_up()
                        }
                        .size(px(12.0))
                        .text_color(if is_staged {
                            rgb(BLUE)
                        } else {
                            rgb(GREEN)
                        }),
                    )
                    .into_any_element()
            } else {
                div().w(px(20.0)).flex_shrink_0().into_any_element()
            };
            let file_row = file_row.child(stage_btn);

            // ステータス文字
            let file_row = file_row.child(
                div()
                    .w_3()
                    .text_right()
                    .text_xs()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(status_color))
                    .child(status),
            );

            result = result.child(file_row);
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
            .id("all-files-tree-scroll")
            .flex_1()
            .overflow_scroll()
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
        let indent = depth * 14;
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
                .pr_2()
                .h_6()
                .cursor_pointer()
                .hover(|el| el.bg(rgb(BG_SURFACE0)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_dir_expanded(&click_path);
                    cx.notify();
                }))
                .on_mouse_move(cx.listener(move |this, _: &gpui::MouseMoveEvent, _, cx| {
                    if this.hovered_file_path.is_some() || this.hovered_file_section.is_some() {
                        this.hovered_file_path = None;
                        this.hovered_file_section = None;
                        cx.notify();
                    }
                }))
                .flex()
                .items_center()
                .gap_1();
            let (arrow, spacer) = render_dir_icons(is_expanded);
            let node_element = node_element
                .child(arrow)
                .child(spacer)
                .child(div().text_color(rgb(TEXT)).text_xs().child(node_name));

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
            let is_selected = self.selected_file_path.as_ref() == Some(&relative_path)
                && self.selected_file_section.is_none();
            let hover_path = relative_path.clone();
            let is_hovered = self.hovered_file_path.as_ref() == Some(&relative_path)
                && self.hovered_file_section.is_none();

            let node_element = div()
                .id(format!("lazy-file-{}", path.to_string_lossy()))
                .pl(px(indent as f32))
                .pr_2()
                .h_6()
                .cursor_pointer()
                .rounded_sm()
                .when(is_selected, |el| el.bg(rgb(BG_SURFACE1)))
                .when(!is_selected && is_hovered, |el| el.bg(rgb(BG_SURFACE0)))
                .on_mouse_move(cx.listener(move |this, _: &gpui::MouseMoveEvent, _, cx| {
                    if this.hovered_file_path.as_ref() != Some(&hover_path)
                        || this.hovered_file_section.is_some()
                    {
                        this.hovered_file_path = Some(hover_path.clone());
                        this.hovered_file_section = None;
                        cx.notify();
                    }
                }))
                .hover(|el| el.bg(rgb(BG_SURFACE0)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.on_file_selected(click_path.clone(), None, None, None, cx);
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
                .gap_1()
                .child(
                    div()
                        .flex_1()
                        .text_color(rgb(TEXT))
                        .text_xs()
                        .truncate()
                        .child(node_name),
                );

            result = result.child(node_element);
        }

        result.into_any_element()
    }
}
