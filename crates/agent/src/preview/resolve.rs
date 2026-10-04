//! Names for what a card would otherwise show as raw ids, from two places:
//! what the run's earlier reads already returned ([`Names`]), and the reads a
//! hand mapping names ([`enrich`]), each bounded so a slow service can only
//! leave an id unnamed, never hold the card back.

use super::fallback::{self, SAMPLE};
use super::hand::{self, Mapping, Sample};
use super::{ActionPreview, Field, Role};
use futures_util::future::join_all;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::time::Duration;

/// Ids remembered per run.
// ponytail: first come, first kept — a run that reads past 1,000 things shows
// the later ones as raw ids. Evicting the oldest is the upgrade if one does.
const MAX_IDS: usize = 1_000;

/// How long one card read may take. A calibration knob: the card waits on the
/// slowest read, and past this an id stays shown raw rather than late.
const READ_TIMEOUT: Duration = Duration::from_millis(1_500);

/// Keys a read result names a thing by, best first.
const NAME_KEYS: &[&str] = &["real_name", "name", "title", "summary", "subject", "email"];
/// Keys a thing's own id sits under. Gmail's message lists say `messageId`.
const ID_KEYS: &[&str] = &["id", "messageId"];

/// Every id the run's reads returned: what each was called, and which read
/// found it.
#[derive(Default, Debug)]
pub struct Names {
    seen: HashMap<String, Seen>,
}

#[derive(Debug)]
struct Seen {
    /// Every distinct name a read gave this id; more than one means no name.
    names: Vec<String>,
    /// The read that first returned it, for "how were these picked".
    query: String,
}

impl Names {
    /// Remembers every object in `result` that has an id, with its name when
    /// it has one, as found by `query` — the search or read that returned it,
    /// in words a card can show.
    pub fn harvest(&mut self, result: &Value, query: &str) {
        match result {
            Value::Object(map) => {
                if let Some(id) = ID_KEYS.iter().find_map(|k| id_text(map.get(*k)?)) {
                    let name = NAME_KEYS.iter().find_map(|k| {
                        map.get(*k)?
                            .as_str()
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                    });
                    self.remember(id, name, query);
                }
                for value in map.values() {
                    self.harvest(value, query);
                }
            }
            Value::Array(list) => {
                for value in list {
                    self.harvest(value, query);
                }
            }
            _ => {}
        }
    }

    fn remember(&mut self, id: String, name: Option<&str>, query: &str) {
        if self.seen.len() >= MAX_IDS && !self.seen.contains_key(&id) {
            return;
        }
        let seen = self.seen.entry(id).or_insert_with(|| Seen {
            names: Vec::new(),
            query: query.to_string(),
        });
        if let Some(name) = name.filter(|n| !seen.names.iter().any(|known| known == n)) {
            seen.names.push(name.to_string());
        }
    }

    /// The id's name, only when every read that returned it agreed on one.
    pub fn name_of(&self, id: &str) -> Option<&str> {
        match self.seen.get(id)?.names.as_slice() {
            [only] => Some(only),
            _ => None,
        }
    }

    /// The reads that returned these ids, and how many came from none.
    pub(super) fn picked_by(&self, ids: &[&str]) -> Option<String> {
        let queries: Vec<&str> = ids
            .iter()
            .filter_map(|id| self.seen.get(*id))
            .map(|seen| seen.query.as_str())
            .filter(|q| !q.trim().is_empty())
            .collect();
        if queries.is_empty() {
            return None;
        }
        let mut distinct: Vec<&str> = Vec::new();
        for query in &queries {
            if !distinct.contains(query) {
                distinct.push(query);
            }
        }
        let mut text = distinct.join("; ");
        if queries.len() < ids.len() {
            text.push_str(&format!(
                "; {} not from any search",
                ids.len() - queries.len()
            ));
        }
        Some(text)
    }
}

