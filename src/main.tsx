import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import { DownloadCompleteWindow } from "./components/windows/download-complete-window";
import { DownloadProgressWindow } from "./components/windows/progress/download-progress-window";
import { startAppearanceSync } from "./lib/appearance";
import { DownloadProvider } from "./state/download-store";
import type { DownloadInfo } from "./types";
import "./index.css";

// The appearance is settled before the first paint, so the window opens in the
// theme, face and weight the user chose instead of flashing defaults. The same
// call makes this window follow changes made in the settings window, which is
// what keeps a progress window in the theme it was opened beside.
startAppearanceSync();

// The backend opens the two small windows with an initialization script, so
// the page knows the download before it renders: the completion window gets a
// finished one, the progress window a live one. Every other window is the app.
const progress = (
  window as Window & { __IDM_PROGRESS__?: DownloadInfo }
).__IDM_PROGRESS__;
const completed = (
  window as Window & { __IDM_COMPLETE__?: DownloadInfo }
).__IDM_COMPLETE__;

// The two small windows round their own corners. The page behind them has to
// be the same surface, or the curve shows a sliver of the app canvas outside
// it. The token is overridden here, before the first paint.
if (progress || completed) {
  document.documentElement.style.setProperty(
    "--background",
    "var(--chrome-popup)",
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {progress ? (
      <DownloadProgressWindow initial={progress} />
    ) : completed ? (
      <DownloadCompleteWindow info={completed} />
    ) : (
      <DownloadProvider>
        <App />
      </DownloadProvider>
    )}
  </StrictMode>,
);
