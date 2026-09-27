import react from "@vitejs/plugin-react";
import process from "node:process";
import { defineConfig } from "vitest/config";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],

  // Vite options tailored for Tauri, applied in `tauri dev` and `tauri build`:
  // 1. don't hide Rust errors
  clearScreen: false,
  // 2. Tauri expects a fixed port, so fail if it's taken
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
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
