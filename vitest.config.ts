import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

// Kept separate from vite.config.ts on purpose: vitest.config.ts takes
// precedence, so the Tailwind plugin and the Tauri dev-server settings never
// load during a test run. The same "@/..." alias keeps imports identical to
// the application.
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: ["src/test/setup.ts"],
    restoreMocks: true,
  },
});
