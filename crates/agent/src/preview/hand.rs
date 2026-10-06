//! Hand mappings: how a tool we know well is drawn, written once in
//! `hand.json` instead of guessed from its schema. A mapping only says which
//! argument fills which slot; every value still comes out of the call's own
//! arguments by JSON pointer.

use super::fallback::{self, build, MAIN_FIELDS};
use super::{ActionPreview, CardKind, Field, Names, Origin, Role, Safer, Tier};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

/// One tool's card, as `hand.json` writes it.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct Mapping {
    pub(super) kind: CardKind,
    /// The confirm button's label.
    pub(super) verb: String,
    /// What the verb acts on, in the title. Defaults to the kind's own word.
    #[serde(default)]
    pub(super) noun: Option<String>,
    /// The word before the target in the title: "to", "with", "in", or empty
    /// for a direct object, which takes the noun's place once it has a name
    /// ("Charge Acme Inc", else "Charge card"). Absent, messages and email go
    /// "to" someone, events and shares are "with" someone, and every other
    /// kind names no target.
    #[serde(default)]
    pub(super) to: Option<String>,
    #[serde(default)]
    pub(super) tier: Option<Tier>,
    /// Card order. An argument no slot names still lands under "more".
    pub(super) slots: Vec<Slot>,
    /// Slot paths the user may edit on the card.
    #[serde(default)]
    pub(super) editable: Vec<String>,
    #[serde(default)]
    pub(super) lookups: Vec<Lookup>,
    #[serde(default)]
    pub(super) current: Option<Current>,
    #[serde(default)]
    pub(super) sample: Option<Sample>,
    #[serde(default)]
    pub(super) risks: Vec<Risk>,
    #[serde(default)]
    pub(super) recoverable: Option<String>,
    #[serde(default)]
    pub(super) safer: Option<Safer>,
    /// What the card says when it names nobody to send to: no person or
    /// destination field has a value.
    #[serde(default)]
    pub(super) ask: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct Slot {
    pub(super) role: Role,
    pub(super) path: String,
    #[serde(default)]
    pub(super) label: Option<String>,
    /// What a coded value means, for an API that takes a number where people
    /// say a word: Linear's priority `2` is "High".
    #[serde(default)]
    pub(super) labels: BTreeMap<String, String>,
}

/// A sentence the card adds when the argument at `path` equals `equals`, or,
/// with no `equals`, whenever it is set. An unset argument is compared by its
/// schema default, then as `null`.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct Risk {
    pub(super) path: String,
    #[serde(default, deserialize_with = "present")]
    pub(super) equals: Option<Value>,
    pub(super) text: String,
    /// Raises the card to this tier when the risk applies.
    #[serde(default)]
    pub(super) tier: Option<Tier>,
}

/// `equals: null` is a value to match, not a missing field.
fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}

/// A read that names the id at `path`: `tool` called with `args` (read
/// argument ← pointer into the call's arguments) plus `with` as written, and
/// the result's values at `show` joined into the name.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct Lookup {
    pub(super) path: String,
    pub(super) tool: String,
    #[serde(default)]
    pub(super) args: BTreeMap<String, String>,
    #[serde(default)]
    pub(super) with: Map<String, Value>,
    pub(super) show: Vec<String>,
}

/// A read of the thing an update changes, for its before-values: `before`
/// maps a slot path to where the read's result holds that value now.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct Current {
    pub(super) tool: String,
    #[serde(default)]
    pub(super) args: BTreeMap<String, String>,
    #[serde(default)]
    pub(super) with: Map<String, Value>,
    pub(super) before: BTreeMap<String, String>,
}

/// Reads that say what the first few ids of the list at `path` are: each id
/// goes to `tool` as `arg`.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct Sample {
    pub(super) path: String,
    pub(super) tool: String,
    pub(super) arg: String,
    #[serde(default)]
    pub(super) with: Map<String, Value>,
    pub(super) show: Vec<String>,
}

fn parse() -> serde_json::Result<HashMap<String, Mapping>> {
    serde_json::from_str(include_str!("hand.json"))
}

fn table() -> &'static HashMap<String, Mapping> {
    static HAND: OnceLock<HashMap<String, Mapping>> = OnceLock::new();
    HAND.get_or_init(|| {
        // `hand_mappings_parse_and_point_at_real_fields` keeps this unreachable;
        // if it ever happens, every card still draws from its schema.
        parse().unwrap_or_else(|e| {
            tracing::error!(%e, "hand.json does not parse; previews fall back to schemas");
            HashMap::new()
        })
    })
}

pub(super) fn hand(name: &str) -> Option<&'static Mapping> {
    table().get(name)
}

