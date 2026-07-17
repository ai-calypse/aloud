# Auto-updater

The `tauri-plugin-updater` plugin is installed, registered, and wired to a
launch-time check (`spawn_update_check` in `src/lib.rs`). It is a deliberate
no-op until you complete the two steps below, because signing keys are yours to
generate and cannot be committed.

## Activate

1. **Generate a signing key** (once):

   ```sh
   pnpm tauri signer generate -w ~/.tauri/aloud.key
   ```

   This prints a public key and writes the private key to the path given. Keep
   the private key secret; it is what proves an update really came from you.

2. **Configure `tauri.conf.json`:**
   - Put the printed public key in `plugins.updater.pubkey`. A non-empty pubkey
     is the switch: `spawn_update_check` skips the check while it is empty, so
     nothing runs until this is set.
   - Point `plugins.updater.endpoints` at your real release server. The
     `{{target}}`, `{{arch}}`, and `{{current_version}}` placeholders are filled
     by the plugin.
   - Add `"createUpdaterArtifacts": true` under `bundle`. This makes
     `tauri build` emit the signed `.sig` + update archive — it **requires** the
     signing env vars below, so it is left off until you are ready.

3. **Build a signed release:**

   ```sh
   export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/aloud.key)"
   export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="…"   # if you set one
   pnpm build
   ```

4. **Publish** the generated `latest.json` (or per-target JSON) and the signed
   archives to the endpoint URL. The plugin fetches that JSON, compares
   versions, verifies the signature against the pubkey, and installs.

## Behaviour

- The check runs in the background on launch. A failure (server down, offline)
  is logged and swallowed — it never blocks the app.
- An available update downloads and installs silently; it applies on next
  launch. A "restart now?" prompt is the upgrade path once there is a UI
  surface for it (noted as a `ponytail:` in `spawn_update_check`).
- ⚠️ The Windows path is unexercised, like the rest of the app.
