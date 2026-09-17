//! [`FakeLlm`] — a deterministic, network-less test double for [`LlmAdapter`].
//!
//! Feature crates depend on the L2 [`LlmAdapter`] port and must be testable with
//! no live model (CI has none, and the AI safe-module's schema/enqueue logic has
//! to be exercisable offline). `FakeLlm` answers without a runtime or socket:
//!
//! - **Scripted replies** — preload exact `prompt -> reply` pairs with
//!   [`FakeLlm::with_reply`]; an exact prompt match returns the scripted text.
//! - **Echo fallback** — any unscripted prompt yields a deterministic
//!   `"echo: {prompt}"`, so a test that doesn't care about content still gets a
//!   stable, non-empty answer.
//! - **Structured output** — [`LlmAdapter::generate_structured`] returns a JSON
//!   object whose keys are exactly the schema's declared `properties`, each
//!   filled with a deterministic placeholder of the declared `type`. This lets a
//!   caller's own schema-validation / field-mapping code run against realistic,
//!   on-schema shapes without a model. A scripted structured reply keyed by
//!   prompt (via [`FakeLlm::with_structured`]) overrides the derived shape.
//! - **Outage** — [`FakeLlm::reachable`] with `false` makes every call fail the
//!   way a down model does (a transport error, not `Ok(false)`), so callers can
//!   test their offline path without a socket.
//! - **Failing model** — [`FakeLlm::fail_completions`] keeps `health` answering
//!   but makes every generation call fail, like a model that cannot load or
//!   times out after the health check passed.
//!
//! It is intentionally tiny and synchronous-at-heart (the `async fn`s resolve
//! immediately) — no I/O, no clock, no randomness, so every test is repeatable.

use std::collections::HashMap;

use async_trait::async_trait;
use phosk_core::error::PhoskError;
use serde_json::{Map, Value};

use crate::LlmAdapter;

/// Deterministic in-crate test double for [`LlmAdapter`]. Construct with
/// [`FakeLlm::new`] (echo-only) or [`FakeLlm::default`], then optionally script
/// replies with [`FakeLlm::with_reply`] / [`FakeLlm::with_structured`].
///
/// # Examples
/// ```
/// use phosk_adapter_llm::{FakeLlm, LlmAdapter};
///
/// # async fn demo() -> Result<(), phosk_core::error::PhoskError> {
/// // Echo-only, default model name:
/// let llm = FakeLlm::new();
/// assert_eq!(llm.complete("hi").await?, "echo: hi");
///
/// // Scripted + custom model name (builder style):
/// let llm = FakeLlm::with_model("qwen3.6:35b-custom")
///     .with_reply("ping", "pong");
/// assert_eq!(llm.model(), "qwen3.6:35b-custom");
/// assert_eq!(llm.complete("ping").await?, "pong");
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct FakeLlm {
    model: String,
    replies: HashMap<String, String>,
    structured: HashMap<String, Value>,
    healthy: bool,
    reachable: bool,
    fail_completions: bool,
}

impl Default for FakeLlm {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeLlm {
    /// Default model identifier the fake reports unless overridden, matching the
    /// model installed on the dev box.
    pub const DEFAULT_MODEL: &'static str = "qwen3.6:35b-custom";

    /// A fresh echo-only fake reporting [`Self::DEFAULT_MODEL`] and healthy.
    #[must_use]
    pub fn new() -> Self {
        Self {
            model: Self::DEFAULT_MODEL.to_owned(),
            replies: HashMap::new(),
            structured: HashMap::new(),
            healthy: true,
            reachable: true,
            fail_completions: false,
        }
    }

    /// A fresh echo-only fake reporting `model` as its model identifier.
    #[must_use]
    pub fn with_model(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            ..Self::new()
        }
    }

    /// Script an exact-match free-text reply: a [`LlmAdapter::complete`] call
    /// whose `prompt` equals `prompt` returns `reply` verbatim. Builder-style.
    #[must_use]
    pub fn with_reply(mut self, prompt: impl Into<String>, reply: impl Into<String>) -> Self {
        self.replies.insert(prompt.into(), reply.into());
        self
    }

    /// Script an exact-match structured reply: a
    /// [`LlmAdapter::generate_structured`] call whose `prompt` equals `prompt`
    /// returns `value` verbatim (overriding the schema-derived placeholder
    /// shape). Builder-style.
    #[must_use]
    pub fn with_structured(mut self, prompt: impl Into<String>, value: Value) -> Self {
        self.structured.insert(prompt.into(), value);
        self
    }

