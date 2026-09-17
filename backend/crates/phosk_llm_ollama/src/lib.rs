//! `phosk_llm_ollama` — the concrete **L3** [`LlmAdapter`] impl over a local
//! Ollama server (`http://localhost:11434`, ADR-008 constrained output /
//! ADR-010 layering).
//!
//! This is a composition-root-only crate: a `bin/*` entry point or the Dioxus
//! server context constructs an [`OllamaLlm`], boxes it as
//! `Arc<dyn LlmAdapter + Send + Sync>`, and hands that to feature code. Feature
//! crates never see this type, never see [`reqwest`], and never see the raw
//! Ollama response JSON — those vendor concerns are confined here and only the
//! port's vendor-neutral shapes (`String`, [`serde_json::Value`],
//! [`PhoskError`]) cross the seam.
//!
//! ## Endpoints used
//! - [`OllamaLlm::health`] → `GET /api/tags` with a short timeout. A successful,
//!   parseable response that lists this adapter's model ⇒ `Ok(true)`; reachable
//!   but the model is absent ⇒ `Ok(false)`; unreachable ⇒ `Err`.
//! - [`OllamaLlm::complete`] → `POST /api/generate` with `stream: false`.
//! - [`OllamaLlm::generate_structured`] → `POST /api/generate` with the caller's
//!   JSON Schema in Ollama's `format` field (constrained decoding). On a reply
//!   that does not parse as schema-shaped JSON the adapter performs **one**
//!   bounded repair retry (ADR-008 reject-vs-repair) before giving up with
//!   [`PhoskError::Invalid`].
//!
//! ## Model selection
//! Defaults to [`OllamaLlm::DEFAULT_MODEL`] (`"qwen3.6:35b-custom"`), overridable
//! at construction or via the `PHOSK_LLM_MODEL` environment variable
//! ([`OllamaLlm::from_env`]).

use async_trait::async_trait;
use phosk_adapter_llm::LlmAdapter;
use phosk_core::error::PhoskError;
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;

/// Default Ollama base URL on the dev box.
const DEFAULT_BASE_URL: &str = "http://localhost:11434";

/// Environment variable that overrides the model name.
const MODEL_ENV: &str = "PHOSK_LLM_MODEL";

/// Concrete [`LlmAdapter`] backed by a local Ollama HTTP server.
///
/// Cheap to clone the handle conceptually, but constructed once at a composition
/// root and shared as `Arc<dyn LlmAdapter>`. Holds a single pooled
/// [`reqwest::Client`].
#[derive(Debug, Clone)]
pub struct OllamaLlm {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

/// Subset of the Ollama `/api/generate` (`stream:false`) response we read.
#[derive(Debug, Deserialize)]
struct GenerateResponse {
    /// The model's text. For a structured request this is a JSON *string* that
    /// itself parses into the schema-shaped value.
    #[serde(default)]
    response: String,
    /// Thinking/reasoning output for *thinking-capable* models. A defensive
    /// fallback: even though structured requests disable thinking (`think:false`)
    /// to force the schema-constrained JSON into `response`, some thinking models
    /// still emit the final JSON here. If `response` is empty we fall back to
    /// this field rather than mis-report an empty completion.
    #[serde(default)]
    thinking: String,
}

impl GenerateResponse {
    /// The effective completion text: `response`, or `thinking` when `response`
    /// is blank (the thinking-model fallback described above).
    fn text(self) -> String {
        if self.response.trim().is_empty() {
            self.thinking
        } else {
            self.response
        }
    }
}

/// Subset of `/api/tags` we read for the health probe.
#[derive(Debug, Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<TagModel>,
}

#[derive(Debug, Deserialize)]
struct TagModel {
    #[serde(default)]
    name: String,
}

impl OllamaLlm {
    /// The model this adapter targets unless overridden.
    pub const DEFAULT_MODEL: &'static str = "qwen3.6:35b-custom";

