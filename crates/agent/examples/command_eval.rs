//! Scores how commands get routed: for each utterance in `evals/commands.jsonl`,
//! does the fast path or the brain pick the right tool with the right
//! arguments? Nothing is executed — only the brain's first turn is asked for.
//!
//! ```sh
//! cargo run -p aloud-agent --example command_eval                   # every brain with a key
//! cargo run -p aloud-agent --example command_eval -- gemini         # one brain
//! EVAL_SIZE=1 cargo run -p aloud-agent --example command_eval        # + tools and schema tokens per turn
//! ```
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
    selection: Option<String>,
    fast: Option<bool>,
    expected: Vec<Value>,
}

enum Verdict {
    Pass,
    Fail(String),
    Skip(String),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let settings = aloud_store::Settings::load();
    let only = std::env::args().nth(1);
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
        let (mut passed, mut failed, mut skipped) = (0, 0, 0);
        let mut latencies: Vec<Duration> = Vec::new();
        for case in &cases {
            let started = Instant::now();
            let (verdict, routed) = judge(case, brain, &installed, composio).await;
            let took = started.elapsed();
            let mark = match &verdict {
                Verdict::Pass => {
                    passed += 1;
                    "✓"
                }
                Verdict::Fail(_) => {
                    failed += 1;
                    "✗"
                }
                Verdict::Skip(_) => {
                    skipped += 1;
                    "–"
                }
            };
            if !matches!(verdict, Verdict::Skip(_)) && routed == "brain" {
                latencies.push(took);
            }
            let note = match verdict {
                Verdict::Fail(why) | Verdict::Skip(why) => format!("  ({why})"),
                Verdict::Pass => String::new(),
            };
            println!(
                "{mark} {:>5.2}s [{routed}] {}{note}",
                took.as_secs_f32(),
                case.input
            );
        }
        latencies.sort();
        let median = latencies
            .get(latencies.len() / 2)
            .map_or(0.0, Duration::as_secs_f32);
        println!(
            "{label}: {passed}/{} passed, {failed} failed, {skipped} skipped; median brain turn {median:.2}s",
            passed + failed
        );
    }
    Ok(())
}

async fn judge(
    case: &Case,
    brain: &Brain,
    installed: &[aloud_selection::InstalledApp],
    composio: Option<&ComposioClient>,
) -> (Verdict, &'static str) {
    let fast = aloud_native::fast::route(&case.input, installed);
    if let Some(want) = case.fast {
        if want != fast.is_some() {
            let got = if fast.is_some() { "taken" } else { "missed" };
            return (Verdict::Fail(format!("fast path {got}")), "fast");
        }
    }
    let (called, routed): (Option<(String, Value)>, _) = match fast {
        Some(Fast::Tool { name, args, .. }) => (Some((name.to_string(), args)), "fast"),
        Some(Fast::Web) => (
            Some((
                aloud_engine::BROWSER_TASK.to_string(),
                json!({ "goal": case.input }),
            )),
            "fast",
        ),
        None => {
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
                    return (Verdict::Skip(format!("{name} not offered here")), "brain");
                }
            }
            let command = aloud_agent::with_context(&case.input, None, case.selection.as_deref());
            if std::env::var_os("EVAL_SIZE").is_some() {
                let schema_chars: usize =
                    tools.iter().map(|t| t.to_openai().to_string().len()).sum();
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
                tokio::time::sleep(*retry_after + Duration::from_secs(1)).await;
                turn = aloud_agent::first_turn(brain, &command, &tools, &more).await;
            }
            match turn {
                Ok(turn) => (
                    turn.tool_calls.first().map(|call| {
                        let args =
                            serde_json::from_str(&call.function.arguments).unwrap_or(Value::Null);
                        (call.function.name.clone(), args)
                    }),
                    "brain",
                ),
                Err(e) => return (Verdict::Fail(format!("error: {e:#}")), "brain"),
            }
        }
    };
    (check(case, called.as_ref()), routed)
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
