//! Reading a transcript to say what an agent is doing.
//!
//! A transcript is JSONL, appended a record at a time. Everything here works on
//! the tail: the title, the user's last prompt, whether the turn is still
//! running, and which tool is running right now.
//!
//! The record shapes were read off real transcripts (Claude Code 2.1.215)
//! rather than any published schema, so each rule below names the record it
//! came from.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
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
    /// The model, from an assistant record's `message.model`, with the vendor
    /// prefix stripped: `claude-opus-4-8` reads as `opus-4-8`. What the row's
    /// tag shows.
    pub model: Option<String>,
    /// The git branch the agent is on, from any record's `gitBranch` — with
    /// the project folder, it names what the session is working on.
    pub branch: Option<String>,
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
    #[serde(rename = "gitBranch")]
    git_branch: Option<String>,
    #[serde(default)]
    message: Message,
}

#[derive(Deserialize, Default)]
struct Message {
    /// A string when the user simply typed something, a list of blocks when it
    /// carries tool calls or results. Only the list form matters here.
    #[serde(default)]
    content: Content,
    /// Only assistant records carry it; the id form, e.g. `claude-opus-4-8`.
    #[serde(default)]
    model: Option<String>,
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

/// Reads a whole transcript in one pass.
///
/// Malformed lines are skipped rather than failing the scan: a transcript is
/// appended to live, so the last line can be half-written at the moment it is
/// read. The polling path uses [`Tail`] instead, which remembers its offset;
/// this stays for one-shot reads and the tests.
pub fn scan(path: &Path) -> anyhow::Result<Scan> {
    let text = std::fs::read_to_string(path)?;
    Ok(scan_lines(&text))
}

/// The parse itself, over the text — so every rule below is testable without a
/// file on disk.
pub fn scan_lines(text: &str) -> Scan {
    let mut state = TailState::default();
    for line in text.lines() {
        state.apply(line);
    }
    state.resolved()
}

/// Transcript readers that remember how far they got.
///
/// A transcript reaches hundreds of megabytes over a long session, and the
/// poll runs every few seconds — re-reading whole files was measurable I/O for
/// no new information. Each path keeps its byte offset and its accumulated
/// scan; a poll reads only what was appended. A file that *shrank* is a new
/// session reusing the path, and its state restarts from zero.
#[derive(Default)]
pub struct Tail {
    states: HashMap<PathBuf, TailState>,
    /// Bumped by [`Tail::begin`]; states not touched since the last bump are
    /// dropped by [`Tail::prune`], so paths that stopped being scanned do not
    /// accumulate forever.
    generation: u64,
}

impl Tail {
    /// Marks the start of a poll cycle, for [`Tail::prune`].
    pub fn begin(&mut self) {
        self.generation += 1;
    }

    /// Drops state for every path not scanned since [`Tail::begin`].
    pub fn prune(&mut self) {
        let generation = self.generation;
        self.states.retain(|_, s| s.generation == generation);
    }

