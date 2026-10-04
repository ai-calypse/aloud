//! Composio REST client: discovers the user's connected accounts, fetches
//! slimmed tool schemas for the model, and executes tool calls.
//!
//! Managed auth throughout — the Composio project already holds an auth config
//! per toolkit, so this never supplies OAuth credentials.
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

/// Tools always offered for a toolkit — a floor, not a catalogue; everything
/// else stays reachable via [`relevance`]. Pins cover what ranking cannot: "what
/// meetings do I have tomorrow" scores every tool zero, since "meeting" appears
/// nowhere in `FIND_EVENT`. Pin core verbs only; a long list is a catalogue.
///
/// Composio's "important" tag cannot do this job — it covers 25 of
/// googlecalendar's 49 tools, so it falls through to alphabetical order.
///
/// Each toolkit's directory lookup is pinned for the same reason: "email Ravi
/// the notes" scores every people-search tool zero, and without one in the list
/// the model asks for an address the user already expects it to know. All are
/// `readOnlyHint`, so resolving a name never hits the write confirmation.
/// Calendar has no directory of its own — attendee lookup rides on
/// `GMAIL_SEARCH_PEOPLE`, so calendar-without-gmail still needs full addresses.
///
/// `GMAIL_SEARCH_PEOPLE` reads Google Contacts, not the mailbox: someone you
/// have a year of threads with but never saved is simply absent, even with
/// `other_contacts` on. `GMAIL_FETCH_EMAILS` is the fallback the tool's own
/// description names, and is pinned above anyway — the prompt in `tool_loop`
/// is what sends the model there second.
const PINNED_TOOLS: &[(&str, &[&str])] = &[
    (
        "gmail",
        &[
            "GMAIL_FETCH_EMAILS",
            "GMAIL_SEND_EMAIL",
            "GMAIL_CREATE_EMAIL_DRAFT",
            "GMAIL_SEARCH_PEOPLE",
        ],
    ),
    (
        "github",
        &[
            "GITHUB_CREATE_AN_ISSUE",
            "GITHUB_LIST_REPOSITORY_ISSUES",
            "GITHUB_GET_THE_AUTHENTICATED_USER",
            "GITHUB_SEARCH_USERS",
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
    // Slack's send verbs rank fine from the command; only the directory needs
    // the pin.
    ("slack", &["SLACK_FIND_USERS"]),
    // Look someone up, and write one down. `CREATE_CONTACT` carries no
    // `readOnlyHint`, so it takes the confirmation gate like any other write —
    // which is the whole approval flow for "save Dad to my contacts", with no
    // second channel to ask the user anything.
    (
        "googlecontacts",
        &[
            "GOOGLECONTACTS_SEARCH_CONTACTS",
            "GOOGLECONTACTS_CREATE_CONTACT",
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

/// Words that mark a command as being about a toolkit. Offering every connected
/// toolkit cost 23–35k tokens a request (Google Contacts alone was up to 18k),
/// which tripped Cerebras's tokens-per-minute budget after a handful of
/// commands and made "create a note" pay for Gmail and GitHub. ponytail:
/// hand-kept for the common toolkits — one not listed is always offered, and a
/// gated one is still loadable by the model (`ALOUD_MORE_TOOLS`) on a miss.
const TOOLKIT_WORDS: &[(&str, &[&str])] = &[
    (
        "gmail",
        &[
            "email",
            "emails",
            "mail",
            "mails",
            "inbox",
            "gmail",
            "unread",
            "reply",
            "draft",
            "drafts",
            "newsletter",
            "sender",
        ],
    ),
    (
        "googlecalendar",
        &[
            "calendar",
            "meeting",
            "meetings",
            "event",
            "events",
            "schedule",
            "appointment",
            "invite",
            "agenda",
            "availability",
            "busy",
            "free",
        ],
    ),
    (
        "googlecontacts",
        &["contact", "contacts", "phone", "address"],
    ),
    (
        "github",
        &[
            "github",
            "issue",
            "issues",
            "pull",
            "pr",
            "prs",
            "repo",
            "repos",
            "repository",
            "commit",
            "commits",
            "branch",
            "gist",
            "star",
        ],
    ),
    ("slack", &["slack", "channel", "dm", "workspace", "thread"]),
    ("notion", &["notion", "wiki"]),
    ("linear", &["linear", "ticket", "tickets"]),
    ("googledrive", &["drive", "file", "files", "folder"]),
    ("googledocs", &["doc", "docs", "document", "documents"]),
    (
        "googlesheets",
        &["sheet", "sheets", "spreadsheet", "spreadsheets"],
    ),
];

/// Whether `command` is plausibly about `toolkit`, by [`TOOLKIT_WORDS`]. A
/// toolkit with no entry is assumed relevant: offering it costs tokens, hiding
/// it costs the command.
pub fn concerns(toolkit: &str, command: &str) -> bool {
    !TOOLKIT_WORDS.iter().any(|(slug, _)| *slug == toolkit) || mentions(toolkit, command)
}

/// Whether `command` says one of `toolkit`'s [`TOOLKIT_WORDS`]. Unlike
/// [`concerns`], a toolkit with no entry is never assumed: this decides whether
/// the user named an app, which is trust, not what to offer.
pub fn mentions(toolkit: &str, command: &str) -> bool {
    TOOLKIT_WORDS
        .iter()
        .find(|(slug, _)| *slug == toolkit)
        .is_some_and(|(_, words)| {
            command
                .split(|c: char| !c.is_alphanumeric())
                .map(str::to_lowercase)
                .any(|said| words.contains(&said.as_str()))
        })
}

/// Context budget per connected toolkit for one command. Uncapped, three
/// toolkits measured ~77k tokens of schema, re-sent every round of the loop.
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
    /// Carried, not derived: 96 of Composio's 500 toolkit slugs contain an `_`
    /// (`microsoft_teams`, `google_maps`), so splitting cannot recover it.
    pub toolkit: String,
    /// The app a permission names. Composio's tool items carry only
    /// `toolkit.name`, which is the slug again ("slack"), so that is what it is.
    pub app: String,
    pub description: String,
    /// JSON-schema object for the function's `parameters`.
    pub parameters: Value,
    /// Composio's `readOnlyHint`. Anything else — including no annotation at
    /// all — counts as a write and must be confirmed before it runs.
    pub read_only: bool,
    /// Every MCP annotation tag on the tool (`destructiveHint`, `updateHint`…),
    /// mixed in Composio's `tags` with categories like `important` and `chat`.
    pub hints: Vec<String>,
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

/// How long a fetched page of the tool catalogue is reused. It changes with Composio releases, not
/// between commands, and fetching it put a round trip in front of every command.
const CATALOGUE_TTL: Duration = Duration::from_secs(600);

/// Catalogue responses by hash of (credential, path), so a key change never serves another
/// account's view. ponytail: never evicted — a handful of toolkits is a handful of entries.
fn catalogue(
) -> &'static std::sync::Mutex<std::collections::HashMap<u64, (std::time::Instant, Value)>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<u64, (std::time::Instant, Value)>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

pub struct ComposioClient {
    client: reqwest::Client,
    api_key: String,
    /// Connection id → its mailbox address, learned on demand. `None` marks an
    /// account that could not tell us, so a second command does not ask again.
    /// Only consulted when a toolkit has more than one active connection.
    addresses: std::sync::Mutex<std::collections::HashMap<String, Option<String>>>,
}

impl ComposioClient {
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .context("building composio http client")?,
            api_key: api_key.into(),
            addresses: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Every connected account, any toolkit, all statuses. Filtering by toolkit
    /// here once made a freshly connected one look unconnected everywhere.
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
    /// Selection is per command, not per build. Pass an empty `command` for the
    /// pins plus a stable head of the toolkit, which the settings pane wants.
    pub async fn available_tools(&self, command: &str) -> Result<Vec<ToolDef>> {
        Ok(self.tools_for(command, |_| true).await?.0)
    }

    /// Like [`Self::available_tools`], but only for the toolkits `offer`
    /// accepts; the rest come back by name so the model can load one if the
    /// command needs it after all.
    pub async fn tools_for(
        &self,
        command: &str,
        offer: impl Fn(&str) -> bool,
    ) -> Result<(Vec<ToolDef>, Vec<String>)> {
        let connections = self.connections().await?;
        let connected: Vec<String> = connections
            .iter()
            .filter(|c| c.is_active())
            .map(|c| c.toolkit_slug.clone())
            .collect();
        let (active, skipped): (Vec<String>, Vec<String>) =
            connected.iter().cloned().partition(|slug| offer(slug));
        let skipped = dedup(skipped);
        let active: Vec<String> = connected
            .iter()
            .filter(|slug| active.contains(slug))
            .cloned()
            .collect();

        // Toolkits the user has connected more than once — only those need to
        // be told apart, and only their tools get the `account` argument.
        let multi: Vec<String> = dedup(active.clone())
            .into_iter()
            .filter(|slug| active.iter().filter(|s| *s == slug).count() > 1)
            .collect();

        // Concurrently: one at a time, five connected toolkits were five round trips before the
        // model was even asked.
        let loaded =
            futures_util::future::join_all(dedup(active).into_iter().map(|slug| async move {
                let found = self.toolkit_tools(&slug, command, TOOLS_PER_TOOLKIT).await;
                (slug, found)
            }))
            .await;
        let mut tools = Vec::new();
        for (slug, found) in loaded {
            match found {
                Ok(mut found) => tools.append(&mut found),
                // One broken toolkit must not cost the user the others.
                Err(e) => tracing::warn!(%e, %slug, "could not load toolkit tools"),
            }
        }

        if !multi.is_empty() {
            let addresses = self.addresses_for(&connections, &multi).await;
            for tool in tools.iter_mut().filter(|t| multi.contains(&t.toolkit)) {
                add_account_argument(&mut tool.parameters, &addresses);
            }
        }
        Ok((tools, skipped))
    }

    /// The known addresses across every connection of the given toolkits, for
    /// the `account` argument's description. Best-effort: an account that
    /// cannot say is simply absent from the list.
    async fn addresses_for(&self, connections: &[Connection], toolkits: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        for connection in connections
            .iter()
            .filter(|c| c.is_active() && toolkits.contains(&c.toolkit_slug))
        {
            if let Some(address) = self.address_of(connection).await {
                if !out.contains(&address) {
                    out.push(address);
                }
            }
        }
        out
    }

    /// The `limit` tools of one toolkit most likely to serve `command`: its
    /// pins, then the best-scoring of everything else. Pins are fetched by slug
    /// because the toolkit page is paginated — GitHub runs past 200 tools and
    /// `LIST_REPOSITORY_ISSUES` is on none of the first pages. Concurrent, so
    /// this costs the latency of one request.
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

    /// A toolkit's tools, best match for `command` first. Scored locally
    /// because Composio's `?search=` ANDs every term, so one unmatched word
    /// empties the result — "delete the netflix event from my calendar" returns
    /// nothing live.
    ///
    /// Destructive tools are *not* filtered out: `DELETE_EVENT` carries the
    /// same `destructiveHint` as `CLEAR_CALENDAR`, so filtering made "delete
    /// that event" impossible. The tool loop's confirmation gate is the safety.
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

        // An all-zero score falls back to reads and writes interleaved:
        // Composio's own order put three Gmail delete tools ahead of "fetch
        // emails", and a toolkit needs both halves to be useful.
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

    /// Every toolkit the user has an ACTIVE connection to. Asked for by slug,
    /// not searched, so a connected toolkit shows even outside a search page.
    /// Concurrent: awaiting in a loop cost the Connectors pane one full latency
    /// per connected service.
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

    /// Starts an OAuth connection and returns the URL the user must visit.
    /// Composio needs an auth config per toolkit first; this reuses the
    /// project's or creates a managed one, so the user supplies no credentials.
    pub async fn connect(&self, toolkit: &str) -> Result<String> {
        let auth_config = match self.auth_config_for(toolkit).await? {
            Some(id) => id,
            // Composio brokers OAuth only for toolkits it holds its own app
            // registration for; `googlecontacts` advertises OAUTH2 as a scheme
            // and still has no managed credentials behind it. Asking anyway
            // returns a 400 that reads as a bug in Aloud. The `auth_config_for`
            // arm above is the escape hatch: an auth config created in the
            // Composio dashboard against your own OAuth client is found there,
            // and the rest of this function does not care where it came from.
            None if !self.has_managed_auth(toolkit).await? => bail!(
                "Composio has no credentials of its own for {toolkit}, so it \
                 cannot be connected from here. Add an auth config for it in \
                 the Composio dashboard using your own OAuth client, then try \
                 again."
            ),
            None => self.create_auth_config(toolkit).await?,
        };

        // One identity for all accounts, not one per toolkit. `/link`, because
        // `POST /connected_accounts` now 400s for Composio-managed OAuth.
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

    /// Whether Composio can supply the OAuth credentials itself. `auth_schemes`
    /// is not the same question — it lists what the *service* accepts, which is
    /// why a toolkit can show an OAuth2 badge and still be unconnectable.
    async fn has_managed_auth(&self, toolkit: &str) -> Result<bool> {
        let json = self.get(&format!("/toolkits/{toolkit}")).await?;
        Ok(json["composio_managed_auth_schemes"]
            .as_array()
            .is_some_and(|schemes| !schemes.is_empty()))
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

    /// Picks the connection whose mailbox matches `wanted`.
    ///
    /// Composio's connection metadata does not carry the address, so it is
    /// asked of the account itself — one `GMAIL_GET_PROFILE` per connection,
    /// cached for the process. Matching is loose on purpose: the address
    /// arrives from speech, so "yaksh dev" has to find `yakksh.dev@gmail.com`.
    /// An unmatched name falls back to the first connection rather than
    /// failing — reading the wrong inbox is recoverable, refusing to read at
    /// all mid-demo is not.
    async fn connection_for(&self, candidates: &[Connection], wanted: &str) -> Result<Connection> {
        if candidates.len() == 1 {
            return Ok(candidates[0].clone());
        }
        let wanted_key = fold_address(wanted);
        let mut known = Vec::new();
        for candidate in candidates {
            if let Some(address) = self.address_of(candidate).await {
                known.push((candidate, fold_address(&address), address));
            }
        }

        // Exact, then substring, then closest — a transcript says "yaksh dev"
        // for `yakksh.dev`, which shares no substring in either direction but
        // is unmistakably nearer to it than to `kortexnotebooklm`. Without the
        // third pass that command silently read the wrong mailbox.
        let exact = known.iter().find(|(_, key, _)| *key == wanted_key);
        let substring = known
            .iter()
            .find(|(_, key, _)| key.contains(&wanted_key) || wanted_key.contains(key));
        let nearest = known
            .iter()
            .map(|entry| (similarity(&entry.1, &wanted_key), entry))
            .filter(|(score, _)| *score >= 0.6)
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, entry)| entry);

        if let Some((connection, _, address)) = exact.or(substring).or(nearest) {
            tracing::info!(%address, "routed to the requested account");
            return Ok((*connection).clone());
        }

        tracing::warn!(%wanted, "no connected account matched; using the first");
        candidates.first().cloned().context("no active connection")
    }

    /// The email address behind a connection, cached. `None` when the account
    /// cannot say — the caller then treats it as "not a match" rather than
    /// erroring, so one unreachable account cannot block the others.
    async fn address_of(&self, connection: &Connection) -> Option<String> {
        if let Some(hit) = self.addresses.lock().ok()?.get(&connection.id) {
            return hit.clone();
        }
        let profile = self
            .post(
                "/tools/execute/GMAIL_GET_PROFILE",
                &json!({
                    "connected_account_id": connection.id,
                    "user_id": connection.user_id,
                    "arguments": {},
                }),
                Duration::from_secs(20),
            )
            .await
            .ok();
        let address = profile
            .as_ref()
            .and_then(|p| {
                p.pointer("/data/emailAddress")
                    .or(p.pointer("/data/response_data/emailAddress"))
            })
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Ok(mut cache) = self.addresses.lock() {
            cache.insert(connection.id.clone(), address.clone());
        }
        address
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
        // Match against connected toolkits, not a slug guess:
        // `MICROSOFT_TEAMS_SEND_MESSAGE` split on `_` gives "microsoft", which
        // matches nothing while Teams sits connected. Longest match wins, so a
        // `google` connection cannot shadow `google_maps`.
        let connections = self.connections().await?;
        let candidates: Vec<Connection> = connections
            .into_iter()
            .filter(|c| c.is_active() && slug_belongs_to(slug, &c.toolkit_slug))
            .collect();
        let best = candidates
            .iter()
            .map(|c| c.toolkit_slug.len())
            .max()
            .with_context(|| format!("no active connection for {}", toolkit_of_slug(slug)))?;
        let candidates: Vec<Connection> = candidates
            .into_iter()
            .filter(|c| c.toolkit_slug.len() == best)
            .collect();

        // `account` is ours, not Composio's: with three Gmail connections the
        // longest-match rule ties and picks arbitrarily, so "summarise my work
        // email" could read the wrong mailbox entirely. Stripped before the
        // arguments go upstream, which would reject an unknown field.
        let mut arguments = arguments.clone();
        let wanted = arguments
            .as_object_mut()
            .and_then(|a| a.remove("account"))
            .and_then(|a| a.as_str().map(str::to_string));

        let connection = match &wanted {
            Some(wanted) => self.connection_for(&candidates, wanted).await?,
            // No preference: the first active one, which is what every
            // single-account setup wants and what happened before.
            None => candidates
                .into_iter()
                .next()
                .context("no active connection")?,
        };

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

        // Raw payloads are huge (a Gmail message carries ~94KB of MIME).
        // Compact structurally, then cap — blind truncation cut JSON mid-string.
        let mut compact = Map::new();
        compact.insert(
            "successful".into(),
            json.get("successful")
                .cloned()
                .unwrap_or(Value::Bool(false)),
        );
        if let Some(data) = json.get("data") {
            // A bulk mail fetch is slimmed per message before the byte cap.
            // Capping it raw kept 11 of 100 messages and cut the JSON mid-object,
            // so the model saw a malformed result and reported failure — the
            // "summarise my last 100 emails" case failing for a formatting
            // reason rather than a real one.
            let data = match message_list(data) {
                Some(slim) => slim,
                None => compact_value(data),
            };
            compact.insert("data".into(), data);
        }
        if let Some(error) = json.get("error") {
            compact.insert("error".into(), error.clone());
        }
        let out = serde_json::to_string(&Value::Object(compact)).unwrap_or_else(|_| "{}".into());
        Ok(cap(out))
    }

    // MARK: transport

    async fn get(&self, path: &str) -> Result<Value> {
        // Only the tool catalogue: connections stay live, so an account connected a moment ago
        // shows up on the next command.
        let cacheable = path.starts_with("/tools?");
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            (&self.api_key, path).hash(&mut hasher);
            hasher.finish()
        };
        if cacheable {
            let hit = catalogue().lock().ok().and_then(|c| c.get(&key).cloned());
            if let Some((at, value)) = hit.filter(|(at, _)| at.elapsed() < CATALOGUE_TTL) {
                tracing::debug!(
                    path,
                    age_s = at.elapsed().as_secs(),
                    "tool catalogue from cache"
                );
                return Ok(value);
            }
        }
        let value = self
            .request(reqwest::Method::GET, path, None, Duration::from_secs(30))
            .await?;
        if cacheable {
            if let Ok(mut cache) = catalogue().lock() {
                cache.insert(key, (std::time::Instant::now(), value.clone()));
            }
        }
        Ok(value)
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
            let body = response.text().await.unwrap_or_default();
            bail!("{}", error_message(&body, status.as_u16()));
        }
        response.json().await.context("decoding composio response")
    }
}

/// What to show a user when Composio refuses a request.
///
/// Composio wraps failures as `{"error":{"message":…}}`, and that message is a
/// written sentence — "Default auth config not found for toolkit X". Bubbling
/// the envelope up instead put a raw JSON blob in the connectors pane. The
/// status-code form is only for a body that is not that shape at all.
fn error_message(body: &str, status: u16) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|json| json["error"]["message"].as_str().map(str::to_string))
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| {
            let snippet: String = body.chars().take(300).collect();
            format!("Composio HTTP {status}: {snippet}")
        })
}

