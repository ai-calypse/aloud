//! Scores how commands get routed: for each utterance in `evals/commands.jsonl`,
//! does the fast path or the brain pick the right tool with the right
//! arguments? Nothing is executed — only the brain's first turn is asked for.
//!
//! ```sh
//! cargo run -p aloud-agent --example command_eval                   # every brain with a key
//! cargo run -p aloud-agent --example command_eval -- gemini         # one brain
//! EVAL_K=3 cargo run -p aloud-agent --example command_eval          # each brain case 3 times
//! EVAL_SIZE=1 cargo run -p aloud-agent --example command_eval        # + tools and schema tokens per turn
//! cargo run -p aloud-agent --example command_eval -- runs           # whole runs, tools mocked
//! cargo run -p aloud-agent --example command_eval -- runs cerebras
//! ```
//!
//! `runs` plays each case in `evals/runs.jsonl` through the whole tool loop
//! with every tool answered from the case's `mocks` (default
//! `{"successful":true}`; a key ending in `_` answers every tool it prefixes)
//! and questions from `answers`. Gates are answered as a user would: yes,
//! unless the card is strict or warns that outside content asked for it —
//! those are declined and listed as "asked". Every call the run made is then
//! scored: all `expected` entries must be met, and no `forbid` entry may match
//! a call that actually executed (a declined attempt is the gate working).
//! `{"callsTool":{"name":…,"count":N}}` passes when
//! the matching calls' `times` (1 each when absent) add up to exactly N, so
//! five calls and one call with `times: 5` both count. The first turn alone
//! cannot see a run that stops after its first step; this can.
//!
//! Each brain's run is also saved to `<app-data>/evals/<time>-<brain>.json`,
//! which the dashboard (`pnpm evals`) reads. Repeats exist because one pass
//! hides flakiness: a case counts as reliable only if all `EVAL_K` attempts pass.
//!
//! The case format is Raycast's AI-extension evals (MIT), trimmed: `input`,
//! `expected: [{callsTool: {name, arguments}}]` or `[{not: {callsTool: {}}}]`.
//! A `name` is one tool, a list of acceptable ones, or a prefix ending in `_`.
//! An argument matcher is a case-insensitive substring, `{"eq": v}` or
//! `{"or": [..]}`. `fast` asserts whether the no-model path takes it. A case
//! whose tool this machine or account doesn't offer is skipped, not failed.
//! A brain write that passes arguments its approval card would not show fails
//! as "blank preview", however right the call. The run ends with which layer
//! would draw the card for each write tool offered: hand, generated
//! (`<app-data>/action-previews.json`) or schema.

use aloud_agent::{Brain, ComposioClient};
use aloud_native::fast::Fast;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Deserialize)]
struct Case {
    input: String,
    /// The app on screen, as `with_context` hears it.
    front: Option<String>,
    selection: Option<String>,
    fast: Option<bool>,
    expected: Vec<Value>,
}

enum Verdict {
    Pass,
    Fail(String),
    Skip(String),
}

/// What one attempt did besides its verdict, for the saved report.
struct Trace {
    route: &'static str,
    called: Option<(String, Value)>,
    /// Tools offered and their schema size in tokens: what the prompt costs.
    offered: Option<(usize, usize)>,
    /// The brain said what it did in the same reply as the call, so the run
    /// could end without a second model call.
    with_prose: bool,
    /// Time spent waiting out rate limits, which is the quota's, not the brain's.
    waited: Duration,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let settings = aloud_store::Settings::load();
    let runs = std::env::args().nth(1).as_deref() == Some("runs");
    let only = std::env::args().nth(if runs { 2 } else { 1 });
    let k: usize = std::env::var("EVAL_K")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .max(1);
    let mut brains: Vec<(&str, Brain)> = Vec::new();
    for (name, key) in [
        ("groq", &settings.groq_api_key),
        ("nvidia", &settings.nvidia_api_key),
        ("gemini", &settings.gemini_api_key),
        // Only when asked for by name: off the default list while it returns 402.
        ("cerebras", &settings.cerebras_api_key),
    ] {
        let wanted = match only.as_deref() {
            Some(asked) => asked == name,
            None => name != "cerebras",
        };
        if !key.trim().is_empty() && wanted {
            brains.push((name, aloud_agent::brain(name, key)?));
        }
    }
    if brains.is_empty() {
        anyhow::bail!("no brain to evaluate: set GROQ_API_KEY, NVIDIA_API_KEY or GEMINI_API_KEY");
    }