fn id_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Runs the reads the tool's hand mapping names — lookups for ids still
/// unnamed, the current values an update replaces, a few of a bulk action's
/// items — all at once, each cut off after [`READ_TIMEOUT`], and writes what
/// came back into `preview`. A read that fails or times out changes nothing.
///
/// `read(slug, arguments)` runs one read tool and gives back its result; the
/// app passes a cached Composio read, tests a stub.
pub async fn enrich<F, Fut>(preview: &mut ActionPreview, name: &str, args: &Value, read: F)
where
    F: Fn(&str, Value) -> Fut,
    Fut: Future<Output = Option<Value>>,
{
    let Some(m) = hand::hand(name) else {
        return;
    };
    let lookups: Vec<_> = m
        .lookups
        .iter()
        .filter(|l| field(preview, &l.path).is_some_and(|f| f.unresolved))
        .collect();
    let items: Vec<(&Sample, &str)> = m
        .sample
        .iter()
        .flat_map(|s| {
            let list = args.pointer(&s.path).and_then(Value::as_array);
            list.into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .take(SAMPLE)
                .map(move |id| (s, id))
        })
        .collect();

    let (named, current, sampled) = tokio::join!(
        join_all(
            lookups
                .iter()
                .map(|l| bounded(read(&l.tool, call_args(&l.args, &l.with, args))))
        ),
        async {
            match &m.current {
                Some(c) => bounded(read(&c.tool, call_args(&c.args, &c.with, args))).await,
                None => None,
            }
        },
        join_all(items.iter().map(|(s, id)| {
            let mut with = s.with.clone();
            with.insert(s.arg.clone(), Value::String((*id).to_string()));
            bounded(read(&s.tool, Value::Object(with)))
        })),
    );

    for (lookup, result) in lookups.iter().zip(named) {
        let shown = result.and_then(|r| show(&r, &lookup.show));
        if let (Some(shown), Some(f)) = (shown, field_mut(preview, &lookup.path)) {
            f.name = Some(shown);
            f.unresolved = false;
        }
    }
    if let (Some(c), Some(result)) = (&m.current, current) {
        before(preview, &c.before, &result);
    }
    if let Some(s) = &m.sample {
        // Each item read is shown as what it is; one that failed keeps its id.
        let shown: Vec<String> = items
            .iter()
            .zip(sampled)
            .map(|((_, id), r)| {
                r.and_then(|r| show(&r, &s.show))
                    .unwrap_or_else(|| id.to_string())
            })
            .collect();
        if let Some(field) = field_mut(preview, &s.path) {
            let (value, items, _) =
                fallback::sampled(shown.into_iter(), field.count as usize, ", ");
            (field.value, field.items) = (value, items);
        }
    }
    retitle(preview, m);
}

/// The values an update replaces, beside the fields that replace them.
fn before(preview: &mut ActionPreview, map: &BTreeMap<String, String>, result: &Value) {
    for (path, at) in map {
        let (Some(field), Some(value)) = (field_mut(preview, path), get(result, at)) else {
            continue;
        };
        field.before = Some(match (field.role, value.as_str()) {
            (Role::When, Some(raw)) => fallback::format_when(raw),
            _ => fallback::inline(value),
        });
    }
}

fn retitle(preview: &mut ActionPreview, m: &Mapping) {
    preview.title = hand::title(m, &preview.fields);
}

async fn bounded(read: impl Future<Output = Option<Value>>) -> Option<Value> {
    tokio::time::timeout(READ_TIMEOUT, read)
        .await
        .ok()
        .flatten()
}

/// A read's arguments: each named one taken from the call, the fixed ones as written.
fn call_args(from: &BTreeMap<String, String>, with: &Map<String, Value>, args: &Value) -> Value {
    let mut out = with.clone();
    for (arg, pointer) in from {
        if let Some(value) = args.pointer(pointer).filter(|v| !fallback::is_empty(v)) {
            out.insert(arg.clone(), value.clone());
        }
    }
    Value::Object(out)
}

/// The result's values at `pointers`, joined; `None` when it has none of them.
fn show(result: &Value, pointers: &[String]) -> Option<String> {
    let parts: Vec<String> = pointers
        .iter()
        .filter_map(|p| get(result, p))
        .map(fallback::inline)
        .filter(|s| !s.trim().is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// A value in a read's result, which may still be in Composio's envelope.
fn get<'a>(result: &'a Value, pointer: &str) -> Option<&'a Value> {
    result
        .pointer(pointer)
        .or_else(|| result.get("data")?.pointer(pointer))
        .filter(|v| !fallback::is_empty(v))
}

fn field<'a>(preview: &'a ActionPreview, path: &str) -> Option<&'a Field> {
    preview
        .fields
        .iter()
        .chain(&preview.more)
        .find(|f| f.path == path)
}

