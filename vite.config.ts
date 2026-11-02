import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

// Tauri drives this dev server; the port must match `devUrl` in tauri.conf.json.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // The Rust workspace puts its build tree at the repository root, so the
    // watcher must stay out of it (cargo locks binaries while linking, which
    // made Vite crash with EBUSY).
    watch: { ignored: ["**/src-tauri/**", "**/site/**", "**/target/**"] },
  },
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
});
