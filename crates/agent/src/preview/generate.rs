//! Generated mappings: the card for a connected write tool nobody hand-mapped,
//! asked of the brain once and kept on disk. A generated mapping says only the
//! kind, the verb, which argument fills which slot, and whether to raise the
//! tier — every value is still read from the call's own arguments, and no
//! words the model wrote reach the card. Anything that fails validation is
//! stored as `null`, so the card falls back to the schema layer.

use super::fallback::{build, VERBS};
use super::hand::{hand, prop_at, Mapping, Slot};
use super::{CardKind, Role, Tier};
use crate::composio::{ComposioClient, ToolDef};
use crate::Brain;
use aloud_llm::{LlmError, Message, ToolTurn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

/// Bump when the prompt or the `map_tool` schema changes: every stored mapping
/// is then regenerated.
pub const PROMPT_VERSION: u32 = 1;
/// One pass's ceiling; the next pass picks up where it stopped. Small, because
/// the pass spends the same keys live commands do and free tiers are thin.
const MAX_CALLS: usize = 25;
/// Between calls, so a pass never bursts a per-minute budget.
const SPACING: Duration = Duration::from_secs(2);
/// A tool the brain could not map is asked again after this, not every launch.
const RETRY_FAILED_AFTER_S: u64 = 7 * 24 * 60 * 60;

/// Which layer draws a tool's card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Hand,
    Generated,
    Schema,
}

/// What one pass did.
#[derive(Debug, Default)]
pub struct Pass {
    pub asked: usize,
    pub mapped: Vec<String>,
    /// Answered, but failed validation or named no mapping: stored as `null`.
    pub rejected: Vec<String>,
    /// Why the pass ended early, when it did.
    pub stopped: Option<String>,
}

/// `action-previews.json`.
#[derive(Serialize, Deserialize)]
struct Store {
    version: u32,
    tools: BTreeMap<String, Entry>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            version: PROMPT_VERSION,
            tools: BTreeMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct Entry {
    /// FNV-1a of the prompt version, name, description and schema: a change to
    /// any of them regenerates.
    hash: String,
    /// FNV-1a of the schema alone. The gate hands `render` the schema but not
    /// the description, so this is what a card can check against.
    schema: String,
    /// Unix seconds of the attempt.
    at: u64,
    mapping: Option<Generated>,
}

/// The `map_tool` arguments, and what is stored. Unknown keys are ignored:
/// nothing past these four is ever read.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Generated {
    kind: CardKind,
    verb: String,
    slots: Vec<GeneratedSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tier: Option<Tier>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct GeneratedSlot {
    role: Role,
    path: String,
}

struct Cached {
    schema: String,
    mapping: Arc<Mapping>,
}

/// The stored mappings `render` reads, by tool. `null` entries are left out.
fn cache() -> &'static RwLock<HashMap<String, Cached>> {
    static CACHE: OnceLock<RwLock<HashMap<String, Cached>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// The generated mapping for `name`, only while `schema` is the one it was
/// generated against.
pub(super) fn generated(name: &str, schema: Option<&Value>) -> Option<Arc<Mapping>> {
    let schema = fingerprint(schema?);
    let cache = cache().read().ok()?;
    let cached = cache.get(name).filter(|c| c.schema == schema)?;
    Some(cached.mapping.clone())
}

/// Which layer [`super::render`] would draw `name` with.
pub fn layer(name: &str, schema: Option<&Value>) -> Layer {
    if hand(name).is_some() {
        Layer::Hand
    } else if generated(name, schema).is_some() {
        Layer::Generated
    } else {
        Layer::Schema
    }
}

/// Makes the mappings stored at `path` visible to cards. Call once at startup,
/// before any pass has run.
pub fn load_generated(path: &Path) {
    for (name, entry) in read(path).tools {
        remember(&name, &entry);
    }
}

fn remember(name: &str, entry: &Entry) {
    let Ok(mut cache) = cache().write() else {
        return;
    };
    match &entry.mapping {
        Some(generated) => {
            let cached = Cached {
                schema: entry.schema.clone(),
                mapping: Arc::new(to_mapping(generated)),
            };
            cache.insert(name.to_string(), cached);
        }
        None => {
            cache.remove(name);
        }
    }
}

fn to_mapping(g: &Generated) -> Mapping {
    Mapping {
        kind: g.kind,
        verb: g.verb.clone(),
        noun: None,
        to: None,
        tier: g.tier,
        slots: g
            .slots
            .iter()
            .map(|s| Slot {
                role: s.role,
                path: s.path.clone(),
                label: None,
            })
            .collect(),
        editable: Vec::new(),
        lookups: Vec::new(),
        current: None,
        sample: None,
        risks: Vec::new(),
        recoverable: None,
        safer: None,
        ask: None,
    }
}

/// A missing or unreadable file, or one from another prompt version, starts
/// empty: the cost of losing it is regenerating, never a wrong card.
fn read(path: &Path) -> Store {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Store::default();
    };
    match serde_json::from_str::<Store>(&text) {
        Ok(store) if store.version == PROMPT_VERSION => store,
        Ok(_) => Store::default(),
        Err(e) => {
            tracing::warn!(%e, path = %path.display(), "unreadable action previews; regenerating");
            Store::default()
        }
    }
}