    let cases: Vec<Case> = include_str!("../evals/commands.jsonl")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let installed = aloud_selection::installed_apps();
    let composio = ComposioClient::new(&settings.composio_api_key)?;
    let composio = (!settings.composio_api_key.trim().is_empty()).then_some(&composio);
    if runs {
        return whole_runs(&brains, &installed, composio).await;
    }
    aloud_agent::preview::load_generated(&aloud_notes::data_dir()?.join("action-previews.json"));
    let mut writes: BTreeMap<String, Value> = BTreeMap::new();

    for (label, brain) in &brains {
        println!("\n== {label} ({})", brain.model);
        let started_at = chrono::Utc::now();
        let (mut passed, mut failed, mut skipped, mut with_prose, mut called) = (0, 0, 0, 0, 0);
        let mut latencies: Vec<Duration> = Vec::new();
        let mut report = Vec::new();
        for case in &cases {
            let mut attempts = Vec::new();
            for _ in 0..k {
                let started = Instant::now();
                let (verdict, trace) = judge(case, brain, &installed, composio, &mut writes).await;
                let took = started.elapsed().saturating_sub(trace.waited);
                let (mark, outcome, why) = match &verdict {
                    Verdict::Pass => {
                        passed += 1;
                        ("✓", "pass", None)
                    }
                    Verdict::Fail(why) => {
                        failed += 1;
                        ("✗", "fail", Some(why.clone()))
                    }
                    Verdict::Skip(why) => {
                        skipped += 1;
                        ("–", "skip", Some(why.clone()))
                    }
                };
                if !matches!(verdict, Verdict::Skip(_)) && trace.route == "brain" {
                    latencies.push(took);
                }
                with_prose += usize::from(trace.with_prose);
                called += usize::from(trace.route == "brain" && trace.called.is_some());
                let note = why.as_ref().map(|w| format!("  ({w})")).unwrap_or_default();
                println!(
                    "{mark} {:>5.2}s [{}] {}{note}",
                    took.as_secs_f32(),
                    trace.route,
                    case.input
                );
                attempts.push(json!({
                    "verdict": outcome,
                    "why": why,
                    "route": trace.route,
                    "ms": took.as_millis() as u64,
                    "rateLimitedMs": trace.waited.as_millis() as u64,
                    "called": trace.called.as_ref().map(|(name, args)| json!({ "name": name, "args": args })),
                    "tools": trace.offered.map(|(n, _)| n),
                    "schemaTokens": trace.offered.map(|(_, t)| t),
                    "withProse": trace.with_prose,
                }));
                // Only the brain is worth asking twice: the fast path and a
                // skip come out the same every time.
                if trace.route != "brain" || matches!(verdict, Verdict::Skip(_)) {
                    break;
                }
            }
            report.push(json!({
                "input": case.input,
                "front": case.front,
                "selection": case.selection,
                "fast": case.fast,
                "expected": case.expected,
                "attempts": attempts,
            }));
        }
        latencies.sort();
        let median = latencies
            .get(latencies.len() / 2)
            .map_or(0.0, Duration::as_secs_f32);
        println!(
            "{label}: {passed}/{} passed, {failed} failed, {skipped} skipped; median brain turn {median:.2}s",
            passed + failed
        );
        println!(
            "{label}: {with_prose} of {called} tool calls came with the confirmation alongside (early finish possible)"
        );
        let path = save(label, brain.model, k, started_at, report)?;
        println!("{label}: report saved to {}", path.display());
    }
    coverage(&writes);
    Ok(())
}

/// Which layer draws the card for each write tool the cases were offered.
fn coverage(writes: &BTreeMap<String, Value>) {
    use aloud_agent::preview::{layer, Layer};
    let mut by: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (name, schema) in writes {
        let key = match layer(name, Some(schema)) {
            Layer::Hand => "hand",
            Layer::Generated => "generated",
            Layer::Schema => "schema",
        };
        by.entry(key).or_default().push(name);
    }
    let count = |key| by.get(key).map_or(0, Vec::len);
    println!(
        "\npreviews: {} write tools offered — hand {}, generated {}, schema {}",
        writes.len(),
        count("hand"),
        count("generated"),
        count("schema")
    );
    if let Some(schema) = by.get("schema") {
        println!("   schema only: {}", schema.join(", "));
    }
}

