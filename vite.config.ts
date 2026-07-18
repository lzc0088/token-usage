import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri expects a fixed port; keep strictPort so dev server errors if busy.
// See docs/plan.md §M3 (popover served from this dev server in dev).
const HOST = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [svelte()],

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
}));
