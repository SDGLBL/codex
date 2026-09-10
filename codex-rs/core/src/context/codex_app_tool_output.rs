use super::ContextualUserFragment;
use codex_protocol::models::ContentItemKind;

/// A request-only representation of a standalone Codex App event for providers
/// that require every function-call output to reference a model-issued call.
/// Reuses the existing output verbatim rather than adding new event content or
/// truncating or rewriting the saved event.
pub(crate) struct CodexAppToolOutput<'a> {
    pub(crate) name: &'a str,
    pub(crate) output: &'a str,
}

impl ContextualUserFragment for CodexAppToolOutput<'_> {
    fn content_kind(&self) -> ContentItemKind {
        ContentItemKind("codex_app.tool_output".to_string())
    }

    fn role(&self) -> &'static str {
        "user"
    }

    fn markers(&self) -> (&'static str, &'static str) {
        Self::type_markers()
    }

    fn type_markers() -> (&'static str, &'static str) {
        ("<codex_app_tool_output>", "</codex_app_tool_output>")
    }

    fn body(&self) -> String {
        let Self { name, output } = self;
        format!("\nSource: codex_app.{name}\n\n{output}\n")
    }
}
