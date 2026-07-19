//! Composio REST client: discovers the user's connected accounts, whatever the
//! toolkit, fetches slimmed tool schemas for the model, and executes tool
//! calls.
//!
//! A direct port of voice-dict's `ComposioService.swift`, verified against the
//! live v3.1 API. Managed auth: the Composio project already holds an auth
//! config for each managed toolkit, so this
//! never supplies OAuth credentials — it just reads the connected accounts that
//! exist and acts through them.
//!
//! Endpoints (all `x-api-key` authed against `backend.composio.dev/api/v3.1`):
//! - `GET  /connected_accounts`      → items[].{id,status,user_id,toolkit.slug}
//! - `GET  /toolkits?search=&limit=` → items[].{slug,name,no_auth,meta.{logo,description,tools_count}}
//! - `GET  /tools?toolkit_slug=&limit=` → items[].{slug,description,input_parameters,tags}
//! - `POST /auth_configs`            → {auth_config.id}, using Composio-managed OAuth
//! - `POST /connected_accounts/link` → {redirect_url} to send the user through OAuth
//! - `POST /tools/execute/{slug}` (needs BOTH connected_account_id AND user_id) → {data,error,successful}

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::time::Duration;

const API_BASE: &str = "https://backend.composio.dev/api/v3.1";

/// Tools always offered for a toolkit, whatever the command says.
///
/// This is a floor, not a catalogue: everything else in the toolkit is still
/// reachable, ranked per command by [`relevance`]. Pins exist for the cases
/// ranking cannot cover — a user says "what meetings do I have tomorrow" and
/// the word "meeting" appears nowhere in `FIND_EVENT`'s name or description,
/// so every tool scores zero. Pin the core verbs of a toolkit and nothing
/// else; a long pin list is the old catalogue by another name.
///
/// Composio's own signals cannot do this job: its "important" tag covers 25 of
/// googlecalendar's 49 tools, so sorting by it falls through to alphabetical
/// order and yields ACL plumbing, `GITHUB_API_ROOT`, and Gmail without
/// "send email".
const PINNED_TOOLS: &[(&str, &[&str])] = &[
    (
        "gmail",
        &[
            "GMAIL_FETCH_EMAILS",
            "GMAIL_SEND_EMAIL",
            "GMAIL_CREATE_EMAIL_DRAFT",
        ],
    ),
    (
        "github",
        &[
            "GITHUB_CREATE_AN_ISSUE",
            "GITHUB_LIST_REPOSITORY_ISSUES",
            "GITHUB_GET_THE_AUTHENTICATED_USER",
        ],
    ),
    (
        "googlecalendar",
        &[
            "GOOGLECALENDAR_CREATE_EVENT",
            "GOOGLECALENDAR_FIND_EVENT",
            "GOOGLECALENDAR_UPDATE_EVENT",
            "GOOGLECALENDAR_DELETE_EVENT",
        ],
    ),
];

/// Words carrying no signal about which tool is wanted. Kept short on purpose:
/// a proper noun the scorer cannot match ("netflix", "sam") costs nothing,
/// while dropping a real verb would cost the ranking.
const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "his", "her", "its", "from", "into", "can", "you",
    "please", "about", "have", "what", "your", "there",
];

/// How many tools per connected toolkit are offered to the model *for one
/// command*. Not a catalogue: every tool in a connected toolkit is a candidate
/// on every request, and this is the context budget for choosing among them.
///
/// The budget is real. Uncapped, three connected toolkits measured ~77k
/// tokens of schema, re-sent on every round of the tool loop.
pub const TOOLS_PER_TOOLKIT: usize = 8;
/// Toolkits offered in connector search. Composio has 800+; nobody scrolls
/// past the first screen of a search.
const SEARCH_LIMIT: usize = 20;

/// Composio tags its tools with the MCP annotation hints. `readOnlyHint` is
/// the only one that means "cannot change anything out there".
const READ_ONLY_TAG: &str = "readOnlyHint";

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
    /// The toolkit this tool belongs to, as Composio reports it. Carried
    /// rather than derived: a slug's toolkit cannot be recovered by splitting
    /// on `_`, since 96 of Composio's 500 toolkits have one in their slug
    /// (`microsoft_teams`, `google_maps`, `one_drive`).
    pub toolkit: String,
    pub description: String,
    /// JSON-schema object for the function's `parameters`.
    pub parameters: Value,
    /// Whether Composio marks this tool `readOnlyHint`. Anything else — a
    /// create, an update, a delete, or a tool with no annotation at all —
    /// counts as a write and must be confirmed before it runs.
    pub read_only: bool,
}

