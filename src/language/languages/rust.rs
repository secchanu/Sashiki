use crate::language::config::{LanguageConfig, LspServerSpec, TreeSitterSpec};

pub fn config() -> LanguageConfig {
    LanguageConfig {
        id: "rust",
        extensions: &["rs"],
        tree_sitter: Some(TreeSitterSpec {
            language_fn: || tree_sitter_rust::LANGUAGE.into(),
            highlights_query: tree_sitter_rust::HIGHLIGHTS_QUERY.into(),
            injections_query: Some(tree_sitter_rust::INJECTIONS_QUERY),
            locals_query: None,
            extra_capture_names: &[],
        }),
        lsp: Some(LspServerSpec {
            server_id: "rust-analyzer",
            command: "rust-analyzer",
            args: &[],
            language_id: None,
        }),
    }
}
