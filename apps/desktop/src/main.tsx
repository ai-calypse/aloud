import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

const mount = () =>
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );

// A plain browser (screenshots, checking a pane without a rebuild) has no
// Rust: stand one in first, as overlay.tsx does. A dynamic import, so the real
// app never loads it.
if ("__TAURI_INTERNALS__" in window) mount();
else void import("./lib/preview-backend").then((m) => (m.installPreviewBackend("main"), mount()));
