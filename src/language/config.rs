pub struct LanguageConfig {
    pub id: &'static str,
    pub extensions: &'static [&'static str],
    pub tree_sitter: Option<TreeSitterSpec>,
    pub lsp: Option<LspServerSpec>,
}

pub struct TreeSitterSpec {
    pub language_fn: fn() -> tree_sitter::Language,
    pub highlights_query: std::borrow::Cow<'static, str>,
    pub injections_query: Option<&'static str>,
    pub locals_query: Option<&'static str>,
    pub extra_capture_names: &'static [&'static str],
}

pub struct LspServerSpec {
    pub server_id: &'static str,
    pub command: &'static str,
    pub args: &'static [&'static str],
    /// LSP languageId sent in textDocument/didOpen (e.g. "typescriptreact").
    /// Falls back to LanguageConfig::id if None.
    pub language_id: Option<&'static str>,
}
