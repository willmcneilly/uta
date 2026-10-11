import react from "@vitejs/plugin-react";
import process from "node:process";
import { defineConfig } from "vitest/config";
import { designTuning } from "./scripts/designServer.ts";

const host = process.env.TAURI_DEV_HOST;
// scripts/dev-app picks a free port and passes it here; 1420 otherwise.
const port = Number(process.env.UTA_DEV_PORT ?? 1420);

// https://vite.dev/config/
export default defineConfig({
  // designTuning: the tuning panel's endpoints, on the dev server only.
  plugins: [react(), designTuning()],

  // Vite options tailored for Tauri, applied in `tauri dev` and `tauri build`:
  // 1. don't hide Rust errors
  clearScreen: false,
  // 2. Tauri expects a fixed port, so fail if it's taken
  server: {
    port,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: port + 1,
        }
      : undefined,
    watch: {
      // 3. don't watch the Rust side
      ignored: ["**/src-tauri/**"],
    },
  },

  test: {
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
  },
});