fn field_mut<'a>(preview: &'a mut ActionPreview, path: &str) -> Option<&'a mut Field> {
    preview
        .fields
        .iter_mut()
        .chain(preview.more.iter_mut())
        .find(|f| f.path == path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::hand::tests::schema_of;
    use crate::preview::{render, Context};
    use serde_json::json;

    fn slack(names: &Names) -> (Value, ActionPreview) {
        let args = json!({ "channel": "C0123ABCD45", "markdown_text": "hi" });
        let schema = schema_of("SLACK_CHAT_POST_MESSAGE");
        let preview = render(
            "SLACK_CHAT_POST_MESSAGE",
            &args,
            schema.as_ref(),
            &Context { said: "", names },
        );
        (args, preview)
    }

    #[tokio::test(start_paused = true)]
    async fn a_lookup_that_times_out_leaves_the_id_unresolved() {
        let names = Names::default();
        let found = || json!({ "data": { "channel": { "id": "C0123ABCD45", "name": "launch" } } });

        let (args, mut preview) = slack(&names);
        let started = tokio::time::Instant::now();
        enrich(
            &mut preview,
            "SLACK_CHAT_POST_MESSAGE",
            &args,
            |_, _| async move {
                tokio::time::sleep(Duration::from_secs(30)).await;
                Some(found())
            },
        )
        .await;
        assert_eq!(
            started.elapsed(),
            READ_TIMEOUT,
            "the card waited for the read"
        );
        let channel = field(&preview, "/channel").expect("on the card");
        assert!(channel.unresolved);
        assert_eq!(channel.name, None);
        assert_eq!(preview.title, "Send message");

        // The same lookup answered in time names the channel, from the read
        // the mapping names and the id the call carries.
        let (args, mut preview) = slack(&names);
        enrich(
            &mut preview,
            "SLACK_CHAT_POST_MESSAGE",
            &args,
            |slug, read_args| {
                let ok = slug == "SLACK_RETRIEVE_CONVERSATION_INFORMATION"
                    && read_args == json!({ "channel": "C0123ABCD45" });
                async move { ok.then(found) }
            },
        )
        .await;
        let channel = field(&preview, "/channel").expect("on the card");
        assert_eq!(
            (channel.name.as_deref(), channel.unresolved),
            (Some("launch"), false)
        );
        assert_eq!(preview.title, "Send message to launch");
    }

    #[tokio::test(start_paused = true)]
    async fn an_update_shows_what_it_replaces_and_a_delete_what_it_deletes() {
        let names = Names::default();
        let args = json!({ "event_id": "ev1", "summary": "Lunch with Priya",
                           "start_datetime": "2026-10-09T13:00:00" });
        let mut preview = render(
            "GOOGLECALENDAR_UPDATE_EVENT",
            &args,
            schema_of("GOOGLECALENDAR_UPDATE_EVENT").as_ref(),
            &Context {
                said: "",
                names: &names,
            },
        );
        enrich(&mut preview, "GOOGLECALENDAR_UPDATE_EVENT", &args, |_, _| async {
            Some(json!({ "data": { "summary": "Lunch", "start": { "dateTime": "2026-10-09T12:00:00" } } }))
        })
        .await;
        let summary = field(&preview, "/summary").expect("on the card");
        assert_eq!(summary.before.as_deref(), Some("Lunch"));
        let start = field(&preview, "/start_datetime").expect("on the card");
        assert_eq!(start.before.as_deref(), Some("Fri, Oct 9, 2026, 12:00 PM"));
        assert_eq!(
            field(&preview, "/event_id").and_then(|f| f.name.as_deref()),
            Some("Lunch")
        );

        let args = json!({ "messageIds": ["a1", "b2", "c3", "d4"] });
        let mut preview = render(
            "GMAIL_BATCH_DELETE_MESSAGES",
            &args,
            schema_of("GMAIL_BATCH_DELETE_MESSAGES").as_ref(),
            &Context {
                said: "",
                names: &names,
            },
        );
        enrich(&mut preview, "GMAIL_BATCH_DELETE_MESSAGES", &args, |_, read_args| {
            let id = read_args["message_id"].as_str().unwrap_or_default().to_string();
            async move {
                (id != "b2").then(|| json!({ "data": { "sender": "Figma", "subject": format!("Issue {id}") } }))
            }
        })
        .await;
        let items = field(&preview, "/messageIds").expect("on the card");
        assert_eq!(items.items, ["Figma · Issue a1", "b2", "Figma · Issue c3"]);
        assert_eq!(items.count, 4);
        assert_eq!(preview.title, "Delete 4 emails");
    }

    #[test]
    fn harvest_is_capped_and_keeps_the_first_query() {
        let mut names = Names::default();
        let many: Vec<Value> = (0..MAX_IDS + 50)
            .map(|i| json!({ "id": format!("id{i}"), "name": format!("n{i}") }))
            .collect();
        names.harvest(&Value::Array(many), "first");
        assert_eq!(names.seen.len(), MAX_IDS);
        assert_eq!(names.name_of(&format!("id{}", MAX_IDS + 1)), None);
        names.harvest(&json!({ "id": "id0", "name": "n0" }), "second");
        assert_eq!(names.name_of("id0"), Some("n0"));
        assert_eq!(names.picked_by(&["id0"]).as_deref(), Some("first"));
    }
}
