import { useEffect, useRef, useState } from "react";
import {
  commands,
  type AudioDevice,
  type DefaultPrompt,
  type Settings,
} from "../../lib/bindings";
import { or } from "../../lib/result";
import { HotkeyEditor } from "./HotkeyEditor";
import { MacroEditor } from "./MacroEditor";
import { AppRuleEditor } from "./AppRuleEditor";
import { ConnectorEditor } from "./ConnectorEditor";
import { RunLogView } from "./RunLogView";
import { AUTO_DETECT, LANGUAGES, SAME_AS_SPOKEN } from "./languages";
import {
  Group,
  PaneTitle,
  Row,
  Toggle,
  Value,
  rowInput,
  rowSelect,
} from "@/components/ui/rows";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

/** One settings pane, selected from the app-level sidebar. */
export type SettingsSection =
  | "general"
  | "hotkeys"
  | "macros"
  | "rules"
  | "prompts"
  | "connectors"
  | "runlog";

/**
 * Settings forms bound to the Rust `Settings` struct, laid out as macOS System
 * Settings grouped lists: an uppercase caption over a solid card of
 * hairline-separated rows, label left and control right.
 *
 * The Swift app had seven tabs over ~70 UserDefaults keys. This is the subset
 * that exists so far; the rest arrive with the features they configure. Which
 * pane shows is driven by the app-level sidebar via the `section` prop.
 */
