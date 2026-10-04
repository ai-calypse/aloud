//! Scores how commands get routed: for each utterance in `evals/commands.jsonl`,
//! does the fast path or the brain pick the right tool with the right
//! arguments? Nothing is executed — only the brain's first turn is asked for.
//!
//! ```sh
//! cargo run -p aloud-agent --example command_eval                   # every brain with a key
//! cargo run -p aloud-agent --example command_eval -- gemini         # one brain
//! EVAL_K=3 cargo run -p aloud-agent --example command_eval          # each brain case 3 times
//! EVAL_SIZE=1 cargo run -p aloud-agent --example command_eval        # + tools and schema tokens per turn
//! ```
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

use aloud_agent::{Brain, ComposioClient};
use aloud_native::fast::Fast;
use serde::Deserialize;
use serde_json::{json, Value};
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
    let only = std::env::args().nth(1);
    let k: usize = std::env::var("EVAL_K")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .max(1);
    let mut brains: Vec<(&str, Brain)> = Vec::new();
    for (name, key) in [
        ("cerebras", &settings.cerebras_api_key),
        ("gemini", &settings.gemini_api_key),
    ] {
        if !key.trim().is_empty() && only.as_deref().is_none_or(|o| o == name) {
            brains.push((name, aloud_agent::brain(name, key)?));
        }
    }
    if brains.is_empty() {
        anyhow::bail!("no brain to evaluate: set CEREBRAS_API_KEY or GEMINI_API_KEY");
    }

    let cases: Vec<Case> = include_str!("../evals/commands.jsonl")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let installed = aloud_selection::installed_apps();
    let composio = ComposioClient::new(&settings.composio_api_key)?;
    let composio = (!settings.composio_api_key.trim().is_empty()).then_some(&composio);

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
                let (verdict, trace) = judge(case, brain, &installed, composio).await;
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
    Ok(())
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
) -> (Verdict, Trace) {
    let mut trace = Trace {
        route: "fast",
        called: None,
        offered: None,
        with_prose: false,
        waited: Duration::ZERO,
    };
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
                case.selection.as_deref(),
            );
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
            match turn {
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
            }
        }
    };
    (check(case, trace.called.as_ref()), trace)
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
