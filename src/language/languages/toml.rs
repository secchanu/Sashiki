use crate::language::config::{LanguageConfig, TreeSitterSpec};

pub fn config() -> LanguageConfig {
    LanguageConfig {
        id: "toml",
        extensions: &["toml"],
        tree_sitter: Some(TreeSitterSpec {
            language_fn: || tree_sitter_toml_ng::LANGUAGE.into(),
            highlights_query: tree_sitter_toml_ng::HIGHLIGHTS_QUERY.into(),
            injections_query: None,
            locals_query: None,
            extra_capture_names: &[],
        }),
        lsp: None,
    }
}
