import React from "react";
import ReactDOM from "react-dom/client";
import { RouterProvider } from "@tanstack/react-router";
import { router } from "./router";
import { TtsSettingsProvider } from "./ttsSettings";
import "./index.css";
import "./App.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <TtsSettingsProvider>
      <RouterProvider router={router} />
    </TtsSettingsProvider>
  </React.StrictMode>,
);
