use crate::language::config::{LanguageConfig, LspServerSpec, TreeSitterSpec};

pub fn config() -> LanguageConfig {
    LanguageConfig {
        id: "python",
        extensions: &["py"],
        tree_sitter: Some(TreeSitterSpec {
            language_fn: || tree_sitter_python::LANGUAGE.into(),
            highlights_query: tree_sitter_python::HIGHLIGHTS_QUERY.into(),
            injections_query: None,
            locals_query: None,
            extra_capture_names: &[],
        }),
        lsp: Some(LspServerSpec {
            server_id: "pyright",
            command: "pyright-langserver",
            args: &["--stdio"],
            language_id: None,
        }),
    }
}