    /// Health-probe timeout. Short: a probe must not block the caller on a hung
    /// or absent server.
    const HEALTH_TIMEOUT: Duration = Duration::from_secs(3);

    /// Generation timeout. Generous: the ~23 GB model loads slowly on first hit.
    const GENERATE_TIMEOUT: Duration = Duration::from_secs(300);

    /// Build an adapter against `base_url`, targeting `model`.
    ///
    /// # Errors
    /// Returns [`PhoskError::Invalid`] if `model` is empty or the HTTP client
    /// cannot be constructed.
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Result<Self, PhoskError> {
        let model = model.into();
        if model.trim().is_empty() {
            return Err(PhoskError::Invalid("ollama model name is empty".to_owned()));
        }
        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| PhoskError::Invalid(format!("could not build http client: {e}")))?;
        Ok(Self {
            client,
            base_url: base_url.into(),
            model,
        })
    }

    /// Build an adapter against the default local Ollama URL, taking the model
    /// from `PHOSK_LLM_MODEL` if set, else [`Self::DEFAULT_MODEL`].
    ///
    /// # Errors
    /// Returns [`PhoskError::Invalid`] if the resolved model name is empty or
    /// the HTTP client cannot be constructed.
    pub fn from_env() -> Result<Self, PhoskError> {
        let model = std::env::var(MODEL_ENV)
            .ok()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| Self::DEFAULT_MODEL.to_owned());
        Self::new(DEFAULT_BASE_URL, model)
    }

    /// `POST /api/generate` with `stream:false` and an optional `format` (the
    /// constrained-output JSON Schema). Returns the model's `response` text.
    async fn generate(&self, prompt: &str, format: Option<&Value>) -> Result<String, PhoskError> {
        let mut body = json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false,
        });
        if let Some(schema) = format {
            // `body` was just built as an object literal; insert defensively.
            if let Some(obj) = body.as_object_mut() {
                obj.insert("format".to_owned(), schema.clone());
                // Disable chain-of-thought for constrained output: thinking
                // models (e.g. qwen3.6) otherwise emit the schema-constrained
                // JSON into a `thinking` field and leave `response` empty. With
                // `think:false` the JSON lands in `response` where we read it.
                obj.insert("think".to_owned(), Value::Bool(false));
            }
        }

        let url = format!("{}/api/generate", self.base_url);
        let resp = self
            .client
            .post(&url)
            .timeout(Self::GENERATE_TIMEOUT)
            .json(&body)
            .send()
            .await
            .map_err(|e| PhoskError::Invalid(format!("ollama request failed: {e}")))?;

        if !resp.status().is_success() {
            let code = resp.status().as_u16();
            return Err(PhoskError::Invalid(format!("ollama returned http {code}")));
        }

        let parsed: GenerateResponse = resp
            .json()
            .await
            .map_err(|e| PhoskError::Invalid(format!("ollama response decode failed: {e}")))?;
        Ok(parsed.text())
    }

    /// Parse a model `response` string into JSON and confirm it is an object
    /// (the shape every `generate_structured` schema roots at). The string may
    /// carry surrounding whitespace; nothing else is tolerated.
    fn parse_structured(raw: &str) -> Result<Value, PhoskError> {
        let trimmed = raw.trim();
        let value: Value = serde_json::from_str(trimmed)
            .map_err(|e| PhoskError::Invalid(format!("model output is not valid json: {e}")))?;
        if !value.is_object() {
            return Err(PhoskError::Invalid(
                "model output json is not an object".to_owned(),
            ));
        }
        Ok(value)
    }
}

#[async_trait]
impl LlmAdapter for OllamaLlm {
    fn model(&self) -> &str {
        &self.model
    }