    /// Make [`LlmAdapter::health`] report `healthy`. Builder-style; lets a test
    /// drive the not-ready branch. Default is healthy.
    #[must_use]
    pub const fn healthy(mut self, healthy: bool) -> Self {
        self.healthy = healthy;
        self
    }

    /// Make the fake behave like an unreachable model when `reachable` is
    /// `false`: [`LlmAdapter::health`], [`LlmAdapter::complete`] and
    /// [`LlmAdapter::generate_structured`] all return
    /// [`PhoskError::Invalid`], scripted replies included. Builder-style; the
    /// default is reachable.
    #[must_use]
    pub const fn reachable(mut self, reachable: bool) -> Self {
        self.reachable = reachable;
        self
    }

    /// Make the fake behave like a model that is reachable but cannot answer
    /// (it fails to load, or generation times out) when `fail` is `true`:
    /// [`LlmAdapter::health`] still reports the [`Self::healthy`] state, while
    /// [`LlmAdapter::complete`] and [`LlmAdapter::generate_structured`] return
    /// [`PhoskError::Invalid`]. Builder-style; the default is to answer.
    #[must_use]
    pub const fn fail_completions(mut self, fail: bool) -> Self {
        self.fail_completions = fail;
        self
    }

    /// The error every generation call returns while completions fail.
    fn generation_failure(&self) -> Result<(), PhoskError> {
        if self.fail_completions {
            Err(PhoskError::Invalid("fake llm completion failed".to_owned()))
        } else {
            Ok(())
        }
    }

    /// The transport-style error every call returns while unreachable.
    fn outage(&self) -> Result<(), PhoskError> {
        if self.reachable {
            Ok(())
        } else {
            Err(PhoskError::Invalid("fake llm unreachable".to_owned()))
        }
    }

    /// Build a placeholder JSON value of the `type` declared by a JSON-Schema
    /// node, recursing into `object`/`array`. Deterministic, content-free — just
    /// enough shape for a caller's validation/mapping code to run.
    fn placeholder_for(schema: &Value) -> Value {
        match schema.get("type").and_then(Value::as_str) {
            Some("string") => Value::String(String::new()),
            Some("integer") => Value::from(0_i64),
            Some("number") => Value::from(0_f64),
            Some("boolean") => Value::Bool(false),
            Some("array") => {
                // One representative element so array consumers see a sample.
                let item = schema
                    .get("items")
                    .map_or(Value::Null, Self::placeholder_for);
                Value::Array(vec![item])
            }
            Some("object") => Self::object_from_schema(schema),
            // Unknown / untyped node: a JSON null is the safest neutral fill.
            _ => Value::Null,
        }
    }

    /// Build an object whose keys are exactly the schema's `properties`, each
    /// filled by [`Self::placeholder_for`].
    fn object_from_schema(schema: &Value) -> Value {
        let mut out = Map::new();
        if let Some(props) = schema.get("properties").and_then(Value::as_object) {
            for (key, sub) in props {
                out.insert(key.clone(), Self::placeholder_for(sub));
            }
        }
        Value::Object(out)
    }
}

#[async_trait]
impl LlmAdapter for FakeLlm {
    fn model(&self) -> &str {
        &self.model
    }

    async fn health(&self) -> Result<bool, PhoskError> {
        self.outage()?;
        Ok(self.healthy)
    }

    async fn complete(&self, prompt: &str) -> Result<String, PhoskError> {
        self.outage()?;
        self.generation_failure()?;
        if prompt.is_empty() {
            return Err(PhoskError::Invalid("empty prompt".to_owned()));
        }
        Ok(self
            .replies
            .get(prompt)
            .cloned()
            .unwrap_or_else(|| format!("echo: {prompt}")))
    }

    async fn generate_structured(
        &self,
        prompt: &str,
        json_schema: &Value,
    ) -> Result<Value, PhoskError> {
        self.outage()?;
        self.generation_failure()?;
        if prompt.is_empty() {
            return Err(PhoskError::Invalid("empty prompt".to_owned()));
        }
        if !json_schema.is_object() {
            return Err(PhoskError::Invalid(
                "json_schema must be a JSON object".to_owned(),
            ));
        }
        // Scripted structured reply wins; otherwise derive a schema-shaped
        // placeholder (root is treated as an object schema).
        if let Some(scripted) = self.structured.get(prompt) {
            return Ok(scripted.clone());
        }
        Ok(Self::object_from_schema(json_schema))
    }
}
