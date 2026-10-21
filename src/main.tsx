import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import { DownloadProvider } from "./state/download-store";
import "./index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <DownloadProvider>
      <App />
    </DownloadProvider>
  </StrictMode>,
);