    /// The scan so far, advanced by whatever the file gained since last time.
    pub fn scan(&mut self, path: &Path) -> anyhow::Result<Scan> {
        use std::io::{Read, Seek, SeekFrom};

        let state = self.states.entry(path.to_path_buf()).or_default();
        state.generation = self.generation;

        let mut file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        if len < state.offset {
            // Shrunk: a new transcript took the path. Start over.
            *state = TailState {
                generation: self.generation,
                ..TailState::default()
            };
        }
        if len > state.offset {
            file.seek(SeekFrom::Start(state.offset))?;
            let mut bytes = Vec::with_capacity((len - state.offset) as usize);
            file.read_to_end(&mut bytes)?;
            // Only complete lines are consumed; a half-written last line stays
            // unread and is picked up whole on the next poll. `\n` is a single
            // byte, so the cut can never land inside a UTF-8 sequence.
            let complete = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
            for line in String::from_utf8_lossy(&bytes[..complete]).lines() {
                state.apply(line);
            }
            state.offset += complete as u64;
        }
        Ok(state.resolved())
    }
}

/// One transcript's accumulated read: the scan fields plus the tool calls not
/// yet answered — the part that has to survive between polls for the running
/// tool to resolve across an increment boundary.
#[derive(Default)]
struct TailState {
    offset: u64,
    generation: u64,
    scan: Scan,
    /// Calls with no result yet, in file order. A result removes its call, so
    /// the last entry is what the agent is doing right now. Results always
    /// follow their call in an append-only file, so no "answered" set is
    /// needed — arrival order resolves everything.
    pending: Vec<(String, Tool)>,
}

impl TailState {
    fn apply(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let Ok(record) = serde_json::from_str::<Record>(line) else {
            return;
        };
        match record {
            Record::AiTitle { title } => self.scan.title = Some(title),
            Record::LastPrompt { prompt } => self.scan.prompt = Some(prompt),
            Record::Assistant(turn) | Record::User(turn) => {
                // Only conversational records move the clock. This is the whole
                // reason the enum skips `attachment` and friends.
                if turn.timestamp.is_some() {
                    self.scan.last_activity = turn.timestamp;
                }
                if turn.cwd.is_some() {
                    self.scan.cwd = turn.cwd;
                }
                if turn.git_branch.is_some() {
                    self.scan.branch = turn.git_branch;
                }
                // The model can change mid-session (a /model switch); the last
                // one wins, matching what the agent is answering with now.
                if let Some(model) = turn.message.model {
                    self.scan.model =
                        Some(model.strip_prefix("claude-").unwrap_or(&model).to_string());
                }
                if let Content::Blocks(blocks) = turn.message.content {
                    for block in blocks {
                        match block {
                            Block::ToolUse { id, name, input } => {
                                let detail = headline(&name, &input);
                                self.pending.push((id, Tool { name, detail }));
                            }
                            Block::ToolResult { tool_use_id } => {
                                self.pending.retain(|(id, _)| *id != tool_use_id);
                            }
                            Block::Other => {}
                        }
                    }
                }
            }
        }
    }

    /// The scan with `running` filled in from the unanswered tail.
    fn resolved(&self) -> Scan {
        let mut out = self.scan.clone();
        // Later beats earlier: a parallel batch shows its most recent call.
        out.running = self.pending.last().map(|(_, tool)| tool.clone());
        out
    }
}

/// The argument worth putting in a row this narrow.
///
/// Named per tool rather than "first string field": `Bash` wants its command,
/// `Read` its path, and picking whichever key happens to serialize first gives
/// a different answer as the model varies its arguments. Public because hook
/// payloads carry the same `tool_input` shape and want the same one line.
pub fn headline(name: &str, input: &serde_json::Value) -> Option<String> {
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
        self.active_within(now, BUSY_WINDOW)
    }

