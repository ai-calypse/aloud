import { useEffect, useRef, useState } from "react";
import {
  commands,
  type AudioDevice,
  type BuildInfo,
  type DefaultPrompt,
  type OverlayDisplay,
  type Settings,
} from "../../lib/bindings";
import { or } from "../../lib/result";
import { HotkeyEditor } from "./HotkeyEditor";
import { MacroEditor } from "./MacroEditor";
import { TermListEditor } from "./TermListEditor";
import { AppRuleEditor } from "./AppRuleEditor";
import { ConnectorEditor } from "./ConnectorEditor";
import { RunLogView } from "./RunLogView";
import { MeetingsView } from "./MeetingsView";
import { DebugView } from "./DebugView";
import { PermissionsCard } from "./PermissionsCard";
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

/** The wings overlay is macOS-only, and the backend forces the pill on Windows
 * regardless — the row is hidden rather than offering a dead choice. */
const IS_MAC = navigator.userAgent.includes("Mac");

/** The `named:` prefix keeps a monitor called "primary" from colliding. */
const NAMED = "named:";

function displayValue(display: OverlayDisplay | undefined): string {
  if (display === undefined || display === "active" || display === "primary") {
    return display ?? "active";
  }
  return NAMED + display.named;
}

/** One settings pane, selected from the app-level sidebar. */
export type SettingsSection =
  | "general"
  | "hotkeys"
  | "dictionary"
  | "rules"
  | "prompts"
  | "connectors"
  | "runlog"
  | "meetings"
  /** Behind `developer_tools`; the sidebar hides the row when it is off. */
  | "debug";

/** Settings forms bound to the Rust `Settings` struct, as macOS System
 * Settings grouped lists. Which pane shows comes from the `section` prop. */
