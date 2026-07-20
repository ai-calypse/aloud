//! Chat completions against any OpenAI-compatible endpoint.
//!
//! Ported from `PostProcessingService.swift` + `LLMCooldownManager.swift`.
//! Used for transcript cleanup, rewriting a selection, Ask, and meeting
//! summaries —
//! the Swift app hit the same Groq endpoint for all four.

pub mod ask;
pub mod cleanup;
pub mod cooldown;
pub mod guard;
pub mod prompts;
pub mod trigger;

use anyhow::{anyhow, Context, Result};
use cooldown::CooldownTracker;
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tracing::Instrument;

pub const DEFAULT_BASE_URL: &str = "https://api.groq.com/openai/v1";
const TIMEOUT: Duration = Duration::from_secs(30);

/// The transcript was empty or pure filler. The cleanup prompt is specified to
/// return exactly this sentinel, which callers treat as "paste nothing".
pub const EMPTY_SENTINEL: &str = "EMPTY";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    /// A tool-role message carries only a result string; an assistant message
    /// that only calls tools has no content, hence the option.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Present on an assistant message that decided to call tools. Passed back
    /// verbatim in the next request so the model sees its own call history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Present on a tool-role message: which call this result answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// One tool call the model requested, in OpenAI's function-calling shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", default = "default_tool_type")]
    pub kind: String,
    pub function: FunctionCall,
    /// Provider-specific data that must be echoed back verbatim on the next
    /// turn. Gemini 3 puts a `thought_signature` here and rejects the whole
    /// request without it — "Function call is missing a thought_signature",
    /// HTTP 400, on the second round of every tool loop. Opaque on purpose:
    /// this crate does not interpret it, it just carries it back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_content: Option<serde_json::Value>,
}

fn default_tool_type() -> String {
    "function".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    /// Raw JSON string of arguments, exactly as the model emitted it.
    pub arguments: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self::text(Role::System, content)
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self::text(Role::User, content)
    }

    fn text(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// A tool result fed back to the model, keyed to the call it answers.
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("model {model} is rate-limited, retry in {}s", retry_after.as_secs())]
    RateLimited {
        model: String,
        retry_after: Duration,
    },
    #[error("request failed ({status}): {body}")]
    Failed { status: u16, body: String },
    #[error("model returned empty output")]
    EmptyOutput,
}

pub struct LlmClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    cooldowns: Arc<CooldownTracker>,
}

/// One connection pool for the process. A client is built per request — once
/// per dictation for cleanup, again for a rewrite or an Ask — and a fresh
/// `reqwest::Client` means a fresh pool, so every one of those paid a full
/// TCP+TLS handshake. Measured against Groq: 85-130ms per request, twice per
/// dictation once transcription is counted. `reqwest::Client` is an `Arc`
/// inside, so cloning is free and the pool is shared.
static HTTP: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();

/// The breaker, process-wide for the same reason. Per-client it was born empty
/// on every call and died at the end of it, which made `is_limited` always
/// false — so a rate limit was rediscovered with a wasted request every single
/// dictation, exactly the cost the code was written to avoid.
///
/// ponytail: in-memory only, so a restart forgets a cooldown. Persisting to
/// the store is the upgrade path if a daily limit proves annoying.
static COOLDOWNS: std::sync::OnceLock<Arc<CooldownTracker>> = std::sync::OnceLock::new();

