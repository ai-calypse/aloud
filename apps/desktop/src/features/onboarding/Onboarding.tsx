import { useEffect, useState } from "react";
import { commands, type Settings } from "../../lib/bindings";

// Repeated on every nav button; the "go" button layers onto it.
const BTN =
  "rounded-[8px] border-[0.5px] border-surface-border bg-field px-4 py-[0.45rem] disabled:opacity-40";
const GO =
  "border-transparent bg-brand font-semibold hover:bg-[color-mix(in_srgb,var(--accent)_80%,white)]";

function Lead({ children }: { children: React.ReactNode }) {
  return <p className="mb-4 leading-[1.55] text-ink-2">{children}</p>;
}

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
        <Lead>
          Hold a key, speak, and clean text lands wherever you're typing. A
          few quick choices and you're set.
        </Lead>
      ),
    },
    {
      title: "Grant accessibility",
      body: (
        <>
          <Lead>
            Aloud needs Accessibility permission to read your hotkey and paste
            text. Grant it in System Settings → Privacy &amp; Security →
            Accessibility.
          </Lead>
          <p className={permitted ? "text-success" : "text-warning"}>
            {permitted ? "✓ Permission granted" : "Not granted yet"}
          </p>
        </>
      ),
    },
    {
      title: "Choose an engine",
      body: (
        <>
          <Lead>
            On-device whisper works offline with no key. Cloud is faster and
            needs an API key.
          </Lead>
          <label className="flex cursor-pointer items-center gap-2 py-[0.4rem]">
            <input
              type="radio"
              name="engine"
              checked={engine === "local"}
              onChange={() => setEngine("local")}
            />
            On-device whisper (recommended)
          </label>
          <label className="flex cursor-pointer items-center gap-2 py-[0.4rem]">
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
          <Lead>
            {engine === "cloud"
              ? "Cloud transcription needs a key."
              : "Add a key now to enable cleanup, Ask, and Agent — or skip and add it later in Settings."}
          </Lead>
          <input
            className="w-full rounded-[8px] border-[0.5px] border-surface-border bg-field px-[0.7rem] py-[0.55rem]"
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
    <div className="flex min-h-screen items-center justify-center p-8">
      <div className="w-full max-w-[440px]">
        <div className="mb-6 flex gap-[0.4rem]">
          {steps.map((_, i) => (
            <span
              key={i}
              className={`size-[7px] rounded-full ${i === step ? "bg-brand" : "bg-separator"}`}
            />
          ))}
        </div>

        <h1 className="mb-3 text-[22px] font-semibold">{current.title}</h1>
        <div className="min-h-[140px]">{current.body}</div>

        <div className="mt-6 flex justify-end gap-2">
          {step > 0 && (
            <button className={BTN} onClick={() => setStep((s) => s - 1)}>
              Back
            </button>
          )}
          {isLast ? (
            <button className={`${BTN} ${GO}`} onClick={finish} disabled={blocked}>
              Finish
            </button>
          ) : (
            <button className={`${BTN} ${GO}`} onClick={() => setStep((s) => s + 1)}>
              Next
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