/// Temp file and rename, so a crash mid-write never leaves half a file.
fn write(path: &Path, store: &Store) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(store)?)?;
    std::fs::rename(&tmp, path)
}

/// FNV-1a, not `DefaultHasher`: the hash is persisted, and std's algorithm may
/// change between Rust releases.
fn fnv(parts: &[&str]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        // The 0 separates parts, so "ab"+"c" and "a"+"bc" differ.
        for byte in part.bytes().chain([0]) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

fn hash(tool: &ToolDef) -> String {
    let version = PROMPT_VERSION.to_string();
    let params = tool.parameters.to_string();
    fnv(&[&version, &tool.name, &tool.description, &params])
}

fn fingerprint(schema: &Value) -> String {
    fnv(&[&schema.to_string()])
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Whether a pass should ask about `tool`. A hand mapping always wins, so a
/// tool that has one is never asked about.
fn due(tool: &ToolDef, entry: Option<&Entry>, now: u64) -> bool {
    if tool.read_only || hand(&tool.name).is_some() {
        return false;
    }
    match entry {
        None => true,
        Some(e) if e.hash != hash(tool) => true,
        Some(e) => e.mapping.is_none() && now.saturating_sub(e.at) >= RETRY_FAILED_AFTER_S,
    }
}

/// Connected write tools no hand mapping covers get a card, in the background.
/// Loads what earlier passes stored first, so cards draw from it at once.
/// Skipped without a brain; a pass already running makes this a no-op.
pub async fn warm_previews(brains: &[Brain], composio: &ComposioClient, path: &Path) {
    load_generated(path);
    if brains.is_empty() {
        return;
    }
    let toolkits: BTreeSet<String> = match composio.connections().await {
        Ok(connections) => connections
            .into_iter()
            .filter(|c| c.is_active())
            .map(|c| c.toolkit_slug)
            .collect(),
        Err(e) => {
            tracing::warn!(%e, "action previews: no connections");
            return;
        }
    };
    let mut tools = Vec::new();
    for toolkit in &toolkits {
        match composio.all_tools(toolkit).await {
            Ok(all) => tools.extend(all.into_iter().filter(|t| !t.read_only)),
            Err(e) => tracing::warn!(%e, toolkit, "action previews: no tools"),
        }
    }
    // The last brain, not the first: live commands lean on the first's
    // per-minute budget (Groq's free tier is about one step a minute), while
    // the fallback's quota is daily and mostly idle.
    let pass = generate(&brains[brains.len() - 1..], &tools, path).await;
    tracing::info!(
        asked = pass.asked,
        mapped = pass.mapped.len(),
        rejected = pass.rejected.len(),
        stopped = pass.stopped,
        "action previews pass"
    );
}

/// One pass over `tools`: asks the brain about each that is due, up to
/// [`MAX_CALLS`], storing every answer as it arrives. Moves to the next brain
/// when one is out of budget or unpaid, as the tool loop does.
pub async fn generate(brains: &[Brain], tools: &[ToolDef], path: &Path) -> Pass {
    static RUNNING: AtomicBool = AtomicBool::new(false);
    struct Release;
    impl Drop for Release {
        fn drop(&mut self) {
            RUNNING.store(false, Ordering::Release);
        }
    }
    if brains.is_empty() || RUNNING.swap(true, Ordering::AcqRel) {
        return Pass::default();
    }
    let _release = Release;

    // An atomic, not a `&mut`: the future each call returns must be `Send` to
    // be spawned, which an async closure borrowing its state cannot yet prove.
    let current = AtomicUsize::new(0);
    let current = &current;
    let ask = |tool: &ToolDef| {
        let (messages, schemas) = request(tool);
        async move {
            loop {
                let at = current.load(Ordering::Relaxed);
                let brain = &brains[at];
                let turn = brain
                    .client
                    .chat_with_tools(brain.model, &messages, Some(&schemas), Some(0.0))
                    .await;
                match turn {
                    Err(e)
                        if at + 1 < brains.len()
                            && e.downcast_ref().is_some_and(crate::tool_loop::fails_over) =>
                    {
                        current.store(at + 1, Ordering::Relaxed);
                    }
                    turn => return turn,
                }
            }
        }
    };
    pass(tools, path, now(), ask).await
}

async fn pass<F: Future<Output = anyhow::Result<ToolTurn>>>(
    tools: &[ToolDef],
    path: &Path,
    now: u64,
    ask: impl Fn(&ToolDef) -> F,
) -> Pass {
    let mut store = read(path);
    let asking: Vec<&ToolDef> = tools
        .iter()
        .filter(|t| due(t, store.tools.get(&t.name), now))
        .take(MAX_CALLS)
        .collect();
    let mut out = Pass::default();
    let mut failed_last = false;
    for (i, tool) in asking.into_iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(SPACING).await;
        }
        out.asked += 1;
        let turn = match ask(tool).await {
            Ok(turn) => turn,
            // Nothing is stored, so the tool is asked again next pass. A bad
            // key or a down provider fails every call, and a rate limit means
            // stop now; one tool's request a provider refuses fails only once.
            Err(e) => {
                let limited = matches!(e.downcast_ref(), Some(LlmError::RateLimited { .. }));
                if limited || failed_last {
                    out.stopped = Some(format!("{e:#}"));
                    break;
                }
                failed_last = true;
                continue;
            }
        };
        failed_last = false;
        let mapping = answer(&turn).and_then(|g| validate(tool, g));
        let list = if mapping.is_some() {
            &mut out.mapped
        } else {
            &mut out.rejected
        };
        list.push(tool.name.clone());
        let entry = Entry {
            hash: hash(tool),
            schema: fingerprint(&tool.parameters),
            at: now,
            mapping,
        };
        remember(&tool.name, &entry);
        store.tools.insert(tool.name.clone(), entry);
        // Every answer, not once at the end: a pass cut short keeps its progress.
        if let Err(e) = write(path, &store) {
            tracing::warn!(%e, path = %path.display(), "could not store action previews");
        }
    }
    out
}