fn shared_http() -> Result<reqwest::Client> {
    // No client-wide timeout: the two callers disagree (20s for transcription,
    // 30s here), so it is set per request instead.
    match HTTP.get() {
        Some(client) => Ok(client.clone()),
        None => {
            let client = reqwest::Client::builder()
                .build()
                .context("building http client")?;
            Ok(HTTP.get_or_init(|| client).clone())
        }
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [Message],
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
    /// Tool schemas (OpenAI function format). Omitted on the plain chat path.
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<&'a [serde_json::Value]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<&'a str>,
    /// Both omitted unless [`reasoning_for`] knows the model — sending
    /// `reasoning_effort` to a model that does not reason is a 400.
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    include_reasoning: Option<bool>,
}

/// How much a given model should think, ported from voice-dict's
/// `ModelConfiguration`. The Rust port carried the model *names* across and
/// dropped the per-model request config with them.
///
/// This is worth real time: the default cleanup model reasons by default, and
/// cleanup is a deterministic rewrite at temperature 0 with nothing to reason
/// about. Measured against Groq, gpt-oss-20b spent ~185 reasoning tokens to
/// produce a twelve-word answer — 854ms, against 487ms at "low".
///
/// Keyed on the model rather than the call site because
/// `complete_with_fallback` re-enters `complete` with a *different* model, and
/// the two do not accept the same values: qwen takes "none" and rejects "low".
/// Anything unrecognised sends neither field, so a user-set model or a
/// non-Groq base URL is unaffected.
fn reasoning_for(model: &str) -> (Option<&'static str>, Option<bool>) {
    match model {
        "openai/gpt-oss-20b" | "openai/gpt-oss-120b" => (Some("low"), Some(false)),
        "qwen/qwen3.6-27b" | "qwen/qwen3-32b" => (Some("none"), Some(false)),
        _ => (None, None),
    }
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCall>>,
}

/// The model's reply to a tool-enabled turn: either final content, or a set of
/// tool calls to run and feed back.
#[derive(Debug, Clone)]
pub struct ToolTurn {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

impl LlmClient {
    pub fn new(api_key: impl Into<String>, base_url: Option<&str>) -> Result<Self> {
        Ok(Self {
            client: shared_http()?,
            base_url: base_url
                .unwrap_or(DEFAULT_BASE_URL)
                .trim_end_matches('/')
                .to_string(),
            api_key: api_key.into(),
            cooldowns: COOLDOWNS
                .get_or_init(|| Arc::new(CooldownTracker::new()))
                .clone(),
        })
    }

    /// The cooldown key. The endpoint is part of it because the tracker is now
    /// process-wide and the app talks to two providers — a bare model name
    /// would let Groq's limit suppress the same-named model at Gemini.
    fn cooldown_key(&self, model: &str) -> String {
        format!("{}|{model}", self.base_url)
    }

    /// Turns a non-2xx response into the right [`LlmError`], registering a
    /// 429's cooldown on the way through so the breaker learns from every
    /// attempt. A success passes straight back.
    ///
    /// Every request path goes through this. Written out three times, the
    /// rate-limit branch could be fixed in one and left stale in the others —
    /// and it is the branch a user only hits when they are already rate
    /// limited, which is the worst time to find out it drifted.
    async fn check(&self, model: &str, response: reqwest::Response) -> Result<reqwest::Response> {
        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let cooldown = cooldown::from_headers(response.headers());
            self.cooldowns.set(&self.cooldown_key(model), cooldown);
            return Err(LlmError::RateLimited {
                model: model.to_string(),
                retry_after: cooldown.duration,
            }
            .into());
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::Failed {
                status: status.as_u16(),
                body,
            }
            .into());
        }
        Ok(response)
    }

    /// One chat completion.
    ///
    /// A 429 registers the model's cooldown before returning
    /// [`LlmError::RateLimited`], so the breaker learns from every attempt —
    /// including fallback attempts, which call back through here.
    pub async fn complete(
        &self,
        model: &str,
        messages: &[Message],
        temperature: Option<f32>,
    ) -> Result<String> {
        let (reasoning_effort, include_reasoning) = reasoning_for(model);
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            // Per request, not on the client: the client is shared with the
            // transcribe crate, which wants a different one.
            .timeout(TIMEOUT)
            .bearer_auth(&self.api_key)
            .json(&ChatRequest {
                model,
                messages,
                temperature,
                stream: false,
                tools: None,
                tool_choice: None,
                reasoning_effort,
                include_reasoning,
            })
            .send()
            .await
            .context("chat request failed")?;

        let response = self.check(model, response).await?;

        let parsed: ChatResponse = response.json().await.context("decoding chat response")?;
        let content = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| anyhow!("missing choices[0].message.content"))?;

        Ok(prompts::strip_think_tags(&content).trim().to_string())
    }

    /// Tries `model`, then `fallback` if the first is rate-limited.
    ///
    /// Ported from the Swift fallback path: a rate limit is the *only* reason
    /// to switch models. A genuine failure (bad key, malformed request) would
    /// fail identically on the fallback, so it surfaces immediately.
    pub async fn complete_with_fallback(
        &self,
        model: &str,
        fallback: Option<&str>,
        messages: &[Message],
        temperature: Option<f32>,
    ) -> Result<String> {
        // Skip a model already known to be limited rather than spending a
        // request to rediscover it.
        if !self.cooldowns.is_limited(&self.cooldown_key(model)) {
            match self.complete(model, messages, temperature).await {
                Err(e) if is_rate_limited(&e) => {
                    tracing::info!(model, "rate-limited; trying fallback");
                }
                result => return result,
            }
        }

        let fallback = fallback
            .ok_or_else(|| anyhow!("{model} is rate-limited and no fallback model is set"))?;

        // Spending a request to rediscover a limit we already recorded costs
        // the caller a full timeout — up to 30s on top of the 30s the primary
        // just burned, with the pill up and uncancellable the whole time.
        if let Some(wait) = self.cooldowns.remaining(&self.cooldown_key(fallback)) {
            return Err(LlmError::RateLimited {
                model: fallback.to_string(),
                retry_after: wait,
            }
            .into());
        }
        // The retry is invisible in per-stage durations but doubles what the
        // user waits, so it gets its own span rather than hiding inside the
        // caller's.
        self.complete(fallback, messages, temperature)
            .instrument(tracing::info_span!(
                "llm_fallback",
                primary = model,
                fallback
            ))
            .await
    }

    /// One tool-enabled chat turn. Sends the tool schemas with `tool_choice`
    /// "auto"; the model either answers (`content`) or asks to call tools
    /// (`tool_calls`). Pass `tools = None` on the final round to force an answer.
    ///
    /// A 429 surfaces as `LlmError::RateLimited` immediately — the tool loop
    /// owns the retry, because it has the event channel to tell the UI about
    /// the wait. Sleeping here made the panel look hung for 30s.
    pub async fn chat_with_tools(
        &self,
        model: &str,
        messages: &[Message],
        tools: Option<&[serde_json::Value]>,
        temperature: Option<f32>,
    ) -> Result<ToolTurn> {
        let (reasoning_effort, include_reasoning) = reasoning_for(model);
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            // Per request, not on the client: the client is shared with the
            // transcribe crate, which wants a different one.
            .timeout(TIMEOUT)
            .bearer_auth(&self.api_key)
            .json(&ChatRequest {
                model,
                messages,
                temperature,
                stream: false,
                tools,
                tool_choice: tools.map(|_| "auto"),
                reasoning_effort,
                include_reasoning,
            })
            .send()
            .await
            .context("tool chat request failed")?;

        let response = self.check(model, response).await?;

        let parsed: ChatResponse = response.json().await.context("decoding tool response")?;
        let message = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message)
            .ok_or_else(|| anyhow!("missing choices[0].message"))?;

        Ok(ToolTurn {
            content: message
                .content
                .map(|c| prompts::strip_think_tags(&c).trim().to_string()),
            tool_calls: message.tool_calls.unwrap_or_default(),
        })
    }

    /// Streams a completion, yielding incremental text deltas.
    ///
    /// Used by Ask mode, where the answer renders as it arrives.
    pub async fn stream(
        &self,
        model: &str,
        messages: &[Message],
        temperature: Option<f32>,
    ) -> Result<impl Stream<Item = Result<String>>> {
        let (reasoning_effort, include_reasoning) = reasoning_for(model);
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            // Per request, not on the client: the client is shared with the
            // transcribe crate, which wants a different one.
            .timeout(TIMEOUT)
            .bearer_auth(&self.api_key)
            .json(&ChatRequest {
                model,
                messages,
                temperature,
                stream: true,
                tools: None,
                tool_choice: None,
                reasoning_effort,
                include_reasoning,
            })
            .send()
            .await
            .context("stream request failed")?;

        let response = self.check(model, response).await?;

        Ok(sse_deltas(response.bytes_stream()))
    }
}

