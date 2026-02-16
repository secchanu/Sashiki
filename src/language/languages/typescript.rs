use crate::language::config::{LanguageConfig, LspServerSpec, TreeSitterSpec};

/// TypeScript-specific queries come first for higher priority,
/// then JavaScript base queries provide the bulk of highlighting.
fn ts_highlights() -> String {
    [
        tree_sitter_typescript::HIGHLIGHTS_QUERY,
        tree_sitter_javascript::HIGHLIGHT_QUERY,
    ]
    .join("\n")
}

fn tsx_highlights() -> String {
    [
        tree_sitter_typescript::HIGHLIGHTS_QUERY,
        tree_sitter_javascript::HIGHLIGHT_QUERY,
        tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
    ]
    .join("\n")
}

pub fn config() -> LanguageConfig {
    LanguageConfig {
        id: "typescript",
        extensions: &["ts", "mts", "cts"],
        tree_sitter: Some(TreeSitterSpec {
            language_fn: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            highlights_query: ts_highlights().into(),
            injections_query: None,
            locals_query: Some(tree_sitter_typescript::LOCALS_QUERY),
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

pub fn tsx_config() -> LanguageConfig {
    LanguageConfig {
        id: "typescriptreact",
        extensions: &["tsx"],
        tree_sitter: Some(TreeSitterSpec {
            language_fn: || tree_sitter_typescript::LANGUAGE_TSX.into(),
            highlights_query: tsx_highlights().into(),
            injections_query: None,
            locals_query: Some(tree_sitter_typescript::LOCALS_QUERY),
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

pub fn jsx_config() -> LanguageConfig {
    LanguageConfig {
        id: "javascriptreact",
        extensions: &["jsx"],
        tree_sitter: Some(TreeSitterSpec {
            language_fn: || tree_sitter_typescript::LANGUAGE_TSX.into(),
            highlights_query: tsx_highlights().into(),
            injections_query: None,
            locals_query: Some(tree_sitter_typescript::LOCALS_QUERY),
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
