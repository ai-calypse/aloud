//! Reading a whole transcript into an ordered thread.
//!
//! [`scan`](crate::scan) reads only the *tail* — what the agent is doing right
//! now — and deliberately drops assistant text, tool results, and token counts
//! so the poll can stay cheap and hold no transcript in memory. This module is
//! the other half: the full, ordered read for when the user opens a session and
//! wants to see what was said. It is meant to be called **on demand**, never on
//! the poll path — a transcript reaches hundreds of megabytes, and holding one
//! per session is exactly what the `Tail` offset machinery exists to avoid.
//!
//! Record shapes match [`scan`]; this reader keeps the parts that one throws
//! away.

use std::path::Path;

use serde::Deserialize;

/// One rendered entry in a transcript, in file order. Flat on purpose — the UI
/// wants a scrollable list, not a tree.
///
/// Plain here; the webview wire type (`specta`) lives in `src-tauri`, the same
/// split as [`Scan`](crate::Scan) → `AgentSession`.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub kind: Kind,
    /// The tool name, for `ToolUse`. `None` for messages.
    pub tool: Option<String>,
    /// For a message: the text. For a `ToolUse`: the one-line prose describing
    /// the call ("Reading /path", "Running: git commit").
    pub text: String,
    /// A tool call's result, folded onto the same entry once its `tool_result`
    /// arrives — so a call and its outcome are one row, not two. `None` while
    /// still running or for a message.
    pub result: Option<String>,
    /// The tool result came back an error. Meaningful only on a `ToolUse`.
    pub is_error: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    User,
    Assistant,
    ToolUse,
}

/// A parsed transcript: ordered entries plus the token totals that live only in
/// the `.jsonl` (the hook stream never carries them).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Transcript {
    pub entries: Vec<Entry>,
    // u32, not u64: specta forbids exporting 64-bit ints to the webview, and a
    // session's token counts fit u32 (~4.3B) with room to spare. A saturating
    // add keeps a pathological transcript from wrapping.
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    /// Estimated cost in USD, summed per assistant turn at its own model's
    /// price. `0.0` when no priced model was seen (an unknown model contributes
    /// nothing rather than a wrong number).
    pub cost_usd: f64,
}

/// Keep only the tail this many entries — assistant messages, tool calls and
/// prompts all count toward it, since the stream interleaves them. A long
/// session is thousands of turns and the recent ones are what a reader wants;
/// the head is dropped, not the tail. 20 keeps the feed a recent-activity
/// window, not an archive.
const MAX_ENTRIES: usize = 20;

/// Clip a single field to something a narrow panel can show.
const CLIP: usize = 400;

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Record {
    #[serde(rename = "assistant")]
    Assistant(Turn),
    #[serde(rename = "user")]
    User(Turn),
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct Turn {
    #[serde(default)]
    message: Message,
}

#[derive(Deserialize, Default)]
struct Message {
    #[serde(default)]
    content: Content,
    #[serde(default)]
    usage: Option<Usage>,
    /// The model this turn ran on — needed to price it. A session can switch
    /// models mid-way (a `/model` change), so cost is summed per assistant line.
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize, Default)]
struct Usage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
    #[serde(default)]
    cache_read_input_tokens: u32,
    /// Cache *writes* — the priciest input class, so cost needs it even though
    /// the ↑⟳↓ header doesn't.
    #[serde(default)]
    cache_creation_input_tokens: u32,
}

#[derive(Deserialize, Default)]
#[serde(untagged)]
enum Content {
    Blocks(Vec<Block>),
    /// A plain user prompt typed as a bare string.
    Text(String),
    #[default]
    None,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Block {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        #[serde(default)]
        id: String,
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        #[serde(default)]
        tool_use_id: String,
        #[serde(default)]
        content: serde_json::Value,
        #[serde(default)]
        is_error: bool,
    },
    #[serde(other)]
    Other,
}

/// Read a session's `.jsonl` into an ordered transcript. Malformed lines are
/// skipped, the same way [`scan`](crate::scan) tolerates a half-written tail.
pub fn read(path: &Path) -> anyhow::Result<Transcript> {
    let text = std::fs::read_to_string(path)?;
    Ok(read_lines(&text))
}

