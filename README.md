# Aloud

Voice dictation for macOS and Windows. Hold a key, speak, and clean text lands
in whatever app has focus.

- **Local by default.** Transcription runs on-device via whisper.cpp. Audio
  never leaves your machine unless you choose a cloud model.
- **Bring your own AI.** Any OpenAI-compatible endpoint works — Groq, OpenAI,
  or something self-hosted.
- **Your notes are files.** Plain markdown on disk, greppable and syncable.

Built with Tauri v2: Rust for the work, React for the interface.

## Status

A port of a macOS-only Swift app to run on both platforms. The macOS feature
set has largely landed — dictation, rewrite, ask, agent and meeting modes, a
Run Log, streaming transcription, and three overlay styles. Windows compiles
but is unexercised, and release signing is still open. See
[`PROGRESS.md`](PROGRESS.md) for the detail.

## Building

Needs [Rust](https://rustup.rs), [pnpm](https://pnpm.io), and `cmake`
(whisper.cpp builds from source).

```sh
pnpm install
pnpm dev      # run
pnpm build    # bundle
```

See [`CLAUDE.md`](CLAUDE.md) for architecture and conventions.

## Bringing Cerebras back

`cerebras_api_key` was removed in PR #59. It was never wired to anything — the
field was stored and filled from the environment, and no code read it. Agent
mode has always run on Gemini (`src-tauri/src/agent.rs`), and Groq handles
transcription and cleanup.

It is documented here rather than left commented out, because commented-out
code goes stale without anyone noticing. To restore it as a *working* provider:

1. **Settings field** — in `crates/store/src/settings.rs`, add
   `pub cerebras_api_key: String` to `Settings`, `cerebras_api_key:
   String::new()` to the `Default` impl, and `("CEREBRAS_API_KEY", &mut
   self.cerebras_api_key)` to the array in `fill_keys_from_env`. `#[serde(default)]`
   means old settings files that still carry the key will pick it up again.

2. **Use it somewhere.** This is the step that was missing before. Cerebras is
   OpenAI-compatible, so `aloud_llm::LlmClient::new(&key, Some(base_url))`
   works as-is — the base URL is `https://api.cerebras.ai/v1`. The obvious
   place is a fallback in `agent.rs`, which currently bails when the Gemini key
   is empty:

   ```rust
   // agent.rs, replacing the bail on an empty Gemini key
   let (key, base) = if !settings.gemini_api_key.trim().is_empty() {
       (&settings.gemini_api_key, GEMINI_BASE_URL)
   } else if !settings.cerebras_api_key.trim().is_empty() {
       (&settings.cerebras_api_key, CEREBRAS_BASE_URL)
   } else {
       bail!("Agent mode needs a Gemini or Cerebras API key — add one in Settings")
   };
   ```

   Note the model name changes too: `GEMINI_MODEL` is a Gemini id and will 404
   against Cerebras. Pick a model per provider, not one constant.

3. **No UI.** Users do not supply keys — every input was removed in Phase A.
   A restored `CEREBRAS_API_KEY` resolves from `.env` like every other key,
   via `fill_keys_from_env`. Step 1 is the whole of it.

4. **Bindings** — run the app once (`pnpm dev`) so `tauri-specta` regenerates
   `src/lib/bindings.ts`. Never hand-edit it.

The original code is in git if you want the exact diff: `git show <the commit
that merged PR #59>`.

## License

MIT
