# Agent support matrix

What the notch actually supports per agent, and what "robust" requires. The
short version: live-testing is only half of it — for most agents the gap is
not testing, it is that support is not built yet. Testing what does not exist
would just confirm it does not exist.

Kept honest on purpose: a row here says what shipped, not what is planned.
The plan and build order live in [AGENT_NOTCH.md](AGENT_NOTCH.md).

## Per-agent state

| Agent | What exists today | What's missing before testing even matters |
|---|---|---|
| **Claude Code** | Full: hooks, approvals (`PermissionRequest`, 24 h), `AskUserQuestion` cards, edit diffs, process discovery, transcript enrichment, `project @ branch` rows | Nothing — this is the one where more live testing pays off (the ✗ checklist in AGENT_NOTCH.md's Verifying section) |
| **Codex** | Discovery finds the process; the bridge parses its dialect; a directive encoder exists | Real support: no installer for `~/.codex`, and the transcript scanner parses *Claude's* JSONL schema — Codex rollout files have a different shape (`session_meta` etc.), so rows appear but thin (no title/tool/busy). Build-order step 4 |
| **Kimi** | Speaks the Claude hook dialect — a free ride on the existing parser | Binary name `kimi` is not in `classify()`; the installer does not write `~/.kimi/config.toml` (TOML `[[hooks]]`, needs the line-oriented editor the plan describes so user comments survive) |
| **Claude forks (Qoder, Qwen, Factory, CodeBuddy)** | Same free ride on the Claude dialect | Installers for their config dirs (`~/.qoder/`, `~/.qwen/`, `~/.factory/`, `~/.codebuddy/`); binary names in `classify()` |
| **Cursor (composer agent)** | Nothing | A different protocol entirely: its own event names (`beforeShellExecution`, `afterFileEdit`), flat `{"permission": "allow"}` replies, `~/.cursor/hooks.json`; `cursor-agent` not classified |
| **Gemini CLI** | Nothing | Fire-and-forget hook dialect (no approval flow); `~/.gemini/settings.json` installer |
| **Antigravity** | Nothing — not in this matrix *or* open-vibe-island's | Unknown hook surface; needs its own protocol investigation before anything can be built |

## The robustness recipe, in order of value

1. **Fixtures beat live testing for parsers.** The one-time valuable act per
   agent is *capturing* real data — a hook payload, a transcript file, a `ps`
   line — and freezing it as a test fixture, the way the Cursor-hosted
   `ps` sample and the `--bg-spare` daemon lines are pinned in
   `crates/sessions/src/discovery.rs`. Live behavior changes with versions; a
   pinned fixture catches regressions forever and runs in plain `cargo test`
   (which matters here, because CI is not available as evidence — see
   CLAUDE.md). When adding an agent: save one transcript and one payload
   first, pin the parser against them, then build.

2. **Fail-open already protects against the unknown.** An agent this app does
   not support either does not appear (unclassified binary) or appears via
   hooks and degrades gracefully. Nothing about an untested agent can hang or
   break that agent — the sidecar exits silently with empty stdout on every
   failure path, and that property is tested. Robust does not require
   exhaustive coverage; it requires that unsupported ≠ broken, and that
   holds.

3. **Live-test only what claims support.** Today that is Claude Code — the
   remaining ✗ items in AGENT_NOTCH.md (kill -9 reap, terminal-parallel
   answer, delayed approve, `AskUserQuestion` multiSelect). Codex joins the
   list after step 4 is built, and its trust-review gate (`/hooks` inside
   Codex CLI) can only be verified live.

4. **Manual smoke after agent updates.** Dialect drift is the one thing no
   fixture can predict: when Claude Code updates, the transcript record
   shapes and hook payloads are what silently change. A five-minute smoke
   after an update (row appears, card appears, answer lands) catches what
   pinned tests cannot.

## Cheapest expansion order

1. **Kimi + the Claude forks** — one `classify()` entry each plus
   config-path installers; zero new protocol.
2. **Codex end to end** — real work: rollout-schema parser, TOML installer,
   live trust-gate verification.
3. **Cursor / Gemini** — only if actually installed on a user's machine;
   each is its own dialect.
4. **Antigravity** — protocol investigation first; everything else after.
