# Aloud

Voice dictation and voice control for macOS and Windows. Hold a key, speak, and
clean text lands in whatever app has focus — or say what to do ("make a cake
shopping list in Notes", "remind me to call Ana at 5", "summarise my unread
email") and it is done.

- **Local by default.** Transcription runs on-device via whisper.cpp. Audio
  never leaves your machine unless you choose a cloud model.
- **Bring your own AI.** Any OpenAI-compatible endpoint works — Groq, OpenAI,
  or something self-hosted.
- **Your notes are files.** Plain markdown on disk, greppable and syncable.

Built with Tauri v2: Rust for the work, React for the interface.

## Status

A port of a macOS-only Swift app to run on both platforms. The macOS feature
set has largely landed — dictation and rewrite, one Command mode for asking
and acting (with an always-listening bar), meeting notes, a Run Log,
streaming transcription, and three overlay styles. Windows compiles
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

## License

MIT