export function SettingsView({ section }: { section: SettingsSection }) {
  const [settings, setSettings] = useState<Settings | null>(null);
  // Long-lived closures call `update` from an older render: a hotkey capture
  // waits up to ten seconds, and everything on this pane stays clickable
  // throughout. Spreading that render's `settings` silently reverted anything
  // changed by mouse in the meantime.
  const live = useRef(settings);
  live.current = settings;
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  // The shipped prompt and its version, for the Prompts pane's banner and its
  // View Default button. Null until it loads; both surfaces hide until then.
  const [builtIn, setBuiltIn] = useState<DefaultPrompt | null>(null);
  const [sample, setSample] = useState("");
  const [running, setRunning] = useState(false);
  // One panel for both the run's output and the built-in prompt, since only
  // one is ever worth reading at a time.
  const [panel, setPanel] = useState<{ title: string; body: string } | null>(
    null,
  );
  const [saved, setSaved] = useState(false);
  // Owned by the OS (a login item / registry key), not settings.json, so it is
  // read from the autostart plugin rather than from `settings`.
  const [atLogin, setAtLogin] = useState(false);

  useEffect(() => {
    commands.getSettings().then(setSettings);
    commands.defaultPrompt().then(setBuiltIn);
    or(commands.inputDevices(), [], "listing microphones").then(setDevices);
    or(commands.launchAtLogin(), false, "reading launch-at-login").then(setAtLogin);
  }, []);

  if (!settings) return <p className="text-ink-2">Loading…</p>;

  // Every field is optional in TS because the Rust struct is #[serde(default)]
  // — a partial settings file is valid. Read through these rather than
  // scattering `??` through the markup.
  const engine = settings.engine ?? "local";
  const vocabulary = settings.vocabulary ?? [];
  // A partial settings file may omit these; fall back to empty so the editors
  // render. hotkeys is never null in practice (Config::defaults), but TS types
  // it optional because the struct is #[serde(default)].
  const hotkeys = settings.hotkeys ?? {
    hold: null,
    toggle: null,
    copy_again: null,
    ask: null,
    rewrite: null,
    meeting_notes: null,
    agent: null,
  };
  const macros = settings.macros ?? [];
  const appRules = settings.appRules ?? [];
  const tidy = settings.postProcessingEnabled ?? true;
  // Read once: specta types every field optional (the Rust struct is
  // #[serde(default)]), and repeating the fallback per row is how one of them
  // ends up with a different default from the rest.
  const commandMode = settings.commandModeEnabled ?? true;
  const sounds = settings.playSounds ?? true;

  /** Applies one field and persists immediately — no Save button to forget. */
  async function update(patch: Partial<Settings>) {
    const next = { ...live.current!, ...patch };
    // Written here too, so two updates in one React batch cannot lose a field.
    live.current = next;
    setSettings(next);

    const result = await commands.updateSettings(next);
    if (result.status === "error") {
      console.error(`saving settings: ${result.error}`);
      return;
    }
    setSaved(true);
    setTimeout(() => setSaved(false), 1200);
  }

  // Only a fork can go stale: an empty prompt always *is* the current default.
  const promptOutdated =
    builtIn !== null &&
    (settings.customSystemPrompt ?? "").trim() !== "" &&
    settings.seenPromptVersion !== builtIn.version;

  /** Records that the user has seen this default, whatever they chose to do. */
  function acknowledgePrompt() {
    update({ seenPromptVersion: builtIn!.version });
  }

  /** Runs the prompt in the editor against the sample. */
  async function runPrompt() {
    setRunning(true);
    try {
      const result = await commands.testPrompt(
        settings!.customSystemPrompt ?? "",
        sample,
      );
      setPanel({
        title: result.status === "error" ? "Failed" : "Result",
        body: result.status === "error" ? result.error : result.data,
      });
    } catch (e) {
      // A rejected invoke — serialization, a panic, the window going away —
      // never reaches the Result type, and without this the button stays
      // disabled until the pane remounts.
      setPanel({ title: "Failed", body: String(e) });
    } finally {
      setRunning(false);
    }
  }

  return (
    <div className="flex flex-col gap-6">
      {section === "general" && (
        <>
          <PaneTitle title="General" />

          <Group title="Transcription">
            <Row label="Engine">
              <Value>
                <Select
                  value={engine}
                  onValueChange={(v) =>
                    update({ engine: v as Settings["engine"] })
                  }
                >
                  <SelectTrigger className={rowSelect}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="local">
                      On-device (no key needed)
                    </SelectItem>
                    <SelectItem value="cloud">Cloud</SelectItem>
                  </SelectContent>
                </Select>
              </Value>
            </Row>

            {engine === "cloud" && (
              <>
                <Row label="Model">
                  <Value className="flex-1">
                    <Input
                      className={`${rowInput} font-mono text-xs`}
                      value={settings.transcriptionModel ?? ""}
                      onChange={(e) =>
                        update({ transcriptionModel: e.target.value })
                      }
                    />
                  </Value>
                </Row>
                <Row label="Base URL">
                  <Value className="flex-1">
                    <Input
                      className={`${rowInput} font-mono text-xs`}
                      value={settings.baseUrl ?? ""}
                      onChange={(e) => update({ baseUrl: e.target.value })}
                    />
                  </Value>
                </Row>
              </>
            )}

            <Row label="Microphone">
              <Value>
                <Select
                  value={settings.selectedMicrophone ?? "default"}
                  onValueChange={(v) =>
                    update({ selectedMicrophone: v === "default" ? null : v })
                  }
                >
                  <SelectTrigger className={rowSelect}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="default">System default</SelectItem>
                    {devices.map((d) => (
                      <SelectItem key={d.id} value={d.id}>
                        {d.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Value>
            </Row>

            <Row
              label="Spoken language"
              hint="The language you dictate in. Auto-detect can guess wrong on short utterances; naming it avoids that."
            >
              <Value>
                <Select
                  // Lowercased to match: the field this replaced was free
                  // text, so a stored "EN" would otherwise select nothing and
                  // render an empty trigger.
                  value={settings.language?.toLowerCase() || AUTO_DETECT}
                  onValueChange={(v) =>
                    update({ language: v === AUTO_DETECT ? "" : v })
                  }
                >
                  <SelectTrigger className={rowSelect}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value={AUTO_DETECT}>Auto-detect</SelectItem>
                    {LANGUAGES.map((l) => (
                      <SelectItem key={l.code} value={l.code}>
                        {l.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Value>
            </Row>
          </Group>

          <Group title="Cleanup">
            <Row label="Tidy transcripts with an LLM" className="py-[11px]">
              <Toggle
                checked={tidy}
                label="Tidy transcripts with an LLM"
                onChange={(c) => update({ postProcessingEnabled: c })}
              />
            </Row>

            {tidy && (
              <>
                <Row label="Model">
                  <Value className="flex-1">
                    <Input
                      className={`${rowInput} font-mono text-xs`}
                      value={settings.postProcessingModel ?? ""}
                      onChange={(e) =>
                        update({ postProcessingModel: e.target.value })
                      }
                    />
                  </Value>
                </Row>
                <Row label="Fallback model">
                  <Value className="flex-1">
                    <Input
                      className={`${rowInput} font-mono text-xs`}
                      value={settings.fallbackModel ?? ""}
                      onChange={(e) =>
                        update({ fallbackModel: e.target.value })
                      }
                    />
                  </Value>
                </Row>
                {/* Deliberately in Cleanup, not Transcription: whisper's
                    translate task only ever targets English, so this is an
                    instruction to the cleanup model and lives beside it. */}
                <Row
                  label="Output language"
                  hint="Translate the finished text before pasting. “Same as spoken” leaves it alone."
                >
                  <Value>
                    <Select
                      // The name, not the code: it is interpolated into the
                      // system prompt verbatim, and "de" is not an instruction.
                      value={settings.outputLanguage || SAME_AS_SPOKEN}
                      onValueChange={(v) =>
                        update({
                          outputLanguage: v === SAME_AS_SPOKEN ? "" : v,
                        })
                      }
                    >
                      <SelectTrigger className={rowSelect}>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value={SAME_AS_SPOKEN}>
                          Same as spoken
                        </SelectItem>
                        {LANGUAGES.map((l) => (
                          <SelectItem key={l.code} value={l.name}>
                            {l.name}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </Value>
                </Row>
                <Row
                  label="Vocabulary"
                  hint="Names the model should spell correctly"
                >
                  <Value className="flex-1">
                    <Input
                      className={rowInput}
                      value={vocabulary.join(", ")}
                      placeholder="Kubernetes, Tauri, Aloud"
                      onChange={(e) =>
                        update({
                          vocabulary: e.target.value
                            .split(",")
                            .map((v) => v.trim())
                            .filter(Boolean),
                        })
                      }
                    />
                  </Value>
                </Row>
              </>
            )}
          </Group>

          <Group title="Behaviour">
            <Row label="Launch at login" className="py-[11px]">
              <Toggle
                checked={atLogin}
                label="Launch at login"
                onChange={async (enabled) => {
                  setAtLogin(enabled);
                  const result = await commands.setLaunchAtLogin(enabled);
                  // The OS is the source of truth: if it refused, snap back
                  // rather than showing a switch that lies.
                  if (result.status === "error") {
                    console.error(`launch at login: ${result.error}`);
                    setAtLogin(!enabled);
                  }
                }}
              />
            </Row>

            <Row label="Say “press enter” at the end to submit" className="py-[11px]">
              <Toggle
                checked={settings.pressEnterCommand ?? true}
                label="Say press enter at the end to submit"
                onChange={(c) => update({ pressEnterCommand: c })}
              />
            </Row>

            <Row
              label="Keep my clipboard after pasting"
              hint="Off leaves the dictated text there to paste again."
              className="py-[11px]"
            >
              <Toggle
                checked={settings.preserveClipboard ?? true}
                label="Keep my clipboard after pasting"
                onChange={(c) => update({ preserveClipboard: c })}
              />
            </Row>

            <Row label="Sound on start / stop" className="py-[11px]">
              <Toggle
                checked={sounds}
                label="Sound on start / stop"
                onChange={(c) => update({ playSounds: c })}
              />
            </Row>

            {sounds && (
              <Row label="Volume">
                <Value className="flex flex-1 items-center gap-3">
                  <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.1}
                    className="flex-1 accent-brand"
                    value={settings.soundVolume ?? 0.5}
                    onChange={(e) =>
                      update({ soundVolume: Number(e.target.value) })
                    }
                  />
                  <Button
                    variant="plain"
                    size="text"
                    onClick={() =>
                      commands.previewSound(settings.soundVolume ?? 0.5)
                    }
                  >
                    Preview
                  </Button>
                </Value>
              </Row>
            )}

            <Row
              label="Show menu bar icon"
              hint="Hotkeys keep working when hidden. Open Aloud again to get back here."
              className="py-[11px]"
            >
              <Toggle
                checked={settings.showMenuBarIcon ?? true}
                label="Show menu bar icon"
                onChange={(c) => update({ showMenuBarIcon: c })}
              />
            </Row>

            <Row
              label="Mute while dictating"
              hint="Silences system audio so music does not reach the microphone."
              className="py-[11px]"
            >
              <Toggle
                checked={settings.muteSystemAudio ?? false}
                label="Mute while dictating"
                onChange={(c) => update({ muteSystemAudio: c })}
              />
            </Row>

            <Row
              label="Start delay"
              hint="Wait before capturing, so a slow key press does not clip the first word"
            >
              <Value className="w-24">
                <Input
                  type="number"
                  min={0}
                  max={2000}
                  step={50}
                  className={rowInput}
                  value={settings.shortcutStartDelayMs ?? 0}
                  onChange={(e) =>
                    update({
                      shortcutStartDelayMs: Math.max(
                        0,
                        Number(e.target.value) || 0,
                      ),
                    })
                  }
                />
              </Value>
            </Row>

            <Row label="History kept">
              <Value className="w-24">
                <Input
                  type="number"
                  min={0}
                  max={1000}
                  className={rowInput}
                  value={settings.historyRetention ?? 50}
                  onChange={(e) =>
                    update({ historyRetention: Number(e.target.value) || 0 })
                  }
                />
              </Value>
            </Row>

            <Row
              label="Developer tools"
              hint="Shows the latency waterfall in the sidebar."
              className="py-[11px]"
            >
              <Toggle
                checked={settings.developerTools ?? false}
                label="Developer tools"
                onChange={(c) => update({ developerTools: c })}
              />
            </Row>
          </Group>
        </>
      )}

      {section === "prompts" && (
        <>
          <PaneTitle
            title="Prompts"
            subtitle="Replaces the built-in prompt that tidies every transcript. Leave it empty to use the default — an App Rule’s style hint is appended to whichever prompt is in play here."
          />
          {promptOutdated && (
            <Alert>
              <AlertTitle>The built-in prompt has been improved</AlertTitle>
              <AlertDescription>
                <p>
                  Your custom prompt was written against an older default, so
                  none of the fixes since have reached you.
                </p>
                <div className="flex gap-4 pt-1">
                  <Button
                    variant="plain"
                    size="text"
                    // Deliberately does NOT acknowledge: dismissing here takes
                    // "Switch to it" with it, and view-then-decide is the whole
                    // point of the button. Swift's View Default only toggles a
                    // disclosure too.
                    onClick={() =>
                      setPanel({
                        title: "Built-in prompt",
                        body: builtIn!.text,
                      })
                    }
                  >
                    View default
                  </Button>
                  <Button
                    variant="plain"
                    size="text"
                    onClick={() =>
                      update({
                        customSystemPrompt: "",
                        seenPromptVersion: builtIn!.version,
                      })
                    }
                  >
                    Switch to it
                  </Button>
                  <Button variant="plain" size="text" onClick={acknowledgePrompt}>
                    Keep mine
                  </Button>
                </div>
              </AlertDescription>
            </Alert>
          )}

          <Group>
            <Row className="block py-3">
              <Textarea
                rows={14}
                className="w-full resize-none border-0 bg-transparent p-0 font-mono text-xs shadow-none focus-visible:ring-0 dark:bg-transparent"
                placeholder="Leave empty to use the built-in cleanup prompt."
                value={settings.customSystemPrompt ?? ""}
                // The version is stamped on every edit, mirroring Swift's
                // `customSystemPromptLastModified`. Without it a fresh install
                // that types one character into an empty box is immediately
                // told its prompt "was written against an older default",
                // which is false — it was written against this one.
                onChange={(e) =>
                  update({
                    customSystemPrompt: e.target.value,
                    ...(builtIn ? { seenPromptVersion: builtIn.version } : {}),
                  })
                }
              />
            </Row>
            {(settings.customSystemPrompt ?? "").trim() !== "" && (
              <Row label="Custom prompt in use">
                <Button
                  variant="plain"
                  size="text"
                  onClick={() => update({ customSystemPrompt: "" })}
                >
                  Reset to default
                </Button>
              </Row>
            )}
          </Group>

          <Group title="Test">
            <Row className="block py-3">
              <Textarea
                rows={3}
                className="w-full resize-none border-0 bg-transparent p-0 text-xs shadow-none focus-visible:ring-0 dark:bg-transparent"
                placeholder="A raw transcript to run the prompt against — as messy as a real one."
                value={sample}
                onChange={(e) => setSample(e.target.value)}
              />
            </Row>
            <Row
              label="Run"
              hint="Uses your current model and vocabulary, so this is what the next dictation would do."
            >
              <Button
                variant="plain"
                size="text"
                disabled={running}
                onClick={runPrompt}
              >
                {running ? "Running…" : "Run"}
              </Button>
            </Row>
          </Group>

          {panel && (
            <Group title={panel.title}>
              <Row className="block py-3">
                {/* Read-only, and errors land here as text: a toast for a
                    rate limit vanishes before it can be acted on. */}
                <p className="whitespace-pre-wrap font-mono text-xs text-foreground">
                  {panel.body}
                </p>
              </Row>
            </Group>
          )}
        </>
      )}

      {section === "connectors" && (
        <>
          {/* Owns the pane title: the design puts the search box on the same
              row as it. Remounts when the key changes so the list reloads. */}
          <ConnectorEditor key={settings.composioApiKey ?? ""} />
        </>
      )}

      {section === "hotkeys" && (
        <>
          <PaneTitle
            title="Hotkeys"
            subtitle="Two roles on the same key both fire — give each its own. Escape cancels a recording."
          />
          <HotkeyEditor
            config={hotkeys}
            onChange={(hotkeys) => update({ hotkeys })}
          />

          {/* A selection is the whole trigger. The extra-modifier gesture Swift
              offered came out: it collided with the dictate key in most of the
              modifier choices, and the Rewrite hotkey above already covers
              rewriting on demand. */}
          <Group title="Command mode">
            <Row
              label="Rewrite the selection"
              hint="Dictating with text selected changes it instead of typing over it. With nothing selected, you dictate as usual."
              className="py-[11px]"
            >
              <Toggle
                checked={commandMode}
                label="Rewrite the selection"
                onChange={(c) => update({ commandModeEnabled: c })}
              />
            </Row>
          </Group>
        </>
      )}

      {section === "macros" && (
        <>
          <PaneTitle
            title="Voice Macros"
            subtitle="Say a phrase mid-dictation and it expands to the text you set."
          />
          <MacroEditor
            macros={macros}
            onChange={(macros) => update({ macros })}
          />
        </>
      )}

      {section === "rules" && (
        <AppRuleEditor
          rules={appRules}
          onChange={(appRules) => update({ appRules })}
        />
      )}

      {/* Owns its own data and its own title: it reads history and timings,
          not `settings`, so nothing above is of any use to it. */}
      {section === "runlog" && <RunLogView />}

      <p
        className={`text-xs text-ink-2 transition-opacity duration-150 ${
          saved ? "opacity-100" : "opacity-0"
        }`}
      >
        Saved
      </p>
    </div>
  );
}
