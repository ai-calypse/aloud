//! The schema layer: a preview for any tool from its JSON schema and the call's
//! arguments alone. Nothing here knows a toolkit; hand mappings refine it.

use super::{ActionPreview, CardKind, Field, Role, Tier};
use serde_json::{Map, Value};

/// Main fields on a card; the rest go under "more".
pub(super) const MAIN_FIELDS: usize = 8;
/// Entries of an array or object shown before "and N more".
pub(super) const SAMPLE: usize = 3;

/// The preview for a call to `name` with `args`, read against the tool's JSON
/// `schema` when there is one. Empty and default arguments are hidden but
/// counted in `set`; every other argument lands in `fields` or `more`.
pub fn build(name: &str, args: &Value, schema: Option<&Value>) -> ActionPreview {
    let empty = Map::new();
    let args = args.as_object().unwrap_or(&empty);
    let props = schema
        .and_then(|s| s.get("properties"))
        .and_then(Value::as_object);
    let prop = |key: &str| props.and_then(|p| p.get(key));
    let required: Vec<&str> = schema
        .and_then(|s| s.get("required"))
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let money = money_pair(args);
    let mut shown: Vec<(bool, Field)> = args
        .iter()
        // The currency is folded into the amount's line, not dropped.
        .filter(|(key, _)| money.is_none_or(|(_, currency, _)| currency != key.as_str()))
        .filter_map(|(key, value)| {
            let required = required.contains(&key.as_str());
            // A required argument is the action itself, so it shows even when it
            // happens to equal its default.
            if is_empty(value) || (!required && is_default(value, prop(key))) {
                return None;
            }
            let currency = money
                .filter(|(amount, _, _)| amount == key)
                .map(|(_, _, code)| code);
            Some((required, field(key, value, prop(key), currency)))
        })
        .collect();
    shown.sort_by_key(|(required, f)| (!required, rank(f.role)));

    let words = words(name);
    let has = |list: &[&str]| words.iter().any(|w| list.contains(&w.as_str()));
    let kind = kind(&has, money.is_some(), &shown);
    // ponytail: money and slug words only. The real tier rules (destructiveHint,
    // public visibility, password words, hand tiers) replace this.
    let tier = if kind == CardKind::Money
        || (kind == CardKind::Delete
            && has(&[
                "permanent",
                "permanently",
                "batch",
                "bulk",
                "all",
                "empty",
                "purge",
            ])) {
        Tier::Strict
    } else {
        Tier::Confirm
    };
    // Skip the toolkit word so a prefix like `ALOUD_` never reads as an action.
    let verb_word = words
        .iter()
        .skip(1)
        .find_map(|w| VERBS.iter().find(|(word, _)| word == w));
    let verb = verb_word
        .map_or_else(|| default_verb(kind), |(_, label)| label)
        .to_string();
    let noun = noun(kind, &words, verb_word.map(|(word, _)| *word));
    let target = shown
        .iter()
        .map(|(_, f)| f)
        .find(|f| matches!(f.role, Role::Destination | Role::Person) && !f.unresolved)
        .map(|f| match f.items.first() {
            Some(first) if f.count > 1 => format!("to {first} and {} more", f.count - 1),
            _ => format!("to {}", f.value),
        });
    let title = [Some(verb.clone()), Some(noun), target]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    let total = props.map_or(0, Map::len) + args.keys().filter(|k| prop(k).is_none()).count();
    let mut fields: Vec<Field> = shown.into_iter().map(|(_, f)| f).collect();
    let more = fields.split_off(MAIN_FIELDS.min(fields.len()));
    ActionPreview {
        kind,
        tier,
        verb,
        title,
        fields,
        more,
        set: count(args.len()),
        total: count(total),
        risks: Vec::new(),
        picked_by: None,
        recoverable: None,
        safer: None,
        auto_ms: None,
        revision: 0,
        scope: None,
    }
}

