import { useEffect, useState, type ReactNode } from "react";
import { commands, type AudioDevice, type Settings } from "../../lib/bindings";
import { or } from "../../lib/result";
import { HotkeyEditor } from "./HotkeyEditor";
import { MacroEditor } from "./MacroEditor";
import { AppRuleEditor } from "./AppRuleEditor";
import { ConnectorEditor } from "./ConnectorEditor";
import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
import { Button } from "@/components/ui/button";
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

/** Glass card grouping related fields (mirrors the Swift SettingsCard). */
function Card({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="rounded-2xl border border-border bg-card p-[18px] backdrop-blur-2xl">
      {title && <h3 className="mb-3.5 text-[0.95rem] font-semibold">{title}</h3>}
      <div className="flex flex-col gap-3">{children}</div>
    </section>
  );
}

/** A labelled field row: label on the left, control on the right. */
function Field({
  label,
  children,
  hint,
}: {
  label: string;
  children: ReactNode;
  hint?: string;
}) {
  return (
    <label className="grid grid-cols-[9rem_1fr] items-center gap-x-4 gap-y-1 text-sm">
      <span>{label}</span>
      {children}
      {hint && (
        <small className="col-start-2 text-xs text-muted-foreground">
          {hint}
        </small>
      )}
    </label>
  );
}

