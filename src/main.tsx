import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import { DownloadCompleteWindow } from "./components/windows/download-complete-window";
import { loadSettings } from "./lib/settings";
import { applyTheme } from "./lib/theme";
import { DownloadProvider } from "./state/download-store";
import type { DownloadInfo } from "./types";
import "./index.css";

// The appearance is settled before the first paint, so the window opens in the
// theme the user chose instead of flashing the other one.
applyTheme(loadSettings().theme);

// The backend opens the completion window with this script, so the page knows
// the finished download before it renders. Every other window is the app.
const completed = (
  window as Window & { __IDM_COMPLETE__?: DownloadInfo }
).__IDM_COMPLETE__;

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {completed ? (
      <DownloadCompleteWindow info={completed} />
    ) : (
      <DownloadProvider>
        <App />
      </DownloadProvider>
    )}
  </StrictMode>,
);
