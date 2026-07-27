import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import pkg from "./package.json" with { type: "json" };

// Tauri expects a fixed port; keep strictPort so dev server errors if busy.
// See docs/plan.md §M3 (popover served from this dev server in dev).
const HOST = process.env.TAURI_DEV_HOST;

// Git hosting repo for update checks. Supports:
//   Gitee  : "gitee.com/owner/repo"
//   GitHub : "owner/repo"
// Default: your Gitee repo. Override via VITE_UPDATE_REPO env var or .env file.
const UPDATE_REPO = process.env.VITE_UPDATE_REPO || "gitee.com/lzc0088/token-usage";

export default defineConfig(async () => {
  // Inject package.json version + update repo as env vars so the frontend
  // can read them via `import.meta.env.VITE_*`.
  const env = {
    VITE_APP_VERSION: pkg.version,
    VITE_UPDATE_REPO: UPDATE_REPO,
  };

  return {
    plugins: [svelte()],

    envPrefix: ["VITE_"],
    env,

    // Vite options for Tauri: no clearing of screen, fixed port, relative base.
    clearScreen: false,
    server: {
      port: 1420,
      strictPort: true,
      host: HOST || false,
      hmr: HOST
        ? { protocol: "ws", host: HOST, port: 1421 }
        : undefined,
      watch: {
        // Don't watch the Rust source; Tauri handles its own reload.
        ignored: ["**/src-tauri/**"],
      },
    },
    // Produce sourcemaps for debug builds.
    build: {
      target: "es2021",
      sourcemap: !!process.env.TAURI_DEBUG,
    },
  };
});