/**
 * Settings forms bound to the Rust `Settings` struct.
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

  if (!settings) return <p className="text-muted-foreground">Loading…</p>;

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
    command: null,
    meeting_notes: null,
    agent: null,
  };
  const macros = settings.macros ?? [];
  const appRules = settings.appRules ?? [];

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
    <div className="flex flex-col gap-5">
      {section === "general" && (
        <>
          <Card title="Transcription">
            <Field label="Engine">
              <Select
                value={engine}
                onValueChange={(v) =>
                  update({ engine: v as Settings["engine"] })
                }
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="local">
                    On-device (no key needed)
                  </SelectItem>
                  <SelectItem value="cloud">Cloud</SelectItem>
                </SelectContent>
              </Select>
            </Field>

            {engine === "cloud" && (
              <>
                <Field label="API key">
                  <Input
                    type="password"
                    value={settings.groqApiKey ?? ""}
                    placeholder="gsk_…"
                    onChange={(e) => update({ groqApiKey: e.target.value })}
                  />
                </Field>
                <Field label="Model">
                  <Input
                    value={settings.transcriptionModel ?? ""}
                    onChange={(e) =>
                      update({ transcriptionModel: e.target.value })
                    }
                  />
                </Field>
                <Field label="Base URL">
                  <Input
                    value={settings.baseUrl ?? ""}
                    onChange={(e) => update({ baseUrl: e.target.value })}
                  />
                </Field>
              </>
            )}

            <Field label="Microphone">
              <Select
                value={settings.selectedMicrophone ?? "default"}
                onValueChange={(v) =>
                  update({ selectedMicrophone: v === "default" ? null : v })
                }
              >
                <SelectTrigger>
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
            </Field>

            <Field label="Language">
              <Input
                value={settings.language ?? ""}
                placeholder="Auto-detect"
                onChange={(e) => update({ language: e.target.value })}
              />
            </Field>
          </Card>

          <Card title="Cleanup">
            <label className="flex items-center gap-2 text-sm">
              <Checkbox
                checked={settings.postProcessingEnabled ?? true}
                onCheckedChange={(c) =>
                  update({ postProcessingEnabled: c === true })
                }
              />
              Tidy transcripts with an LLM
            </label>

            {(settings.postProcessingEnabled ?? true) && (
              <>
                <Field label="Model">
                  <Input
                    value={settings.postProcessingModel ?? ""}
                    onChange={(e) =>
                      update({ postProcessingModel: e.target.value })
                    }
                  />
                </Field>
                <Field label="Fallback model">
                  <Input
                    value={settings.fallbackModel ?? ""}
                    onChange={(e) => update({ fallbackModel: e.target.value })}
                  />
                </Field>
                <Field
                  label="Vocabulary"
                  hint="Names and jargon the model should spell correctly, comma separated."
                >
                  <Input
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
                </Field>
              </>
            )}
          </Card>

          <Card title="Behaviour">
            <label className="flex items-center gap-2 text-sm">
              <Checkbox
                checked={atLogin}
                onCheckedChange={async (c) => {
                  const enabled = c === true;
                  setAtLogin(enabled);
                  const result = await commands.setLaunchAtLogin(enabled);
                  // The OS is the source of truth: if it refused, snap back
                  // rather than showing a checkbox that lies.
                  if (result.status === "error") {
                    console.error(`launch at login: ${result.error}`);
                    setAtLogin(!enabled);
                  }
                }}
              />
              Launch Aloud at login
            </label>

            <label className="flex items-center gap-2 text-sm">
              <Checkbox
                checked={settings.pressEnterCommand ?? true}
                onCheckedChange={(c) =>
                  update({ pressEnterCommand: c === true })
                }
              />
              Say “press enter” at the end to submit
            </label>

            <label className="flex items-center gap-2 text-sm">
              <Checkbox
                checked={settings.playSounds ?? true}
                onCheckedChange={(c) => update({ playSounds: c === true })}
              />
              Play sounds
            </label>

            {(settings.playSounds ?? true) && (
              <Field label="Volume">
                <div className="flex items-center gap-3">
                  <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.1}
                    className="flex-1 accent-primary"
                    value={settings.soundVolume ?? 0.5}
                    onChange={(e) =>
                      update({ soundVolume: Number(e.target.value) })
                    }
                  />
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() =>
                      commands.previewSound(settings.soundVolume ?? 0.5)
                    }
                  >
                    Preview
                  </Button>
                </div>
              </Field>
            )}

            <Field label="History kept">
              <Input
                type="number"
                min={0}
                max={1000}
                value={settings.historyRetention ?? 50}
                onChange={(e) =>
                  update({ historyRetention: Number(e.target.value) || 0 })
                }
              />
            </Field>
          </Card>
        </>
      )}

      {section === "prompts" && (
        <Card title="Cleanup prompt">
          <p className="m-0 text-xs text-muted-foreground">
            Replaces the built-in prompt that tidies every transcript. Leave it
            empty to use the default. An App Rule’s style hint is appended to
            whichever prompt is in play here.
          </p>
          <Textarea
            rows={14}
            className="font-mono text-xs"
            placeholder="Leave empty to use the built-in cleanup prompt."
            value={settings.customSystemPrompt ?? ""}
            onChange={(e) => update({ customSystemPrompt: e.target.value })}
          />
          {(settings.customSystemPrompt ?? "").trim() !== "" && (
            <Button
              variant="secondary"
              size="sm"
              className="self-start"
              onClick={() => update({ customSystemPrompt: "" })}
            >
              Reset to default
            </Button>
          )}
        </Card>
      )}

      {section === "connectors" && (
        <>
          <Card title="Keys">
            <p className="m-0 text-xs text-muted-foreground">
              Agent mode needs a Cerebras key to understand a spoken command and
              a Composio key to reach the services below.
            </p>
            <Field label="Cerebras key">
              <Input
                type="password"
                placeholder="csk-…"
                value={settings.cerebrasApiKey ?? ""}
                onChange={(e) => update({ cerebrasApiKey: e.target.value })}
              />
            </Field>
            <Field label="Composio key">
              <Input
                type="password"
                placeholder="ak_…"
                value={settings.composioApiKey ?? ""}
                onChange={(e) => update({ composioApiKey: e.target.value })}
              />
            </Field>
          </Card>

          {/* Remounts when the key changes so the list reloads against it. */}
          <ConnectorEditor key={settings.composioApiKey ?? ""} />
        </>
      )}

      {section === "hotkeys" && (
        <Card>
          <HotkeyEditor
            config={hotkeys}
            onChange={(hotkeys) => update({ hotkeys })}
          />
        </Card>
      )}

      {section === "macros" && (
        <Card>
          <MacroEditor
            macros={macros}
            onChange={(macros) => update({ macros })}
          />
        </Card>
      )}

      {section === "rules" && (
        <Card>
          <AppRuleEditor
            rules={appRules}
            onChange={(appRules) => update({ appRules })}
          />
        </Card>
      )}

      <p
        className={`text-xs text-muted-foreground transition-opacity duration-150 ${
          saved ? "opacity-100" : "opacity-0"
        }`}
      >
        Saved
      </p>
    </div>
  );
}
