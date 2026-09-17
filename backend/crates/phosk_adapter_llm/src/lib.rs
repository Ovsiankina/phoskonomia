//! `phosk_adapter_llm` — the `LlmAdapter` **port** (ADR-008 constrained output,
//! ADR-010 layering).
//!
//! This is the L2 contract seam between feature code (the `phosk_ai` safe-module
//! and any future suggestion pipeline) and whatever language model actually
//! answers. It mirrors [`phosk_adapter_db`]: one async, object-safe trait that
//! feature crates hold as `Arc<dyn LlmAdapter + Send + Sync>`. Concrete adapters
//! (e.g. an Ollama/HTTP impl talking to `localhost:11434`) are wired in **only**
//! by a `bin/*` composition root or the Dioxus server context, and are **never**
//! imported here or by feature crates. Vendor types — the raw Ollama response
//! JSON, reqwest errors — die at the L3 adapter and never cross this seam; the
//! only JSON that crosses is a vendor-neutral [`serde_json::Value`] whose shape
//! the caller constrains via a JSON Schema.
//!
//! ## Two surfaces (small-model-friendly, deliberately flat)
//! - [`LlmAdapter::complete`] — free-text completion. Prompt in, text out. Use
//!   it for explanations and chat turns where the shape is prose.
//! - [`LlmAdapter::generate_structured`] — **constrained / structured output**
//!   (ADR-008). The caller passes a JSON Schema; the adapter must return JSON
//!   that validates against it. This is how the AI safe-module gets machine-
//!   readable suggestions it can schema-check *before* it ever enqueues an
//!   approval (AI write-tools ENQUEUE, never auto-write).
//!
//! Both are `async` (a model call is I/O) and return `Result<_, PhoskError>`
//! (the one taxonomy, ADR-010): an adapter maps its own transport / decode /
//! schema failures into a [`PhoskError`] and never panics.
//!
//! ## Testing without a live model
//! [`FakeLlm`] is a deterministic in-crate test double (no `async` runtime
//! beyond the caller's, no network). Feature crates depend on this port and can
//! script its answers, so their tests stay hermetic — there is no live model on
//! CI and the structured path must be exercisable offline.

use async_trait::async_trait;
use phosk_core::error::PhoskError;

mod fake;
pub use fake::FakeLlm;

/// The single language-model port: an async, object-safe trait the AI feature
/// code holds as `Arc<dyn LlmAdapter + Send + Sync>`. A model/vendor swap is a
/// new `impl` of this trait, never a change to feature code (ADR-010).
///
/// Object-safe by construction (async methods via [`async_trait`], no generic
/// methods, no `Self`-returning methods).
#[async_trait]
pub trait LlmAdapter: Send + Sync {
    /// A short, stable identifier for the model behind this adapter (e.g.
    /// `"qwen3.6:35b-custom"`). Used for provenance — every AI-derived artefact
    /// records *which* model produced it — and for surfacing the active model in
    /// diagnostics. Pure metadata; never makes a model call.
    fn model(&self) -> &str;

    /// Cheap liveness probe: `Ok(true)` if the model is reachable and ready to
    /// answer, `Ok(false)` if it is reachable but not ready (e.g. still warming
    /// up / pulling weights). A transport failure is an `Err(PhoskError)`, not
    /// `Ok(false)` — "unreachable" and "reachable-but-not-ready" are different
    /// states the caller may treat differently (retry vs. degrade).
    ///
    /// # Errors
    /// Returns a [`PhoskError`] if the health check itself fails to complete
    /// (e.g. the endpoint cannot be reached at all).
    async fn health(&self) -> Result<bool, PhoskError>;

    /// Free-text completion: send `prompt`, get the model's text answer back.
    ///
    /// The returned `String` is the model's raw completion with no envelope —
    /// trimming / post-processing is the caller's choice. An empty prompt is the
    /// adapter's contract to reject as [`PhoskError::Invalid`] (a model call with
    /// nothing to complete is a caller bug, not a degenerate success).
    ///
    /// # Errors
    /// Returns a [`PhoskError`] if the prompt is rejected
    /// ([`PhoskError::Invalid`]) or the underlying model call fails (transport /
    /// decode fault mapped into the taxonomy).
    async fn complete(&self, prompt: &str) -> Result<String, PhoskError>;

    /// **Constrained / structured output** (ADR-008): send `prompt` together with
    /// a `json_schema`, and get back a [`serde_json::Value`] that the adapter
    /// guarantees validates against that schema.
    ///
    /// `json_schema` is a JSON-Schema document (the same shape an Ollama
    /// `format` field or an `OpenAI` `response_format` takes). The adapter is
    /// responsible for steering the model to that shape *and* for verifying the
    /// model's output validates before returning it — a caller that asked for a
    /// schema must never receive off-schema JSON. This is the path the AI
    /// safe-module uses to get machine-readable suggestions it can map straight
    /// onto domain types and enqueue for human approval.
    ///
    /// # Errors
    /// Returns a [`PhoskError`] if the prompt or schema is rejected
    /// ([`PhoskError::Invalid`] — e.g. empty prompt, or a `json_schema` that is
    /// not a JSON object), or if the model produces output that cannot be made
    /// to satisfy the schema, or the underlying call fails. All map into the one
    /// taxonomy; the adapter never panics on bad model output.
    async fn generate_structured(
        &self,
        prompt: &str,
        json_schema: &serde_json::Value,
    ) -> Result<serde_json::Value, PhoskError>;
}
