//! Reading a transcript to say what an agent is doing.
//!
//! A transcript is JSONL, appended a record at a time. Everything here works on
//! the tail: the title, the user's last prompt, whether the turn is still
//! running, and which tool is running right now.
//!
//! The record shapes were read off real transcripts (Claude Code 2.1.215)
//! rather than any published schema, so each rule below names the record it
//! came from.

use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, SystemTime};

use serde::Deserialize;

/// How recent the last *conversational* record must be for a session to read as
/// busy. Ported from `agent-notch`, where it was tuned against real turns: long
/// enough to bridge a slow model round-trip, short enough that a finished turn
/// stops claiming the indicator.
pub const BUSY_WINDOW: Duration = Duration::from_secs(30);

/// What a transcript's tail says about the session.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Scan {
    /// The model's own title for the session, from an `ai-title` record.
    pub title: Option<String>,
    /// The user's most recent prompt, from a `last-prompt` record.
    pub prompt: Option<String>,
    /// The tool the agent is running right now: a `tool_use` at the tail with no
    /// matching `tool_result`. See [`Tool`].
    pub running: Option<Tool>,
    /// Working directory, as recorded on the last record that carried one.
    pub cwd: Option<String>,
    /// When the last conversational record was written, as an ISO-8601 string.
    /// Housekeeping records do not set it — see [`Scan::busy`].
    pub last_activity: Option<String>,
}

/// A tool call the agent has started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    pub name: String,
    /// The one argument worth showing in a row this narrow — a command, a path,
    /// a pattern. None when the tool's input has no obvious headline.
    pub detail: Option<String>,
}

/// The records we care about. Everything else — `attachment`,
/// `file-history-snapshot`, `queue-operation` — is housekeeping and is skipped,
/// which is the point: those are written constantly and would otherwise read as
/// the agent being busy.
#[derive(Deserialize)]
#[serde(tag = "type")]
enum Record {
    #[serde(rename = "ai-title")]
    AiTitle {
        #[serde(rename = "aiTitle")]
        title: String,
    },
    #[serde(rename = "last-prompt")]
    LastPrompt {
        #[serde(rename = "lastPrompt")]
        prompt: String,
    },
    #[serde(rename = "assistant")]
    Assistant(Turn),
    #[serde(rename = "user")]
    User(Turn),
}

#[derive(Deserialize)]
struct Turn {
    timestamp: Option<String>,
    cwd: Option<String>,
    #[serde(default)]
    message: Message,
}

#[derive(Deserialize, Default)]
struct Message {
    /// A string when the user simply typed something, a list of blocks when it
    /// carries tool calls or results. Only the list form matters here.
    #[serde(default)]
    content: Content,
}

#[derive(Deserialize, Default)]
#[serde(untagged)]
enum Content {
    Blocks(Vec<Block>),
    /// A plain user turn. Its text is not read — the prompt comes from the
    /// `last-prompt` record — but the variant has to exist or the whole record
    /// fails to deserialize and its timestamp is lost.
    Text(serde::de::IgnoredAny),
    #[default]
    None,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Block {
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult { tool_use_id: String },
    #[serde(other)]
    Other,
}

/// Reads the tail of a transcript.
///
/// Malformed lines are skipped rather than failing the scan: a transcript is
/// appended to live, so the last line can be half-written at the moment it is
/// read.
pub fn scan(path: &Path) -> anyhow::Result<Scan> {
    let text = std::fs::read_to_string(path)?;
    Ok(scan_lines(&text))
}

/// The parse itself, over the text — so every rule below is testable without a
/// file on disk.
pub fn scan_lines(text: &str) -> Scan {
    let mut out = Scan::default();
    // A tool is only "running" if nothing later answered it, and the answer can
    // arrive several records after the call. So collect both and resolve after.
    let mut pending: Vec<(String, Tool)> = Vec::new();
    let mut answered: HashSet<String> = HashSet::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Record>(line) else {
            continue;
        };
        match record {
            Record::AiTitle { title } => out.title = Some(title),
            Record::LastPrompt { prompt } => out.prompt = Some(prompt),
            Record::Assistant(turn) | Record::User(turn) => {
                // Only conversational records move the clock. This is the whole
                // reason the enum skips `attachment` and friends.
                if turn.timestamp.is_some() {
                    out.last_activity = turn.timestamp;
                }
                if turn.cwd.is_some() {
                    out.cwd = turn.cwd;
                }
                if let Content::Blocks(blocks) = turn.message.content {
                    for block in blocks {
                        match block {
                            Block::ToolUse { id, name, input } => {
                                let detail = headline(&name, &input);
                                pending.push((id, Tool { name, detail }));
                            }
                            Block::ToolResult { tool_use_id } => {
                                answered.insert(tool_use_id);
                            }
                            Block::Other => {}
                        }
                    }
                }
            }
        }
    }