fn answer(turn: &ToolTurn) -> Option<Generated> {
    let call = turn
        .tool_calls
        .iter()
        .find(|c| c.function.name == "map_tool")?;
    serde_json::from_str(&call.function.arguments).ok()
}

/// Button labels a generated card may use: the schema layer's, and the hand
/// mappings' that fit any toolkit.
fn verbs() -> Vec<&'static str> {
    let mut out: Vec<&str> = VERBS
        .iter()
        .map(|(_, label)| *label)
        .chain([
            "Archive", "Charge", "Mark", "Rename", "Save", "Schedule", "Set", "Trash",
        ])
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

const KINDS: [CardKind; 12] = [
    CardKind::Message,
    CardKind::Email,
    CardKind::Event,
    CardKind::Task,
    CardKind::Note,
    CardKind::Issue,
    CardKind::Row,
    CardKind::File,
    CardKind::Share,
    CardKind::Update,
    CardKind::Delete,
    CardKind::Money,
];

const ROLES: [Role; 13] = [
    Role::Destination,
    Role::Person,
    Role::Heading,
    Role::When,
    Role::Text,
    Role::Money,
    Role::Path,
    Role::Url,
    Role::Id,
    Role::Pill,
    Role::List,
    Role::Flag,
    Role::Plain,
];