/// One connectable service. `connected` is filled from the account list, so a
/// search result can say whether it is already wired up.
#[derive(Debug, Clone)]
pub struct Toolkit {
    pub slug: String,
    pub name: String,
    pub logo: String,
    pub description: String,
    pub tools_count: u32,
    pub connected: bool,
    /// Connected-account id, present only when `connected` — needed to unlink.
    pub connection_id: Option<String>,
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

    /// Every connected account, any toolkit, all statuses. Filtering to
    /// gmail,github here is what once made a freshly connected toolkit look
    /// unconnected everywhere: its Connect button never flipped and its tools
    /// never loaded.
    ///
    /// ponytail: one page of 100 accounts; paginate if anyone connects more.
    pub async fn connections(&self) -> Result<Vec<Connection>> {
        let json = self.get("/connected_accounts?limit=100").await?;
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

    /// Tools for every toolkit with an ACTIVE connection, chosen for `command`.
    ///
    /// Derived from what the user has actually connected rather than a fixed
    /// list, so connecting Slack in Settings makes Slack's tools available to
    /// the agent without a code change.
    ///
    /// The selection is per command, not per build: every tool in a connected
    /// toolkit is a candidate, and [`TOOLS_PER_TOOLKIT`] is the context budget
    /// for this one request. Pass an empty `command` to get the pins plus a
    /// stable head of the toolkit, which is what the settings pane wants.
    pub async fn available_tools(&self, command: &str) -> Result<Vec<ToolDef>> {
        let active: Vec<String> = self
            .connections()
            .await?
            .into_iter()
            .filter(Connection::is_active)
            .map(|c| c.toolkit_slug)
            .collect();

        let mut tools = Vec::new();
        for slug in dedup(active) {
            match self.toolkit_tools(&slug, command, TOOLS_PER_TOOLKIT).await {
                Ok(mut found) => tools.append(&mut found),
                // One broken toolkit must not cost the user the others.
                Err(e) => tracing::warn!(%e, %slug, "could not load toolkit tools"),
            }
        }
        Ok(tools)
    }

    /// The `limit` tools of one toolkit most likely to serve `command`: its
    /// pins, then the best-scoring of everything else.
    ///
    /// Pins are fetched by slug rather than picked out of the toolkit page,
    /// because that page is paginated — GitHub runs past 200 tools, and
    /// `LIST_REPOSITORY_ISSUES` is on none of the first pages. The two
    /// requests run concurrently, so this costs the latency of one.
    pub async fn toolkit_tools(
        &self,
        toolkit: &str,
        command: &str,
        limit: usize,
    ) -> Result<Vec<ToolDef>> {
        let pin_slugs = PINNED_TOOLS
            .iter()
            .find(|(slug, _)| *slug == toolkit)
            .map(|(_, slugs)| *slugs)
            .unwrap_or(&[]);

        let pin_query = pin_slugs.join(",");
        let (pinned, rest) = tokio::try_join!(
            self.tools_by_slug(&pin_query),
            self.ranked_tools(toolkit, command),
        )?;

        let mut out = pinned;
        out.truncate(limit);
        for tool in rest {
            if out.len() >= limit {
                break;
            }
            if !out.iter().any(|t| t.name == tool.name) {
                out.push(tool);
            }
        }
        Ok(out)
    }

    /// Exact tools by slug. An empty slug list would fetch the whole catalogue,
    /// so it short-circuits instead.
    async fn tools_by_slug(&self, slugs: &str) -> Result<Vec<ToolDef>> {
        if slugs.is_empty() {
            return Ok(Vec::new());
        }
        let json = self.get(&format!("/tools?tool_slugs={slugs}")).await?;
        Ok(json["items"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(tool_def)
            .collect())
    }

    /// A toolkit's tools, best match for `command` first.
    ///
    /// Composio's `?search=` cannot do this: it ANDs every term, so one word
    /// that matches no tool empties the result — and a spoken command is full
    /// of them. "delete the netflix event from my calendar" returns nothing
    /// live, while "delete event" returns the right two. Scoring locally over
    /// the page we already fetch has no such cliff, and costs no extra call.
    ///
    /// Destructive tools are *not* filtered out. `DELETE_EVENT` carries the
    /// same `destructiveHint` as `CLEAR_CALENDAR`, so filtering on it made
    /// "delete that event" impossible to obey. The confirmation gate in the
    /// tool loop is what makes a destructive call safe, not its absence.
    async fn ranked_tools(&self, toolkit: &str, command: &str) -> Result<Vec<ToolDef>> {
        let json = self
            .get(&format!("/tools?toolkit_slug={toolkit}&limit=100"))
            .await?;
        let items = json["items"].as_array().cloned().unwrap_or_default();
        let words = keywords(command);

        let mut scored: Vec<(usize, ToolDef)> = items
            .iter()
            .filter_map(|item| Some((relevance(item, &words), tool_def(item)?)))
            .collect();

        // A command the scorer cannot read leaves every tool at zero. Falling
        // back to reads-and-writes interleaved keeps that case usable: sorting
        // by Composio's order alone put three Gmail delete tools ahead of
        // "fetch emails", and a toolkit is only useful if the agent can both
        // look things up and act.
        if scored.iter().all(|(score, _)| *score == 0) {
            let (reads, writes): (Vec<_>, Vec<_>) = scored
                .into_iter()
                .map(|(_, d)| d)
                .partition(|d| d.read_only);
            let mut reads = reads.into_iter();
            let mut writes = writes.into_iter();
            let mut out = Vec::new();
            loop {
                match (reads.next(), writes.next()) {
                    (None, None) => break,
                    (read, write) => out.extend(read.into_iter().chain(write)),
                }
            }
            return Ok(out);
        }

        // Stable sort: equal scores keep Composio's ordering rather than
        // shuffling between identical commands.
        scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        Ok(scored.into_iter().map(|(_, def)| def).collect())
    }

    /// Toolkits matching `search`, with connection state filled in. An empty
    /// search returns Composio's most-used toolkits, which is a better landing
    /// state than an empty list.
    pub async fn toolkits(&self, search: &str) -> Result<Vec<Toolkit>> {
        let connected = self.connected_by_slug().await?;
        let query = urlencode(search.trim());
        let json = self
            .get(&format!(
                "/toolkits?search={query}&limit={SEARCH_LIMIT}&sort_by=usage"
            ))
            .await?;

        Ok(json["items"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|item| {
                let slug = item["slug"].as_str()?.to_string();
                // A no-auth toolkit has nothing to connect; showing it in a
                // connector list would offer a button that does nothing.
                if item["no_auth"].as_bool().unwrap_or(false) {
                    return None;
                }
                let connection_id = connected.get(&slug).cloned();
                Some(Toolkit {
                    name: item["name"].as_str().unwrap_or(&slug).to_string(),
                    logo: item["meta"]["logo"].as_str().unwrap_or("").to_string(),
                    description: item["meta"]["description"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(160)
                        .collect(),
                    tools_count: item["meta"]["tools_count"].as_u64().unwrap_or(0) as u32,
                    connected: connection_id.is_some(),
                    connection_id,
                    slug,
                })
            })
            .collect())
    }

    /// Every toolkit the user has an ACTIVE connection to.
    ///
    /// Asked for by slug rather than searched, so a connected toolkit always
    /// shows even if it would fall outside a search page.
    /// The per-slug lookups run concurrently: they are independent round-trips
    /// to the same API, so awaiting them in a loop made the Connectors pane
    /// cost one full latency per connected service.
    pub async fn connected_toolkits(&self) -> Result<Vec<Toolkit>> {
        let connected = self.connected_by_slug().await?;
        let mut out: Vec<Toolkit> = futures_util::future::try_join_all(connected.into_iter().map(
            |(slug, connection_id)| async move {
                let json = self.get(&format!("/toolkits/{slug}")).await?;
                Ok::<_, anyhow::Error>(Toolkit {
                    name: json["name"].as_str().unwrap_or(&slug).to_string(),
                    logo: json["meta"]["logo"].as_str().unwrap_or("").to_string(),
                    description: json["meta"]["description"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(160)
                        .collect(),
                    tools_count: json["meta"]["tools_count"].as_u64().unwrap_or(0) as u32,
                    connected: true,
                    connection_id: Some(connection_id),
                    slug,
                })
            },
        ))
        .await?;
        out.sort_by_key(|t| t.name.to_lowercase());
        Ok(out)
    }

    /// Connected toolkits paired with the tools each would offer.
    ///
    /// Both levels fan out concurrently: the toolkit lookups inside
    /// `connected_toolkits`, then one `toolkit_tools` call per toolkit. Awaited
    /// serially this cost two round-trips per connected service.
    ///
    /// A toolkit whose tools fail to load still comes back, with an empty list
    /// — one unreachable service must not empty the whole pane.
    pub async fn connected_with_tools(
        &self,
        per_toolkit: usize,
    ) -> Result<Vec<(Toolkit, Vec<ToolDef>)>> {
        let toolkits = self.connected_toolkits().await?;
        Ok(
            futures_util::future::join_all(toolkits.into_iter().map(|toolkit| async move {
                // The agent picks tools per command, so there is no single set
                // to show. An empty command yields the pins plus a stable head
                // of the toolkit — typical of a request, not a promise.
                let tools = self
                    .toolkit_tools(&toolkit.slug, "", per_toolkit)
                    .await
                    .unwrap_or_default();
                (toolkit, tools)
            }))
            .await,
        )
    }

    /// Starts an OAuth connection and returns the URL the user must visit.
    ///
    /// Composio needs an auth config per toolkit before an account can be
    /// linked; this reuses the project's existing one or creates a
    /// Composio-managed one, so the user never supplies OAuth credentials.
    pub async fn connect(&self, toolkit: &str) -> Result<String> {
        let auth_config = match self.auth_config_for(toolkit).await? {
            Some(id) => id,
            None => self.create_auth_config(toolkit).await?,
        };

        // Keep new accounts under the same identity as existing ones so a user
        // ends up with one set of connections, not one per toolkit.
        //
        // `POST /connected_accounts` now 400s for Composio-managed OAuth; the
        // link endpoint is its replacement and returns the same redirect_url.
        let user_id = self.user_id().await;
        let json = self
            .post(
                "/connected_accounts/link",
                &json!({
                    "auth_config_id": auth_config,
                    "user_id": user_id,
                }),
                Duration::from_secs(30),
            )
            .await?;

        json["redirect_url"]
            .as_str()
            .map(str::to_string)
            .context("Composio did not return an OAuth URL for this toolkit")
    }

    /// Unlinks a connected account. The toolkit's tools disappear from the
    /// agent on the next run.
    pub async fn disconnect(&self, connection_id: &str) -> Result<()> {
        self.request(
            reqwest::Method::DELETE,
            &format!("/connected_accounts/{connection_id}"),
            None,
            Duration::from_secs(30),
        )
        .await
        .map(|_| ())
    }

    /// Active connections as slug → connected-account id.
    async fn connected_by_slug(&self) -> Result<std::collections::HashMap<String, String>> {
        Ok(self
            .connections()
            .await?
            .into_iter()
            .filter(Connection::is_active)
            .map(|c| (c.toolkit_slug, c.id))
            .collect())
    }

    /// The identity new connections are filed under: whatever existing
    /// connections use, or Composio's own default on a fresh project.
    async fn user_id(&self) -> String {
        self.connections()
            .await
            .ok()
            .and_then(|c| c.into_iter().find(|c| !c.user_id.is_empty()))
            .map(|c| c.user_id)
            .unwrap_or_else(|| "default".to_string())
    }

    async fn auth_config_for(&self, toolkit: &str) -> Result<Option<String>> {
        let json = self
            .get(&format!("/auth_configs?toolkit_slug={toolkit}&limit=1"))
            .await?;
        Ok(json["items"]
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item["id"].as_str())
            .map(str::to_string))
    }

    async fn create_auth_config(&self, toolkit: &str) -> Result<String> {
        let json = self
            .post(
                "/auth_configs",
                &json!({
                    "toolkit": { "slug": toolkit },
                    "auth_config": { "type": "use_composio_managed_auth" },
                }),
                Duration::from_secs(30),
            )
            .await?;
        json["auth_config"]["id"]
            .as_str()
            .map(str::to_string)
            .with_context(|| format!("Composio cannot manage auth for {toolkit} itself"))
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
        // Match the tool against the toolkits actually connected rather than
        // guessing one from the slug. `MICROSOFT_TEAMS_SEND_MESSAGE` split on
        // the first `_` gives "microsoft", which matches no connection, so
        // every call on such a toolkit failed with "no active microsoft
        // connection" while the account sat there connected. Longest match
        // wins, so a `google` connection cannot shadow `google_maps`.
        let connections = self.connections().await?;
        let connection = connections
            .into_iter()
            .filter(|c| c.is_active() && slug_belongs_to(slug, &c.toolkit_slug))
            .max_by_key(|c| c.toolkit_slug.len())
            .with_context(|| format!("no active connection for {}", toolkit_of_slug(slug)))?;

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
            // `default` stays: without it the model invents values for
            // optional params it should leave alone (calendar_id: "role"
            // instead of the documented default "primary").
            "type" | "required" | "enum" | "format" | "default" => {
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
            // drop examples, title, file_uploadable, …
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

/// The toolkit a tool slug belongs to: Composio prefixes every slug with the
/// toolkit, so `SLACK_SEND_MESSAGE` is `slack`. Previously hardcoded to the
/// two toolkits that shipped.
/// Best-effort only — correct for a single-token toolkit, wrong for the 96 of
/// Composio's 500 whose slug contains an underscore. Use [`ToolDef::toolkit`]
/// or [`slug_belongs_to`] wherever the real answer is available; this is the
/// fallback for a display path that has neither.
pub fn toolkit_of_slug(slug: &str) -> String {
    slug.split('_').next().unwrap_or(slug).to_lowercase()
}

/// Whether `slug` is one of `toolkit`'s tools. Composio names every tool
/// `<TOOLKIT>_<VERB>`, so this is a prefix test on the uppercased slug — exact
/// where splitting on `_` only guesses.
pub fn slug_belongs_to(slug: &str, toolkit: &str) -> bool {
    let prefix = format!("{}_", toolkit.to_uppercase());
    slug.to_uppercase().starts_with(&prefix)
}

/// Percent-encodes a search term. No urlencoding crate for one query param:
/// anything outside this set is escaped, which is stricter than necessary and
/// therefore safe.
fn urlencode(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
                vec![c]
            } else {
                format!("%{:02X}", c as u32 & 0xFF).chars().collect()
            }
        })
        .collect()
}

fn dedup(mut items: Vec<String>) -> Vec<String> {
    items.sort();
    items.dedup();
    items
}

/// The words of a spoken command worth matching a tool against: lowercased,
/// short and common words dropped. Proper nouns ("netflix", "sam") survive and
/// simply match nothing, which costs a point of score but never empties the
/// result the way Composio's server-side AND search does.
fn keywords(command: &str) -> Vec<String> {
    command
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.len() > 2 && !STOP_WORDS.contains(&w.as_str()))
        .collect()
}

/// How well one raw tool object matches `words`.
///
/// A word in the tool's own name is worth more than one buried in its
/// description: every calendar tool mentions "event" somewhere in its prose,
/// but only `DELETE_EVENT` is named for it.
fn relevance(item: &Value, words: &[String]) -> usize {
    let name = item["slug"]
        .as_str()
        .unwrap_or_default()
        .replace('_', " ")
        .to_lowercase();
    let description = item["description"]
        .as_str()
        .unwrap_or_default()
        .to_lowercase();

    words
        .iter()
        .map(|word| {
            if name.split_whitespace().any(|part| part == word) {
                3
            } else if name.contains(word.as_str()) || description.contains(word.as_str()) {
                1
            } else {
                0
            }
        })
        .sum()
}

/// Whether a raw tool object carries `tag`.
fn has_tag(item: &Value, tag: &str) -> bool {
    item["tags"]
        .as_array()
        .is_some_and(|tags| tags.iter().filter_map(Value::as_str).any(|t| t == tag))
}

/// One raw tool object to a model-facing definition.
fn tool_def(item: &Value) -> Option<ToolDef> {
    let name = item["slug"].as_str()?.to_string();
    Some(ToolDef {
        // Composio reports the toolkit on every tool; falling back to the
        // slug's first token is only for a malformed item.
        toolkit: item["toolkit"]["slug"]
            .as_str()
            .map(str::to_lowercase)
            .unwrap_or_else(|| toolkit_of_slug(&name)),
        name,
        description: item["description"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(300)
            .collect(),
        parameters: slim_schema(item.get("input_parameters")?),
        read_only: has_tag(item, READ_ONLY_TAG),
    })
}

fn error_json(message: &str) -> String {
    serde_json::to_string(&json!({ "error": message }))
        .unwrap_or_else(|_| r#"{"error":"unknown"}"#.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Execution routes a tool to its toolkit's connection by slug prefix, so
    /// this has to work for toolkits nobody hardcoded.
    #[test]
    fn toolkit_routing_splits_on_the_slug_prefix() {
        assert_eq!(toolkit_of_slug("GMAIL_FETCH_EMAILS"), "gmail");
        assert_eq!(
            toolkit_of_slug("GITHUB_GET_THE_AUTHENTICATED_USER"),
            "github"
        );
        assert_eq!(toolkit_of_slug("SLACK_SEND_MESSAGE"), "slack");
        assert_eq!(toolkit_of_slug("NOTION_CREATE_PAGE"), "notion");
        // The confirmation panel announced this one as "writes to github".
        assert_eq!(
            toolkit_of_slug("GOOGLECALENDAR_DELETE_EVENT"),
            "googlecalendar"
        );
    }

    /// 96 of Composio's 500 toolkits have an underscore in their slug, so the
    /// first-token split silently routed every one of their tools to a
    /// connection that does not exist: `MICROSOFT_TEAMS_SEND_MESSAGE` looked
    /// for a "microsoft" account and failed while Teams sat connected.
    #[test]
    fn tools_match_their_toolkit_even_when_its_slug_has_an_underscore() {
        assert!(slug_belongs_to(
            "MICROSOFT_TEAMS_SEND_MESSAGE",
            "microsoft_teams"
        ));
        assert!(slug_belongs_to("GOOGLE_MAPS_AUTOCOMPLETE", "google_maps"));
        assert!(slug_belongs_to("ONE_DRIVE_CHECKIN_ITEM", "one_drive"));
        assert!(slug_belongs_to("GMAIL_SEND_EMAIL", "gmail"));

        // A shorter toolkit must not claim a longer one's tools; the caller
        // takes the longest match, but this must not even be a candidate.
        assert!(!slug_belongs_to(
            "GOOGLE_MAPS_AUTOCOMPLETE",
            "googlecalendar"
        ));
        assert!(!slug_belongs_to("GMAIL_SEND_EMAIL", "gm"));
    }

    #[test]
    fn search_terms_are_escaped_into_the_query_string() {
        assert_eq!(urlencode("google drive"), "google%20drive");
        assert_eq!(urlencode("a&b=c"), "a%26b%3Dc");
        assert_eq!(urlencode("slack"), "slack");
    }

    #[test]
    fn relevance_ranks_the_named_verb_above_its_neighbours() {
        let tools = [
            json!({"slug": "GOOGLECALENDAR_DELETE_EVENT", "description": "Deletes an event"}),
            json!({"slug": "GOOGLECALENDAR_CLEAR_CALENDAR", "description": "Deletes every event on a calendar"}),
            json!({"slug": "GOOGLECALENDAR_CREATE_EVENT", "description": "Creates an event"}),
        ];
        // The live phrasing that returned nothing from Composio's own search:
        // "netflix" matches no tool, and must not sink the terms that do.
        let words = keywords("delete the netflix event from my calendar");
        let best = tools
            .iter()
            .max_by_key(|item| relevance(item, &words))
            .unwrap();
        assert_eq!(best["slug"], "GOOGLECALENDAR_DELETE_EVENT");
    }

    #[test]
    fn keywords_drop_filler_but_keep_proper_nouns() {
        assert_eq!(
            keywords("Delete the Netflix event from my calendar"),
            ["delete", "netflix", "event", "calendar"]
        );
    }

    #[test]
    fn slim_schema_keeps_only_model_relevant_keys() {
        let bloated = json!({
            "type": "object",
            "title": "drop me",
            "required": ["to"],
            "properties": {
                "to": {
                    "type": "string",
                    "description": "recipient",
                    "examples": ["a@b.com"],
                    "file_uploadable": false
                },
                "calendar_id": {
                    "type": "string",
                    "default": "primary"
                }
            }
        });
        let slim = slim_schema(&bloated);
        assert!(slim.get("title").is_none(), "title must be dropped");
        assert_eq!(slim["type"], "object");
        assert_eq!(slim["required"], json!(["to"]));
        let to = &slim["properties"]["to"];
        assert_eq!(to["type"], "string");
        assert_eq!(to["description"], "recipient");
        assert!(to.get("examples").is_none(), "examples must be dropped");
        assert!(to.get("file_uploadable").is_none());
        // The model invented calendar_id "role" because it couldn't see the
        // documented default; defaults are model-relevant and must survive.
        assert_eq!(slim["properties"]["calendar_id"]["default"], "primary");
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