/// What a card leans on besides the call itself.
pub struct Context<'a> {
    /// The user's words only, as `Command::said` — never the context lines.
    pub said: &'a str,
    /// Ids the run's earlier reads already named.
    pub names: &'a Names,
}

/// The card for a call: its hand mapping when there is one, else its generated
/// mapping, else the schema layer, with ids named from earlier reads and text
/// the user said tagged as theirs. Synchronous and offline; [`super::enrich`]
/// adds what needs a read.
pub fn render(name: &str, args: &Value, schema: Option<&Value>, ctx: &Context) -> ActionPreview {
    // A generated mapping (`generate.rs`) is a `Mapping` too, so nothing below
    // changes; a hand one always wins.
    let generated = hand(name)
        .is_none()
        .then(|| super::generate::generated(name, schema))
        .flatten();
    let mut preview = match hand(name).or(generated.as_deref()) {
        Some(mapping) => mapped(mapping, name, args, schema, ctx.names),
        None => {
            let mut preview = build(name, args, schema);
            let to = fallback::preposition(preview.kind, &preview.verb);
            let had_target = target(&preview.fields, to).is_some();
            name_ids(&mut preview, ctx.names);
            if let Some(target) = target(&preview.fields, to).filter(|_| !had_target) {
                preview.title = format!("{} {target}", preview.title);
            }
            preview
        }
    };
    tag_origins(&mut preview, ctx.said);
    match preview.kind {
        CardKind::Delete => preview.picked_by = picked_by(&preview, args, ctx.names),
        CardKind::Event => under_the_time(&mut preview),
        CardKind::Row => sheet(&mut preview, args),
        _ => {}
    }
    preview
}

/// An event's time zone and length as one line under its start, the way a
/// calendar shows them, rather than two more rows to read past.
fn under_the_time(preview: &mut ActionPreview) {
    let Some(start) = preview.fields.iter().find(|f| f.role == Role::When) else {
        return;
    };
    let start = start.path.clone();
    let mut zone = Vec::new();
    let mut length = Vec::new();
    for list in [&mut preview.fields, &mut preview.more] {
        list.retain(|f| {
            let words = fallback::words(top(&f.path));
            let has = |list: &[&str]| words.iter().any(|w| list.contains(&w.as_str()));
            if f.role != Role::When && has(&["timezone", "zone", "tz"]) {
                if !zone.contains(&f.value) {
                    zone.push(f.value.clone());
                }
            } else if has(&["duration", "length"]) && f.value.parse::<f64>().is_ok() {
                let unit = if has(&["hour", "hours"]) {
                    "hr"
                } else if has(&["minute", "minutes", "min", "mins"]) {
                    "min"
                } else {
                    return true;
                };
                length.push(format!("{} {unit}", f.value));
            } else {
                return true;
            }
            false
        });
    }
    let line = [zone.join(" "), length.join(" ")]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    if let Some(field) = preview.fields.iter_mut().find(|f| f.path == start) {
        field.items = (!line.is_empty()).then_some(line).into_iter().collect();
    }
}

/// Rows of cells drawn as the sheet draws them: column letters, counted from
/// where the range starts, over the first few rows.
// ponytail: letters, not the sheet's own header row — reading row 1 of the
// range is the upgrade, and the design's.
fn sheet(preview: &mut ActionPreview, args: &Value) {
    let start = args
        .get("range")
        .and_then(Value::as_str)
        .map_or(0, first_column);
    let Some((field, rows)) = preview
        .fields
        .iter_mut()
        .chain(preview.more.iter_mut())
        .filter(|f| f.role == Role::List)
        .find_map(|f| {
            let rows = args.pointer(&f.path)?.as_array()?;
            let rows: Option<Vec<&Vec<Value>>> = rows.iter().map(Value::as_array).collect();
            Some(f).zip(rows.filter(|r| !r.is_empty()))
        })
    else {
        return;
    };
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    field.cells = std::iter::once((start..start + width).map(column).collect())
        .chain(rows.iter().take(fallback::SAMPLE).map(|row| {
            (0..width)
                .map(|i| row.get(i).map(fallback::inline).unwrap_or_default())
                .collect()
        }))
        .collect();
}

/// The column a range starts in, 0 for A: `Expenses!C2:F` is 2. A bare sheet
/// name (`Sheet1`) starts at A.
fn first_column(range: &str) -> usize {
    let cells = range.rsplit('!').next().unwrap_or_default();
    let letters = cells.bytes().take_while(u8::is_ascii_uppercase).count();
    let rest = &cells[letters..];
    if !(1..=3).contains(&letters) || rest.starts_with(|c: char| c != ':' && !c.is_ascii_digit()) {
        return 0;
    }
    cells
        .bytes()
        .take(letters)
        .fold(0, |n, b| n * 26 + usize::from(b - b'A') + 1)
        - 1
}