fn is_rate_limited(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<LlmError>(),
        Some(LlmError::RateLimited { .. })
    )
}

/// Turns an OpenAI SSE body into a stream of content deltas.
///
/// Chunks do not align with lines, so a partial `data:` line is buffered until
/// the rest arrives.
fn sse_deltas(
    body: impl Stream<Item = reqwest::Result<bytes::Bytes>> + Unpin,
) -> impl Stream<Item = Result<String>> {
    let mut buffer = String::new();
    body.flat_map(move |chunk| {
        let mut deltas = Vec::new();
        match chunk {
            Ok(bytes) => {
                buffer.push_str(&String::from_utf8_lossy(&bytes));
                while let Some(newline) = buffer.find('\n') {
                    let line: String = buffer.drain(..=newline).collect();
                    if let Some(delta) = parse_sse_line(line.trim()) {
                        deltas.push(Ok(delta));
                    }
                }
            }
            Err(e) => deltas.push(Err(anyhow!(e).context("reading stream"))),
        }
        futures_util::stream::iter(deltas)
    })
}

/// Extracts the content delta from one SSE line, if it carries one.
fn parse_sse_line(line: &str) -> Option<String> {
    let data = line.strip_prefix("data:")?.trim();
    if data == "[DONE]" || data.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(data).ok()?;
    let delta = value.pointer("/choices/0/delta/content")?.as_str()?;
    (!delta.is_empty()).then(|| delta.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: u16, header: Option<(&str, &str)>) -> reqwest::Response {
        let mut builder = http::Response::builder().status(status);
        if let Some((name, value)) = header {
            builder = builder.header(name, value);
        }
        reqwest::Response::from(builder.body("body text").unwrap())
    }

    /// The three request paths share this, so it is the one place a 429 is
    /// classified and the cooldown registered. A user only reaches it when
    /// already rate limited, which is the worst time to discover it broke.
    #[tokio::test]
    async fn a_429_registers_the_cooldown_and_reports_rate_limited() {
        let client = LlmClient::new("k", None).unwrap();

        let error = client
            .check("m-429", response(429, Some(("retry-after", "30"))))
            .await
            .expect_err("429 must be an error");

        match error.downcast_ref::<LlmError>() {
            Some(LlmError::RateLimited { model, retry_after }) => {
                assert_eq!(model, "m-429", "the error names the bare model");
                assert_eq!(retry_after.as_secs(), 30);
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
        // The breaker must have learned, or the fallback path retries a model
        // the server just refused.
        assert!(client.cooldowns.is_limited(&client.cooldown_key("m-429")));
    }

    #[tokio::test]
    async fn other_failures_carry_the_status_and_body_and_do_not_cool_down() {
        let client = LlmClient::new("k", None).unwrap();

        let error = client
            .check("m-500", response(500, None))
            .await
            .expect_err("500 must be an error");

        match error.downcast_ref::<LlmError>() {
            Some(LlmError::Failed { status, body }) => {
                assert_eq!(*status, 500);
                assert_eq!(body, "body text");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        assert!(
            !client.cooldowns.is_limited(&client.cooldown_key("m-500")),
            "500 is not a rate limit"
        );
    }

    #[tokio::test]
    async fn a_success_passes_through_untouched() {
        let client = LlmClient::new("k", None).unwrap();
        let ok = client.check("m-ok", response(200, None)).await.unwrap();
        assert_eq!(ok.status(), 200);
        assert!(!client.cooldowns.is_limited(&client.cooldown_key("m-ok")));
    }

    #[test]
    fn parses_content_deltas() {
        let line = r#"data: {"choices":[{"delta":{"content":"Hello"}}]}"#;
        assert_eq!(parse_sse_line(line), Some("Hello".into()));
    }

    #[test]
    fn ignores_terminator_and_non_content_lines() {
        assert_eq!(parse_sse_line("data: [DONE]"), None);
        assert_eq!(parse_sse_line(""), None);
        assert_eq!(parse_sse_line(": keep-alive"), None);
        // The role-only opening chunk carries no text.
        assert_eq!(
            parse_sse_line(r#"data: {"choices":[{"delta":{"role":"assistant"}}]}"#),
            None
        );
    }

    #[tokio::test]
    async fn stream_reassembles_lines_split_across_chunks() {
        // The realistic case: a chunk boundary lands mid-JSON.
        let chunks = vec![
            Ok(bytes::Bytes::from(r#"data: {"choices":[{"delta":{"cont"#)),
            Ok(bytes::Bytes::from(
                "ent\":\"Hel\"}}]}\ndata: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n",
            )),
            Ok(bytes::Bytes::from("data: [DONE]\n")),
        ];
        let stream = sse_deltas(futures_util::stream::iter(chunks));
        let text: String = stream
            .filter_map(|d| async { d.ok() })
            .collect::<Vec<_>>()
            .await
            .concat();
        assert_eq!(text, "Hello");
    }

    /// The failure: the tracker was per-client, and a client is built per
    /// request, so it was born empty every time and `is_limited` was always
    /// false. The breaker was dead code — every dictation after a 429 spent a
    /// full request rediscovering the limit, which is exactly the timeout the
    /// code was written to avoid.
    #[tokio::test]
    async fn the_breaker_survives_a_new_client() {
        // A port that refuses connections, so a request that *is* made
        // surfaces as a connection error rather than the message below.
        let first = LlmClient::new("k", Some("http://127.0.0.1:1")).unwrap();
        let _ = first
            .check("m-shared", response(429, Some(("retry-after", "30"))))
            .await;

        let second = LlmClient::new("k", Some("http://127.0.0.1:1")).unwrap();
        assert!(
            second
                .cooldowns
                .is_limited(&second.cooldown_key("m-shared")),
            "a fresh client must still know about the limit"
        );

        let err = second
            .complete_with_fallback("m-shared", None, &[], None)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("no fallback model is set"),
            "the primary must be skipped without a request, got: {err}"
        );
    }

    /// Sending `reasoning_effort` to a model that does not reason is a 400, so
    /// the table has to stay conservative. The llama case is the regression
    /// that matters.
    #[test]
    fn only_known_reasoning_models_carry_the_field() {
        assert_eq!(
            reasoning_for("openai/gpt-oss-20b"),
            (Some("low"), Some(false))
        );
        assert_eq!(
            reasoning_for("qwen/qwen3.6-27b"),
            (Some("none"), Some(false))
        );
        assert_eq!(reasoning_for("llama-3.3-70b-versatile"), (None, None));
        assert_eq!(reasoning_for("whatever-the-user-typed"), (None, None));
    }
}
