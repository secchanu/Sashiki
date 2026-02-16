use crate::language::config::{LanguageConfig, TreeSitterSpec};

pub fn config() -> LanguageConfig {
    LanguageConfig {
        id: "html",
        extensions: &["html", "htm"],
        tree_sitter: Some(TreeSitterSpec {
            language_fn: || tree_sitter_html::LANGUAGE.into(),
            highlights_query: tree_sitter_html::HIGHLIGHTS_QUERY.into(),
            injections_query: Some(tree_sitter_html::INJECTIONS_QUERY),
            locals_query: None,
            // tag.error: 不正なHTMLタグをエラー色で表示
            extra_capture_names: &["tag.error"],
        }),
        lsp: None,
    }
}