    async fn health(&self) -> Result<bool, PhoskError> {
        let url = format!("{}/api/tags", self.base_url);
        let resp = self
            .client
            .get(&url)
            .timeout(Self::HEALTH_TIMEOUT)
            .send()
            .await
            .map_err(|e| PhoskError::Invalid(format!("ollama unreachable: {e}")))?;

        if !resp.status().is_success() {
            // Reachable, but the API answered with an error status: treat as
            // a transport-level failure (the probe could not complete cleanly).
            let code = resp.status().as_u16();
            return Err(PhoskError::Invalid(format!(
                "ollama /api/tags returned http {code}"
            )));
        }

        let tags: TagsResponse = resp
            .json()
            .await
            .map_err(|e| PhoskError::Invalid(format!("ollama /api/tags decode failed: {e}")))?;

        // Reachable AND our target model is listed ⇒ ready. Reachable but the
        // model is absent (not yet pulled) ⇒ reachable-but-not-ready.
        let ready = tags.models.iter().any(|m| m.name == self.model);
        Ok(ready)
    }

    async fn complete(&self, prompt: &str) -> Result<String, PhoskError> {
        if prompt.is_empty() {
            return Err(PhoskError::Invalid("empty prompt".to_owned()));
        }
        self.generate(prompt, None).await
    }

    async fn generate_structured(
        &self,
        prompt: &str,
        json_schema: &Value,
    ) -> Result<Value, PhoskError> {
        if prompt.is_empty() {
            return Err(PhoskError::Invalid("empty prompt".to_owned()));
        }
        if !json_schema.is_object() {
            return Err(PhoskError::Invalid(
                "json_schema must be a JSON object".to_owned(),
            ));
        }

        // First attempt: constrained decode via Ollama's `format` field.
        let raw = self.generate(prompt, Some(json_schema)).await?;
        match Self::parse_structured(&raw) {
            Ok(value) => Ok(value),
            Err(_first_err) => {
                // ONE bounded repair retry (ADR-008 reject-vs-repair): re-ask,
                // showing the model its own malformed output and the schema, and
                // demand a single corrected JSON object. Still constrained by
                // `format`.
                let repair_prompt = format!(
                    "Your previous answer was not valid JSON for the required schema.\n\
                     Required JSON schema:\n{json_schema}\n\n\
                     Your previous (invalid) answer:\n{raw}\n\n\
                     Original request:\n{prompt}\n\n\
                     Reply with ONLY a single valid JSON object matching the schema, nothing else.",
                );
                let repaired = self.generate(&repair_prompt, Some(json_schema)).await?;
                Self::parse_structured(&repaired)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;

    #[test]
    fn rejects_empty_model() {
        let err = OllamaLlm::new(DEFAULT_BASE_URL, "  ").expect_err("empty model must reject");
        assert!(matches!(err, PhoskError::Invalid(_)));
    }

    #[test]
    fn from_env_builds_a_nonempty_model() {
        // We must not mutate process env under `forbid(unsafe_code)` (the env
        // setters are `unsafe` in edition 2024). So we only assert the invariant
        // `from_env` must always uphold: it yields a usable adapter whose model
        // name is non-empty (either the ambient override or the default).
        let llm = OllamaLlm::from_env().expect("from_env builds");
        assert!(!llm.model().trim().is_empty());
        // With no override set in this suite's environment it is the default.
        if std::env::var(MODEL_ENV).is_err() {
            assert_eq!(llm.model(), OllamaLlm::DEFAULT_MODEL);
        }
    }

    #[test]
    fn parse_structured_accepts_object_rejects_scalar() {
        let ok = OllamaLlm::parse_structured("  {\"a\": 1}  ").expect("object parses");
        assert_eq!(ok, json!({"a": 1}));

        assert!(OllamaLlm::parse_structured("42").is_err());
        assert!(OllamaLlm::parse_structured("not json").is_err());
        assert!(OllamaLlm::parse_structured("[1,2,3]").is_err());
    }

    #[test]
    fn model_reports_configured_name() {
        let llm = OllamaLlm::new(DEFAULT_BASE_URL, "gemma4:26b-custom").expect("builds");
        assert_eq!(llm.model(), "gemma4:26b-custom");
    }
}
