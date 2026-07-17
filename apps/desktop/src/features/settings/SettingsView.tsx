import { useEffect, useState } from "react";
import { commands, type AudioDevice, type Settings } from "../../lib/bindings";
import { or } from "../../lib/result";
import "./settings.css";

/**
 * Settings forms bound to the Rust `Settings` struct.
 *
 * The Swift app had seven tabs over ~70 UserDefaults keys. This is the subset
 * that exists so far; the rest arrive with the features they configure.
 */
export function SettingsView() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    commands.getSettings().then(setSettings);
    or(commands.inputDevices(), [], "listing microphones").then(setDevices);
  }, []);

  if (!settings) return <p>Loading…</p>;

  // Every field is optional in TS because the Rust struct is #[serde(default)]
  // — a partial settings file is valid. Read through these rather than
  // scattering `??` through the markup.
  const engine = settings.engine ?? "local";
  const vocabulary = settings.vocabulary ?? [];

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
    <div className="settings">
      <section>
        <h3>Transcription</h3>

        <label className="field">
          <span>Engine</span>
          <select
            value={engine}
            onChange={(e) => update({ engine: e.target.value as Settings["engine"] })}
          >
            <option value="local">On-device (no key needed)</option>
            <option value="cloud">Cloud</option>
          </select>
        </label>

        {engine === "cloud" && (
          <>
            <label className="field">
              <span>API key</span>
              <input
                type="password"
                value={settings.groqApiKey ?? ""}
                placeholder="gsk_…"
                onChange={(e) => update({ groqApiKey: e.target.value })}
              />
            </label>
            <label className="field">
              <span>Model</span>
              <input
                value={settings.transcriptionModel ?? ""}
                onChange={(e) => update({ transcriptionModel: e.target.value })}
              />
            </label>
            <label className="field">
              <span>Base URL</span>
              <input
                value={settings.baseUrl ?? ""}
                onChange={(e) => update({ baseUrl: e.target.value })}
              />
            </label>
          </>
        )}

        <label className="field">
          <span>Microphone</span>
          <select
            value={settings.selectedMicrophone ?? ""}
            onChange={(e) =>
              update({ selectedMicrophone: e.target.value || null })
            }
          >
            <option value="">System default</option>
            {devices.map((d) => (
              <option key={d.id} value={d.id}>
                {d.name}
              </option>
            ))}
          </select>
        </label>

        <label className="field">
          <span>Language</span>
          <input
            value={settings.language ?? ""}
            placeholder="Auto-detect"
            onChange={(e) => update({ language: e.target.value })}
          />
        </label>
      </section>

      <section>
        <h3>Cleanup</h3>

        <label className="field field--check">
          <input
            type="checkbox"
            checked={settings.postProcessingEnabled ?? true}
            onChange={(e) => update({ postProcessingEnabled: e.target.checked })}
          />
          <span>Tidy transcripts with an LLM</span>
        </label>

        {(settings.postProcessingEnabled ?? true) && (
          <>
            <label className="field">
              <span>Model</span>
              <input
                value={settings.postProcessingModel ?? ""}
                onChange={(e) => update({ postProcessingModel: e.target.value })}
              />
            </label>
            <label className="field">
              <span>Fallback model</span>
              <input
                value={settings.fallbackModel ?? ""}
                onChange={(e) => update({ fallbackModel: e.target.value })}
              />
            </label>
            <label className="field field--wide">
              <span>Vocabulary</span>
              <input
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
              <small>
                Names and jargon the model should spell correctly, comma
                separated.
              </small>
            </label>
          </>
        )}
      </section>

      <section>
        <h3>Behaviour</h3>

        <label className="field field--check">
          <input
            type="checkbox"
            checked={settings.playSounds ?? true}
            onChange={(e) => update({ playSounds: e.target.checked })}
          />
          <span>Play sounds</span>
        </label>

        <label className="field">
          <span>History kept</span>
          <input
            type="number"
            min={0}
            max={1000}
            value={settings.historyRetention ?? 50}
            onChange={(e) =>
              update({ historyRetention: Number(e.target.value) || 0 })
            }
          />
        </label>
      </section>

      <p className={`settings__saved ${saved ? "settings__saved--on" : ""}`}>
        Saved
      </p>
    </div>
  );
}
