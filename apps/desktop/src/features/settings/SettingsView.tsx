import { useEffect, useState } from "react";
import { commands, type AudioDevice, type Settings } from "../../lib/bindings";
import { or } from "../../lib/result";
import { HotkeyEditor } from "./HotkeyEditor";
import { MacroEditor } from "./MacroEditor";
import { AppRuleEditor } from "./AppRuleEditor";
import { ConnectorEditor } from "./ConnectorEditor";
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
  | "connectors";

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
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [saved, setSaved] = useState(false);
  // Owned by the OS (a login item / registry key), not settings.json, so it is
  // read from the autostart plugin rather than from `settings`.
  const [atLogin, setAtLogin] = useState(false);

  useEffect(() => {
    commands.getSettings().then(setSettings);
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
  const sounds = settings.playSounds ?? true;

  /** Applies one field and persists immediately — no Save button to forget. */
  async function update(patch: Partial<Settings>) {
    const next = { ...settings!, ...patch };
    setSettings(next);

    const result = await commands.updateSettings(next);
    if (result.status === "error") {
      console.error(`saving settings: ${result.error}`);
      return;
    }
    setSaved(true);
    setTimeout(() => setSaved(false), 1200);
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
                <Row label="API key">
                  <Value className="flex-1">
                    <Input
                      type="password"
                      className={rowInput}
                      value={settings.groqApiKey ?? ""}
                      placeholder="gsk_…"
                      onChange={(e) => update({ groqApiKey: e.target.value })}
                    />
                  </Value>
                </Row>
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

            <Row label="Language">
              <Value className="flex-1">
                <Input
                  className={rowInput}
                  value={settings.language ?? ""}
                  placeholder="Auto-detect"
                  onChange={(e) => update({ language: e.target.value })}
                />
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
          <Group>
            <Row className="block py-3">
              <Textarea
                rows={14}
                className="w-full resize-none border-0 bg-transparent p-0 font-mono text-xs shadow-none focus-visible:ring-0 dark:bg-transparent"
                placeholder="Leave empty to use the built-in cleanup prompt."
                value={settings.customSystemPrompt ?? ""}
                onChange={(e) => update({ customSystemPrompt: e.target.value })}
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
        </>
      )}

      {section === "connectors" && (
        <>
          {/* Owns the pane title: the design puts the search box on the same
              row as it. Remounts when the key changes so the list reloads. */}
          <ConnectorEditor key={settings.composioApiKey ?? ""} />

          {/* Setup, not the point of the pane — so it sits under the list. The
              connector list names the missing key in its own error when unset. */}
          <Group title="Keys">
            {/* Gemini first: agent.rs reaches for this key before Cerebras,
                because Groq's tokens-per-minute limit throttles a multi-round
                tool loop and Cerebras was rate-limiting mid-command. */}
            <Row label="Gemini key" hint="Understands a spoken command">
              <Value className="flex-1">
                <Input
                  type="password"
                  className={rowInput}
                  placeholder="AIza…"
                  value={settings.geminiApiKey ?? ""}
                  onChange={(e) => update({ geminiApiKey: e.target.value })}
                />
              </Value>
            </Row>
            <Row label="Composio key" hint="Reaches the services above">
              <Value className="flex-1">
                <Input
                  type="password"
                  className={rowInput}
                  placeholder="ak_…"
                  value={settings.composioApiKey ?? ""}
                  onChange={(e) => update({ composioApiKey: e.target.value })}
                />
              </Value>
            </Row>
          </Group>
        </>
      )}

      {section === "hotkeys" && (
        <>
          <PaneTitle
            title="Hotkeys"
            subtitle="Two roles on the same key will clash — give each its own."
          />
          <HotkeyEditor
            config={hotkeys}
            onChange={(hotkeys) => update({ hotkeys })}
          />
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