/// The parse over the text, so every rule is testable without a file.
pub fn read_lines(text: &str) -> Transcript {
    let mut out = Transcript::default();
    // tool_use_id → the entry index of its call, so a later `tool_result` folds
    // its outcome onto the same row instead of adding a second one.
    let mut calls: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Record>(line) else {
            continue;
        };
        let (turn, role) = match record {
            Record::Assistant(turn) => (turn, Kind::Assistant),
            Record::User(turn) => (turn, Kind::User),
            Record::Other => continue,
        };

        if let Some(usage) = &turn.message.usage {
            out.input_tokens = out.input_tokens.saturating_add(usage.input_tokens);
            out.output_tokens = out.output_tokens.saturating_add(usage.output_tokens);
            out.cache_read_tokens = out
                .cache_read_tokens
                .saturating_add(usage.cache_read_input_tokens);
            out.cost_usd += cost::turn_usd(turn.message.model.as_deref(), usage);
        }

        match turn.message.content {
            Content::Text(text) => {
                // Message text is NOT clipped — the last assistant turn is the
                // summary, the most important thing to read in full. The UI
                // collapses it to a few lines and expands on tap.
                if let Some(text) = trim_nonempty(&text) {
                    out.entries.push(Entry {
                        kind: role,
                        tool: None,
                        text,
                        result: None,
                        is_error: false,
                    });
                }
            }
            Content::Blocks(blocks) => {
                for block in blocks {
                    match block {
                        Block::Text { text } => {
                            if let Some(text) = trim_nonempty(&text) {
                                out.entries.push(Entry {
                                    kind: role,
                                    tool: None,
                                    text,
                                    result: None,
                                    is_error: false,
                                });
                            }
                        }
                        Block::ToolUse { id, name, input } => {
                            calls.insert(id, out.entries.len());
                            out.entries.push(Entry {
                                kind: Kind::ToolUse,
                                text: describe(&name, &input),
                                tool: Some(name),
                                result: None,
                                is_error: false,
                            });
                        }
                        Block::ToolResult {
                            tool_use_id,
                            content,
                            is_error,
                        } => {
                            // Fold onto the call. An orphan result (call scrolled
                            // out of the window, or malformed) is dropped rather
                            // than shown alone — the row it belongs to is gone.
                            if let Some(&i) = calls.get(&tool_use_id) {
                                out.entries[i].is_error = is_error;
                                // A successful Read/Edit/… result echoes the call
                                // ("File updated") — drop it. Errors always show.
                                let redundant = !is_error
                                    && out.entries[i]
                                        .tool
                                        .as_deref()
                                        .is_some_and(result_is_redundant_on_success);
                                out.entries[i].result = if redundant {
                                    None
                                } else {
                                    result_text(&content)
                                };
                            }
                        }
                        Block::Other => {}
                    }
                }
            }
            Content::None => {}
        }
    }

    // Trim to the tail. Drop the pairing map's now-stale indices with it —
    // nothing reads `calls` after the loop, so just truncate the entries.
    if out.entries.len() > MAX_ENTRIES {
        out.entries.drain(..out.entries.len() - MAX_ENTRIES);
    }
    out
}

/// One line of prose for a tool call — "Reading /path", "Running: git status",
/// "Searching: TODO" — instead of a bare `Tool: arg`. Reuses
/// [`scan::headline`](crate::scan::headline) for the argument worth showing.
fn describe(name: &str, input: &serde_json::Value) -> String {
    let arg = crate::scan::headline(name, input);
    match (name, arg.as_deref()) {
        // Paths: just the file name. The full absolute path is noise in a narrow
        // row — "Editing transcript.rs" reads; "Editing /Users/…/src/x.rs" is a
        // wall of directory.
        ("Read", Some(p)) => format!("Reading {}", base(p)),
        ("Write", Some(p)) => format!("Writing {}", base(p)),
        ("Edit" | "NotebookEdit", Some(p)) => format!("Editing {}", base(p)),
        ("Bash" | "BashOutput", Some(c)) => format!("Running: {c}"),
        ("Grep", Some(p)) => format!("Searching: {p}"),
        ("Glob", Some(p)) => format!("Finding: {p}"),
        ("WebFetch", Some(u)) => format!("Fetching {u}"),
        ("WebSearch", Some(q)) => format!("Searching the web: {q}"),
        ("Task" | "Agent", Some(d)) => d.to_string(),
        ("Skill", Some(s)) => format!("Skill: {s}"),
        // A tool we have no phrasing for, or one whose input had no headline:
        // fall back to the name, optionally with its one argument.
        (_, Some(a)) => format!("{name}: {a}"),
        (_, None) => name.to_string(),
    }
}

