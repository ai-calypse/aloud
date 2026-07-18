//! Composio REST client: discovers the user's connected Gmail/GitHub accounts,
//! fetches slimmed tool schemas for the model, and executes tool calls.
//!
//! A direct port of voice-dict's `ComposioService.swift`, verified against the
//! live v3.1 API. Managed auth: the Composio project already holds an auth
//! config for each of the 35 managed toolkits (Gmail, GitHub are two), so this
//! never supplies OAuth credentials — it just reads the connected accounts that
//! exist and acts through them.
//!
//! Endpoints (all `x-api-key` authed against `backend.composio.dev/api/v3.1`):
//! - `GET  /connected_accounts?toolkit_slugs=gmail,github` → items[].{id,status,user_id,toolkit.slug}
//! - `GET  /tools?tool_slugs=…`                            → items[].{slug,description,input_parameters}
//! - `POST /tools/execute/{slug}` (needs BOTH connected_account_id AND user_id) → {data,error,successful}

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::time::Duration;

const API_BASE: &str = "https://backend.composio.dev/api/v3.1";

/// Tools exposed to the model. Slugs verified against the live tools API. This
/// is the allowlist voice-dict shipped — the same six.
pub const ALLOWED_TOOL_SLUGS: &[&str] = &[
    "GMAIL_SEND_EMAIL",
    "GMAIL_CREATE_EMAIL_DRAFT",
    "GMAIL_FETCH_EMAILS",
    "GITHUB_CREATE_AN_ISSUE",
    "GITHUB_GET_THE_AUTHENTICATED_USER",
    "GITHUB_LIST_REPOSITORY_ISSUES",
];

/// Tools that mutate the outside world — always gated behind a user
/// confirmation before execution.
pub const WRITE_TOOL_SLUGS: &[&str] = &["GMAIL_SEND_EMAIL", "GITHUB_CREATE_AN_ISSUE"];

pub fn is_write_tool(slug: &str) -> bool {
    WRITE_TOOL_SLUGS.contains(&slug)
}

/// `GMAIL_*` → "gmail", everything else → "github". Mirrors the Swift helper;
/// only two toolkits are in play.
fn toolkit_for(slug: &str) -> &'static str {
    if slug.starts_with("GMAIL_") {
        "gmail"
    } else {
        "github"
    }
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub toolkit_slug: String,
    pub id: String,
    pub user_id: String,
    pub status: String,
}

impl Connection {
    pub fn is_active(&self) -> bool {
        self.status == "ACTIVE"
    }
}

/// A tool definition to hand the model, in OpenAI function-calling shape.
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// JSON-schema object for the function's `parameters`.
    pub parameters: Value,
}

impl ToolDef {
    /// The `tools[]` entry the chat API expects.
    pub fn to_openai(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            }
        })
    }
}

pub struct ComposioClient {
    client: reqwest::Client,
    api_key: String,
}

