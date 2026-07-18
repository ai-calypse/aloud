import { useEffect, useState, type ReactNode } from "react";
import { commands, type AudioDevice, type Settings } from "../../lib/bindings";
import { or } from "../../lib/result";
import { HotkeyEditor } from "./HotkeyEditor";
import { MacroEditor } from "./MacroEditor";
import { AppRuleEditor } from "./AppRuleEditor";
import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

/** One settings pane, selected from the app-level sidebar. */
export type SettingsSection = "general" | "hotkeys" | "macros" | "rules";

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

  useEffect(() => {
    commands.getSettings().then(setSettings);
    or(commands.inputDevices(), [], "listing microphones").then(setDevices);
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
                checked={settings.playSounds ?? true}
                onCheckedChange={(c) => update({ playSounds: c === true })}
              />
              Play sounds
            </label>

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