    // The last unanswered call is the one on screen. Later beats earlier: a
    // parallel batch shows its most recent, not its first.
    out.running = pending
        .into_iter()
        .rev()
        .find(|(id, _)| !answered.contains(id))
        .map(|(_, tool)| tool);
    out
}

/// The argument worth putting in a row this narrow.
///
/// Named per tool rather than "first string field": `Bash` wants its command,
/// `Read` its path, and picking whichever key happens to serialize first gives
/// a different answer as the model varies its arguments.
fn headline(name: &str, input: &serde_json::Value) -> Option<String> {
    let key = match name {
        "Bash" | "BashOutput" => "command",
        "Read" | "Write" | "NotebookEdit" => "file_path",
        "Edit" => "file_path",
        "Glob" | "Grep" => "pattern",
        "WebFetch" => "url",
        "WebSearch" => "query",
        "Task" | "Agent" => "description",
        "Skill" => "skill",
        _ => return None,
    };
    let value = input.get(key)?.as_str()?.trim();
    if value.is_empty() {
        return None;
    }
    // A heredoc or a pasted file makes for a command several hundred lines
    // long; the row wants its first line and nothing else.
    Some(value.lines().next().unwrap_or(value).to_string())
}

impl Scan {
    /// Whether the agent is mid-turn: the last conversational record is recent.
    ///
    /// `now` is a parameter so the rule is testable without waiting 30 seconds.
    pub fn busy(&self, now: SystemTime) -> bool {
        let Some(stamp) = self.last_activity.as_deref() else {
            return false;
        };
        let Some(at) = parse_timestamp(stamp) else {
            return false;
        };
        now.duration_since(at).map_or(true, |age| age < BUSY_WINDOW)
    }
}

