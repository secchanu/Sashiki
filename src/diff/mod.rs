//! Diff calculation using the similar crate

use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLineType {
    Equal,
    Insert,
    Delete,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub line_type: DiffLineType,
    pub old_line_num: Option<usize>,
    pub new_line_num: Option<usize>,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct SideBySideLine {
    pub left_line_num: Option<usize>,
    pub left_content: Option<String>,
    pub left_type: DiffLineType,
    pub right_line_num: Option<usize>,
    pub right_content: Option<String>,
    pub right_type: DiffLineType,
}

#[derive(Debug, Clone)]
pub struct DiffResult {
    pub lines: Vec<DiffLine>,
    pub stats: DiffStats,
}

#[derive(Debug, Clone, Default)]
pub struct DiffStats {
    pub additions: usize,
    pub deletions: usize,
    pub unchanged: usize,
}

pub fn calculate_diff(old_text: &str, new_text: &str) -> DiffResult {
    let diff = TextDiff::from_lines(old_text, new_text);
    let mut lines = Vec::new();
    let mut stats = DiffStats::default();

    let mut old_line_num = 1usize;
    let mut new_line_num = 1usize;

    for change in diff.iter_all_changes() {
        let line_type = match change.tag() {
            ChangeTag::Equal => {
                stats.unchanged += 1;
                let line = DiffLine {
                    line_type: DiffLineType::Equal,
                    old_line_num: Some(old_line_num),
                    new_line_num: Some(new_line_num),
                    content: change.value().to_string(),
                };
                old_line_num += 1;
                new_line_num += 1;
                line
            }
            ChangeTag::Delete => {
                stats.deletions += 1;
                let line = DiffLine {
                    line_type: DiffLineType::Delete,
                    old_line_num: Some(old_line_num),
                    new_line_num: None,
                    content: change.value().to_string(),
                };
                old_line_num += 1;
                line
            }
            ChangeTag::Insert => {
                stats.additions += 1;
                let line = DiffLine {
                    line_type: DiffLineType::Insert,
                    old_line_num: None,
                    new_line_num: Some(new_line_num),
                    content: change.value().to_string(),
                };
                new_line_num += 1;
                line
            }
        };
        lines.push(line_type);
    }

    DiffResult { lines, stats }
}

pub fn to_side_by_side(diff_result: &DiffResult) -> Vec<SideBySideLine> {
    let mut result = Vec::new();
    let mut i = 0;
    let lines = &diff_result.lines;

    while i < lines.len() {
        match lines[i].line_type {
            DiffLineType::Equal => {
                result.push(SideBySideLine {
                    left_line_num: lines[i].old_line_num,
                    left_content: Some(lines[i].content.clone()),
                    left_type: DiffLineType::Equal,
                    right_line_num: lines[i].new_line_num,
                    right_content: Some(lines[i].content.clone()),
                    right_type: DiffLineType::Equal,
                });
                i += 1;
            }
            DiffLineType::Delete => {
                let mut deletes = Vec::new();
                let mut inserts = Vec::new();

                while i < lines.len() && lines[i].line_type == DiffLineType::Delete {
                    deletes.push(&lines[i]);
                    i += 1;
                }
                while i < lines.len() && lines[i].line_type == DiffLineType::Insert {
                    inserts.push(&lines[i]);
                    i += 1;
                }

                let max_len = deletes.len().max(inserts.len());
                for j in 0..max_len {
                    let left = deletes.get(j);
                    let right = inserts.get(j);

                    result.push(SideBySideLine {
                        left_line_num: left.and_then(|l| l.old_line_num),
                        left_content: left.map(|l| l.content.clone()),
                        left_type: if left.is_some() {
                            DiffLineType::Delete
                        } else {
                            DiffLineType::Equal
                        },
                        right_line_num: right.and_then(|l| l.new_line_num),
                        right_content: right.map(|l| l.content.clone()),
                        right_type: if right.is_some() {
                            DiffLineType::Insert
                        } else {
                            DiffLineType::Equal
                        },
                    });
                }
            }
            DiffLineType::Insert => {
                result.push(SideBySideLine {
                    left_line_num: None,
                    left_content: None,
                    left_type: DiffLineType::Equal,
                    right_line_num: lines[i].new_line_num,
                    right_content: Some(lines[i].content.clone()),
                    right_type: DiffLineType::Insert,
                });
                i += 1;
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_diff() {
        let old = "line1\nline2\nline3\n";
        let new = "line1\nmodified\nline3\n";

        let result = calculate_diff(old, new);

        assert_eq!(result.stats.unchanged, 2);
        assert_eq!(result.stats.deletions, 1);
        assert_eq!(result.stats.additions, 1);
    }

    #[test]
    fn test_no_changes() {
        let text = "same\ncontent\n";
        let result = calculate_diff(text, text);

        assert_eq!(result.stats.unchanged, 2);
        assert_eq!(result.stats.deletions, 0);
        assert_eq!(result.stats.additions, 0);
    }

    #[test]
    fn test_all_new() {
        let old = "";
        let new = "new line\n";

        let result = calculate_diff(old, new);

        assert_eq!(result.stats.additions, 1);
        assert_eq!(result.stats.deletions, 0);
    }

    #[test]
    fn test_all_deleted() {
        let old = "old line\n";
        let new = "";

        let result = calculate_diff(old, new);

        assert_eq!(result.stats.additions, 0);
        assert_eq!(result.stats.deletions, 1);
    }

    #[test]
    fn test_side_by_side() {
        let old = "line1\nold\nline3\n";
        let new = "line1\nnew\nline3\n";

        let result = calculate_diff(old, new);
        let sbs = to_side_by_side(&result);

        assert_eq!(sbs.len(), 3);
        assert_eq!(sbs[0].left_content, Some("line1\n".to_string()));
        assert_eq!(sbs[0].right_content, Some("line1\n".to_string()));
        assert_eq!(sbs[1].left_content, Some("old\n".to_string()));
        assert_eq!(sbs[1].right_content, Some("new\n".to_string()));
        assert_eq!(sbs[1].left_type, DiffLineType::Delete);
        assert_eq!(sbs[1].right_type, DiffLineType::Insert);
        assert_eq!(sbs[2].left_content, Some("line3\n".to_string()));
        assert_eq!(sbs[2].right_content, Some("line3\n".to_string()));
    }

    #[test]
    fn test_large_diff_performance() {
        let old: String = (0..10_000).map(|i| format!("line {}\n", i)).collect();
        let new: String = (0..10_000)
            .map(|i| {
                if i % 100 == 0 {
                    format!("modified line {}\n", i)
                } else {
                    format!("line {}\n", i)
                }
            })
            .collect();

        let start = std::time::Instant::now();
        let result = calculate_diff(&old, &new);
        let elapsed = start.elapsed();

        assert!(elapsed.as_millis() < 200);
        assert!(result.stats.additions > 0);
        assert!(result.stats.deletions > 0);
    }
}
