import React from "react";
import ReactDOM from "react-dom/client";
import { RouterProvider } from "@tanstack/react-router";
import { router } from "./router";
import { TtsSettingsProvider } from "./ttsSettings";
import "./index.css";
import "./App.css";

// Disable the webview's default right-click menu (reload / inspect element).
// Still allow it inside text fields so copy/paste works.
window.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement | null;
  if (t && t.closest("input, textarea, [contenteditable='true']")) return;
  e.preventDefault();
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <TtsSettingsProvider>
      <RouterProvider router={router} />
    </TtsSettingsProvider>
  </React.StrictMode>,
);