/// Any answer that is not a usable card is `None`: a slot naming no field of
/// the schema, a kind or verb outside its list, a repeated slot, no slots at
/// all, or a tier lower than the schema layer gives this tool. A role or tier
/// outside its enum never parses.
fn validate(tool: &ToolDef, mut g: Generated) -> Option<Generated> {
    let schema = &tool.parameters;
    let mut paths = BTreeSet::new();
    let slots_land = !g.slots.is_empty()
        && g.slots
            .iter()
            .all(|s| prop_at(Some(schema), &s.path).is_some() && paths.insert(s.path.as_str()));
    // The floor is what the card would show without this mapping.
    let floor = if g.kind == CardKind::Money
        || build(&tool.name, &json!({}), Some(schema)).tier == Tier::Strict
    {
        Tier::Strict
    } else {
        Tier::Confirm
    };
    let lowered = floor == Tier::Strict && g.tier == Some(Tier::Confirm);
    let ok =
        slots_land && KINDS.contains(&g.kind) && verbs().contains(&g.verb.as_str()) && !lowered;
    if floor == Tier::Strict {
        g.tier = Some(Tier::Strict);
    }
    ok.then_some(g)
}

fn names<T: Serialize>(all: &[T]) -> Vec<Value> {
    all.iter()
        .filter_map(|v| serde_json::to_value(v).ok())
        .collect()
}

const SYSTEM: &str = "You decide how a confirmation card shows one call of a tool, \
before it runs. Call map_tool once.\n\
kind: the card layout that fits what the tool does. verb: the confirm button's label.\n\
slots: the arguments the card shows first, most important first, three to six of them. \
Each path is a JSON pointer into the arguments (/channel, /event/start) and must name a \
property in the schema. role says how it is drawn: destination is where a message goes \
(a channel or chat); person is who it goes to (an address or attendee list); heading a \
title or subject; when a date or time; text body text; money an amount; path a file \
path; url a link; id an opaque identifier; pill one of an enum; list an array; flag a \
boolean; plain anything else.\n\
tier: set it to strict only when a mistake cannot be undone or reaches many people — \
a permanent delete, a payment, a public post, sharing outside the organisation. \
Otherwise leave it out.";

