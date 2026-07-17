import { useEffect, useState } from "react";
import { commands, type Settings } from "../../lib/bindings";
import "./onboarding.css";

/**
 * First-run wizard. Shown once, when `settings.onboarded` is false, in place of
 * the main window's tabs. Walks the four things a new user must decide:
 * permission, engine, an optional key, and the hold-to-talk key.
 *
 * It reuses the same commands the Settings panel does — this is a guided view
 * over existing settings, not a parallel config path. On finish it flips
 * `onboarded` and hands control back to the app.
 */
export function Onboarding({
  settings,
  onDone,
}: {
  settings: Settings;
  onDone: (settings: Settings) => void;
}) {
  const [step, setStep] = useState(0);
  const [engine, setEngine] = useState<Settings["engine"]>(settings.engine);
  const [apiKey, setApiKey] = useState(settings.groqApiKey ?? "");
  const [permitted, setPermitted] = useState(true);

  // Re-check the permission each time the permission step is shown, so granting
  // it in System Settings and tabbing back reflects immediately.
  useEffect(() => {
    if (step === 1) commands.hotkeysPermitted().then(setPermitted);
  }, [step]);

  async function finish() {
    const next: Settings = {
      ...settings,
      engine,
      groqApiKey: apiKey,
      onboarded: true,
    };
    const result = await commands.updateSettings(next);
    if (result.status === "error") {
      console.error(`saving onboarding: ${result.error}`);
      return;
    }
    onDone(next);
  }

  const steps = [
    {
      title: "Welcome to Aloud",
      body: (
        <p className="ob__lead">
          Hold a key, speak, and clean text lands wherever you're typing. A
          few quick choices and you're set.
        </p>
      ),
    },
    {
      title: "Grant accessibility",
      body: (
        <>
          <p className="ob__lead">
            Aloud needs Accessibility permission to read your hotkey and paste
            text. Grant it in System Settings → Privacy &amp; Security →
            Accessibility.
          </p>
          <p className={permitted ? "ob__ok" : "ob__warn"}>
            {permitted ? "✓ Permission granted" : "Not granted yet"}
          </p>
        </>
      ),
    },
    {
      title: "Choose an engine",
      body: (
        <>
          <p className="ob__lead">
            On-device whisper works offline with no key. Cloud is faster and
            needs an API key.
          </p>
          <label className="ob__choice">
            <input
              type="radio"
              name="engine"
              checked={engine === "local"}
              onChange={() => setEngine("local")}
            />
            On-device whisper (recommended)
          </label>
          <label className="ob__choice">
            <input
              type="radio"
              name="engine"
              checked={engine === "cloud"}
              onChange={() => setEngine("cloud")}
            />
            Cloud
          </label>
        </>
      ),
    },
    {
      title: engine === "cloud" ? "Add your API key" : "Optional: API key",
      body: (
        <>
          <p className="ob__lead">
            {engine === "cloud"
              ? "Cloud transcription needs a key."
              : "Add a key now to enable cleanup, Ask, and Agent — or skip and add it later in Settings."}
          </p>
          <input
            className="ob__input"
            type="password"
            placeholder="API key"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
          />
        </>
      ),
    },
  ];

  const current = steps[step];
  const isLast = step === steps.length - 1;
  // The only hard gate: cloud without a key can't transcribe at all.
  const blocked = isLast && engine === "cloud" && apiKey.trim() === "";

  return (
    <div className="ob">
      <div className="ob__card">
        <div className="ob__dots">
          {steps.map((_, i) => (
            <span key={i} className={`ob__dot ${i === step ? "ob__dot--on" : ""}`} />
          ))}
        </div>

        <h1 className="ob__title">{current.title}</h1>
        <div className="ob__body">{current.body}</div>

        <div className="ob__nav">
          {step > 0 && (
            <button className="ob__btn" onClick={() => setStep((s) => s - 1)}>
              Back
            </button>
          )}
          {isLast ? (
            <button className="ob__btn ob__btn--go" onClick={finish} disabled={blocked}>
              Finish
            </button>
          ) : (
            <button className="ob__btn ob__btn--go" onClick={() => setStep((s) => s + 1)}>
              Next
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