impl ComposioClient {
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .context("building composio http client")?,
            api_key: api_key.into(),
        })
    }

    /// Connected Gmail/GitHub accounts (all statuses).
    pub async fn connections(&self) -> Result<Vec<Connection>> {
        let json = self
            .get("/connected_accounts?toolkit_slugs=gmail,github")
            .await?;
        let items = json["items"].as_array().cloned().unwrap_or_default();
        Ok(items
            .iter()
            .filter_map(|item| {
                let id = item["id"].as_str()?;
                let slug = item["toolkit"]["slug"].as_str()?;
                Some(Connection {
                    toolkit_slug: slug.to_string(),
                    id: id.to_string(),
                    user_id: item["user_id"].as_str().unwrap_or("").to_string(),
                    status: item["status"].as_str().unwrap_or("").to_string(),
                })
            })
            .collect())
    }

    /// Allowlisted tool definitions for toolkits with an ACTIVE connection.
    /// Empty when nothing is connected — the caller then answers without tools.
    pub async fn available_tools(&self) -> Result<Vec<ToolDef>> {
        let active: std::collections::HashSet<String> = self
            .connections()
            .await?
            .into_iter()
            .filter(Connection::is_active)
            .map(|c| c.toolkit_slug)
            .collect();
        if active.is_empty() {
            return Ok(Vec::new());
        }

        let slugs = ALLOWED_TOOL_SLUGS.join(",");
        let json = self.get(&format!("/tools?tool_slugs={slugs}")).await?;
        let items = json["items"].as_array().cloned().unwrap_or_default();
        Ok(items
            .iter()
            .filter_map(|item| {
                let slug = item["slug"].as_str()?;
                let parameters = item.get("input_parameters")?;
                Some(ToolDef {
                    name: slug.to_string(),
                    description: item["description"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(300)
                        .collect(),
                    parameters: slim_schema(parameters),
                })
            })
            .filter(|def| active.contains(toolkit_for(&def.name)))
            .collect())
    }

    /// Executes a tool against the ACTIVE connection for its toolkit, returning
    /// a compact JSON string for the model. Errors come back as `{"error": …}`
    /// (never `Err`) so the tool loop can hand them to the model to recover.
    pub async fn execute(&self, slug: &str, arguments: &Value) -> String {
        match self.execute_inner(slug, arguments).await {
            Ok(s) => s,
            Err(e) => error_json(&e.to_string()),
        }
    }

    async fn execute_inner(&self, slug: &str, arguments: &Value) -> Result<String> {
        let toolkit = toolkit_for(slug);
        let connection = self
            .connections()
            .await?
            .into_iter()
            .find(|c| c.toolkit_slug == toolkit && c.is_active())
            .with_context(|| format!("no active {toolkit} connection"))?;

        let json = self
            .post(
                &format!("/tools/execute/{slug}"),
                &json!({
                    "connected_account_id": connection.id,
                    "user_id": connection.user_id,
                    "arguments": arguments,
                }),
                Duration::from_secs(60),
            )
            .await?;

        // Keep only what the model needs; raw payloads are huge (a Gmail
        // message carries a ~94KB MIME `payload`; GitHub objects dozens of
        // *_url fields). Compact structurally, then cap — blind truncation cut
        // JSON mid-string and confused the model.
        let mut compact = Map::new();
        compact.insert(
            "successful".into(),
            json.get("successful")
                .cloned()
                .unwrap_or(Value::Bool(false)),
        );
        if let Some(data) = json.get("data") {
            compact.insert("data".into(), compact_value(data));
        }
        if let Some(error) = json.get("error") {
            compact.insert("error".into(), error.clone());
        }
        let out = serde_json::to_string(&Value::Object(compact)).unwrap_or_else(|_| "{}".into());
        Ok(cap(out))
    }

    // MARK: transport

    async fn get(&self, path: &str) -> Result<Value> {
        self.request(reqwest::Method::GET, path, None, Duration::from_secs(30))
            .await
    }

    async fn post(&self, path: &str, body: &Value, timeout: Duration) -> Result<Value> {
        self.request(reqwest::Method::POST, path, Some(body), timeout)
            .await
    }

    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&Value>,
        timeout: Duration,
    ) -> Result<Value> {
        let mut req = self
            .client
            .request(method, format!("{API_BASE}{path}"))
            .header("x-api-key", &self.api_key)
            .timeout(timeout);
        if let Some(body) = body {
            req = req.json(body);
        }
        let response = req.send().await.context("composio request failed")?;
        let status = response.status();
        if !status.is_success() {
            let snippet: String = response
                .text()
                .await
                .unwrap_or_default()
                .chars()
                .take(300)
                .collect();
            bail!("Composio HTTP {}: {snippet}", status.as_u16());
        }
        response.json().await.context("decoding composio response")
    }
}

/// Composio schemas are bloated (examples, titles, defaults, vendor keys) —
/// ~5K tokens for six tools, which alone bursts a small tokens-per-minute
/// budget. Keep only the JSON-schema keys the model needs to fill arguments.
fn slim_schema(schema: &Value) -> Value {
    let Some(obj) = schema.as_object() else {
        return schema.clone();
    };
    let mut out = Map::new();
    for (key, value) in obj {
        match key.as_str() {
            "type" | "required" | "enum" | "format" => {
                out.insert(key.clone(), value.clone());
            }
            "description" => {
                if let Some(text) = value.as_str() {
                    out.insert(
                        key.clone(),
                        json!(text.chars().take(150).collect::<String>()),
                    );
                }
            }
            "properties" => {
                if let Some(props) = value.as_object() {
                    let slimmed: Map<String, Value> = props
                        .iter()
                        .map(|(k, v)| (k.clone(), slim_schema(v)))
                        .collect();
                    out.insert(key.clone(), Value::Object(slimmed));
                }
            }
            "items" => {
                out.insert(key.clone(), slim_schema(value));
            }
            // drop examples, default, title, file_uploadable, …
            _ => {}
        }
    }
    Value::Object(out)
}