/// The file name from a path — the signal a row wants, not the directory.
fn base(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Tools whose success result says nothing the call didn't ("File updated") —
/// their result line is dropped unless it's an error worth seeing.
fn result_is_redundant_on_success(tool: &str) -> bool {
    matches!(
        tool,
        "Read" | "Write" | "Edit" | "NotebookEdit" | "TodoWrite"
    )
}

/// A `tool_result`'s content is a string, or a list of blocks (text + images).
/// Keep the text; note a non-text block by type rather than dumping base64.
fn result_text(content: &serde_json::Value) -> Option<String> {
    match content {
        serde_json::Value::String(s) => clip(s),
        serde_json::Value::Array(blocks) => {
            let parts: Vec<String> = blocks
                .iter()
                .filter_map(|block| {
                    let ty = block.get("type").and_then(|t| t.as_str())?;
                    if ty == "text" {
                        block
                            .get("text")
                            .and_then(|t| t.as_str())
                            .map(str::to_string)
                    } else {
                        Some(format!("[{ty}]"))
                    }
                })
                .collect();
            clip(&parts.join(" "))
        }
        _ => None,
    }
}

/// Trim and drop empties, keeping the full text — for messages, which the UI
/// collapses rather than the parser truncating.
fn trim_nonempty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Trim, drop empties, and cap at [`CLIP`] with an ellipsis. For tool args and
/// results, where one line is all a row wants.
fn clip(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out: String = trimmed.chars().take(CLIP - 1).collect();
    if trimmed.chars().count() > CLIP - 1 {
        out.push('…');
    }
    Some(out)
}

/// Pricing a turn from its token usage.
///
/// A local table × the tokens we already parse — no API. Rates are per-token
/// USD (Anthropic's public list prices / 1e6), ported from notchi's bundled
/// fallback. An unknown model prices to nothing rather than guessing wrong.
///
/// ponytail: flat rates, no >200k-context tier and no live models.dev refresh —
/// the common case is one model under 200k. Upgrade path is a tier lookup and a
/// cached remote table if the numbers drift.
mod cost {
    use super::Usage;

    struct Price {
        input: f64,
        output: f64,
        cache_write: f64,
        cache_read: f64,
    }

    /// Public list prices, per token. Keys match on a prefix so dated ids
    /// (`claude-opus-4-8-20260101`) and vendor-prefixed ids both hit.
    fn price(model: &str) -> Option<Price> {
        // Order matters: longer/more-specific prefixes first.
        let table: &[(&str, Price)] = &[
            (
                "opus",
                Price {
                    input: 5e-6,
                    output: 2.5e-5,
                    cache_write: 6.25e-6,
                    cache_read: 5e-7,
                },
            ),
            (
                "sonnet",
                Price {
                    input: 3e-6,
                    output: 1.5e-5,
                    cache_write: 3.75e-6,
                    cache_read: 3e-7,
                },
            ),
            (
                "haiku",
                Price {
                    input: 8e-7,
                    output: 4e-6,
                    cache_write: 1e-6,
                    cache_read: 8e-8,
                },
            ),
        ];
        let model = model.to_ascii_lowercase();
        table
            .iter()
            .find(|(key, _)| model.contains(key))
            .map(|(_, p)| Price {
                input: p.input,
                output: p.output,
                cache_write: p.cache_write,
                cache_read: p.cache_read,
            })
    }

    /// The USD cost of one assistant turn. `0.0` for an unknown/absent model.
    pub fn turn_usd(model: Option<&str>, usage: &Usage) -> f64 {
        let Some(p) = model.and_then(price) else {
            return 0.0;
        };
        f64::from(usage.input_tokens) * p.input
            + f64::from(usage.output_tokens) * p.output
            + f64::from(usage.cache_creation_input_tokens) * p.cache_write
            + f64::from(usage.cache_read_input_tokens) * p.cache_read
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_result_onto_its_call_and_aggregates_tokens() {
        // user prompt → assistant text + tool_use (with usage) → tool_result.
        // The result folds onto the call: three entries, not four.
        let text = r#"
{"type":"user","message":{"role":"user","content":[{"type":"text","text":"list the files"}]}}
{"type":"assistant","message":{"role":"assistant","usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":100},"content":[{"type":"text","text":"On it."},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls -la"}}]}}
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"a.txt\nb.txt"}]}}
"#;
        let t = read_lines(text);
        let kinds: Vec<Kind> = t.entries.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, vec![Kind::User, Kind::Assistant, Kind::ToolUse]);
        assert_eq!(t.entries[0].text, "list the files");
        // The tool row carries prose, its tool name, AND its folded result.
        assert_eq!(t.entries[2].tool.as_deref(), Some("Bash"));
        assert_eq!(t.entries[2].text, "Running: ls -la");
        assert!(t.entries[2].result.as_deref().unwrap().contains("a.txt"));
        assert!(!t.entries[2].is_error);
        assert_eq!(
            (t.input_tokens, t.output_tokens, t.cache_read_tokens),
            (10, 5, 100)
        );
    }

    #[test]
    fn prices_usage_per_model_and_ignores_unknown() {
        // opus: 1000·5e-6 + 500·2.5e-5 + 200·6.25e-6 + 100·5e-7
        //     = 0.005 + 0.0125 + 0.00125 + 0.00005 = 0.0188
        let text = r#"{"type":"assistant","message":{"model":"claude-opus-4-8-20260101","usage":{"input_tokens":1000,"output_tokens":500,"cache_creation_input_tokens":200,"cache_read_input_tokens":100},"content":[{"type":"text","text":"hi"}]}}"#;
        let t = read_lines(text);
        assert!(
            (t.cost_usd - 0.0188).abs() < 1e-9,
            "cost was {}",
            t.cost_usd
        );

        // A model with no price row contributes nothing rather than a wrong sum.
        let unknown = r#"{"type":"assistant","message":{"model":"gpt-5.6","usage":{"input_tokens":1000,"output_tokens":500},"content":[{"type":"text","text":"hi"}]}}"#;
        assert_eq!(read_lines(unknown).cost_usd, 0.0);
    }

    #[test]
    fn folds_error_result_onto_its_call() {
        let text = r#"
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"a.rs"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":"boom"}]}}
"#;
        let t = read_lines(text);
        assert_eq!(t.entries.len(), 1);
        assert_eq!(t.entries[0].kind, Kind::ToolUse);
        assert_eq!(t.entries[0].text, "Editing a.rs");
        assert!(t.entries[0].is_error);
        assert_eq!(t.entries[0].result.as_deref(), Some("boom"));
    }

    #[test]
    fn orphan_result_and_garbage_lines_are_dropped() {
        // A result whose call is not present has no row to fold onto — dropped,
        // not shown alone. Garbage lines are skipped.
        let text = "not json\n{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"x\",\"content\":\"orphan\"}]}}";
        let t = read_lines(text);
        assert!(t.entries.is_empty());
    }

    #[test]
    fn image_result_notes_type_not_base64() {
        // A non-suppressed tool (Bash), so the result survives and we can check
        // the image block is noted by type, never dumped as base64.
        let text = r#"
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"screencapture"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"image","source":{"data":"AAAA"}}]}]}}
"#;
        let t = read_lines(text);
        assert_eq!(t.entries.len(), 1);
        assert_eq!(t.entries[0].result.as_deref(), Some("[image]"));
    }

    #[test]
    fn paths_are_basenamed_and_ok_file_edits_hide_their_result() {
        // Read/Edit/Write show just the file name, and their success result
        // ("File updated") is dropped as redundant — only errors surface.
        let text = r#"
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"/Users/me/proj/src/x.rs"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"The file was updated."}]}}
"#;
        let t = read_lines(text);
        assert_eq!(t.entries.len(), 1);
        assert_eq!(t.entries[0].text, "Editing x.rs"); // basename, not full path
        assert_eq!(t.entries[0].result, None); // redundant success result dropped
        assert!(!t.entries[0].is_error);
    }
}