/// Composio schemas run ~5K tokens for six tools, enough alone to burst a small
/// tokens-per-minute budget. Keep only the keys the model needs for arguments,
/// plus `title`: a few tokens that give the action preview its field labels.
pub(crate) fn slim_schema(schema: &Value) -> Value {
    let Some(obj) = schema.as_object() else {
        return schema.clone();
    };
    let mut out = Map::new();
    for (key, value) in obj {
        match key.as_str() {
            // `default` stays: without it the model invents values for optional
            // params (calendar_id "role" instead of the documented "primary").
            "type" | "required" | "enum" | "format" | "default" | "title" => {
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
            // drop examples, file_uploadable, …
            _ => {}
        }
    }
    Value::Object(out)
}

/// Strips bulk from tool results: drops MIME payloads and URL-catalog fields,
/// truncates strings to ~600 chars, plenty of an email body to summarize.
/// Per-message fields worth a summary. Everything else a Gmail message carries
/// — MIME parts, label ids, thread ids, attachment lists — is bulk that pushes
/// the real content past the byte cap.
const MESSAGE_FIELDS: &[&str] = &["sender", "to", "subject", "messageTimestamp"];

/// How much of each preview to keep.
///
/// Measured against a real 100-message fetch: sender/subject/date alone is
/// 19.4KB, and each 60 chars of preview adds ~12KB. 60 keeps the whole hundred
/// inside the cap at ~31KB while still saying what each message is about; 120
/// pushed it to 48KB and lost a third of them to truncation.
const PREVIEW_CHARS: usize = 60;

/// Slims a `{messages: [...]}` payload to the fields a summary needs, or `None`
/// when this is not a message list and the generic compaction should run.
///
/// Only worth doing above a handful of messages: a single fetched mail should
/// keep its body, which is what the reader actually asked for.
fn message_list(data: &Value) -> Option<Value> {
    let messages = data.get("messages")?.as_array()?;
    if messages.len() < 5 {
        return None;
    }
    let slim: Vec<Value> = messages
        .iter()
        .map(|message| {
            let mut out = Map::new();
            for field in MESSAGE_FIELDS {
                if let Some(value) = message.get(*field) {
                    out.insert((*field).to_string(), value.clone());
                }
            }
            // The preview lives under a nested `body` for a metadata fetch and
            // as `messageText` for a verbose one; either is a usable gist.
            let preview = message
                .pointer("/preview/body")
                .or_else(|| message.get("messageText"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if !preview.is_empty() {
                out.insert(
                    "preview".into(),
                    json!(preview
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .chars()
                        .take(PREVIEW_CHARS)
                        .collect::<String>()),
                );
            }
            Value::Object(out)
        })
        .collect();

    let mut out = Map::new();
    out.insert("count".into(), json!(slim.len()));
    out.insert("messages".into(), Value::Array(slim));
    Some(Value::Object(out))
}

/// How alike two folded addresses are, 0.0 to 1.0, by shared character bigrams.
///
/// Dice coefficient rather than edit distance: it is a few lines, needs no
/// dependency, and rewards a shared run of characters, which is exactly what
/// survives a transcription. "yakkshdev" vs "yakshdev" scores high; either
/// against "kortexnotebooklm" scores near zero.
fn similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let bigrams = |s: &str| -> Vec<[char; 2]> {
        let chars: Vec<char> = s.chars().collect();
        chars.windows(2).map(|w| [w[0], w[1]]).collect()
    };
    let (first, mut second) = (bigrams(a), bigrams(b));
    if first.is_empty() || second.is_empty() {
        return 0.0;
    }
    let total = first.len() + second.len();
    let mut shared = 0usize;
    for gram in first {
        if let Some(at) = second.iter().position(|g| *g == gram) {
            second.remove(at);
            shared += 1;
        }
    }
    2.0 * shared as f64 / total as f64
}

/// Adds the `account` argument to a tool's schema, listing the addresses the
/// user actually has connected. Only called for toolkits with more than one
/// connection: a single-account setup should never be asked which one.
fn add_account_argument(parameters: &mut Value, addresses: &[String]) {
    let Some(properties) = parameters
        .get_mut("properties")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let description = if addresses.is_empty() {
        "Which connected account to use, when the user names one.".to_string()
    } else {
        format!(
            "Which of the user's connected accounts to use: {}. Set it when the \
             user names or implies an account; omit it otherwise.",
            addresses.join(", ")
        )
    };
    properties.insert(
        "account".into(),
        json!({ "type": "string", "description": description }),
    );
}

/// Folds an address for loose comparison against something that came out of a
/// microphone. "Yaksh dot dev at gmail" and `yakksh.dev@gmail.com` have to meet
/// somewhere, so everything but the letters and digits goes, along with the
/// domain — nobody says "at gmail dot com" to distinguish two Gmail accounts.
fn fold_address(raw: &str) -> String {
    let head = raw.split('@').next().unwrap_or(raw);
    head.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

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

/// The byte ceiling on a tool result.
///
/// 36000, raised from 8000 once bulk mail was slimmed structurally. Measured:
/// 100 messages at sender/subject/date plus a 60-char preview is ~31.5KB, and
/// at 8000 the result was cut mid-object — the model saw malformed JSON and
/// called the fetch failed, which is how "summarise my last 100 emails" broke
/// for a formatting reason rather than a real one. ~9k tokens is affordable
/// once per run for a task that is explicitly about a hundred things; the
/// generic path still strips MIME payloads first, so a verbose single fetch
/// does not get to spend it.
const MAX_RESULT_BYTES: usize = 36_000;

/// Caps a result, backing off to a char boundary — a raw slice panics
/// mid-UTF-8, and email bodies are full of multibyte text.
fn cap(out: String) -> String {
    if out.len() <= MAX_RESULT_BYTES {
        return out;
    }
    let cut = (0..=MAX_RESULT_BYTES)
        .rev()
        .find(|&i| out.is_char_boundary(i))
        .unwrap();
    format!("{}…(truncated)", &out[..cut])
}

/// The toolkit a tool slug belongs to, best-effort: right for a single-token
/// toolkit, wrong for the 96 of Composio's 500 whose slug has an underscore.
/// Prefer [`ToolDef::toolkit`] or [`slug_belongs_to`]; this is the fallback for
/// a display path with neither.
pub fn toolkit_of_slug(slug: &str) -> String {
    slug.split('_').next().unwrap_or(slug).to_lowercase()
}

/// Whether `slug` is one of `toolkit`'s tools. Composio names every tool
/// `<TOOLKIT>_<VERB>`, so a prefix test is exact where splitting only guesses.
pub fn slug_belongs_to(slug: &str, toolkit: &str) -> bool {
    let prefix = format!("{}_", toolkit.to_uppercase());
    slug.to_uppercase().starts_with(&prefix)
}

/// Percent-encodes a search term — no crate for one query param. Escapes
/// anything outside the set, stricter than necessary and therefore safe.
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

/// The words of a command worth matching a tool against. Proper nouns survive
/// and match nothing, costing a point of score rather than emptying the result
/// the way Composio's server-side AND search does.
fn keywords(command: &str) -> Vec<String> {
    command
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.len() > 2 && !STOP_WORDS.contains(&w.as_str()))
        .collect()
}

/// How well one raw tool object matches `words`. A word in the tool's name
/// outweighs one in its description: every calendar tool mentions "event"
/// somewhere in its prose, but only `DELETE_EVENT` is named for it.
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

/// One raw tool object to a model-facing definition.
fn tool_def(item: &Value) -> Option<ToolDef> {
    let name = item["slug"].as_str()?.to_string();
    // Composio reports this on every tool; the split is for malformed items.
    let toolkit = item["toolkit"]["slug"]
        .as_str()
        .map(str::to_lowercase)
        .unwrap_or_else(|| toolkit_of_slug(&name));
    let hints: Vec<String> = item["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|tag| tag.ends_with("Hint"))
        .map(str::to_string)
        .collect();
    Some(ToolDef {
        app: item["toolkit"]["name"]
            .as_str()
            .filter(|n| !n.trim().is_empty())
            .map_or_else(|| toolkit.clone(), str::to_string),
        toolkit,
        name,
        description: item["description"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(300)
            .collect(),
        parameters: slim_schema(item.get("input_parameters")?),
        read_only: hints.iter().any(|h| h == READ_ONLY_TAG),
        hints,
    })
}

fn error_json(message: &str) -> String {
    serde_json::to_string(&json!({ "error": message }))
        .unwrap_or_else(|_| r#"{"error":"unknown"}"#.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape `/tools` returned live on 2026-10-03: hints mixed in with
    /// category tags, and `toolkit.name` just the slug again.
    #[test]
    fn hints_are_the_tags_ending_in_hint() {
        let item = json!({
            "slug": "GMAIL_MOVE_TO_TRASH",
            "toolkit": { "slug": "gmail", "name": "gmail" },
            "tags": ["important", "openWorldHint", "updateHint"],
            "input_parameters": { "type": "object", "properties": {} }
        });
        let tool = tool_def(&item).unwrap();
        assert_eq!(tool.hints, ["openWorldHint", "updateHint"]);
        assert_eq!(tool.app, "gmail");
        assert!(!tool.read_only);

        let mut read = item.clone();
        read["tags"] = json!(["readOnlyHint", "chat"]);
        assert!(tool_def(&read).unwrap().read_only);
    }

    #[test]
    fn only_toolkits_the_command_is_about_are_offered() {
        assert!(concerns("gmail", "summarize my unread emails"));
        assert!(concerns("googlecalendar", "what's on my calendar tomorrow"));
        assert!(concerns("github", "list my open PRs"));
        assert!(!concerns("gmail", "make a cake list and add it to a note"));
        assert!(!concerns("googlecontacts", "create a new note"));
        assert!(!concerns("github", "remind me to call Ana at 5"));
        // An unlisted toolkit is offered rather than silently hidden.
        assert!(concerns("hubspot", "create a new note"));
    }

    /// Three Gmail accounts tie on the longest-match rule, so the user has to
    /// be able to name one — and the name arrives from a microphone, not a
    /// keyboard.
    #[test]
    fn spoken_account_names_match_the_real_address() {
        let mailbox = fold_address("yakksh.dev@gmail.com");
        // The forms a transcript actually produces.
        for spoken in [
            "yakksh.dev@gmail.com",
            "yakksh.dev",
            "Yakksh Dev",
            "yakksh dev",
            "YAKKSH.DEV@GMAIL.COM",
        ] {
            assert_eq!(fold_address(spoken), mailbox, "{spoken:?} should match");
        }

        // A different mailbox must not collide with it.
        assert_ne!(fold_address("kortexnotebooklm@gmail.com"), mailbox);

        // A misheard spelling shares no substring either way, so folding alone
        // routed it to the wrong inbox — verified live before this was added.
        let misheard = fold_address("yaksh dev");
        assert!(!mailbox.contains(&misheard) && !misheard.contains(&mailbox));
        assert!(
            similarity(&mailbox, &misheard) >= 0.6,
            "a plausible transcription must still reach its own mailbox"
        );
        assert!(
            similarity(&fold_address("kortexnotebooklm@gmail.com"), &misheard) < 0.6,
            "and must not reach a different one"
        );
        // The domain is dropped, so two accounts differing only by domain are
        // indistinguishable here — acceptable, since nobody says "at gmail dot
        // com" to tell two Gmail accounts apart.
        assert_eq!(fold_address("a@gmail.com"), fold_address("a@work.com"));
    }

    /// "Summarise my last 100 emails" returned 11 of them and reported failure:
    /// the byte cap cut the JSON mid-object. A bulk fetch is slimmed to the
    /// fields a summary needs so all hundred survive.
    #[test]
    fn a_hundred_emails_survive_the_cap_and_stay_parseable() {
        let messages: Vec<Value> = (0..100)
            .map(|i| {
                json!({
                    "sender": format!("Sender {i} <s{i}@example.com>"),
                    "to": "me@example.com",
                    "subject": format!("Subject line number {i}"),
                    "messageTimestamp": "2026-07-27T05:08:20Z",
                    "preview": { "body": "x".repeat(4000) },
                    // The bulk a real message carries, none of it summarisable.
                    "payload": { "parts": ["y".repeat(40_000)] },
                    "labelIds": ["INBOX", "CATEGORY_PROMOTIONS"],
                    "threadId": format!("thread-{i}"),
                })
            })
            .collect();

        let slim = message_list(&json!({ "messages": messages })).expect("a message list");
        assert_eq!(slim["count"], 100, "every message must be kept");

        let capped = cap(serde_json::to_string(&slim).unwrap());
        assert!(
            !capped.ends_with("…(truncated)"),
            "100 slimmed messages must fit: got {} bytes",
            capped.len()
        );
        // Malformed JSON is what made the model report the fetch as failed.
        let parsed: Value = serde_json::from_str(&capped).expect("must stay parseable");
        assert_eq!(parsed["messages"].as_array().unwrap().len(), 100);
        assert_eq!(parsed["messages"][99]["subject"], "Subject line number 99");
        assert!(
            parsed["messages"][0].get("payload").is_none(),
            "MIME bulk must not reach the model"
        );
    }

    /// A single fetched mail should keep its body — the reader asked for that
    /// one message, so slimming it to a 60-char preview would be the wrong
    /// trade.
    #[test]
    fn a_small_fetch_keeps_its_bodies() {
        let messages: Vec<Value> = (0..3)
            .map(|i| json!({ "sender": "a@b.com", "messageText": format!("body {i}") }))
            .collect();
        assert!(
            message_list(&json!({ "messages": messages })).is_none(),
            "a handful of messages should take the generic path"
        );
    }

    /// The `account` argument is ours and Composio would reject it upstream, so
    /// it is only offered where there is genuinely a choice to make.
    #[test]
    fn the_account_argument_lists_the_connected_addresses() {
        let mut schema =
            json!({ "type": "object", "properties": { "query": { "type": "string" } } });
        add_account_argument(&mut schema, &["a@gmail.com".into(), "b@gmail.com".into()]);
        let account = &schema["properties"]["account"];
        assert_eq!(account["type"], "string");
        let description = account["description"].as_str().unwrap();
        assert!(description.contains("a@gmail.com"), "{description}");
        assert!(description.contains("b@gmail.com"), "{description}");
        // The original arguments survive.
        assert_eq!(schema["properties"]["query"]["type"], "string");
    }

    /// Execution routes by slug prefix, so this must work for toolkits nobody
    /// hardcoded.
    #[test]
    fn toolkit_routing_splits_on_the_slug_prefix() {
        assert_eq!(toolkit_of_slug("GMAIL_FETCH_EMAILS"), "gmail");
        assert_eq!(
            toolkit_of_slug("GITHUB_GET_THE_AUTHENTICATED_USER"),
            "github"
        );
        assert_eq!(toolkit_of_slug("SLACK_SEND_MESSAGE"), "slack");
        assert_eq!(toolkit_of_slug("NOTION_CREATE_PAGE"), "notion");
        // The confirmation panel once announced this as "writes to github".
        assert_eq!(
            toolkit_of_slug("GOOGLECALENDAR_DELETE_EVENT"),
            "googlecalendar"
        );
    }

    /// Regression: the first-token split routed the 96 underscored toolkits to
    /// connections that do not exist — Teams failed while sitting connected.
    #[test]
    fn tools_match_their_toolkit_even_when_its_slug_has_an_underscore() {
        assert!(slug_belongs_to(
            "MICROSOFT_TEAMS_SEND_MESSAGE",
            "microsoft_teams"
        ));
        assert!(slug_belongs_to("GOOGLE_MAPS_AUTOCOMPLETE", "google_maps"));
        assert!(slug_belongs_to("ONE_DRIVE_CHECKIN_ITEM", "one_drive"));
        assert!(slug_belongs_to("GMAIL_SEND_EMAIL", "gmail"));

        // A shorter toolkit must not even be a candidate for a longer one's
        // tools, before the caller's longest-match tiebreak.
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
        // The live phrasing Composio's own search returned nothing for.
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
            "title": "SendEmailRequest",
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
        // The action preview labels its fields from these.
        assert_eq!(slim["title"], "SendEmailRequest");
        assert_eq!(slim["type"], "object");
        assert_eq!(slim["required"], json!(["to"]));
        let to = &slim["properties"]["to"];
        assert_eq!(to["type"], "string");
        assert_eq!(to["description"], "recipient");
        assert!(to.get("examples").is_none(), "examples must be dropped");
        assert!(to.get("file_uploadable").is_none());
        // The model invented calendar_id "role" when the default was stripped.
        assert_eq!(slim["properties"]["calendar_id"]["default"], "primary");
    }

    #[test]
    fn cap_never_slices_mid_utf8_char() {
        // The cut lands inside the first crab — a raw byte slice panicked here.
        // Sized off the constant so raising the cap cannot silently stop
        // exercising the boundary.
        let s = "x".repeat(MAX_RESULT_BYTES - 1) + "🦀🦀🦀";
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

    /// The reported bug: the connectors pane showed
    /// `Composio HTTP 400: {"error":{"message":"Default auth config not found …`
    /// — the envelope, not the sentence inside it.
    #[test]
    fn composio_errors_surface_their_message_not_their_envelope() {
        let body = r#"{"error":{"message":"Default auth config not found for toolkit \"googlecontacts\".","code":306,"status":400}}"#;
        assert_eq!(
            error_message(body, 400),
            "Default auth config not found for toolkit \"googlecontacts\"."
        );
    }

    #[test]
    fn a_body_that_is_not_an_error_envelope_still_says_something() {
        // A gateway's HTML, an empty body, or a shape Composio changes to.
        assert!(error_message("<html>502</html>", 502).contains("502"));
        assert!(error_message("", 500).contains("500"));
        assert!(error_message(r#"{"error":{"message":""}}"#, 400).contains("400"));
    }
}