/// Slug words that name the action, and the button label each gets.
const VERBS: &[(&str, &str)] = &[
    ("send", "Send"),
    ("post", "Send"),
    ("reply", "Send"),
    ("forward", "Send"),
    ("create", "Create"),
    ("insert", "Create"),
    ("add", "Add"),
    ("delete", "Delete"),
    ("remove", "Remove"),
    ("update", "Update"),
    ("edit", "Update"),
    ("patch", "Update"),
    ("append", "Append"),
    ("move", "Move"),
    ("copy", "Copy"),
    ("share", "Share"),
    ("invite", "Invite"),
    ("upload", "Upload"),
    ("write", "Write"),
    ("open", "Open"),
    ("quit", "Quit"),
    ("take", "Take"),
    ("play", "Play"),
];

/// The label when the slug names no action, e.g. `ALOUD_DISCORD_MESSAGE`.
fn default_verb(kind: CardKind) -> &'static str {
    match kind {
        CardKind::Message | CardKind::Email => "Send",
        CardKind::Event | CardKind::Task | CardKind::Note | CardKind::Issue | CardKind::Row => {
            "Create"
        }
        CardKind::Delete => "Delete",
        CardKind::Update => "Update",
        _ => "Run",
    }
}

/// A best guess from the slug and the fields; hand mappings correct it.
fn kind(has: &impl Fn(&[&str]) -> bool, money: bool, shown: &[(bool, Field)]) -> CardKind {
    if has(&["delete", "remove", "trash", "purge"]) {
        CardKind::Delete
    } else if money
        || has(&[
            "pay", "payment", "charge", "refund", "payout", "purchase", "buy",
        ])
    {
        CardKind::Money
    } else if has(&["email", "mail", "gmail", "outlook"]) {
        CardKind::Email
    } else if has(&["message", "messages", "chat", "dm"]) {
        CardKind::Message
    } else if has(&["event", "events", "calendar", "googlecalendar", "meeting"]) {
        CardKind::Event
    } else if has(&["task", "tasks", "reminder", "reminders", "todo"]) {
        CardKind::Task
    } else if has(&["note", "notes", "page", "doc", "document"]) {
        CardKind::Note
    } else if has(&["issue", "issues", "ticket"]) {
        CardKind::Issue
    } else if has(&["row", "rows", "sheet", "spreadsheet", "googlesheets"]) {
        CardKind::Row
    } else if has(&["share", "permission", "permissions", "invite"]) {
        CardKind::Share
    } else if has(&["file", "files", "folder"]) || shown.iter().any(|(_, f)| f.role == Role::Path) {
        CardKind::File
    } else if has(&["update", "edit", "patch"]) {
        CardKind::Update
    } else {
        CardKind::Fallback
    }
}

