// Throwaway: renders the settings panes so the refactor can be compared
// against the screenshots taken before it.
const DATA: Record<string, unknown> = {
  get_settings: {
    onboarded: true, engine: "cloud", language: "", selectedMicrophone: null,
    groqApiKey: "gsk_x", transcriptionModel: "whisper-large-v3", baseUrl: "https://api.groq.com",
    postProcessingEnabled: true, postProcessingModel: "llama-3.3-70b-versatile",
    fallbackModel: "llama-3.1-8b-instant", vocabulary: ["Kubernetes", "Tauri"],
    pressEnterCommand: true, playSounds: true, soundVolume: 0.5, historyRetention: 50,
    customSystemPrompt: "", macros: [{ trigger: "my email", expansion: "me@example.com" }],
    appRules: [{ app_matches: "Cursor", post_processing_enabled: false, custom_prompt: "" }],
    hotkeys: { hold: { kind: "modifierKey", key: "fn", modifiers: [] }, toggle: null,
      copy_again: null, ask: { kind: "modifierKey", key: "rightOption", modifiers: [] },
      command: null, agent: null, meeting_notes: null },
  },
  input_devices: [{ id: "1", name: "MacBook Pro Microphone" }],
  launch_at_login: true, installed_apps: [],
};
(window as any).__TAURI_INTERNALS__ = {
  invoke: (cmd: string) => Promise.resolve(DATA[cmd] ?? []),
  transformCallback: (cb: unknown) => cb,
};
const { createRoot } = await import("react-dom/client");
const { SettingsView } = await import("./features/settings/SettingsView");
await import("./index.css");
createRoot(document.getElementById("root")!).render(
  <div style={{ background: "var(--bg)", color: "var(--fg)", padding: "28px 30px", width: 686 }}>
    <SettingsView section="general" />
  </div>,
);
