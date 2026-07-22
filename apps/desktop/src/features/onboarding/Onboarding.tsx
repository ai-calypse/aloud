import { useEffect, useState } from "react";
import { commands, type Settings } from "../../lib/bindings";

import { Button } from "@/components/ui/button";

function Lead({ children }: { children: React.ReactNode }) {
  return <p className="mb-4 leading-[1.55] text-ink-2">{children}</p>;
}

/**
 * First-run wizard, shown while `settings.onboarded` is false. A guided view
 * over the same commands Settings uses, not a parallel config path; on finish
 * it flips `onboarded` and hands control back.
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
  const [permitted, setPermitted] = useState(true);

  // Re-checked on every visit to the step, so granting in System Settings and
  // tabbing back reflects immediately.
  useEffect(() => {
    if (step === 1) commands.hotkeysPermitted().then(setPermitted);
  }, [step]);

  async function finish() {
    const next: Settings = { ...settings, engine, onboarded: true };
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
            On-device whisper works offline. Cloud is faster, and falls back to
            on-device if it is unavailable.
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
  ];

  const current = steps[step];
  const isLast = step === steps.length - 1;

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
            <Button variant="field" size="row" onClick={() => setStep((s) => s - 1)}>
              Back
            </Button>
          )}
          {isLast ? (
            <Button variant="accent" size="row" onClick={finish}>
              Finish
            </Button>
          ) : (
            <Button variant="accent" size="row" onClick={() => setStep((s) => s + 1)}>
              Next
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