/// What the action acts on: the kind's own word where it has one, else the
/// slug's remaining words ("Open app", "Collect files").
fn noun(kind: CardKind, words: &[String], verb: Option<&str>) -> String {
    const FILLER: &[&str] = &[
        "a", "an", "the", "new", "to", "for", "in", "of", "on", "from", "by", "with",
    ];
    let fixed = match kind {
        CardKind::Message => "message",
        CardKind::Email => "email",
        CardKind::Event => "event",
        CardKind::Task => "task",
        CardKind::Note => "note",
        CardKind::Issue => "issue",
        CardKind::Row => "row",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_string();
    }
    words
        .iter()
        .skip(1)
        .filter(|w| Some(w.as_str()) != verb && !FILLER.contains(&w.as_str()))
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Card order: where and to whom first, flags last.
fn rank(role: Role) -> u8 {
    match role {
        Role::Destination => 0,
        Role::Person => 1,
        Role::Heading => 2,
        Role::When => 3,
        Role::Text => 4,
        Role::Money => 5,
        Role::Path => 6,
        Role::Url => 7,
        Role::Id => 8,
        Role::Pill => 9,
        Role::List => 10,
        Role::Plain => 11,
        Role::Flag => 12,
    }
}

fn field(key: &str, value: &Value, prop: Option<&Value>, currency: Option<&str>) -> Field {
    let role = role(&words(key), value, prop, currency.is_some());
    let path = format!("/{}", key.replace('~', "~0").replace('/', "~1"));
    shape(role, path, key, value, prop, currency)
}

/// A field drawn as `role`, whoever picked the role: the schema rules above or
/// a hand mapping. `key` is the argument's own name, for its label and id test.
pub(super) fn shape(
    role: Role,
    path: String,
    key: &str,
    value: &Value,
    prop: Option<&Value>,
    currency: Option<&str>,
) -> Field {
    let words = words(key);
    let (mut text, mut items, count) = render(value);
    let mut unresolved = false;
    match role {
        Role::Money => {
            if let Some(money) = currency.and_then(|code| format_money(value, code)) {
                text = money;
            }
        }
        Role::When => {
            if let Some(raw) = value.as_str() {
                text = format_when(raw);
            }
        }
        Role::Url => {
            if let Some(host) = value.as_str().and_then(host_of) {
                items = vec![std::mem::replace(&mut text, host)];
            }
        }
        Role::Id => unresolved = true,
        Role::Destination => {
            unresolved = is_id(&words) || value.as_str().is_some_and(looks_like_id);
        }
        _ => {}
    }
    Field {
        role,
        path,
        label: label(key, prop),
        value: text,
        items,
        count,
        name: None,
        unresolved,
        before: None,
        origin: None,
        editable: false,
    }
}

/// The schema rules, most specific first. Body words are matched on the last
/// word only, so `message_ts` is not read as message text.
fn role(words: &[String], value: &Value, prop: Option<&Value>, money: bool) -> Role {
    let has = |list: &[&str]| words.iter().any(|w| list.contains(&w.as_str()));
    let last = words.last().map_or("", String::as_str);
    let format = prop.and_then(|p| p.get("format")).and_then(Value::as_str);
    if money {
        Role::Money
    } else if value.is_boolean() {
        Role::Flag
    } else if prop.is_some_and(|p| p.get("enum").is_some()) {
        Role::Pill
    } else if has(&["channel", "chat", "conversation"]) {
        Role::Destination
    } else if is_id(words) {
        Role::Id
    } else if [
        "text",
        "body",
        "markdown",
        "description",
        "message",
        "content",
        "html",
    ]
    .contains(&last)
    {
        Role::Text
    } else if has(&[
        "email",
        "emails",
        "to",
        "cc",
        "bcc",
        "attendees",
        "recipient",
        "recipients",
    ]) {
        Role::Person
    } else if has(&["title", "subject", "summary"]) {
        Role::Heading
    } else if !has(&["zone", "timezone", "tz"])
        && (has(&["date", "time", "datetime", "due", "deadline", "timestamp"])
            || last == "at"
            || matches!(format, Some("date" | "date-time" | "time")))
    {
        Role::When
    } else if has(&["path", "paths"]) {
        Role::Path
    } else if has(&["url", "uri", "link", "href", "website"]) {
        Role::Url
    } else if value.is_array() || value.is_object() {
        Role::List
    } else {
        Role::Plain
    }
}

fn is_id(words: &[String]) -> bool {
    words.last().is_some_and(|w| w == "id" || w == "ids")
}

/// ponytail: Slack-shaped ids only (`C0123ABCD45`). Names harvested from
/// earlier reads replace guessing once they exist.
fn looks_like_id(value: &str) -> bool {
    value.len() >= 8
        && value.starts_with(|c: char| c.is_ascii_uppercase())
        && value
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && value.chars().any(|c| c.is_ascii_digit())
}

/// `(amount key, currency key, currency code)` when the call carries both.
fn money_pair(args: &Map<String, Value>) -> Option<(&str, &str, &str)> {
    let keyed = |word: &str| {
        args.iter()
            .find(|(k, v)| !is_empty(v) && words(k).iter().any(|w| w == word))
    };
    let (amount, value) = keyed("amount")?;
    if !(value.is_number() || value.is_string()) {
        return None;
    }
    let (currency, code) = keyed("currency")?;
    Some((amount, currency, code.as_str()?))
}

/// The amount in major units with its code: `1999` + `usd` → `19.99 USD`.
///
/// ponytail: an integer is read as minor units (Stripe's convention, which most
/// payment APIs share); a decimal is taken as already major. A hand mapping is
/// the fix for an API that takes whole units as integers.
fn format_money(amount: &Value, currency: &str) -> Option<String> {
    let code = currency.trim().to_ascii_uppercase();
    let minor = match amount {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    };
    let shown = match (minor, amount) {
        (Some(minor), _) => major_units(minor, minor_digits(&code)),
        (None, Value::Number(n)) => n.to_string(),
        (None, Value::String(s)) => s.trim().to_string(),
        _ => return None,
    };
    Some(format!("{shown} {code}"))
}

/// ISO 4217 minor-unit exponents other than 2.
fn minor_digits(code: &str) -> u32 {
    const ZERO: &[&str] = &[
        "BIF", "CLP", "DJF", "GNF", "ISK", "JPY", "KMF", "KRW", "PYG", "RWF", "UGX", "VND", "VUV",
        "XAF", "XOF", "XPF",
    ];
    const THREE: &[&str] = &["BHD", "IQD", "JOD", "KWD", "LYD", "OMR", "TND"];
    if ZERO.contains(&code) {
        0
    } else if THREE.contains(&code) {
        3
    } else {
        2
    }
}

fn major_units(minor: i64, digits: u32) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let abs = minor.unsigned_abs();
    let scale = 10u64.pow(digits);
    let whole = grouped(abs / scale);
    if digits == 0 {
        format!("{sign}{whole}")
    } else {
        format!(
            "{sign}{whole}.{:0width$}",
            abs % scale,
            width = digits as usize
        )
    }
}

fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A date for people. Anything unparseable is shown as written.
pub(super) fn format_when(raw: &str) -> String {
    use chrono::{DateTime, Local, NaiveDate, NaiveDateTime};
    const DAY: &str = "%a, %b %-d, %Y";
    const DAY_TIME: &str = "%a, %b %-d, %Y, %-I:%M %p";
    let s = raw.trim();
    // A date-only value has no zone. Parsed as a timestamp it is midnight UTC,
    // which reads as the day before anywhere west of Greenwich.
    if let Ok(day) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return day.format(DAY).to_string();
    }
    if let Ok(at) = DateTime::parse_from_rfc3339(s) {
        return at.with_timezone(&Local).format(DAY_TIME).to_string();
    }
    // No offset means the user's own clock: show it as written, not shifted.
    [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
    ]
    .iter()
    .find_map(|pattern| NaiveDateTime::parse_from_str(s, pattern).ok())
    .map_or_else(|| raw.to_string(), |at| at.format(DAY_TIME).to_string())
}

/// The whole host, so `paypal.com.evil.example` cannot hide behind a truncated
/// URL. `Url` gives IDNs in punycode, which is the honest form too.
fn host_of(url: &str) -> Option<String> {
    reqwest::Url::parse(url.trim())
        .ok()?
        .host_str()
        .filter(|host| !host.is_empty())
        .map(str::to_string)
}

/// `(value, first few entries, entry count)`; scalars have no entries.
fn render(value: &Value) -> (String, Vec<String>, u32) {
    match value {
        Value::Array(list) => sampled(list.iter().map(inline), list.len(), ", "),
        Value::Object(map) => sampled(
            map.iter().map(|(k, v)| format!("{k}: {}", inline(v))),
            map.len(),
            "; ",
        ),
        other => (inline(other), Vec::new(), 0),
    }
}

