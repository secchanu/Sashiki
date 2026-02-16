pub mod config;
pub mod languages;

use std::path::Path;

use config::LanguageConfig;

pub const COMMON_CAPTURE_NAMES: &[&str] = &[
    "attribute",
    "boolean",
    "comment",
    "comment.documentation",
    "constant",
    "constant.builtin",
    "constructor",
    "embedded",
    "escape",
    "function",
    "function.builtin",
    "function.macro",
    "function.method",
    "keyword",
    "label",
    "namespace",
    "number",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "string",
    "string.special",
    "string.special.key",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
];

pub struct LanguageRegistry {
    pub languages: Vec<LanguageConfig>,
}

impl LanguageRegistry {
    pub fn new() -> Self {
        let mut languages = Vec::new();

        #[cfg(feature = "lang-rust")]
        languages.push(languages::rust::config());

        #[cfg(feature = "lang-typescript")]
        {
            languages.push(languages::typescript::config());
            languages.push(languages::typescript::tsx_config());
            languages.push(languages::typescript::jsx_config());
        }

        #[cfg(feature = "lang-javascript")]
        languages.push(languages::javascript::config());

        #[cfg(feature = "lang-python")]
        languages.push(languages::python::config());

        #[cfg(feature = "lang-json")]
        languages.push(languages::json::config());

        #[cfg(feature = "lang-bash")]
        languages.push(languages::bash::config());

        #[cfg(feature = "lang-toml")]
        languages.push(languages::toml::config());

        #[cfg(feature = "lang-css")]
        languages.push(languages::css::config());

        #[cfg(feature = "lang-html")]
        languages.push(languages::html::config());

        Self { languages }
    }

    pub fn detect(&self, path: &Path) -> Option<&LanguageConfig> {
        let extension = path.extension()?.to_str()?;
        self.languages.iter().find(|config| {
            config
                .extensions
                .iter()
                .any(|ext| ext.eq_ignore_ascii_case(extension))
        })
    }
}

pub fn capture_names_for(config: &LanguageConfig) -> Vec<String> {
    let mut names = COMMON_CAPTURE_NAMES
        .iter()
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();

    if let Some(spec) = config.tree_sitter.as_ref() {
        names.extend(
            spec.extra_capture_names
                .iter()
                .map(|name| (*name).to_string()),
        );
    }

    names
}
