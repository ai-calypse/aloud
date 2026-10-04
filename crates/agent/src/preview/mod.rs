//! Action previews: what a write is about to do, read from its real arguments.
//!
//! A card is never generated per call. Every value comes out of the call's
//! arguments by JSON pointer and is rendered to text here — money, dates and
//! hosts included — so the frontend draws strings and never formats anything
//! itself. That also keeps the types specta-exportable: no `serde_json::Value`
//! crosses the bridge.
//!
//! [`render`] draws a call: its hand mapping (`hand.json`) when there is one,
//! else the mapping the brain generated for it ([`warm_previews`]), else
//! [`build`], the schema layer every tool gets for free — with ids named from
//! the run's earlier reads ([`Names`]). [`enrich`] then makes the reads a
//! mapping asks for, each bounded in time.

mod fallback;
mod generate;
mod hand;
mod resolve;

pub use fallback::build;
pub use generate::{generate, layer, load_generated, warm_previews, Layer, Pass, PROMPT_VERSION};
pub use hand::{check_hand, hand_tools, render, Context};
pub use resolve::{enrich, Names};

use serde::{Deserialize, Serialize};

/// Which card layout draws the preview.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CardKind {
    Message,
    Email,
    Event,
    Task,
    Note,
    Issue,
    Row,
    File,
    Share,
    Update,
    Delete,
    Money,
    Fallback,
}

/// How hard the card makes it to say yes. Default-Always grants never reach a
/// card, so there is no tier below Confirm.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Tier {
    Confirm,
    /// Red, needs a click, never auto-runs, and "Always allow" cannot cover it.
    Strict,
}

/// What a field is, which decides how the card draws it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    /// Where a message goes: a channel or chat.
    Destination,
    /// Who it goes to: an address or attendee list.
    Person,
    Heading,
    /// A date or time, already formatted. On an event, `items[0]` is the line
    /// under it: time zone and length.
    When,
    /// Body text, shown in full and never truncated.
    Text,
    /// An amount with its currency folded in, in major units.
    Money,
    Path,
    /// `value` is the full host; `items[0]` the whole URL.
    Url,
    /// An opaque identifier nothing has named yet.
    Id,
    /// One of a schema `enum`.
    Pill,
    /// An array or object: `count` entries, the first few in `items`.
    List,
    /// A boolean that differs from its default.
    Flag,
    Plain,
}

/// Whose words a text field holds.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    /// The user said it.
    Yours,
    /// The model wrote it.
    Aloud,
}

/// A less destructive tool the card can offer instead, e.g. Trash over a
/// permanent delete.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Safer {
    pub label: String,
    pub tool: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ActionPreview {
    pub kind: CardKind,
    pub tier: Tier,
    /// The confirm button's label: Send, Create, Delete…
    pub verb: String,
    /// Verb plus resolved target. Never argument prose.
    pub title: String,
    pub fields: Vec<Field>,
    /// Every shown argument past the main fields. Nothing set is dropped.
    pub more: Vec<Field>,
    /// Arguments the call passed, empties and defaults included.
    pub set: u32,
    /// Arguments the tool takes: its schema's properties plus any extras passed.
    pub total: u32,
    pub risks: Vec<String>,
    pub picked_by: Option<String>,
    pub recoverable: Option<String>,
    pub safer: Option<Safer>,
    /// Exact-words countdown, in ms.
    pub auto_ms: Option<u32>,
    pub revision: u32,
    /// Permission scope label, e.g. "Slack · send messages".
    pub scope: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub role: Role,
    /// RFC 6901 pointer into the call's arguments.
    pub path: String,
    pub label: String,
    pub value: String,
    pub items: Vec<String>,
    pub count: u32,
    /// The display name an id resolved to.
    pub name: Option<String>,
    pub unresolved: bool,
    pub before: Option<String>,
    pub origin: Option<Origin>,
    pub editable: bool,
    /// A sheet's rows as the sheet draws them: column letters first, then the
    /// first few rows' cells. Empty for anything that is not rows of cells.
    pub cells: Vec<Vec<String>>,
}