pub(super) fn sampled(
    all: impl Iterator<Item = String>,
    len: usize,
    sep: &str,
) -> (String, Vec<String>, u32) {
    let items: Vec<String> = all.take(SAMPLE).collect();
    let mut text = items.join(sep);
    if len > SAMPLE {
        text = format!("{text} and {} more", len - SAMPLE);
    }
    (text, items, count(len))
}

pub(super) fn inline(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(true) => "Yes".into(),
        Value::Bool(false) => "No".into(),
        Value::Null => String::new(),
        Value::Array(list) => list.iter().map(inline).collect::<Vec<_>>().join(", "),
        other => other.to_string(),
    }
}

pub(super) fn is_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(list) => list.is_empty(),
        Value::Object(map) => map.is_empty(),
        _ => false,
    }
}

fn is_default(value: &Value, prop: Option<&Value>) -> bool {
    match prop.and_then(|p| p.get("default")) {
        Some(default) => default == value,
        // A flag with no stated default is off until set.
        None => *value == Value::Bool(false),
    }
}

/// The schema's title, else the key in words: `markdown_text` → "Markdown text".
fn label(key: &str, prop: Option<&Value>) -> String {
    if let Some(title) = prop
        .and_then(|p| p.get("title"))
        .and_then(Value::as_str)
        .filter(|t| !t.trim().is_empty())
    {
        return title.to_string();
    }
    let text = words(key).join(" ");
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_else(|| key.to_string())
}

