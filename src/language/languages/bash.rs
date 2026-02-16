use crate::language::config::{LanguageConfig, TreeSitterSpec};

pub fn config() -> LanguageConfig {
    LanguageConfig {
        id: "bash",
        extensions: &["sh", "bash"],
        tree_sitter: Some(TreeSitterSpec {
            language_fn: || tree_sitter_bash::LANGUAGE.into(),
            highlights_query: tree_sitter_bash::HIGHLIGHT_QUERY.into(),
            injections_query: None,
            locals_query: None,
            extra_capture_names: &[],
        }),
        lsp: None,
    }
}
