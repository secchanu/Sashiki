use crate::language::config::{LanguageConfig, LspServerSpec, TreeSitterSpec};

pub fn config() -> LanguageConfig {
    LanguageConfig {
        id: "javascript",
        extensions: &["js", "mjs", "cjs"],
        tree_sitter: Some(TreeSitterSpec {
            language_fn: || tree_sitter_javascript::LANGUAGE.into(),
            highlights_query: [
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
            ]
            .join("\n")
            .into(),
            injections_query: Some(tree_sitter_javascript::INJECTIONS_QUERY),
            locals_query: Some(tree_sitter_javascript::LOCALS_QUERY),
            extra_capture_names: &[],
        }),
        lsp: Some(LspServerSpec {
            server_id: "typescript-language-server",
            command: "typescript-language-server",
            args: &["--stdio"],
            language_id: None,
        }),
    }
}
