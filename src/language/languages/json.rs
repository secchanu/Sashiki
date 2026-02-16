use crate::language::config::{LanguageConfig, TreeSitterSpec};

pub fn config() -> LanguageConfig {
    LanguageConfig {
        id: "json",
        extensions: &["json"],
        tree_sitter: Some(TreeSitterSpec {
            language_fn: || tree_sitter_json::LANGUAGE.into(),
            highlights_query: tree_sitter_json::HIGHLIGHTS_QUERY.into(),
            injections_query: None,
            locals_query: None,
            extra_capture_names: &[],
        }),
        lsp: None,
    }
}
