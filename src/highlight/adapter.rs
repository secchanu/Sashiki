use std::ops::Range;

use tree_sitter_highlight::{Highlight, HighlightConfiguration, HighlightEvent, Highlighter};

use crate::language::config::LanguageConfig;

use super::theme_map;

#[derive(Clone, Debug, PartialEq)]
pub struct HighlightedSpan {
    pub range: Range<usize>,
    pub style: gpui::HighlightStyle,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HighlightedLine {
    pub text: String,
    pub spans: Vec<HighlightedSpan>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HighlightedDoc {
    pub lines: Vec<HighlightedLine>,
}

#[derive(Copy, Clone, Debug)]
struct LineMeta {
    start: usize,
    end: usize,
}

pub fn highlight_source(
    source: &str,
    config: &LanguageConfig,
    capture_names: &[String],
) -> Option<HighlightedDoc> {
    let tree_sitter = config.tree_sitter.as_ref()?;
    let mut highlighter = Highlighter::new();
    let mut highlight_config = HighlightConfiguration::new(
        (tree_sitter.language_fn)(),
        config.id,
        &tree_sitter.highlights_query,
        tree_sitter.injections_query.unwrap_or_default(),
        tree_sitter.locals_query.unwrap_or_default(),
    )
    .ok()?;

    highlight_config.configure(capture_names);

    let events = highlighter
        .highlight(&highlight_config, source.as_bytes(), None, |_| None)
        .ok()?;

    let (mut lines, line_meta) = build_lines(source);
    let mut highlight_stack: Vec<Highlight> = Vec::new();

    for event in events {
        match event.ok()? {
            HighlightEvent::HighlightStart(highlight) => highlight_stack.push(highlight),
            HighlightEvent::HighlightEnd => {
                let _ = highlight_stack.pop();
            }
            HighlightEvent::Source { start, end } => {
                if start >= end {
                    continue;
                }

                // ハイライトスタックが空 = tree-sitterがキャプチャしていない領域。
                // spanを作らないことで、クリック対象から除外し不要なgo-to-definitionを防ぐ。
                let Some(highlight) = highlight_stack.last() else {
                    continue;
                };
                let style = theme_map::highlight_style_for_capture(highlight.0, capture_names);

                push_span_for_byte_range(start, end, style, &mut lines, &line_meta);
            }
        }
    }

    Some(HighlightedDoc { lines })
}

fn build_lines(source: &str) -> (Vec<HighlightedLine>, Vec<LineMeta>) {
    let segments: Vec<&str> = source.split('\n').collect();
    let mut lines = Vec::with_capacity(segments.len());
    let mut line_meta = Vec::with_capacity(segments.len());
    let mut start = 0usize;

    for (index, segment) in segments.iter().enumerate() {
        // Strip trailing \r so span byte offsets match the displayed text
        let text = segment.strip_suffix('\r').unwrap_or(segment);
        let text_end = start + text.len();
        lines.push(HighlightedLine {
            text: text.to_string(),
            spans: Vec::new(),
        });
        // LineMeta covers only the visible text portion (excludes \r)
        line_meta.push(LineMeta {
            start,
            end: text_end,
        });

        let raw_end = start + segment.len();
        let has_newline = index + 1 < segments.len();
        start = raw_end + usize::from(has_newline);
    }

    (lines, line_meta)
}

fn push_span_for_byte_range(
    start: usize,
    end: usize,
    style: gpui::HighlightStyle,
    lines: &mut [HighlightedLine],
    line_meta: &[LineMeta],
) {
    if line_meta.is_empty() {
        return;
    }

    let mut cursor = start;
    let mut line_index = line_meta
        .partition_point(|meta| meta.start <= cursor)
        .saturating_sub(1);

    while cursor < end && line_index < line_meta.len() {
        let meta = line_meta[line_index];

        if cursor < meta.start {
            cursor = meta.start;
        }

        if cursor >= meta.end {
            if line_index + 1 >= line_meta.len() {
                break;
            }
            line_index += 1;
            cursor = line_meta[line_index].start.max(cursor.saturating_add(1));
            continue;
        }

        let segment_end = end.min(meta.end);
        if segment_end > cursor {
            lines[line_index].spans.push(HighlightedSpan {
                range: (cursor - meta.start)..(segment_end - meta.start),
                style,
            });
        }
        cursor = segment_end;

        if cursor < end && cursor >= meta.end {
            if line_index + 1 >= line_meta.len() {
                break;
            }
            line_index += 1;
            cursor = line_meta[line_index].start;
        }
    }
}