/// A column's letters: 0 is A, 26 is AA.
fn column(index: usize) -> String {
    let mut out = Vec::new();
    let mut n = index + 1;
    while n > 0 {
        n -= 1;
        out.push(b'A' + u8::try_from(n % 26).unwrap_or(0));
        n /= 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

fn mapped(
    m: &Mapping,
    name: &str,
    args: &Value,
    schema: Option<&Value>,
    names: &Names,
) -> ActionPreview {
    // The schema layer still counts the arguments and draws the ones no slot
    // names, so a mapping can never hide an argument.
    let base = build(name, args, schema);
    let mut fields: Vec<Field> = m
        .slots
        .iter()
        .filter_map(|slot| {
            let prop = prop_at(schema, &slot.path);
            let editable = m.editable.contains(&slot.path);
            // An unset flag shows its default: a Meet link nobody asked for is
            // still a Meet link. An editable address shows empty, so the card
            // can take the one thing the model could not find.
            let value = given(args, &slot.path)
                .or_else(|| {
                    prop.and_then(|p| p.get("default"))
                        .filter(|d| d.is_boolean())
                })
                .or_else(|| (editable && is_address(slot.role)).then_some(&EMPTY))?;
            let currency = (slot.role == Role::Money)
                .then(|| sibling(args, &slot.path, "currency"))
                .flatten();
            let key = unescape(slot.path.rsplit('/').next().unwrap_or_default());
            let mut field =
                fallback::shape(slot.role, slot.path.clone(), &key, value, prop, currency);
            if let Some(label) = &slot.label {
                field.label.clone_from(label);
            }
            if let Some(word) = slot.labels.get(&field.value) {
                field.value.clone_from(word);
            }
            field.editable = editable;
            if is_address(slot.role) {
                field.name = given(args, &name_of(&slot.path))
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            Some(field)
        })
        .collect();
    let covered = |path: &str| {
        m.slots
            .iter()
            .any(|s| top(&s.path) == top(path) || name_of(&s.path) == path)
    };
    let mut more = fields.split_off(MAIN_FIELDS.min(fields.len()));
    more.extend(
        base.fields
            .into_iter()
            .chain(base.more)
            .filter(|f| !covered(&f.path)),
    );

    let mut preview = ActionPreview {
        kind: m.kind,
        tier: m.tier.unwrap_or(Tier::Confirm),
        verb: m.verb.clone(),
        title: String::new(),
        fields,
        more,
        set: base.set,
        total: base.total,
        risks: Vec::new(),
        picked_by: None,
        recoverable: m.recoverable.clone(),
        safer: m.safer.clone(),
        auto_ms: None,
        revision: 0,
        scope: None,
    };
    name_ids(&mut preview, names);
    for risk in m.risks.iter().filter(|r| r.applies(args, schema)) {
        if !preview.risks.contains(&risk.text) {
            preview.risks.push(risk.text.clone());
        }
        if risk.tier == Some(Tier::Strict) {
            preview.tier = Tier::Strict;
        }
    }
    let addressed = all(&preview).any(|f| is_address(f.role) && !f.value.is_empty());
    if let Some(ask) = m.ask.as_ref().filter(|_| !addressed) {
        preview.risks.insert(0, ask.clone());
    }
    preview.title = title(m, &preview.fields);
    preview
}

impl Risk {
    fn applies(&self, args: &Value, schema: Option<&Value>) -> bool {
        let set = given(args, &self.path);
        match &self.equals {
            None => set.is_some(),
            Some(want) => {
                let default = || prop_at(schema, &self.path)?.get("default");
                set.or_else(default).unwrap_or(&Value::Null) == want
            }
        }
    }
}

/// Verb, noun, then who it is for: "Send message to launch". Never argument
/// prose: a subject or body is the user's to read on the card, not a label.
pub(super) fn title(m: &Mapping, fields: &[Field]) -> String {
    let noun = m.noun.as_deref().unwrap_or(match m.kind {
        CardKind::Message => "message",
        CardKind::Email => "email",
        CardKind::Event => "event",
        CardKind::Task => "task",
        CardKind::Note => "note",
        CardKind::Issue => "issue",
        CardKind::Row => "row",
        _ => "",
    });
    // A bulk delete leads with how many: "Delete 38 emails".
    let many = fields
        .iter()
        .find(|f| f.role == Role::List)
        .map(|f| f.count)
        .filter(|n| *n > 1);
    if let (CardKind::Delete, Some(n)) = (m.kind, many) {
        let noun = if noun.is_empty() { "item" } else { noun };
        // ponytail: English plurals by "s"; a `nouns` field is the fix for the
        // first noun that needs another ending.
        return format!("{} {n} {noun}s", m.verb);
    }
    let to = m.to.as_deref().or(match m.kind {
        CardKind::Message | CardKind::Email | CardKind::Event | CardKind::Share => {
            Some(fallback::preposition(m.kind, &m.verb))
        }
        _ => None,
    });
    let target = match to {
        // A direct object is the first person, place or record the call
        // names, an id a read named included; one nobody named yet leaves the
        // noun standing rather than falling through to a receipt address.
        Some("") => fields
            .iter()
            .find(|f| matches!(f.role, Role::Id | Role::Person | Role::Destination))
            .filter(|f| !f.unresolved)
            .map(fallback::who),
        Some(to) => target(fields, to),
        None => None,
    };
    let noun = if to == Some("") && target.is_some() {
        ""
    } else {
        noun
    };
    [Some(m.verb.clone()), Some(noun.to_string()), target]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The first person or destination with a name to show, after `to`.
fn target(fields: &[Field], to: &str) -> Option<String> {
    let field = fields
        .iter()
        .find(|f| is_address(f.role) && !f.unresolved && !f.value.is_empty())?;
    let who = fallback::who(field);
    Some(if to.is_empty() {
        who
    } else {
        format!("{to} {who}")
    })
}

/// Ids an earlier read named, shown by that name.
fn name_ids(preview: &mut ActionPreview, names: &Names) {
    for field in preview.fields.iter_mut().chain(preview.more.iter_mut()) {
        if !field.unresolved || !matches!(field.role, Role::Id | Role::Destination) {
            continue;
        }
        if let Some(name) = names.name_of(&field.value) {
            field.name = Some(name.to_string());
            field.unresolved = false;
        }
    }
}

/// Text is the user's when their words contain it whole, ignoring case and
/// punctuation; anything else the model wrote.
fn tag_origins(preview: &mut ActionPreview, said: &str) {
    let said = format!(" {} ", normalise(said));
    for field in preview.fields.iter_mut().chain(preview.more.iter_mut()) {
        if field.role != Role::Text {
            continue;
        }
        let text = normalise(&field.value);
        field.origin = Some(if !text.is_empty() && said.contains(&format!(" {text} ")) {
            Origin::Yours
        } else {
            Origin::Aloud
        });
    }
}

fn normalise(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The searches that returned the ids a delete names.
fn picked_by(preview: &ActionPreview, args: &Value, names: &Names) -> Option<String> {
    let ids: Vec<&str> = all(preview)
        .filter(|f| matches!(f.role, Role::List | Role::Id))
        .filter_map(|f| args.pointer(&f.path))
        .flat_map(|v| match v {
            Value::Array(list) => list.iter().filter_map(Value::as_str).collect(),
            other => other.as_str().into_iter().collect::<Vec<_>>(),
        })
        .collect();
    names.picked_by(&ids)
}

fn all(preview: &ActionPreview) -> impl Iterator<Item = &Field> {
    preview.fields.iter().chain(&preview.more)
}

/// What an editable address with nothing in it shows.
static EMPTY: Value = Value::String(String::new());

fn is_address(role: Role) -> bool {
    matches!(role, Role::Person | Role::Destination)
}

/// Where a call carries the name its address at `pointer` resolved to: `to`
/// beside `to_name`, as `aloud_native::prepare` writes them. A new address
/// clears it ([`super::apply_edits`]).
pub(super) fn name_of(pointer: &str) -> String {
    format!("{pointer}_name")
}

/// The argument at `pointer`, when it holds anything.
fn given<'a>(args: &'a Value, pointer: &str) -> Option<&'a Value> {
    args.pointer(pointer).filter(|v| !fallback::is_empty(v))
}

/// The currency code next to an amount: `/amount` → `/currency`.
fn sibling<'a>(args: &'a Value, pointer: &str, key: &str) -> Option<&'a str> {
    let parent = &pointer[..pointer.rfind('/')?];
    args.pointer(&format!("{parent}/{key}"))?.as_str()
}

/// The schema of the argument at `pointer`: properties by name, `items` by
/// index.
pub(super) fn prop_at<'a>(schema: Option<&'a Value>, pointer: &str) -> Option<&'a Value> {
    let mut at = schema?;
    for segment in pointer.strip_prefix('/')?.split('/') {
        let segment = unescape(segment);
        at = match at.get("properties").and_then(|p| p.get(&segment)) {
            Some(next) => next,
            None if segment.parse::<usize>().is_ok() => at.get("items")?,
            None => return None,
        };
    }
    Some(at)
}