export function SettingsView({
  section,
  onSettings,
}: {
  section: SettingsSection;
  // The app shell keeps its own copy for the sidebar; without this a toggle
  // here persists to disk but the shell stays stale until the next launch.
  onSettings?: (s: Settings) => void;
}) {
  const [settings, setSettings] = useState<Settings | null>(null);
  // A ref, not state: long-lived closures call `update` from an older render
  // (a hotkey capture waits up to ten seconds while the pane stays clickable),
  // and spreading that render's `settings` reverted anything changed since.
  const live = useRef(settings);
  live.current = settings;
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [displays, setDisplays] = useState<string[]>([]);
  const [build, setBuild] = useState<BuildInfo | null>(null);
  const [copied, setCopied] = useState(false);
  // Null until it loads; the Prompts banner and View Default hide until then.
  const [builtIn, setBuiltIn] = useState<DefaultPrompt | null>(null);
  const [sample, setSample] = useState("");
  const [running, setRunning] = useState(false);
  // One panel for both the run's output and the built-in prompt.
  const [panel, setPanel] = useState<{ title: string; body: string } | null>(
    null,
  );
  const [saved, setSaved] = useState(false);
  // Owned by the OS (a login item / registry key), not settings.json.
  const [atLogin, setAtLogin] = useState(false);

  useEffect(() => {
    commands.getSettings().then(setSettings);
    commands.defaultPrompt().then(setBuiltIn);
    or(commands.inputDevices(), [], "listing microphones").then(setDevices);
    or(commands.launchAtLogin(), false, "reading launch-at-login").then(
      setAtLogin,
    );
    or(commands.overlayDisplays(), [], "listing displays").then(setDisplays);
    commands
      .buildInfo()
      .then(setBuild)
      .catch((e) => console.error(`reading build info: ${e}`));
  }, []);

  if (!settings) return <p className="text-ink-2">Loading…</p>;

  // Every field is optional in TS because the Rust struct is #[serde(default)]
  // — a partial settings file is valid. Defaulted once here rather than
  // scattering `??` through the markup, where one row drifts from the rest.
  const engine = settings.engine ?? "local";
  const vocabulary = settings.vocabulary ?? [];
  const hotkeys = settings.hotkeys ?? {
    hold: null,
    toggle: null,
    copy_again: null,
    ask: null,
    rewrite: null,
    meeting_notes: null,
    agent: null,
  };
  const preservePhrases = settings.preservePhrases ?? [];
  const macros = settings.macros ?? [];
  const appRules = settings.appRules ?? [];
  const tidy = settings.postProcessingEnabled ?? true;
  const commandMode = settings.commandModeEnabled ?? true;
  const overlayDisplay = settings.overlayDisplay;
  const chosenDisplay =
    overlayDisplay &&
    typeof overlayDisplay === "object" &&
    "named" in overlayDisplay
      ? overlayDisplay.named
      : null;
  const sounds = settings.playSounds ?? true;

  /** Applies one field and persists immediately — no Save button to forget. */
  async function update(patch: Partial<Settings>) {
    const next = { ...live.current!, ...patch };
    // Written here too, so two updates in one React batch cannot lose a field.
    live.current = next;
    setSettings(next);
    onSettings?.(next);

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
      // A rejected invoke (serialization, a panic) never reaches the Result
      // type; without this the button stays disabled until the pane remounts.
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
              label="Show words as you speak"
              hint="Shows the words in a panel as you speak them, read on-device by the local model. A preview only — it does not change what gets pasted, which is still one clean pass over the whole recording. Needs the local model downloaded, and keeps a core busy while you talk."
              className="py-[11px]"
            >
              <Toggle
                checked={settings.streamingTranscription ?? false}
                label="Show words as you speak"
                onChange={(c) => update({ streamingTranscription: c })}
              />
            </Row>

            <Row
              label="Spoken language"
              hint="The language you dictate in. Auto-detect can guess wrong on short utterances; naming it avoids that."
            >
              <Value>
                <Select
                  // Lowercased: this field used to be free text, and a stored
                  // "EN" matches no item, rendering an empty trigger.
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
                {/* In Cleanup, not Transcription: whisper's translate task
                    only targets English, so this instructs the cleanup model. */}
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
                  label="Guard against answered dictations"
                  hint="If the model replies to what you said instead of tidying it, retry once and otherwise paste exactly what you dictated."
                  className="py-[11px]"
                >
                  <Toggle
                    checked={settings.instructionGuardEnabled ?? true}
                    label="Guard against answered dictations"
                    onChange={(c) => update({ instructionGuardEnabled: c })}
                  />
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

            <Row
              label="Say “press enter” at the end to submit"
              className="py-[11px]"
            >
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
              label="Recordings kept"
              hint="Megabytes of dictation audio to keep on disk. Off by default; 250 is about four hours. 0 keeps none, and the Run Log's player and its transcription retry go with it."
            >
              <Value className="w-24">
                <Input
                  type="number"
                  min={0}
                  max={10000}
                  step={50}
                  className={rowInput}
                  value={settings.audioRetentionMb ?? 0}
                  onChange={(e) =>
                    update({
                      audioRetentionMb: Math.max(
                        0,
                        Number(e.target.value) || 0,
                      ),
                    })
                  }
                />
              </Value>
            </Row>

            <Row
              label="Recordings expire after"
              hint="Days. 0 leaves the size limit as the only one."
            >
              <Value className="w-24">
                <Input
                  type="number"
                  min={0}
                  max={365}
                  className={rowInput}
                  value={settings.audioRetentionDays ?? 30}
                  onChange={(e) =>
                    update({
                      audioRetentionDays: Math.max(
                        0,
                        Number(e.target.value) || 0,
                      ),
                    })
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

          <Group title="Overlay">
            {IS_MAC && (
              <Row
                label="Style"
                hint="Both weld to the top edge. Wings stay inside the menu bar, and hand over to the drop-down pill whenever there is something to show that will not fit in one — a live transcript, an answer, or an error."
              >
                <Value>
                  <Select
                    value={settings.overlayStyle ?? "dropPill"}
                    onValueChange={(v) =>
                      update({ overlayStyle: v as Settings["overlayStyle"] })
                    }
                  >
                    <SelectTrigger className={rowSelect}>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="dropPill">Drop-down pill</SelectItem>
                      <SelectItem value="wings">Menu bar wings</SelectItem>
                    </SelectContent>
                  </Select>
                </Value>
              </Row>
            )}

            <Row
              label="Show on"
              hint="A named display that is unplugged falls back to the primary one."
            >
              <Value>
                <Select
                  value={displayValue(settings.overlayDisplay)}
                  onValueChange={(v) =>
                    update({
                      overlayDisplay: v.startsWith(NAMED)
                        ? { named: v.slice(NAMED.length) }
                        : (v as OverlayDisplay),
                    })
                  }
                >
                  <SelectTrigger className={rowSelect}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="active">Active window</SelectItem>
                    <SelectItem value="primary">Primary display</SelectItem>
                    {/* Radix renders the trigger blank when the value matches
                        no item, so an unplugged display needs one of its own. */}
                    {chosenDisplay && !displays.includes(chosenDisplay) && (
                      <SelectItem value={NAMED + chosenDisplay}>
                        {chosenDisplay} (not connected)
                      </SelectItem>
                    )}
                    {displays.map((name) => (
                      <SelectItem key={name} value={NAMED + name}>
                        {name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Value>
            </Row>
          </Group>

          <PermissionsCard />

          {build && (
            <Group title="About">
              <Row
                label="Version"
                hint={`${build.commit} · built ${build.date}`}
              >
                <Button
                  variant="plain"
                  size="text"
                  onClick={() =>
                    navigator.clipboard
                      .writeText(
                        `Aloud ${build.version} (${build.commit}) built ${build.date}`,
                      )
                      .then(() => {
                        setCopied(true);
                        setTimeout(() => setCopied(false), 1200);
                      })
                  }
                >
                  {copied ? "Copied" : build.version}
                </Button>
              </Row>
            </Group>
          )}
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
                    // Deliberately does NOT acknowledge: dismissing here would
                    // take "Switch to it" with it, and the point is to view
                    // then decide.
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
                  <Button
                    variant="plain"
                    size="text"
                    onClick={acknowledgePrompt}
                  >
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
                // The version is stamped on every edit: without it, typing one
                // character into an empty box is immediately told the prompt
                // "was written against an older default", which is false.
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
                {/* Errors land here as text: a toast for a rate limit vanishes
                    before it can be acted on. */}
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
          {/* Owns its own pane title. Keyed on the API key so it remounts and
              reloads the list when that changes. */}
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

      {/* Three mechanisms, one pane, in the order a dictation passes through
          them: the transcriber is biased, then cleanup is fenced off from
          certain phrases, then the finished text is substituted into. Together
          because what separates them — better odds, left alone, always exactly
          this — is only legible side by side. */}
      {section === "dictionary" && (
        <>
          <PaneTitle
            title="Dictionary"
            subtitle="Teach Aloud the words you use, the phrases it should not touch, and the shortcuts that save you typing."
          />

          <TermListEditor
            title="Words to get right"
            terms={vocabulary}
            placeholder="Kubernetes"
            addLabel="Add Word"
            empty={{
              title: "No Words Yet",
              body: "Add a name or word Aloud keeps hearing wrong.",
            }}
            onChange={(vocabulary) => update({ vocabulary })}
          >
            <p className="max-w-[520px] text-hint text-ink-2">
              Names, brands and jargon Aloud mishears. Add "Kubernetes" and
              "cuber netties" comes out right. One word or name per line — this
              helps a lot of the time, but it is not a promise.
            </p>
          </TermListEditor>

          <TermListEditor
            title="Say it, keep it"
            terms={preservePhrases}
            placeholder="user underscore id"
            addLabel="Add Phrase"
            empty={{
              title: "Nothing Here Yet",
              body: "Add a phrase Aloud should type word for word.",
            }}
            onChange={(preservePhrases) => update({ preservePhrases })}
          >
            <p className="max-w-[520px] text-hint text-ink-2">
              Aloud tidies what you say — it drops "um", trims repeated words
              and turns spoken shortcuts into symbols. Anything listed here it
              types word for word instead. Say "user underscore id" and you get
              "user underscore id", not "user_id". The rest of the sentence is
              still tidied.
            </p>
          </TermListEditor>

          <MacroEditor
            macros={macros}
            onChange={(macros) => update({ macros })}
          >
            <p className="max-w-[520px] text-hint text-ink-2">
              Say something short, get something long. Say "my email" and Aloud
              types your full address. This one always happens, every time.
              Dictation only — it does not apply when you ask a question or use
              the agent.
            </p>
          </MacroEditor>
        </>
      )}

      {section === "rules" && (
        <AppRuleEditor
          rules={appRules}
          onChange={(appRules) => update({ appRules })}
        />
      )}

      {/* These three own their data and their titles: they read history and
          timings, not `settings`. */}
      {section === "runlog" && <RunLogView />}
      {section === "meetings" && <MeetingsView />}
      {section === "debug" && <DebugView />}

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