/// Recursively strips bulk from tool results: drops raw MIME payloads and
/// URL-catalog fields, truncates long strings (email bodies keep their first
/// ~600 chars, plenty for summarizing).
fn compact_value(value: &Value) -> Value {
    match value {
        Value::Object(dict) => {
            let mut out = Map::new();
            for (key, inner) in dict {
                if key == "payload"
                    || key == "node_id"
                    || key == "url"
                    || key.ends_with("_url")
                    || key == "labelIds"
                {
                    continue;
                }
                out.insert(key.clone(), compact_value(inner));
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(compact_value).collect()),
        Value::String(text) if text.chars().count() > 600 => {
            json!(format!("{}…", text.chars().take(600).collect::<String>()))
        }
        other => other.clone(),
    }
}

/// Caps a result at ~8000 bytes for the model's context. Backs off to the
/// nearest char boundary — a raw `&out[..8000]` panics mid-UTF-8, and email
/// bodies are full of multibyte text.
fn cap(out: String) -> String {
    if out.len() <= 8000 {
        return out;
    }
    let cut = (0..=8000).rev().find(|&i| out.is_char_boundary(i)).unwrap();
    format!("{}…(truncated)", &out[..cut])
}

fn error_json(message: &str) -> String {
    serde_json::to_string(&json!({ "error": message }))
        .unwrap_or_else(|_| r#"{"error":"unknown"}"#.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_tools_are_gated() {
        assert!(is_write_tool("GMAIL_SEND_EMAIL"));
        assert!(is_write_tool("GITHUB_CREATE_AN_ISSUE"));
        assert!(!is_write_tool("GMAIL_FETCH_EMAILS"));
        assert!(!is_write_tool("GITHUB_GET_THE_AUTHENTICATED_USER"));
    }

    #[test]
    fn toolkit_routing_splits_on_prefix() {
        assert_eq!(toolkit_for("GMAIL_FETCH_EMAILS"), "gmail");
        assert_eq!(toolkit_for("GITHUB_GET_THE_AUTHENTICATED_USER"), "github");
    }

    #[test]
    fn slim_schema_keeps_only_model_relevant_keys() {
        let bloated = json!({
            "type": "object",
            "title": "drop me",
            "default": "drop me",
            "required": ["to"],
            "properties": {
                "to": {
                    "type": "string",
                    "description": "recipient",
                    "examples": ["a@b.com"],
                    "file_uploadable": false
                }
            }
        });
        let slim = slim_schema(&bloated);
        assert!(slim.get("title").is_none(), "title must be dropped");
        assert!(slim.get("default").is_none(), "default must be dropped");
        assert_eq!(slim["type"], "object");
        assert_eq!(slim["required"], json!(["to"]));
        let to = &slim["properties"]["to"];
        assert_eq!(to["type"], "string");
        assert_eq!(to["description"], "recipient");
        assert!(to.get("examples").is_none(), "examples must be dropped");
        assert!(to.get("file_uploadable").is_none());
    }

    #[test]
    fn cap_never_slices_mid_utf8_char() {
        // Byte 8000 lands inside the first crab (7999..8003) — the old byte
        // slice panicked here.
        let s = "x".repeat(7999) + "🦀🦀🦀";
        let capped = cap(s);
        assert!(capped.ends_with("…(truncated)"));
        assert!(capped.starts_with("xxx"));

        let short = "under the cap".to_string();
        assert_eq!(cap(short.clone()), short);
    }

    #[test]
    fn compact_value_strips_bulk_and_truncates() {
        let raw = json!({
            "id": "msg1",
            "payload": "……94KB of MIME……",
            "self_url": "https://…",
            "body": "x".repeat(1000),
            "nested": [{ "node_id": "keep-out", "keep": "in" }]
        });
        let compact = compact_value(&raw);
        assert_eq!(compact["id"], "msg1");
        assert!(compact.get("payload").is_none(), "MIME payload dropped");
        assert!(compact.get("self_url").is_none(), "*_url dropped");
        assert!(
            compact["body"].as_str().unwrap().chars().count() <= 601,
            "long body truncated to ~600"
        );
        assert!(compact["nested"][0].get("node_id").is_none());
        assert_eq!(compact["nested"][0]["keep"], "in");
    }
}