/// Parses the ISO-8601 UTC stamp Claude Code writes (`2026-07-22T21:19:09.394Z`).
///
/// ponytail: a hand-rolled parse of exactly that shape rather than pulling
/// `chrono` in for one field. It returns None on anything else, and the only
/// consequence of None is that a session reads as not-busy. If a second date
/// format ever shows up here, take the dependency.
fn parse_timestamp(text: &str) -> Option<SystemTime> {
    let text = text.strip_suffix('Z')?;
    let (date, time) = text.split_once('T')?;
    let mut date = date.split('-');
    let year: i64 = date.next()?.parse().ok()?;
    let month: i64 = date.next()?.parse().ok()?;
    let day: i64 = date.next()?.parse().ok()?;
    let mut time = time.split(':');
    let hour: i64 = time.next()?.parse().ok()?;
    let minute: i64 = time.next()?.parse().ok()?;
    // Seconds carry the fractional part; the whole part is all we need.
    let second: i64 = time.next()?.split('.').next()?.parse().ok()?;

    // Days since the Unix epoch, by the civil-date algorithm (Howard Hinnant's
    // `days_from_civil`): shift the year to start in March so the leap day is
    // last, which removes every special case for February.
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;

    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second;
    let seconds = u64::try_from(seconds).ok()?;
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug this catches: `attachment` and `file-history-snapshot` records
    /// are written constantly and bump the file's mtime. Counting them as
    /// activity leaves every idle session reading as busy forever.
    #[test]
    fn housekeeping_writes_do_not_count_as_activity() {
        let text = r#"
{"type":"assistant","timestamp":"2026-07-22T21:19:09.394Z","message":{"content":[]}}
{"type":"attachment","timestamp":"2026-07-22T23:00:00.000Z"}
{"type":"file-history-snapshot","timestamp":"2026-07-22T23:00:00.000Z"}
{"type":"queue-operation","timestamp":"2026-07-22T23:00:00.000Z"}
"#;
        let scan = scan_lines(text);
        assert_eq!(
            scan.last_activity.as_deref(),
            Some("2026-07-22T21:19:09.394Z")
        );
    }

    /// The tail's unanswered `tool_use` is what the agent is doing. Verified
    /// against a live transcript mid-call: the line is written *before* the
    /// tool runs, which is what makes this readable at all.
    #[test]
    fn orphan_tool_use_is_what_it_is_doing_now() {
        let text = r#"
{"type":"assistant","timestamp":"2026-07-22T21:00:00.000Z","message":{"content":[{"type":"tool_use","id":"a","name":"Bash","input":{"command":"cargo build"}}]}}
{"type":"user","timestamp":"2026-07-22T21:00:01.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"a"}]}}
{"type":"assistant","timestamp":"2026-07-22T21:00:02.000Z","message":{"content":[{"type":"tool_use","id":"b","name":"Bash","input":{"command":"cargo test --workspace"}}]}}
"#;
        let running = scan_lines(text).running.expect("a tool is running");
        assert_eq!(running.name, "Bash");
        assert_eq!(running.detail.as_deref(), Some("cargo test --workspace"));
    }

    /// Every call answered means the agent is between tools, not inside one.
    #[test]
    fn a_finished_turn_is_running_nothing() {
        let text = r#"
{"type":"assistant","timestamp":"2026-07-22T21:00:00.000Z","message":{"content":[{"type":"tool_use","id":"a","name":"Read","input":{"file_path":"/tmp/x"}}]}}
{"type":"user","timestamp":"2026-07-22T21:00:01.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"a"}]}}
"#;
        assert_eq!(scan_lines(text).running, None);
    }

    /// A result can arrive several records after its call, so pairing has to be
    /// by id across the whole file — not "is the next record a result".
    #[test]
    fn a_result_is_matched_across_intervening_records() {
        let text = r#"
{"type":"assistant","timestamp":"2026-07-22T21:00:00.000Z","message":{"content":[{"type":"tool_use","id":"a","name":"Bash","input":{"command":"ls"}}]}}
{"type":"attachment"}
{"type":"user","timestamp":"2026-07-22T21:00:01.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"a"}]}}
{"type":"attachment"}
"#;
        assert_eq!(scan_lines(text).running, None);
    }

    /// A transcript is appended live, so the last line can be half-written at
    /// the moment it is read. That must not lose the records before it.
    #[test]
    fn a_half_written_last_line_does_not_lose_the_scan() {
        let text = "{\"type\":\"ai-title\",\"aiTitle\":\"Fix the parser\"}\n{\"type\":\"assis";
        assert_eq!(scan_lines(text).title.as_deref(), Some("Fix the parser"));
    }

    #[test]
    fn title_and_prompt_come_from_their_own_records() {
        let text = r#"
{"type":"ai-title","aiTitle":"Add tab system to agent notch"}
{"type":"last-prompt","lastPrompt":"let's execute AGENT_NOTCH.md"}
"#;
        let scan = scan_lines(text);
        assert_eq!(scan.title.as_deref(), Some("Add tab system to agent notch"));
        assert_eq!(scan.prompt.as_deref(), Some("let's execute AGENT_NOTCH.md"));
    }

    /// A user turn is plain text, not blocks. The untagged `Content` must take
    /// both without the string form failing the whole record.
    #[test]
    fn a_plain_text_user_turn_still_sets_activity() {
        let text = r#"{"type":"user","timestamp":"2026-07-22T21:19:09.394Z","cwd":"/tmp/p","message":{"content":"hello"}}"#;
        let scan = scan_lines(text);
        assert_eq!(
            scan.last_activity.as_deref(),
            Some("2026-07-22T21:19:09.394Z")
        );
        assert_eq!(scan.cwd.as_deref(), Some("/tmp/p"));
    }

    /// A heredoc makes a command hundreds of lines long; a row shows the first.
    #[test]
    fn a_multi_line_command_is_cut_to_its_first_line() {
        let text = r#"{"type":"assistant","timestamp":"2026-07-22T21:00:00.000Z","message":{"content":[{"type":"tool_use","id":"a","name":"Bash","input":{"command":"cat <<EOF\nline two\nEOF"}}]}}"#;
        let running = scan_lines(text).running.unwrap();
        assert_eq!(running.detail.as_deref(), Some("cat <<EOF"));
    }

    #[test]
    fn busy_follows_the_last_conversational_record() {
        let scan = Scan {
            last_activity: Some("2026-07-22T21:00:00.000Z".into()),
            ..Scan::default()
        };
        let at = parse_timestamp("2026-07-22T21:00:00.000Z").unwrap();
        assert!(scan.busy(at + Duration::from_secs(5)));
        assert!(!scan.busy(at + BUSY_WINDOW + Duration::from_secs(1)));
        // Nothing ever recorded is not busy.
        assert!(!Scan::default().busy(at));
    }

    /// The date maths is the part that can be silently wrong all year, so it is
    /// pinned against values `date -u -j` produced.
    #[test]
    fn timestamps_parse_to_the_right_instant() {
        let epoch = parse_timestamp("1970-01-01T00:00:00.000Z").unwrap();
        assert_eq!(epoch, SystemTime::UNIX_EPOCH);

        let seconds = |text: &str| {
            parse_timestamp(text)
                .unwrap()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        };
        assert_eq!(seconds("2026-07-22T21:19:09.394Z"), 1_784_755_149);
        // A leap day, and the day after it — the case the civil-date shift exists for.
        assert_eq!(seconds("2024-02-29T00:00:00.000Z"), 1_709_164_800);
        assert_eq!(seconds("2024-03-01T00:00:00.000Z"), 1_709_251_200);
        assert_eq!(parse_timestamp("not a date"), None);
    }
}