/// Writes one brain's run where the dashboard looks: the app's data folder,
/// shared by every worktree, so a branch's run lines up against main's.
fn save(
    label: &str,
    model: &str,
    k: usize,
    started_at: chrono::DateTime<chrono::Utc>,
    cases: Vec<Value>,
) -> anyhow::Result<std::path::PathBuf> {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    let report = json!({
        "brain": label,
        "model": model,
        "k": k,
        "startedAt": started_at.to_rfc3339(),
        "finishedAt": chrono::Utc::now().to_rfc3339(),
        "git": {
            "sha": git(&["rev-parse", "--short", "HEAD"]),
            "branch": git(&["branch", "--show-current"]),
            "dirty": git(&["status", "--porcelain"]).is_some_and(|s| !s.is_empty()),
        },
        "cases": cases,
    });
    let dir = aloud_notes::data_dir()?.join("evals");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!(
        "{}-{label}.json",
        started_at.format("%Y%m%d-%H%M%S")
    ));
    std::fs::write(&path, serde_json::to_string_pretty(&report)?)?;
    Ok(path)
}

async fn judge(
    case: &Case,
    brain: &Brain,
    installed: &[aloud_selection::InstalledApp],
    composio: Option<&ComposioClient>,
    writes: &mut BTreeMap<String, Value>,
) -> (Verdict, Trace) {
    let mut trace = Trace {
        route: "fast",
        called: None,
        offered: None,
        with_prose: false,
        waited: Duration::ZERO,
    };
    let mut blank = false;
    let fast = aloud_native::fast::route(&case.input, installed);
    if let Some(want) = case.fast {
        if want != fast.is_some() {
            let got = if fast.is_some() { "taken" } else { "missed" };
            return (Verdict::Fail(format!("fast path {got}")), trace);
        }
    }
    trace.called = match fast {
        Some(Fast::Tool { name, args, .. }) => Some((name.to_string(), args)),
        Some(Fast::Web { .. }) => Some((
            aloud_engine::BROWSER_TASK.to_string(),
            json!({ "goal": case.input }),
        )),
        None => {
            trace.route = "brain";
            let aloud_agent::CommandTools { tools, more } =
                aloud_agent::command_tools(&case.input, installed, composio).await;
            if let Some(name) = wanted_name(case) {
                let loadable = more.iter().any(|kit| {
                    let prefix = format!("{}_", kit.to_uppercase());
                    match &name {
                        Value::String(one) => one.starts_with(&prefix),
                        Value::Array(many) => many
                            .iter()
                            .filter_map(Value::as_str)
                            .any(|n| n.starts_with(&prefix)),
                        _ => false,
                    }
                });
                if !tools.iter().any(|t| name_matches(&name, &t.name)) && !loadable {
                    return (Verdict::Skip(format!("{name} not offered here")), trace);
                }
            }
            let command = aloud_agent::with_context(
                &case.input,
                case.front.as_deref(),
                None,
                None,
                case.selection.as_deref(),
            );
            for tool in tools.iter().filter(|t| !t.read_only) {
                writes.insert(tool.name.clone(), tool.parameters.clone());
            }
            let schema_chars: usize = tools.iter().map(|t| t.to_openai().to_string().len()).sum();
            trace.offered = Some((tools.len(), schema_chars / 4));
            if std::env::var_os("EVAL_SIZE").is_some() {
                eprintln!(
                    "   {} tools, ~{} schema tokens",
                    tools.len(),
                    schema_chars / 4
                );
                let mut by_kit: std::collections::BTreeMap<&str, (usize, usize)> =
                    Default::default();
                for t in &tools {
                    let e = by_kit.entry(t.toolkit.as_str()).or_default();
                    e.0 += 1;
                    e.1 += t.to_openai().to_string().len() / 4;
                }
                eprintln!("   by toolkit (tools, tokens): {by_kit:?}");
            }
            // A quota trip is not a routing failure: wait it out and ask again.
            let mut turn = aloud_agent::first_turn(brain, &command, &tools, &more).await;
            for _ in 0..2 {
                let Err(e) = &turn else { break };
                let Some(aloud_llm::LlmError::RateLimited { retry_after, .. }) = e.downcast_ref()
                else {
                    break;
                };
                let wait = *retry_after + Duration::from_secs(1);
                trace.waited += wait;
                tokio::time::sleep(wait).await;
                turn = aloud_agent::first_turn(brain, &command, &tools, &more).await;
            }
            let called = match turn {
                Ok(turn) => {
                    trace.with_prose = !turn.tool_calls.is_empty()
                        && turn
                            .content
                            .as_deref()
                            .is_some_and(|c| !c.trim().is_empty());
                    turn.tool_calls.first().map(|call| {
                        let args =
                            serde_json::from_str(&call.function.arguments).unwrap_or(Value::Null);
                        (call.function.name.clone(), args)
                    })
                }
                Err(e) => return (Verdict::Fail(format!("error: {e:#}")), trace),
            };
            blank = called.as_ref().is_some_and(|(name, args)| {
                tools
                    .iter()
                    .find(|t| &t.name == name && !t.read_only)
                    .is_some_and(|tool| blank_preview(name, args, &tool.parameters))
            });
            called
        }
    };
    // Right tool, right arguments, and still an approval card with nothing on
    // it: the user would be confirming a write they cannot read.
    match check(case, trace.called.as_ref()) {
        Verdict::Pass if blank => (Verdict::Fail("blank preview".into()), trace),
        verdict => (verdict, trace),
    }
}