pub(super) fn unescape(segment: &str) -> String {
    segment.replace("~1", "/").replace("~0", "~")
}

/// The top-level argument a pointer is inside.
fn top(pointer: &str) -> &str {
    pointer.split('/').nth(1).unwrap_or_default()
}

/// Every tool `hand.json` maps, then the tools those mappings read or offer
/// instead, each once.
pub fn hand_tools() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    let mut names: Vec<&'static String> = table().keys().collect();
    names.sort();
    for name in names {
        let m = &table()[name];
        let reads = m.lookups.iter().map(|l| &l.tool);
        let rest = [
            m.current.as_ref().map(|c| &c.tool),
            m.sample.as_ref().map(|s| &s.tool),
            m.safer.as_ref().map(|s| &s.tool),
        ];
        for tool in std::iter::once(name)
            .chain(reads)
            .chain(rest.into_iter().flatten())
        {
            if !out.contains(&tool.as_str()) {
                out.push(tool.as_str());
            }
        }
    }
    out
}

/// What is wrong with `hand.json` against the tools' schemas, one line each:
/// a slot naming no real field, a read given an argument it does not take, a
/// tool with no schema at all. Empty means every pointer lands.
pub fn check_hand(schema_of: impl Fn(&str) -> Option<Value>) -> Vec<String> {
    let table = match parse() {
        Ok(table) => table,
        Err(e) => return vec![format!("hand.json: {e}")],
    };
    let mut names: Vec<&String> = table.keys().collect();
    names.sort();
    let mut problems = Vec::new();
    for name in names {
        let m = &table[name];
        let Some(schema) = schema_of(name) else {
            problems.push(format!("{name}: no schema"));
            continue;
        };
        let slot = |path: &str| m.slots.iter().any(|s| s.path == path);
        let mut fields: Vec<(&str, &str)> =
            m.slots.iter().map(|s| ("slot", s.path.as_str())).collect();
        fields.extend(m.risks.iter().map(|r| ("risk", r.path.as_str())));
        fields.extend(
            m.lookups
                .iter()
                .flat_map(|l| l.args.values().map(|p| ("lookup", p.as_str()))),
        );
        fields.extend(
            m.current
                .iter()
                .flat_map(|c| c.args.values().map(|p| ("current", p.as_str()))),
        );
        for (what, path) in fields {
            if prop_at(Some(&schema), path).is_none() {
                problems.push(format!("{name}: {what} {path} is not a field"));
            }
        }
        // These patch a field on the card, so the field has to be one.
        let patched = m
            .editable
            .iter()
            .chain(m.lookups.iter().map(|l| &l.path))
            .chain(m.current.iter().flat_map(|c| c.before.keys()))
            .chain(m.sample.iter().map(|s| &s.path));
        for path in patched.filter(|p| !slot(p)) {
            problems.push(format!("{name}: {path} is not a slot"));
        }
        let reads = m
            .lookups
            .iter()
            .map(|l| {
                (
                    &l.tool,
                    l.args.keys().chain(l.with.keys()).collect::<Vec<_>>(),
                )
            })
            .chain(
                m.current
                    .iter()
                    .map(|c| (&c.tool, c.args.keys().chain(c.with.keys()).collect())),
            )
            .chain(m.sample.iter().map(|s| {
                (
                    &s.tool,
                    std::iter::once(&s.arg).chain(s.with.keys()).collect(),
                )
            }));
        for (tool, keys) in reads {
            let Some(read) = schema_of(tool) else {
                problems.push(format!("{name}: reads {tool}, which has no schema"));
                continue;
            };
            for key in keys {
                if prop_at(Some(&read), &format!("/{key}")).is_none() {
                    problems.push(format!("{name}: {tool} takes no {key}"));
                }
            }
        }
        if let Some(safer) = m.safer.as_ref().filter(|s| schema_of(&s.tool).is_none()) {
            problems.push(format!("{name}: safer {} has no schema", safer.tool));
        }
    }
    problems
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use serde_json::json;

    /// Live Composio input schemas for every connector hand.json names, as
    /// `examples/connectors_probe -- previews` reads them.
    fn connectors() -> &'static Map<String, Value> {
        static FIXTURE: OnceLock<Map<String, Value>> = OnceLock::new();
        FIXTURE.get_or_init(|| {
            serde_json::from_str(include_str!("fixtures/connectors.json")).expect("fixture is JSON")
        })
    }

    /// The schema a tool runs with: its native definition on this machine, or
    /// the connector fixture.
    pub(in crate::preview) fn schema_of(name: &str) -> Option<Value> {
        if let Some(schema) = connectors().get(name) {
            return Some(schema.clone());
        }
        let installed: Vec<_> = [
            "Music",
            "Spotify",
            "Reminders",
            "Notes",
            "Photo Booth",
            "Discord",
            "Messages",
            "Mail",
            "FaceTime",
        ]
        .into_iter()
        .map(|name| aloud_selection::InstalledApp {
            name: name.to_string(),
            bundle_id: None,
            icon: None,
        })
        .collect();
        aloud_native::tools(&installed)
            .into_iter()
            .map(|t| (t.name, t.parameters))
            .chain(
                aloud_screen::tools()
                    .into_iter()
                    .map(|t| (t.name.to_string(), t.parameters)),
            )
            .find(|(tool, _)| tool == name)
            .map(|(_, schema)| schema)
    }

    fn card(name: &str, args: Value, said: &str, names: &Names) -> ActionPreview {
        render(
            name,
            &args,
            schema_of(name).as_ref(),
            &Context { said, names },
        )
    }

    fn find<'a>(preview: &'a ActionPreview, path: &str) -> &'a Field {
        all(preview)
            .find(|f| f.path == path)
            .unwrap_or_else(|| panic!("{path} is not on the card"))
    }

    #[test]
    fn hand_mappings_parse_and_point_at_real_fields() {
        // Native tools exist only on macOS, and screen tools only with Cua
        // Driver installed; anything else must have a schema.
        let absent = |name: &str| {
            name.starts_with("ALOUD_")
                && (!cfg!(target_os = "macos")
                    || (name.starts_with("ALOUD_SCREEN_") && aloud_screen::tools().is_empty()))
        };
        let problems: Vec<String> = check_hand(schema_of)
            .into_iter()
            .filter(|p| !p.strip_suffix(": no schema").is_some_and(absent))
            .collect();
        assert!(problems.is_empty(), "{problems:#?}");
        assert!(table().len() >= 40, "only {} mappings", table().len());
    }

    #[test]
    fn a_channel_id_from_an_earlier_list_shows_its_name() {
        let mut names = Names::default();
        names.harvest(
            &json!({ "data": { "channels": [
                { "id": "C0123ABCD45", "name": "launch" },
                { "id": "C0999ZZZZ99", "name": "random" },
            ] } }),
            "SLACK_FIND_CHANNELS launch",
        );
        let args = json!({ "channel": "C0123ABCD45", "markdown_text": "hi" });
        let p = card("SLACK_CHAT_POST_MESSAGE", args, "send hi to launch", &names);
        let channel = find(&p, "/channel");
        assert_eq!(
            (channel.name.as_deref(), channel.unresolved),
            (Some("launch"), false)
        );
        assert_eq!(p.title, "Send message to launch");

        // Two reads naming one id two ways: no guess.
        names.harvest(&json!([{ "id": "C0123ABCD45", "name": "launch-old" }]), "x");
        let args = json!({ "channel": "C0123ABCD45", "markdown_text": "hi" });
        let p = card("SLACK_CHAT_POST_MESSAGE", args, "", &names);
        assert!(find(&p, "/channel").unresolved);
        assert_eq!(p.title, "Send message");

        // The schema layer names ids the same way.
        let p = card(
            "SLACK_SCHEDULE_MESSAGE",
            json!({ "channel": "C0999ZZZZ99", "text": "hi" }),
            "",
            &names,
        );
        assert_eq!(p.title, "Send message to random");
    }

    #[test]
    fn the_send_card_shows_the_whole_address_domain_included() {
        let names = Names::default();
        let to = "priya@acme.co.evil.example";
        let p = card(
            "GMAIL_SEND_EMAIL",
            json!({ "recipient_email": to, "cc": ["dana@acme.co", "ravi@lookalike-acme.co"],
                    "subject": "Q4", "body": "Thursday works." }),
            "",
            &names,
        );
        assert_eq!(p.kind, CardKind::Email);
        let field = find(&p, "/recipient_email");
        assert_eq!((field.role, field.value.as_str()), (Role::Person, to));
        assert_eq!(p.title, format!("Send email to {to}"));
        assert_eq!(
            find(&p, "/cc").items,
            ["dana@acme.co", "ravi@lookalike-acme.co"]
        );
    }

    #[test]
    fn a_send_with_no_recipient_says_so() {
        let names = Names::default();
        let p = card(
            "GMAIL_SEND_EMAIL",
            json!({ "subject": "Q4", "body": "Thursday works." }),
            "",
            &names,
        );
        assert_eq!(
            p.risks.first().map(String::as_str),
            Some("No recipient — cancel and ask again.")
        );
        assert_eq!(p.title, "Send email");

        let p = card("ALOUD_MESSAGES_SEND", json!({ "text": "hi" }), "", &names);
        assert!(p.risks[0].starts_with("No recipient"), "{:?}", p.risks);

        // Cc alone is a recipient, and so is Gmail's undeclared `to` alias.
        for args in [
            json!({ "cc": ["dana@acme.co"], "body": "hi" }),
            json!({ "to": "dana@acme.co", "body": "hi" }),
        ] {
            let p = card("GMAIL_SEND_EMAIL", args, "", &names);
            assert!(
                !p.risks.iter().any(|r| r.starts_with("No recipient")),
                "{:?}",
                p.risks
            );
        }
    }

    #[test]
    fn text_the_user_said_is_tagged_as_theirs() {
        let names = Names::default();
        let said = "text mom: running late, be there at 6!";
        let p = card(
            "ALOUD_MESSAGES_SEND",
            json!({ "to": "+15551234567", "text": "Running late, be there at 6" }),
            said,
            &names,
        );
        assert_eq!(find(&p, "/text").origin, Some(Origin::Yours));
        assert!(find(&p, "/text").editable);

        // Words the model added, and a match inside a longer word, are not theirs.
        for (text, said) in [
            ("Running late, I'll be there at 6", said),
            ("late", "chocolate cake"),
        ] {
            let p = card(
                "ALOUD_MESSAGES_SEND",
                json!({ "to": "+1555", "text": text }),
                said,
                &names,
            );
            assert_eq!(find(&p, "/text").origin, Some(Origin::Aloud), "{text}");
        }
        // Only text is tagged: a recipient has no author.
        assert_eq!(find(&p, "/to").origin, None);
    }

    #[test]
    fn the_title_never_contains_argument_prose() {
        let names = Names::default();
        const PROSE: &str = "Zebra crossing at noon";
        for (name, m) in table() {
            let Some(schema) = schema_of(name) else {
                continue;
            };
            let mut args = Map::new();
            for slot in &m.slots {
                let addressed = matches!(slot.role, Role::Person | Role::Destination);
                let text = if addressed { "dest@example.com" } else { PROSE };
                let ty = prop_at(Some(&schema), &slot.path).and_then(|p| p.get("type"));
                let value = match ty.and_then(Value::as_str) {
                    Some("array") => json!([text]),
                    Some("integer" | "number") => json!(1),
                    Some("boolean") => json!(true),
                    Some("object") => json!({ "note": text }),
                    _ => json!(text),
                };
                args.insert(top(&slot.path).to_string(), value);
            }
            if args.contains_key("amount") {
                args.insert("currency".into(), json!("usd"));
            }
            let p = render(
                name,
                &Value::Object(args),
                Some(&schema),
                &Context {
                    said: "",
                    names: &names,
                },
            );
            assert!(!p.title.is_empty(), "{name} has no title");
            assert!(!p.title.contains("Zebra"), "{name}: {}", p.title);
        }
    }

    #[test]
    fn a_bulk_delete_says_how_its_items_were_picked() {
        let mut names = Names::default();
        let query = "category:promotions older_than:30d";
        names.harvest(
            &json!({ "messages": [
                { "messageId": "18c2a", "sender": "Morning Brew", "subject": "Tuesday's edition" },
                { "messageId": "18c1b", "sender": "Figma", "subject": "Config replays" },
                { "messageId": "18c0c", "sender": "Substack", "subject": "Weekly digest" },
            ] }),
            query,
        );
        let ids = json!({ "messageIds": ["18c2a", "18c1b", "18c0c"] });
        let p = card("GMAIL_BATCH_DELETE_MESSAGES", ids, "", &names);
        assert_eq!(p.picked_by.as_deref(), Some(query));
        assert_eq!((p.kind, p.tier), (CardKind::Delete, Tier::Strict));
        assert_eq!(p.title, "Delete 3 emails");
        assert_eq!(
            p.safer.as_ref().map(|s| s.tool.as_str()),
            Some("GMAIL_MOVE_TO_TRASH")
        );
        assert!(p.risks.iter().any(|r| r.contains("skip the Trash")));

        // One the model added on its own is called out, not folded in.
        let ids = json!({ "messageIds": ["18c2a", "18c1b", "ffff9"] });
        let p = card("GMAIL_BATCH_DELETE_MESSAGES", ids, "", &names);
        assert_eq!(
            p.picked_by.as_deref(),
            Some("category:promotions older_than:30d; 1 not from any search")
        );

        // Trash is recoverable and only asks.
        let p = card(
            "GMAIL_MOVE_TO_TRASH",
            json!({ "message_id": "18c2a" }),
            "",
            &names,
        );
        assert_eq!(p.tier, Tier::Confirm);
        assert_eq!(p.recoverable.as_deref(), Some("In Trash for 30 days."));
        assert_eq!(
            find(&p, "/message_id").name.as_deref(),
            Some("Tuesday's edition")
        );
    }

    #[test]
    fn a_charge_is_titled_by_who_pays_not_the_api_call() {
        let mut names = Names::default();
        let args = || {
            json!({ "amount": 4200, "currency": "usd", "customer": "cus_Q3f2xY",
                    "confirm": true, "receipt_email": "billing@acme.co" })
        };
        let p = card("STRIPE_CREATE_PAYMENT_INTENT", args(), "", &names);
        assert_eq!(
            (p.verb.as_str(), p.title.as_str()),
            ("Charge", "Charge card"),
            "an unnamed customer is not swapped for the receipt address"
        );
        names.harvest(&json!([{ "id": "cus_Q3f2xY", "name": "Acme Inc" }]), "x");
        let p = card("STRIPE_CREATE_PAYMENT_INTENT", args(), "", &names);
        assert_eq!(p.title, "Charge Acme Inc");
        assert_eq!(find(&p, "/amount").value, "42.00 USD");
    }

    #[test]
    fn an_issue_priority_shows_its_word_not_its_number() {
        let names = Names::default();
        for (priority, word) in [(0, "No priority"), (1, "Urgent"), (2, "High"), (4, "Low")] {
            let p = card(
                "LINEAR_CREATE_LINEAR_ISSUE",
                json!({ "title": "Login loops", "team_id": "t1", "priority": priority }),
                "",
                &names,
            );
            assert_eq!(find(&p, "/priority").value, word);
        }
    }

    #[test]
    fn an_event_shows_its_zone_and_length_on_one_line_under_the_time() {
        let names = Names::default();
        let p = card(
            "GOOGLECALENDAR_CREATE_EVENT",
            json!({ "summary": "Lunch", "start_datetime": "2026-10-09T13:00:00",
                    "event_duration_hour": 1, "event_duration_minutes": 30,
                    "timezone": "Asia/Kolkata" }),
            "",
            &names,
        );
        assert_eq!(
            find(&p, "/start_datetime").items,
            ["Asia/Kolkata · 1 hr 30 min"]
        );
        for gone in [
            "/timezone",
            "/event_duration_hour",
            "/event_duration_minutes",
        ] {
            assert!(all(&p).all(|f| f.path != gone), "{gone} is still a row");
        }
    }

    #[test]
    fn a_sheet_append_shows_its_cells_under_column_letters() {
        let names = Names::default();
        let p = card(
            "GOOGLESHEETS_SPREADSHEETS_VALUES_APPEND",
            json!({ "spreadsheetId": "1Bx", "range": "Expenses!C:F",
                    "values": [["3 Oct", "Lunch", 42, "Food"], ["4 Oct", "Taxi"]] }),
            "",
            &names,
        );
        assert_eq!(
            find(&p, "/values").cells,
            [
                vec!["C", "D", "E", "F"],
                vec!["3 Oct", "Lunch", "42", "Food"],
                vec!["4 Oct", "Taxi", "", ""],
            ]
        );
        assert_eq!(
            [
                first_column("Sheet1"),
                first_column("A1:D4"),
                first_column("'Q4'!AA2")
            ],
            [0, 0, 26]
        );
        assert_eq!([column(0), column(25), column(26)], ["A", "Z", "AA"]);
    }

    #[test]
    fn an_unset_meet_link_still_shows_because_it_defaults_on() {
        let names = Names::default();
        let p = card(
            "GOOGLECALENDAR_CREATE_EVENT",
            json!({ "summary": "Lunch", "start_datetime": "2026-10-09T13:00:00",
                    "attendees": ["priya@acme.co"] }),
            "",
            &names,
        );
        assert_eq!(find(&p, "/create_meeting_room").value, "Yes");
        assert!(p.risks.contains(&"Adds a Google Meet link.".to_string()));
        assert_eq!(p.title, "Create event with priya@acme.co");
        assert_eq!(p.tier, Tier::Confirm);

        let p = card(
            "GOOGLECALENDAR_CREATE_EVENT",
            json!({ "start_datetime": "2026-10-09T13:00:00", "create_meeting_room": false,
                    "visibility": "public" }),
            "",
            &names,
        );
        assert_eq!(find(&p, "/create_meeting_room").value, "No");
        assert_eq!(p.tier, Tier::Strict, "a public event is public posting");
    }
}
