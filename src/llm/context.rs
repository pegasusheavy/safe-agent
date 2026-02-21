use crate::skills::PromptSkill;
use crate::tools::ToolRegistry;

/// Per-call generation context passed to every LlmBackend::generate invocation.
///
/// Bundles all per-request inputs so the LlmBackend trait signature stays
/// stable as new context fields are added.  All fields are references with
/// the lifetime of the call site — no heap allocation required.
pub struct GenerateContext<'a> {
    /// The full prompt text (may be a multi-turn conversation string).
    pub message: &'a str,
    /// Tool registry; when Some the system prompt includes tool schemas.
    pub tools: Option<&'a ToolRegistry>,
    /// Prompt skills resolved for this specific request.  May be empty.
    pub prompt_skills: &'a [PromptSkill],
}