/// Lowercase words of a key or slug, split on punctuation and camelCase:
/// `chatId` and `chat_id` both give `["chat", "id"]`.
fn words(key: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut after_lower = false;
    for c in key.chars() {
        if (!c.is_alphanumeric() || (c.is_uppercase() && after_lower)) && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        if c.is_alphanumeric() {
            current.extend(c.to_lowercase());
        }
        after_lower = c.is_lowercase() || c.is_ascii_digit();
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The live Composio schema, slimmed the way the model and the gate get it.
    fn slack() -> Value {
        let raw = serde_json::from_str(include_str!("fixtures/slack_chat_post_message.json"))
            .expect("fixture is JSON");
        crate::composio::slim_schema(&raw)
    }

    fn find<'a>(preview: &'a ActionPreview, path: &str) -> Option<&'a Field> {
        preview
            .fields
            .iter()
            .chain(&preview.more)
            .find(|f| f.path == path)
    }

    #[test]
    fn slack_post_message_shows_channel_and_text_not_just_the_slug() {
        let schema = slack();
        let p = build(
            "SLACK_CHAT_POST_MESSAGE",
            &json!({ "channel": "general", "markdown_text": "Standup moves to 10:30" }),
            Some(&schema),
        );
        assert_eq!(p.kind, CardKind::Message);
        assert_eq!(p.verb, "Send");
        assert_eq!(p.title, "Send message to general");

        let channel = &p.fields[0];
        assert_eq!(channel.role, Role::Destination);
        assert_eq!(channel.path, "/channel");
        assert_eq!(channel.value, "general");
        assert!(!channel.unresolved);
        assert_eq!(
            channel.label, "Channel",
            "the schema title survives slim_schema"
        );

        let text = find(&p, "/markdown_text").expect("the message itself is on the card");
        assert_eq!(text.role, Role::Text);
        assert_eq!(text.value, "Standup moves to 10:30");
        assert!(
            !p.title.contains("Standup"),
            "the title is not argument prose"
        );

        let props = schema["properties"].as_object().expect("properties").len();
        assert_eq!((p.set, p.total), (2, props as u32));
    }

    #[test]
    fn every_set_argument_appears_somewhere() {
        let schema = slack();
        let args = json!({
            "channel": "general",
            "markdown_text": "hi",
            "text": "hi",
            "parse": "full",
            "blocks": [{ "type": "section" }],
            "mrkdwn": true,
            "thread_ts": "1618033790.001500",
            "link_names": true,
            "attachments": "[{\"color\": \"#36a64f\"}]",
            "unfurl_links": true,
            "unfurl_media": true,
            "reply_broadcast": true,
            "icon_emoji": ":wave:",
        });
        let p = build("SLACK_CHAT_POST_MESSAGE", &args, Some(&schema));
        for key in args.as_object().expect("object").keys() {
            assert!(find(&p, &format!("/{key}")).is_some(), "{key} was dropped");
        }
        assert_eq!(
            (p.fields.len(), p.more.len()),
            (MAIN_FIELDS, 13 - MAIN_FIELDS)
        );
        assert_eq!(p.set, 13);

        // No schema at all, an odd key, and a currency folded into its amount.
        let p = build(
            "STRIPE_CREATE_PAYMENT_INTENT",
            &json!({ "amount": 1999, "currency": "usd", "customer": "cus_123",
                     "metadata": { "order": "42" }, "a/b~c": "odd" }),
            None,
        );
        for path in ["/amount", "/customer", "/metadata", "/a~1b~0c"] {
            assert!(find(&p, path).is_some(), "{path} was dropped");
        }
        assert!(find(&p, "/amount").is_some_and(|f| f.value.ends_with("USD")));
        assert_eq!((p.set, p.total), (5, 5));
    }

    #[test]
    fn defaults_and_empties_are_hidden_but_counted() {
        let schema = json!({
            "type": "object",
            "required": ["summary"],
            "properties": {
                "summary": { "type": "string" },
                "calendar_id": { "type": "string", "default": "primary" },
                "send_updates": { "type": "boolean", "default": true },
                "guests_can_modify": { "type": "boolean" },
                "description": { "type": "string" },
                "attendees": { "type": "array", "items": { "type": "string" } },
                "location": { "type": "string" },
                "recurrence": { "type": "array" },
            }
        });
        let p = build(
            "GOOGLECALENDAR_CREATE_EVENT",
            &json!({ "summary": "Lunch with Ana", "calendar_id": "primary", "send_updates": true,
                     "guests_can_modify": false, "description": "  ", "attendees": [],
                     "location": null }),
            Some(&schema),
        );
        let shown: Vec<&str> = p
            .fields
            .iter()
            .chain(&p.more)
            .map(|f| f.path.as_str())
            .collect();
        assert_eq!(shown, ["/summary"]);
        assert_eq!((p.set, p.total), (7, 8));

        let p = build(
            "GOOGLECALENDAR_CREATE_EVENT",
            &json!({ "summary": "Lunch", "calendar_id": "work", "send_updates": false }),
            Some(&schema),
        );
        assert!(find(&p, "/calendar_id").is_some() && find(&p, "/send_updates").is_some());
    }

    #[test]
    fn an_unknown_id_shows_as_unresolved_not_as_a_name() {
        let id = "c_8f3k2j1h0g@group.calendar.google.com";
        let p = build(
            "GOOGLECALENDAR_CREATE_EVENT",
            &json!({ "calendar_id": id, "summary": "Lunch" }),
            None,
        );
        let field = find(&p, "/calendar_id").expect("shown");
        assert_eq!(field.role, Role::Id);
        assert!(field.unresolved);
        assert_eq!((field.name.as_deref(), field.value.as_str()), (None, id));

        let schema = slack();
        let p = build(
            "SLACK_CHAT_POST_MESSAGE",
            &json!({ "channel": "C0123ABCD45", "markdown_text": "hi" }),
            Some(&schema),
        );
        let channel = find(&p, "/channel").expect("shown");
        assert_eq!(channel.role, Role::Destination);
        assert!(channel.unresolved);
        assert_eq!(channel.name, None);
        assert_eq!(p.title, "Send message", "an id is not a name to send to");
    }

    #[test]
    fn minor_units_are_not_shown_as_whole_currency() {
        let money = |amount: Value, currency: &str| {
            let p = build(
                "STRIPE_CREATE_PAYMENT_INTENT",
                &json!({ "amount": amount, "currency": currency }),
                None,
            );
            assert_eq!((p.kind, p.tier), (CardKind::Money, Tier::Strict));
            let field = find(&p, "/amount").expect("shown");
            assert_eq!(field.role, Role::Money);
            field.value.clone()
        };
        assert_eq!(money(json!(1999), "usd"), "19.99 USD");
        assert_eq!(money(json!(123_456_789), "usd"), "1,234,567.89 USD");
        assert_eq!(money(json!(5), "eur"), "0.05 EUR");
        assert_eq!(money(json!(-250), "gbp"), "-2.50 GBP");
        assert_eq!(money(json!(500), "jpy"), "500 JPY");
        assert_eq!(money(json!(1500), "KWD"), "1.500 KWD");
        assert_eq!(money(json!("12.50"), "eur"), "12.50 EUR");
    }

    #[test]
    fn a_date_only_value_is_not_shifted_a_day() {
        let when = |due: &str| {
            let p = build(
                "ALOUD_REMINDERS_ADD",
                &json!({ "text": "Call mom", "due": due }),
                None,
            );
            let field = find(&p, "/due").expect("shown");
            assert_eq!(field.role, Role::When);
            field.value.clone()
        };
        assert_eq!(when("2026-10-03"), "Sat, Oct 3, 2026");
        assert_eq!(when("2026-10-03T17:00"), "Sat, Oct 3, 2026, 5:00 PM");
        assert_eq!(when("next week"), "next week");
    }

    #[test]
    fn a_url_shows_its_full_host() {
        let url = "https://accounts.google.com.sign-in.example/login?next=%2F";
        let p = build(
            "ALOUD_BROWSER_TAB",
            &json!({ "action": "open", "url": url }),
            None,
        );
        let field = find(&p, "/url").expect("shown");
        assert_eq!(field.role, Role::Url);
        assert_eq!(field.value, "accounts.google.com.sign-in.example");
        assert_eq!(field.items, [url]);
    }

    #[test]
    fn every_native_write_previews_its_required_fields() {
        fn sample(prop: &Value) -> Value {
            if let Some(first) = prop.get("enum").and_then(|e| e.get(0)) {
                return first.clone();
            }
            match prop.get("type").and_then(Value::as_str) {
                Some("array") => json!([sample(&prop["items"])]),
                Some("object") => json!({ "Sample": sample(&prop["additionalProperties"]) }),
                Some("boolean") => json!(true),
                Some("integer" | "number") => json!(1),
                _ => json!("sample"),
            }
        }
        let installed: Vec<_> = [
            "Music",
            "Spotify",
            "Reminders",
            "Notes",
            "Photo Booth",
            "Discord",
            "Messages",
        ]
        .into_iter()
        .map(|name| aloud_selection::InstalledApp {
            name: name.to_string(),
            bundle_id: None,
            icon: None,
        })
        .collect();
        let writes: Vec<_> = aloud_native::tools(&installed)
            .into_iter()
            .filter(|t| !t.read_only)
            .collect();
        #[cfg(target_os = "macos")]
        assert!(writes.len() >= 20, "only {} native writes", writes.len());

        for tool in &writes {
            let required: Vec<&str> = tool.parameters["required"]
                .as_array()
                .map(|r| r.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let args: Map<String, Value> = required
                .iter()
                .map(|key| (key.to_string(), sample(&tool.parameters["properties"][key])))
                .collect();
            let p = build(&tool.name, &Value::Object(args), Some(&tool.parameters));
            assert!(!p.title.is_empty(), "{} has no title", tool.name);
            for key in &required {
                let field = p
                    .fields
                    .iter()
                    .find(|f| f.path == format!("/{key}"))
                    .unwrap_or_else(|| panic!("{} hides required {key}", tool.name));
                assert!(
                    !field.value.trim().is_empty(),
                    "{} shows {key} blank",
                    tool.name
                );
            }
        }
    }
}
