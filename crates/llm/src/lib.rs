//! Chat completions against any OpenAI-compatible endpoint.
//!
//! Ported from `PostProcessingService.swift` + `LLMCooldownManager.swift`.
//! Used for transcript cleanup, command mode, Ask, and meeting summaries —
//! the Swift app hit the same Groq endpoint for all four.

pub mod ask;
pub mod cooldown;
pub mod prompts;

use anyhow::{anyhow, Context, Result};
use cooldown::CooldownTracker;
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};

pub const DEFAULT_BASE_URL: &str = "https://api.groq.com/openai/v1";
const TIMEOUT: Duration = Duration::from_secs(30);

/// The transcript was empty or pure filler. The cleanup prompt is specified to
/// return exactly this sentinel, which callers treat as "paste nothing".
pub const EMPTY_SENTINEL: &str = "EMPTY";

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
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

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [Message],
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
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
    content: Option<String>,
}

impl LlmClient {
    pub fn new(api_key: impl Into<String>, base_url: Option<&str>) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(TIMEOUT)
                .build()
                .context("building http client")?,
            base_url: base_url
                .unwrap_or(DEFAULT_BASE_URL)
                .trim_end_matches('/')
                .to_string(),
            api_key: api_key.into(),
            cooldowns: Arc::new(CooldownTracker::new()),
        })
    }

    /// Shared with callers that want to check a model before using it.
    pub fn cooldowns(&self) -> Arc<CooldownTracker> {
        self.cooldowns.clone()
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
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&ChatRequest {
                model,
                messages,
                temperature,
                stream: false,
            })
            .send()
            .await
            .context("chat request failed")?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let cooldown = cooldown::from_headers(response.headers());
            self.cooldowns.set(model, cooldown);
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
        if !self.cooldowns.is_limited(model) {
            match self.complete(model, messages, temperature).await {
                Err(e) if is_rate_limited(&e) => {
                    tracing::info!(model, "rate-limited; trying fallback");
                }
                result => return result,
            }
        }

        let fallback = fallback
            .ok_or_else(|| anyhow!("{model} is rate-limited and no fallback model is set"))?;
        self.complete(fallback, messages, temperature).await
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
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&ChatRequest {
                model,
                messages,
                temperature,
                stream: true,
            })
            .send()
            .await
            .context("stream request failed")?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let cooldown = cooldown::from_headers(response.headers());
            self.cooldowns.set(model, cooldown);
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
}