/// A write passed arguments, yet its card would show none of them.
fn blank_preview(name: &str, args: &Value, schema: &Value) -> bool {
    let passed = args.as_object().is_some_and(|a| !a.is_empty());
    let preview = aloud_agent::preview::build(name, args, Some(schema));
    passed && preview.fields.iter().all(|f| f.value.trim().is_empty())
}

#[derive(Deserialize)]
struct RunCase {
    input: String,
    front: Option<String>,
    selection: Option<String>,
    /// Tool name → its canned result. Unlisted tools answer `{"successful":true}`.
    #[serde(default)]
    mocks: serde_json::Map<String, Value>,
    /// Replies to the run's questions, in order; past the end they go unanswered.
    #[serde(default)]
    answers: Vec<String>,
    #[serde(default)]
    expected: Vec<Value>,
    /// Scored against executed calls only.
    #[serde(default)]
    forbid: Vec<Value>,
}

/// Everything one whole run did: each call it attempted, those that executed
/// and succeeded, and the gates the simulated user declined.
#[derive(Default)]
struct Played {
    calls: Vec<(String, Value)>,
    executed: Vec<(String, Value)>,
    asked: Vec<String>,
}

async fn whole_runs(
    brains: &[(&str, Brain)],
    installed: &[aloud_selection::InstalledApp],
    composio: Option<&ComposioClient>,
) -> anyhow::Result<()> {
    let cases: Vec<RunCase> = include_str!("../evals/runs.jsonl")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    // Never called with tools mocked, but the loop's signature wants a client.
    let offline = ComposioClient::new("")?;
    for (label, brain) in brains {
        println!("\n== {label} ({}) — whole runs", brain.model);
        let started_at = chrono::Utc::now();
        let (mut passed, mut failed, mut skipped) = (0, 0, 0);
        let mut report = Vec::new();
        for case in &cases {
            let started = Instant::now();
            let (verdict, played) = play(case, brain, installed, composio, &offline).await;
            let took = started.elapsed();
            let (mark, outcome, why) = match &verdict {
                Verdict::Pass => {
                    passed += 1;
                    ("✓", "pass", None)
                }
                Verdict::Fail(why) => {
                    failed += 1;
                    ("✗", "fail", Some(why.clone()))
                }
                Verdict::Skip(why) => {
                    skipped += 1;
                    ("–", "skip", Some(why.clone()))
                }
            };
            let note = why.as_ref().map(|w| format!("  ({w})")).unwrap_or_default();
            let made: Vec<&str> = played.calls.iter().map(|(n, _)| n.as_str()).collect();
            let asked = if played.asked.is_empty() {
                String::new()
            } else {
                format!("  [asked: {}]", played.asked.join(", "))
            };
            println!(
                "{mark} {:>5.2}s {}{note}\n        {}{asked}",
                took.as_secs_f32(),
                case.input,
                made.join(" → ")
            );
            report.push(json!({
                "input": case.input,
                "front": case.front,
                "selection": case.selection,
                "expected": case.expected,
                "attempts": [{
                    "verdict": outcome,
                    "why": why,
                    "route": "run",
                    "ms": took.as_millis() as u64,
                    "calls": played.calls.iter().map(|(name, args)| json!({ "name": name, "args": args })).collect::<Vec<_>>(),
                    "asked": played.asked,
                }],
            }));
        }
        println!(
            "{label}: {passed}/{} whole runs passed, {failed} failed, {skipped} skipped",
            passed + failed
        );
        let path = save(&format!("{label}-runs"), brain.model, 1, started_at, report)?;
        println!("{label}: report saved to {}", path.display());
    }
    Ok(())
}