/// The one call that maps `tool`: a system prompt, the tool, and the single
/// `map_tool` function whose enums come from the Rust types.
fn request(tool: &ToolDef) -> (Vec<Message>, Vec<Value>) {
    let user = format!(
        "Tool: {}\nDescription: {}\nArguments schema: {}",
        tool.name, tool.description, tool.parameters
    );
    let map_tool = json!({
        "type": "function",
        "function": {
            "name": "map_tool",
            "description": "How the confirmation card for this tool is drawn.",
            "parameters": {
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": names(&KINDS) },
                    "verb": { "type": "string", "enum": verbs() },
                    "slots": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "role": { "type": "string", "enum": names(&ROLES) },
                                "path": { "type": "string" },
                            },
                            "required": ["role", "path"],
                        },
                    },
                    "tier": { "type": "string", "enum": names(&[Tier::Strict]) },
                },
                "required": ["kind", "verb", "slots"],
            },
        },
    });
    (
        vec![Message::system(SYSTEM), Message::user(user)],
        vec![map_tool],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::{render, ActionPreview, Context, Names};
    use aloud_llm::{FunctionCall, ToolCall};
    use std::future::ready;
    use std::path::PathBuf;

    const NOW: u64 = 1_800_000_000;
    const DAY: u64 = 24 * 60 * 60;

    fn tool(name: &str, parameters: Value) -> ToolDef {
        ToolDef {
            name: name.to_string(),
            toolkit: "acme".into(),
            app: "acme".into(),
            description: "Does a thing in Acme.".into(),
            parameters,
            read_only: false,
            hints: Vec::new(),
        }
    }

    fn note() -> Value {
        json!({ "type": "object", "properties": {
            "channel": { "type": "string" },
            "text": { "type": "string" },
        } })
    }

    fn turn(args: Value) -> ToolTurn {
        ToolTurn {
            content: None,
            tool_calls: vec![ToolCall {
                id: "1".into(),
                kind: "function".into(),
                function: FunctionCall {
                    name: "map_tool".into(),
                    arguments: args.to_string(),
                },
                extra_content: None,
            }],
        }
    }

    /// A store file of the test's own, removed when the test ends.
    struct Temp(PathBuf);
    impl Temp {
        fn new(test: &str) -> Self {
            let name = format!("aloud-previews-{}-{test}.json", std::process::id());
            let path = std::env::temp_dir().join(name);
            let _ = std::fs::remove_file(&path);
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    /// One pass in which the brain always answers `reply`; how many times it
    /// was asked comes back in `Pass::asked`.
    async fn once(tools: &[ToolDef], path: &Path, now: u64, reply: &Value) -> Pass {
        pass(tools, path, now, |_: &ToolDef| {
            ready(Ok(turn(reply.clone())))
        })
        .await
    }

    fn card(tool: &ToolDef, args: Value, schema: &Value) -> ActionPreview {
        let names = Names::default();
        render(
            &tool.name,
            &args,
            Some(schema),
            &Context {
                said: "",
                names: &names,
            },
        )
    }

    fn stored(path: &Path, name: &str) -> Entry {
        read(path).tools.remove(name).expect("an entry was stored")
    }

    #[tokio::test]
    async fn a_mapping_naming_a_missing_field_is_rejected() {
        let file = Temp::new("missing");
        let t = tool("ACME_MISSING_FIELD_POST", note());
        let reply = json!({ "kind": "message", "verb": "Send", "slots": [
            { "role": "destination", "path": "/channel" },
            { "role": "text", "path": "/body" },
        ] });
        let pass = once(std::slice::from_ref(&t), &file.0, NOW, &reply).await;
        assert_eq!(pass.rejected, std::slice::from_ref(&t.name));
        assert!(stored(&file.0, &t.name).mapping.is_none());
        assert_eq!(layer(&t.name, Some(&t.parameters)), Layer::Schema);

        // The same answer with a real field validates. So does each enum check
        // reject what is outside it.
        let good = json!({ "kind": "message", "verb": "Send", "slots": [
            { "role": "destination", "path": "/channel" },
            { "role": "text", "path": "/text" },
        ] });
        let parse = |v: Value| serde_json::from_value::<Generated>(v).ok();
        assert!(parse(good.clone()).and_then(|g| validate(&t, g)).is_some());
        for bad in [
            json!({ "kind": "message", "verb": "Yeet", "slots": good["slots"] }),
            json!({ "kind": "fallback", "verb": "Send", "slots": good["slots"] }),
            json!({ "kind": "message", "verb": "Send", "slots": [{ "role": "who", "path": "/channel" }] }),
            json!({ "kind": "message", "verb": "Send", "slots": [] }),
            json!({ "kind": "message", "verb": "Send", "slots": [
                { "role": "text", "path": "/text" }, { "role": "plain", "path": "/text" },
            ] }),
        ] {
            assert!(
                parse(bad.clone()).and_then(|g| validate(&t, g)).is_none(),
                "{bad}"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_generated_mapping_cannot_lower_the_tier() {
        let file = Temp::new("tier");
        let ids = json!({ "type": "object", "properties": {
            "ids": { "type": "array", "items": { "type": "string" } },
        } });
        // The schema layer already makes a permanent delete strict.
        let lowers = tool("ACME_PERMANENTLY_DELETE_LOWERED", ids.clone());
        let silent = tool("ACME_PERMANENTLY_DELETE_SILENT", ids.clone());
        let slots = json!([{ "role": "list", "path": "/ids" }]);
        once(
            std::slice::from_ref(&lowers),
            &file.0,
            NOW,
            &json!({ "kind": "delete", "verb": "Delete", "slots": slots, "tier": "confirm" }),
        )
        .await;
        once(
            std::slice::from_ref(&silent),
            &file.0,
            NOW,
            &json!({ "kind": "delete", "verb": "Delete", "slots": slots }),
        )
        .await;

        let args = json!({ "ids": ["r1", "r2"] });
        assert!(stored(&file.0, &lowers.name).mapping.is_none());
        assert_eq!(card(&lowers, args.clone(), &ids).tier, Tier::Strict);
        // Saying nothing keeps the floor.
        assert_eq!(layer(&silent.name, Some(&ids)), Layer::Generated);
        assert_eq!(card(&silent, args, &ids).tier, Tier::Strict);

        // Raising is allowed.
        let raised = tool("ACME_POST_ANNOUNCEMENT_RAISED", note());
        let reply = json!({ "kind": "message", "verb": "Send", "tier": "strict",
                            "slots": [{ "role": "destination", "path": "/channel" }] });
        once(std::slice::from_ref(&raised), &file.0, NOW, &reply).await;
        let p = card(&raised, json!({ "channel": "all", "text": "hi" }), &note());
        assert_eq!((p.kind, p.tier), (CardKind::Message, Tier::Strict));
    }

    #[tokio::test]
    async fn a_changed_schema_ignores_the_cached_mapping() {
        let file = Temp::new("changed");
        let t = tool("ACME_DO_THE_CHANGED_THING", note());
        let reply = json!({ "kind": "message", "verb": "Send", "slots": [
            { "role": "destination", "path": "/channel" },
        ] });
        once(std::slice::from_ref(&t), &file.0, NOW, &reply).await;
        let args = json!({ "channel": "launch", "text": "hi" });
        let p = card(&t, args.clone(), &t.parameters);
        assert_eq!((p.kind, p.verb.as_str()), (CardKind::Message, "Send"));

        // Composio renames a field: the card draws from the schema again.
        let renamed = json!({ "type": "object", "properties": {
            "channel_id": { "type": "string" },
            "text": { "type": "string" },
        } });
        let p = card(&t, args, &renamed);
        assert_eq!((p.kind, p.verb.as_str()), (CardKind::Fallback, "Run"));

        // A restart reads it back, still bound to the schema it was made for.
        cache().write().unwrap().remove(&t.name);
        load_generated(&file.0);
        assert_eq!(layer(&t.name, Some(&t.parameters)), Layer::Generated);
        assert_eq!(layer(&t.name, Some(&renamed)), Layer::Schema);

        // And the next pass asks again about the changed tool.
        let changed = tool(&t.name, renamed);
        let pass = once(std::slice::from_ref(&changed), &file.0, NOW, &reply).await;
        assert_eq!(pass.asked, 1);
        // `/channel` is gone from the new schema, so that answer is refused.
        assert_eq!(pass.rejected, std::slice::from_ref(&t.name));
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_generation_is_not_retried_every_launch() {
        let file = Temp::new("failed");
        let t = tool("ACME_SEND_FAILED_NOTE", note());
        let other = tool("ACME_SEND_OTHER_NOTE", note());
        let both = [t.clone(), other.clone()];

        // A rate limit stops the pass and stores nothing, so it is not a failure.
        let limited = pass(&both, &file.0, NOW, |_: &ToolDef| {
            ready(Err(LlmError::RateLimited {
                model: "m".into(),
                retry_after: Duration::from_secs(5),
            }
            .into()))
        })
        .await;
        assert_eq!(limited.asked, 1);
        assert!(limited.stopped.is_some());
        assert!(read(&file.0).tools.is_empty());

        // An answer in prose maps nothing: stored as null.
        let prose = ToolTurn {
            content: Some("It sends a note.".into()),
            tool_calls: Vec::new(),
        };
        let first = pass(std::slice::from_ref(&t), &file.0, NOW, |_: &ToolDef| {
            ready(Ok(prose.clone()))
        })
        .await;
        assert_eq!(first.rejected, std::slice::from_ref(&t.name));

        let reply = json!({});
        let tomorrow = once(std::slice::from_ref(&t), &file.0, NOW + DAY, &reply).await;
        assert_eq!(tomorrow.asked, 0, "asked again the next day");
        let next_week = once(std::slice::from_ref(&t), &file.0, NOW + 8 * DAY, &reply).await;
        assert_eq!(next_week.asked, 1, "never asked again");
    }
}