    /// Whether the last conversational record falls inside `window` — the
    /// generalised form of [`Scan::busy`], for callers with a longer horizon
    /// (retiring finished subagents wants minutes, not seconds).
    pub fn active_within(&self, now: SystemTime, window: Duration) -> bool {
        let Some(stamp) = self.last_activity.as_deref() else {
            return false;
        };
        let Some(at) = parse_timestamp(stamp) else {
            return false;
        };
        now.duration_since(at).map_or(true, |age| age < window)
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

/// The inverse of [`parse_timestamp`]: a `SystemTime` in the transcripts' own
/// ISO-8601 UTC shape, so hook-born rows and scan-born rows carry timestamps
/// the webview formats the same way.
///
/// Same civil-date algorithm as the parse (Howard Hinnant's `civil_from_days`),
/// and pinned against it by a roundtrip test — the two must agree or a row's
/// `· now` tag drifts.
pub fn format_timestamp(at: SystemTime) -> Option<String> {
    let seconds = at.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs();
    let seconds = i64::try_from(seconds).ok()?;
    let days = seconds.div_euclid(86_400);
    let in_day = seconds.rem_euclid(86_400);
    let (hour, minute, second) = (in_day / 3_600, (in_day % 3_600) / 60, in_day % 60);

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);

    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.000Z"
    ))
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

    /// The formatter and the parser must agree, or hook-born timestamps drift
    /// against transcript-born ones and a row's `· now` tag lies.
    #[test]
    fn format_and_parse_roundtrip() {
        for stamp in [
            "1970-01-01T00:00:00.000Z",
            "2024-02-29T00:00:00.000Z",
            "2024-03-01T00:00:00.000Z",
            "2026-07-22T21:19:09.000Z",
            "2026-12-31T23:59:59.000Z",
        ] {
            let at = parse_timestamp(stamp).unwrap();
            assert_eq!(format_timestamp(at).as_deref(), Some(stamp));
        }
    }

    /// The whole point of the tail: an append advances the scan without the
    /// earlier bytes being re-read, and the running tool resolves across the
    /// increment boundary — the call arrived in one poll, its result in the next.
    #[test]
    fn a_tail_resolves_a_tool_call_across_two_reads() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, r#"{{"type":"ai-title","aiTitle":"Fix the parser"}}"#).unwrap();
        writeln!(
            file,
            r#"{{"type":"assistant","timestamp":"2026-07-22T21:00:00.000Z","message":{{"content":[{{"type":"tool_use","id":"a","name":"Bash","input":{{"command":"cargo test"}}}}]}}}}"#
        )
        .unwrap();

        let mut tail = Tail::default();
        tail.begin();
        let scan = tail.scan(&path).unwrap();
        assert_eq!(scan.title.as_deref(), Some("Fix the parser"));
        assert_eq!(scan.running.as_ref().unwrap().name, "Bash");

        writeln!(
            file,
            r#"{{"type":"user","timestamp":"2026-07-22T21:00:01.000Z","message":{{"content":[{{"type":"tool_result","tool_use_id":"a"}}]}}}}"#
        )
        .unwrap();
        let scan = tail.scan(&path).unwrap();
        assert_eq!(scan.running, None, "the result answered the earlier call");
        assert_eq!(scan.title.as_deref(), Some("Fix the parser"));
    }

    /// A half-written last line stays unconsumed and is read whole next poll —
    /// consuming it early would drop the record's tail when the rest lands.
    #[test]
    fn a_tail_leaves_a_half_written_line_for_the_next_read() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        let half = concat!(
            r#"{"type":"last-prompt","lastPrompt":"hello"}"#,
            "\n",
            r#"{"type":"ai-title","aiTi"#
        );
        write!(file, "{half}").unwrap();

        let mut tail = Tail::default();
        let scan = tail.scan(&path).unwrap();
        assert_eq!(scan.prompt.as_deref(), Some("hello"));
        assert_eq!(scan.title, None);

        // The rest of the line lands; the record must apply exactly once.
        writeln!(file, r#"tle":"Now whole"}}"#).unwrap();
        let scan = tail.scan(&path).unwrap();
        assert_eq!(scan.title.as_deref(), Some("Now whole"));
    }

    /// A shrunk file is a new transcript reusing the path — stale state kept
    /// across that boundary shows the old session's tools on the new one.
    #[test]
    fn a_shrunk_file_restarts_its_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        std::fs::write(
            &path,
            format!(
                "{}\n{}\n",
                r#"{"type":"ai-title","aiTitle":"An earlier, longer session title"}"#,
                r#"{"type":"assistant","timestamp":"2026-07-22T21:00:00.000Z","message":{"content":[{"type":"tool_use","id":"a","name":"Bash","input":{"command":"ls"}}]}}"#
            ),
        )
        .unwrap();
        let mut tail = Tail::default();
        assert!(tail.scan(&path).unwrap().running.is_some());

        std::fs::write(&path, "{\"type\":\"ai-title\",\"aiTitle\":\"New\"}\n").unwrap();
        let scan = tail.scan(&path).unwrap();
        assert_eq!(scan.title.as_deref(), Some("New"));
        assert_eq!(scan.running, None, "the old session's tool must not leak");
    }

    /// Paths that stop being scanned are pruned, or a long-running app holds
    /// state for every transcript it ever watched.
    #[test]
    fn prune_drops_paths_not_scanned_this_generation() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.jsonl");
        let b = dir.path().join("b.jsonl");
        std::fs::write(&a, "{}\n").unwrap();
        std::fs::write(&b, "{}\n").unwrap();

        let mut tail = Tail::default();
        tail.begin();
        tail.scan(&a).unwrap();
        tail.scan(&b).unwrap();
        tail.begin();
        tail.scan(&b).unwrap();
        tail.prune();
        assert_eq!(tail.states.len(), 1);
        assert!(tail.states.contains_key(&b));
    }
}