/// Plays one case through the real loop with every tool mocked, and returns
/// the verdict with what the run did, in order.
async fn play(
    case: &RunCase,
    brain: &Brain,
    installed: &[aloud_selection::InstalledApp],
    composio: Option<&ComposioClient>,
    offline: &ComposioClient,
) -> (Verdict, Played) {
    let aloud_agent::CommandTools { tools, more } =
        aloud_agent::command_tools(&case.input, installed, composio).await;
    for want in case
        .expected
        .iter()
        .filter_map(|e| e.pointer("/callsTool/name"))
    {
        let loadable = more
            .iter()
            .any(|kit| name_matches(want, &format!("{}_", kit.to_uppercase())));
        if !tools.iter().any(|t| name_matches(want, &t.name)) && !loadable {
            return (
                Verdict::Skip(format!("{want} not offered here")),
                Played::default(),
            );
        }
    }
    let prompt = aloud_agent::with_context(
        &case.input,
        case.front.as_deref(),
        None,
        None,
        case.selection.as_deref(),
    );
    let mock = |name: &str, _args: &Value| {
        case.mocks
            .get(name)
            .or_else(|| {
                case.mocks
                    .iter()
                    .find(|(key, _)| key.ends_with('_') && name.starts_with(key.as_str()))
                    .map(|(_, result)| result)
            })
            .map_or_else(|| r#"{"successful":true}"#.to_string(), Value::to_string)
    };
    let answers = std::sync::Mutex::new(case.answers.iter().cloned());
    let asked = std::sync::Mutex::new(Vec::new());
    let mut played = Played::default();
    let result = aloud_agent::run_tool_loop(
        std::slice::from_ref(brain),
        composio.unwrap_or(offline),
        aloud_agent::Command {
            said: &case.input,
            prompt: &prompt,
            tools: &tools,
            more: &more,
            front: case.front.as_deref(),
            document: None,
            project: None,
            selection: case.selection.is_some(),
        },
        aloud_agent::Callbacks {
            on_event: |event| match event {
                // Steps count every start from 0, so a step is its index here.
                aloud_agent::LoopEvent::ToolCallStart { name, args, .. } => {
                    played.calls.push((name, args));
                }
                aloud_agent::LoopEvent::ToolCallResult {
                    step,
                    success: true,
                    ..
                } => {
                    if let Some(call) = played.calls.get(step as usize) {
                        played.executed.push(call.clone());
                    }
                }
                _ => {}
            },
            // The user, simulated with no saved grants: yes to an ordinary
            // card, no to one that is strict or warns the request came from
            // outside content.
            confirm: |call: aloud_agent::ProposedCall| {
                let verdict = match &call.gate {
                    Some(gate) if gate.named_rule || gate.strict => {
                        asked
                            .lock()
                            .unwrap()
                            .push(format!("{} ({})", call.name, gate.key));
                        aloud_agent::Verdict::Decline
                    }
                    _ => aloud_agent::Verdict::Run(call.args),
                };
                async move { verdict }
            },
            ask: |_question: String| {
                let next = answers.lock().ok().and_then(|mut a| a.next());
                async move { next }
            },
            cancelled: || false,
            mock: Some(&mock),
        },
    )
    .await;
    played.asked = asked.into_inner().unwrap();
    if let Err(e) = result {
        return (Verdict::Fail(format!("error: {e:#}")), played);
    }
    (score(case, &played.calls, &played.executed), played)
}

/// Every `expected` entry met among the calls made, and no `forbid` entry met
/// among those executed. A failure says how far the run got: "finished after
/// 1 of 2" is a run that stopped early.
fn score(case: &RunCase, calls: &[(String, Value)], executed: &[(String, Value)]) -> Verdict {
    for entry in &case.forbid {
        if let Some(want) = entry.get("callsTool") {
            if let Some((name, args)) = executed.iter().find(|(n, a)| call_matches(want, n, a)) {
                return Verdict::Fail(format!("forbidden call executed: {name} {args}"));
            }
        }
    }
    let mut met = 0;
    for entry in &case.expected {
        if entry.get("not").is_some() {
            if !calls.is_empty() {
                return Verdict::Fail(format!("answer expected, called {}", calls[0].0));
            }
            met += 1;
            continue;
        }
        let want = &entry["callsTool"];
        let matching: Vec<&Value> = calls
            .iter()
            .filter(|(n, a)| call_matches(want, n, a))
            .map(|(_, a)| a)
            .collect();
        let ok = match want.get("count").and_then(Value::as_u64) {
            Some(count) => {
                let done: u64 = matching
                    .iter()
                    .map(|a| a.get("times").and_then(Value::as_u64).unwrap_or(1))
                    .sum();
                done == count
            }
            None => !matching.is_empty(),
        };
        if !ok {
            let made: Vec<&str> = calls.iter().map(|(n, _)| n.as_str()).collect();
            return Verdict::Fail(format!(
                "finished after {met} of {}: missing {want}; made [{}]",
                case.expected.len(),
                made.join(", ")
            ));
        }
        met += 1;
    }
    Verdict::Pass
}

fn call_matches(want: &Value, name: &str, args: &Value) -> bool {
    name_matches(&want["name"], name)
        && want
            .get("arguments")
            .and_then(Value::as_object)
            .is_none_or(|wanted| {
                wanted.iter().all(|(key, matcher)| {
                    let actual = match args.get(key) {
                        Some(Value::String(s)) => s.clone(),
                        Some(other) => other.to_string(),
                        None => return false,
                    };
                    value_matches(matcher, &actual, args.get(key))
                })
            })
}

/// The expected tool's name matcher, if the case expects a call.
fn wanted_name(case: &Case) -> Option<Value> {
    case.expected.first()?.pointer("/callsTool/name").cloned()
}

fn name_matches(want: &Value, got: &str) -> bool {
    match want {
        Value::String(name) if name.ends_with('_') => got.starts_with(name.as_str()),
        Value::String(name) => got == name,
        Value::Array(names) => names.iter().any(|n| name_matches(n, got)),
        _ => false,
    }
}

fn check(case: &Case, called: Option<&(String, Value)>) -> Verdict {
    let Some(expected) = case.expected.first() else {
        return Verdict::Fail("case has no expectation".into());
    };
    if expected.get("not").is_some() {
        return match called {
            None => Verdict::Pass,
            Some((name, _)) => Verdict::Fail(format!("answer expected, called {name}")),
        };
    }
    let Some((name, args)) = called else {
        return Verdict::Fail("answered without calling a tool".into());
    };
    let want = &expected["callsTool"];
    // Loading the right service first is the right first move when its tools
    // were left out of the prompt.
    if name == "ALOUD_MORE_TOOLS" {
        let service = args["service"].as_str().unwrap_or_default().to_uppercase();
        let prefix = format!("{service}_");
        let wanted: Vec<&str> = match &want["name"] {
            Value::String(one) => vec![one.as_str()],
            Value::Array(many) => many.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        return if wanted.iter().any(|w| w.starts_with(&prefix)) {
            Verdict::Pass
        } else {
            Verdict::Fail(format!("loaded {service}"))
        };
    }
    if !name_matches(&want["name"], name) {
        return Verdict::Fail(format!("called {name} {args}"));
    }
    if let Some(wanted) = want.get("arguments").and_then(Value::as_object) {
        for (key, matcher) in wanted {
            let actual = match args.get(key) {
                Some(Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => return Verdict::Fail(format!("{name} without `{key}`: {args}")),
            };
            if !value_matches(matcher, &actual, args.get(key)) {
                return Verdict::Fail(format!("{name} `{key}` = {actual:?}"));
            }
        }
    }
    Verdict::Pass
}

fn value_matches(matcher: &Value, actual: &str, raw: Option<&Value>) -> bool {
    match matcher {
        Value::String(part) => actual.to_lowercase().contains(&part.to_lowercase()),
        Value::Object(m) if m.contains_key("eq") => raw == m.get("eq"),
        Value::Object(m) if m.contains_key("or") => m["or"]
            .as_array()
            .is_some_and(|any| any.iter().any(|v| value_matches(v, actual, raw))),
        _ => false,
    }
}
